//! An in-memory file system that fails on request. It implements the same
//! [`StoreFs`] trait as the disk, so a failure travels through the real
//! error paths of the store writer.

use crate::fs::{StoreFile, StoreFs};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// What the next `hard_link` call does.
#[derive(Clone, Debug)]
pub(crate) enum Link {
    /// Creates the second name.
    Normal,
    /// Fails with the kind and changes nothing.
    Fail(ErrorKind),
    /// Another run created the target first with these bytes, so the call
    /// fails with `AlreadyExists`.
    Winner(Vec<u8>),
    /// The process dies at this call: it fails, and every later call fails
    /// without a cleanup until [`MemFs::revive`].
    Crash,
}

/// Failures to inject. Every field is off by default.
#[derive(Default)]
pub(crate) struct Faults {
    pub(crate) exists: Option<ErrorKind>,
    pub(crate) list_dir: Option<ErrorKind>,
    /// Reads of paths ending in this text fail; others succeed.
    pub(crate) read: Option<(String, ErrorKind)>,
    pub(crate) create_dir: Option<ErrorKind>,
    pub(crate) create_temp: Option<ErrorKind>,
    /// Bytes that reach the file before the write fails.
    pub(crate) partial_write: Option<usize>,
    pub(crate) sync_file: Option<ErrorKind>,
    pub(crate) links: VecDeque<Link>,
    pub(crate) sync_dir: Option<ErrorKind>,
    /// Directory syncs that succeed before `sync_dir` takes effect.
    pub(crate) sync_dir_after: usize,
    pub(crate) remove: Option<ErrorKind>,
}

#[derive(Default)]
struct State {
    inodes: Vec<Vec<u8>>,
    names: BTreeMap<PathBuf, usize>,
    dirs: BTreeSet<PathBuf>,
    ops: Vec<String>,
    faults: Faults,
    dead: bool,
}

/// The shared file system. Clones see the same files, as two runs of the
/// tool would on one disk.
#[derive(Clone, Default)]
pub(crate) struct MemFs(Rc<RefCell<State>>);

fn io_error(kind: ErrorKind) -> io::Error {
    io::Error::from(kind)
}

impl MemFs {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_faults(&self, faults: Faults) {
        self.0.borrow_mut().faults = faults;
    }

    pub(crate) fn revive(&self) {
        let mut state = self.0.borrow_mut();
        state.dead = false;
        state.faults = Faults::default();
    }

    /// Every operation attempted so far, in order.
    pub(crate) fn ops(&self) -> Vec<String> {
        self.0.borrow().ops.clone()
    }

    pub(crate) fn clear_ops(&self) {
        self.0.borrow_mut().ops.clear();
    }

    /// Name to content of every file.
    pub(crate) fn files(&self) -> BTreeMap<String, Vec<u8>> {
        let state = self.0.borrow();
        state
            .names
            .iter()
            .filter_map(|(p, i)| {
                state
                    .inodes
                    .get(*i)
                    .map(|b| (p.to_string_lossy().into_owned(), b.clone()))
            })
            .collect()
    }

    pub(crate) fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files().get(path).cloned()
    }

    /// Files whose path starts with `prefix`.
    pub(crate) fn names_under(&self, prefix: &str) -> Vec<String> {
        self.files()
            .into_keys()
            .filter(|n| n.starts_with(prefix))
            .collect()
    }

    pub(crate) fn plant(&self, path: &str, bytes: &[u8]) {
        let mut state = self.0.borrow_mut();
        state.inodes.push(bytes.to_vec());
        let inode = state.inodes.len() - 1;
        state.names.insert(PathBuf::from(path), inode);
    }

    fn begin(&self, op: String) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        state.ops.push(op);
        if state.dead {
            Err(io_error(ErrorKind::Other))
        } else {
            Ok(())
        }
    }
}

struct MemFile {
    fs: MemFs,
    inode: usize,
}

impl StoreFile for MemFile {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.fs.begin(format!("write {}", bytes.len()))?;
        let mut state = self.fs.0.borrow_mut();
        let partial = state.faults.partial_write;
        let take = partial.map_or(bytes.len(), |n| n.min(bytes.len()));
        if let Some(file) = state.inodes.get_mut(self.inode) {
            file.extend_from_slice(bytes.get(..take).unwrap_or_default());
        }
        match partial {
            Some(_) => Err(io_error(ErrorKind::WriteZero)),
            None => Ok(()),
        }
    }

    fn sync_all(&mut self) -> io::Result<()> {
        self.fs.begin("sync_file".to_owned())?;
        match self.fs.0.borrow().faults.sync_file {
            Some(kind) => Err(io_error(kind)),
            None => Ok(()),
        }
    }
}

fn show(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl StoreFs for MemFs {
    fn exists(&self, path: &Path) -> io::Result<bool> {
        self.begin(format!("exists {}", show(path)))?;
        let state = self.0.borrow();
        match state.faults.exists {
            Some(kind) => Err(io_error(kind)),
            None => Ok(state.names.contains_key(path)),
        }
    }

    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        self.begin(format!("list_dir {}", show(path)))?;
        let state = self.0.borrow();
        if let Some(kind) = state.faults.list_dir {
            return Err(io_error(kind));
        }
        let found: Vec<PathBuf> = state
            .names
            .keys()
            .filter(|p| p.parent() == Some(path))
            .cloned()
            .collect();
        if found.is_empty() && !state.dirs.contains(path) {
            return Err(io_error(ErrorKind::NotFound));
        }
        Ok(found)
    }

    fn read(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        self.begin(format!("read {}", show(path)))?;
        let state = self.0.borrow();
        if let Some((suffix, kind)) = &state.faults.read {
            if show(path).ends_with(suffix.as_str()) {
                return Err(io_error(*kind));
            }
        }
        let Some(inode) = state.names.get(path).copied() else {
            return Err(io_error(ErrorKind::NotFound));
        };
        let bytes = state.inodes.get(inode).cloned().unwrap_or_default();
        let take = usize::try_from(limit).map_or(bytes.len(), |n| n.min(bytes.len()));
        Ok(bytes.get(..take).unwrap_or_default().to_vec())
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        self.begin(format!("create_dir_all {}", show(path)))?;
        let mut state = self.0.borrow_mut();
        if let Some(kind) = state.faults.create_dir {
            return Err(io_error(kind));
        }
        state.dirs.insert(path.to_path_buf());
        Ok(())
    }

    fn create_new(&self, path: &Path) -> io::Result<Box<dyn StoreFile>> {
        self.begin(format!("create_new {}", show(path)))?;
        let mut state = self.0.borrow_mut();
        if let Some(kind) = state.faults.create_temp {
            return Err(io_error(kind));
        }
        if state.names.contains_key(path) {
            return Err(io_error(ErrorKind::AlreadyExists));
        }
        state.inodes.push(Vec::new());
        let inode = state.inodes.len() - 1;
        state.names.insert(path.to_path_buf(), inode);
        Ok(Box::new(MemFile {
            fs: self.clone(),
            inode,
        }))
    }

    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.begin(format!("link {} {}", show(from), show(to)))?;
        let mut state = self.0.borrow_mut();
        let step = state.faults.links.pop_front().unwrap_or(Link::Normal);
        match step {
            Link::Fail(kind) => return Err(io_error(kind)),
            Link::Crash => {
                state.dead = true;
                return Err(io_error(ErrorKind::Interrupted));
            }
            Link::Winner(bytes) => {
                state.inodes.push(bytes);
                let inode = state.inodes.len() - 1;
                state.names.insert(to.to_path_buf(), inode);
                return Err(io_error(ErrorKind::AlreadyExists));
            }
            Link::Normal => {}
        }
        if state.names.contains_key(to) {
            return Err(io_error(ErrorKind::AlreadyExists));
        }
        let Some(inode) = state.names.get(from).copied() else {
            return Err(io_error(ErrorKind::NotFound));
        };
        state.names.insert(to.to_path_buf(), inode);
        Ok(())
    }

    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        self.begin(format!("sync_dir {}", show(dir)))?;
        let mut state = self.0.borrow_mut();
        if state.faults.sync_dir_after > 0 {
            state.faults.sync_dir_after -= 1;
            return Ok(());
        }
        match state.faults.sync_dir {
            Some(kind) => Err(io_error(kind)),
            None => Ok(()),
        }
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.begin(format!("remove {}", show(path)))?;
        let mut state = self.0.borrow_mut();
        if let Some(kind) = state.faults.remove {
            return Err(io_error(kind));
        }
        match state.names.remove(path) {
            Some(_) => Ok(()),
            None => Err(io_error(ErrorKind::NotFound)),
        }
    }
}
