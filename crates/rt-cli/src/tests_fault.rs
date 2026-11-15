//! `FaultFs`: an in-memory file system that fails or crashes at a chosen
//! numbered operation (design §3.8.3, Req 26.31; policy sections 6 and 10).
//!
//! Contract:
//!
//! * Every call of the [`Fs`] trait is one numbered operation (1-based),
//!   including reads, `try_lock`, `sync_dir` and `list_dir`.
//! * `Fail(n)`: operation `n` returns an error and has no effect. Later
//!   operations behave normally, so the production error handling runs.
//! * `Partial(n)`: operation `n`, which must be a `create_new`, leaves a file
//!   with the first half of the bytes and returns an error.
//! * `CrashAfter(n)`: operation `n` takes full effect, then the process is
//!   dead: every later call fails with no effect. [`FaultFs::revive`] models
//!   the next process.
//! * `CrashPartial(n)`: as `Partial(n)`, then the process is dead.
//! * Several faults may be active in one run (`add_fault`), each with its
//!   own operation number in the same numbering. This models a second fault
//!   during the handling of the first, such as a failed rollback.
//! * `tamper_after(n, path, bytes)` replaces a file's bytes right after
//!   operation `n`, as a person with write access could.
//! * `tamper_after_read(path, n, changes)` does the same right after the `n`th
//!   read of `path`, which keeps a reader test independent of operation
//!   numbers. Several files can change at once, as a commit or rollback does.
//! * Locks are advisory, held in memory and released when the guard drops, also
//!   after a crash, as the kernel releases them. A shared lock needs the lock
//!   file to exist (`NotFound` otherwise, like `StdFs`), fails while the
//!   exclusive lock is held and keeps the exclusive lock out while it is held.
//! * `without_advisory_locks` models a file system with no locks: both lock
//!   calls report `Unsupported`, and a shared lock on a missing file still
//!   reports `NotFound`, as `StdFs` does.
//!
//! Not modelled: loss of data that was written but not synced, and reordering
//! of operations by the storage stack. The tests therefore establish the
//! protocol order and the recovery logic, not the durability of `sync_all`.

use super::*;
use crate::io::LockHeld;
use std::collections::BTreeSet;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    None,
    Fail(usize),
    Partial(usize),
    CrashAfter(usize),
    CrashPartial(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OpRecord {
    pub kind: &'static str,
    pub path: PathBuf,
    pub to: Option<PathBuf>,
}

impl OpRecord {
    /// True for a `rename` of a file named `from` to a file named `to`.
    pub(super) fn is_rename(&self, from: &str, to: &str) -> bool {
        self.kind == "rename"
            && self.path == Path::new(from)
            && self.to.as_deref() == Some(Path::new(to))
    }
}

struct Inner {
    files: BTreeMap<PathBuf, Vec<u8>>,
    ops: usize,
    faults: Vec<Fault>,
    tampers: Vec<(usize, PathBuf, Vec<u8>)>,
    read_tampers: Vec<ReadTamper>,
    read_counts: BTreeMap<PathBuf, usize>,
    pending: Vec<(PathBuf, Vec<u8>)>,
    dead: bool,
    log: Vec<OpRecord>,
    len_calls: Vec<PathBuf>,
}

/// Changes that take effect right after the `nth` read of `path`.
struct ReadTamper {
    path: PathBuf,
    nth: usize,
    changes: Vec<(PathBuf, Vec<u8>)>,
}

enum Act {
    Run,
    Fail,
    Torn,
    RunThenDie,
    TornThenDie,
}

pub(super) struct FaultFs {
    inner: RefCell<Inner>,
    locks_unsupported: Cell<bool>,
    locks: Rc<RefCell<BTreeSet<PathBuf>>>,
    shared: Rc<RefCell<BTreeMap<PathBuf, usize>>>,
}

struct Guard {
    locks: Rc<RefCell<BTreeSet<PathBuf>>>,
    path: PathBuf,
}

impl LockHeld for Guard {}

impl Drop for Guard {
    fn drop(&mut self) {
        self.locks.borrow_mut().remove(&self.path);
    }
}

struct SharedGuard {
    shared: Rc<RefCell<BTreeMap<PathBuf, usize>>>,
    path: PathBuf,
}

impl LockHeld for SharedGuard {}

impl Drop for SharedGuard {
    fn drop(&mut self) {
        let mut shared = self.shared.borrow_mut();
        if let Some(count) = shared.get_mut(&self.path) {
            *count -= 1;
            if *count == 0 {
                shared.remove(&self.path);
            }
        }
    }
}

fn injected() -> io::Error {
    io::Error::other("injected fault")
}

fn act_for(fault: Fault, n: usize, kind: &str) -> Act {
    match fault {
        Fault::Fail(at) if at == n => Act::Fail,
        Fault::Partial(at) if at == n && kind == "create_new" => Act::Torn,
        Fault::Partial(at) if at == n => Act::Fail,
        Fault::CrashAfter(at) if at == n => Act::RunThenDie,
        Fault::CrashPartial(at) if at == n && kind == "create_new" => Act::TornThenDie,
        Fault::CrashPartial(at) if at == n => Act::RunThenDie,
        _ => Act::Run,
    }
}

impl FaultFs {
    pub(super) fn with(files: &[(&str, &[u8])]) -> Self {
        let map = files
            .iter()
            .map(|(p, b)| (PathBuf::from(p), b.to_vec()))
            .collect();
        Self::from_map(map)
    }

    pub(super) fn from_map(files: BTreeMap<PathBuf, Vec<u8>>) -> Self {
        Self {
            inner: RefCell::new(Inner {
                files,
                ops: 0,
                faults: Vec::new(),
                tampers: Vec::new(),
                read_tampers: Vec::new(),
                read_counts: BTreeMap::new(),
                pending: Vec::new(),
                dead: false,
                log: Vec::new(),
                len_calls: Vec::new(),
            }),
            locks_unsupported: Cell::new(false),
            locks: Rc::new(RefCell::new(BTreeSet::new())),
            shared: Rc::new(RefCell::new(BTreeMap::new())),
        }
    }

    /// Models a file system without advisory locks: both lock calls report
    /// that locking is unsupported, as `StdFs` does for an existing file.
    pub(super) fn without_advisory_locks(self) -> Self {
        self.locks_unsupported.set(true);
        self
    }

    pub(super) fn set_fault(&self, fault: Fault) {
        let mut inner = self.inner.borrow_mut();
        inner.faults = vec![fault];
        inner.tampers.clear();
        inner.read_tampers.clear();
        inner.read_counts.clear();
        inner.pending.clear();
        inner.ops = 0;
        inner.log.clear();
        inner.len_calls.clear();
    }

    /// Replaces or creates the listed files right after the `nth` (1-based)
    /// read of `path`, whatever its operation number. This models a commit or
    /// a rollback finishing between two reads of a reader.
    pub(super) fn tamper_after_read(&self, path: &str, nth: usize, changes: &[(&str, &[u8])]) {
        self.inner.borrow_mut().read_tampers.push(ReadTamper {
            path: PathBuf::from(path),
            nth,
            changes: changes
                .iter()
                .map(|(p, b)| (PathBuf::from(p), b.to_vec()))
                .collect(),
        });
    }

    /// Adds a fault to the ones already set, in the same numbering.
    pub(super) fn add_fault(&self, fault: Fault) {
        self.inner.borrow_mut().faults.push(fault);
    }

    /// Replaces the file's bytes right after operation `after` completes.
    pub(super) fn tamper_after(&self, after: usize, path: &str, bytes: &[u8]) {
        self.inner
            .borrow_mut()
            .tampers
            .push((after, PathBuf::from(path), bytes.to_vec()));
    }

    /// The next process: no fault, no crash, operation numbering restarts.
    pub(super) fn revive(&self) {
        self.set_fault(Fault::None);
        self.inner.borrow_mut().dead = false;
    }

    /// Replaces every file, as a person with write access could.
    pub(super) fn replace_all(&self, files: BTreeMap<PathBuf, Vec<u8>>) {
        self.inner.borrow_mut().files = files;
    }

    pub(super) fn snapshot(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        self.inner.borrow().files.clone()
    }

    pub(super) fn log(&self) -> Vec<OpRecord> {
        self.inner.borrow().log.clone()
    }

    /// The paths whose size was asked, in order. Kept apart from `log` so
    /// that operation numbers and positions stay as the schedules expect.
    pub(super) fn len_calls(&self) -> Vec<PathBuf> {
        self.inner.borrow().len_calls.clone()
    }

    pub(super) fn is_dead(&self) -> bool {
        self.inner.borrow().dead
    }

    /// Takes the lock as another process would.
    pub(super) fn hold_lock(&self, path: &str) -> impl Drop {
        self.locks.borrow_mut().insert(PathBuf::from(path));
        Guard {
            locks: Rc::clone(&self.locks),
            path: PathBuf::from(path),
        }
    }

    fn tick(&self, kind: &'static str, path: &Path, to: Option<&Path>) -> Act {
        let mut inner = self.inner.borrow_mut();
        inner.ops += 1;
        let n = inner.ops;
        let landed = std::mem::take(&mut inner.pending);
        for (path, bytes) in landed {
            inner.files.insert(path, bytes);
        }
        inner.log.push(OpRecord {
            kind,
            path: path.to_path_buf(),
            to: to.map(Path::to_path_buf),
        });
        if kind == "read" {
            let count = {
                let c = inner.read_counts.entry(path.to_path_buf()).or_insert(0);
                *c += 1;
                *c
            };
            let due: Vec<(PathBuf, Vec<u8>)> = inner
                .read_tampers
                .iter()
                .filter(|t| t.path == path && t.nth == count)
                .flat_map(|t| t.changes.clone())
                .collect();
            inner.pending.extend(due);
        }
        let due: Vec<(PathBuf, Vec<u8>)> = inner
            .tampers
            .iter()
            .filter(|t| t.0 + 1 == n)
            .map(|t| (t.1.clone(), t.2.clone()))
            .collect();
        for (path, bytes) in due {
            inner.files.insert(path, bytes);
        }
        if inner.dead {
            return Act::Fail;
        }
        for fault in &inner.faults {
            let act = act_for(*fault, n, kind);
            if !matches!(act, Act::Run) {
                return act;
            }
        }
        Act::Run
    }

    fn die_if(&self, act: &Act) {
        if matches!(act, Act::RunThenDie | Act::TornThenDie) {
            self.inner.borrow_mut().dead = true;
        }
    }
}

impl Fs for FaultFs {
    // A size query is recorded in `len_calls` but not counted, so the fault schedules of the
    // tests that number every read, write and rename stay as they are.
    fn len(&self, path: &Path) -> io::Result<u64> {
        let mut inner = self.inner.borrow_mut();
        inner.len_calls.push(path.to_path_buf());
        match inner.files.get(path) {
            Some(b) => Ok(u64::try_from(b.len()).unwrap_or(u64::MAX)),
            None => Err(io::ErrorKind::NotFound.into()),
        }
    }

    fn read(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        let act = self.tick("read", path, None);
        if matches!(act, Act::Fail | Act::Torn) {
            return Err(injected());
        }
        let result = match self.inner.borrow().files.get(path) {
            Some(b) => Ok(b
                .iter()
                .take(usize::try_from(max_bytes).unwrap_or(usize::MAX))
                .copied()
                .collect()),
            None => Err(io::ErrorKind::NotFound.into()),
        };
        self.die_if(&act);
        result
    }

    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let act = self.tick("create_new", path, None);
        if matches!(act, Act::Fail) {
            return Err(injected());
        }
        let mut inner = self.inner.borrow_mut();
        if inner.files.contains_key(path) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let torn = matches!(act, Act::Torn | Act::TornThenDie);
        let kept = if torn { bytes.len() / 2 } else { bytes.len() };
        inner.files.insert(
            path.to_path_buf(),
            bytes.get(..kept).unwrap_or(&[]).to_vec(),
        );
        drop(inner);
        self.die_if(&act);
        if torn {
            Err(injected())
        } else {
            Ok(())
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let act = self.tick("rename", from, Some(to));
        if matches!(act, Act::Fail | Act::Torn) {
            return Err(injected());
        }
        let result = {
            let mut inner = self.inner.borrow_mut();
            match inner.files.remove(from) {
                Some(bytes) => {
                    inner.files.insert(to.to_path_buf(), bytes);
                    Ok(())
                }
                None => Err(io::ErrorKind::NotFound.into()),
            }
        };
        self.die_if(&act);
        result
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        let act = self.tick("remove", path, None);
        if matches!(act, Act::Fail | Act::Torn) {
            return Err(injected());
        }
        let result = self
            .inner
            .borrow_mut()
            .files
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| io::ErrorKind::NotFound.into());
        self.die_if(&act);
        result
    }

    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        let act = self.tick("hard_link", from, Some(to));
        if matches!(act, Act::Fail | Act::Torn) {
            return Err(injected());
        }
        let result = {
            let mut inner = self.inner.borrow_mut();
            if inner.files.contains_key(to) {
                Err(io::ErrorKind::AlreadyExists.into())
            } else {
                match inner.files.get(from).cloned() {
                    Some(bytes) => {
                        inner.files.insert(to.to_path_buf(), bytes);
                        Ok(())
                    }
                    None => Err(io::ErrorKind::NotFound.into()),
                }
            }
        };
        self.die_if(&act);
        result
    }

    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        let act = self.tick("sync_dir", dir, None);
        if matches!(act, Act::Fail | Act::Torn) {
            return Err(injected());
        }
        self.die_if(&act);
        Ok(())
    }

    fn list_dir(&self, dir: &Path) -> io::Result<Vec<PathBuf>> {
        let act = self.tick("list_dir", dir, None);
        if matches!(act, Act::Fail | Act::Torn) {
            return Err(injected());
        }
        let listed: Vec<PathBuf> = self
            .inner
            .borrow()
            .files
            .keys()
            .filter(|p| p.parent() == Some(dir))
            .cloned()
            .collect();
        self.die_if(&act);
        if listed.is_empty() {
            Err(io::ErrorKind::NotFound.into())
        } else {
            Ok(listed)
        }
    }

    fn try_lock(&self, path: &Path) -> io::Result<Box<dyn LockHeld>> {
        let act = self.tick("try_lock", path, None);
        if matches!(act, Act::Fail | Act::Torn) {
            return Err(injected());
        }
        if self.locks_unsupported.get() {
            self.die_if(&act);
            return Err(io::ErrorKind::Unsupported.into());
        }
        let result =
            if self.locks.borrow().contains(path) || self.shared.borrow().contains_key(path) {
                Err(io::ErrorKind::WouldBlock.into())
            } else {
                self.locks.borrow_mut().insert(path.to_path_buf());
                Ok(Box::new(Guard {
                    locks: Rc::clone(&self.locks),
                    path: path.to_path_buf(),
                }) as Box<dyn LockHeld>)
            };
        self.die_if(&act);
        result
    }

    fn try_lock_shared(&self, path: &Path) -> io::Result<Box<dyn LockHeld>> {
        let act = self.tick("try_lock_shared", path, None);
        if matches!(act, Act::Fail | Act::Torn) {
            return Err(injected());
        }
        let missing = !self.inner.borrow().files.contains_key(path);
        let result = if missing {
            Err(io::ErrorKind::NotFound.into())
        } else if self.locks_unsupported.get() {
            Err(io::ErrorKind::Unsupported.into())
        } else if self.locks.borrow().contains(path) {
            Err(io::ErrorKind::WouldBlock.into())
        } else {
            *self
                .shared
                .borrow_mut()
                .entry(path.to_path_buf())
                .or_insert(0) += 1;
            Ok(Box::new(SharedGuard {
                shared: Rc::clone(&self.shared),
                path: path.to_path_buf(),
            }) as Box<dyn LockHeld>)
        };
        self.die_if(&act);
        result
    }
}

#[test]
fn fault_fs_len_is_the_true_length_and_is_not_a_counted_operation() {
    let fs = FaultFs::with(&[("a", b"12345"), ("empty", b"")]);
    assert_eq!(fs.len(Path::new("a")).expect("len"), 5);
    assert_eq!(fs.len(Path::new("empty")).expect("len"), 0);
    assert_eq!(
        fs.len(Path::new("gone")).expect_err("missing").kind(),
        io::ErrorKind::NotFound
    );
    assert_eq!(fs.len_calls().len(), 3);
    assert!(fs.log().is_empty());
    assert_eq!(fs.inner.borrow().ops, 0);
}
