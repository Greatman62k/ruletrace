//! File access of the `ruletrace` executable (design §3.9, §6.2.4).
//!
//! Every file the CLI touches goes through the [`Fs`] trait, so that tests can
//! fail each step. [`commit_files`] is the only way a command output is
//! written: temp file, sync, rename, with the earlier content restored when a
//! later rename fails (Req 17.13).

use rt_foundation::{Code, Diagnostic, InputId, InputLocation, Location};
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// The file operations the CLI needs.
pub trait Fs {
    /// Reads at most `max_bytes` bytes of the file. A longer file is cut at
    /// `max_bytes`, which lets the caller detect "larger than the limit" by
    /// asking for the limit plus one.
    ///
    /// # Errors
    /// Any I/O error, including a missing file or a directory.
    fn read(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>>;

    /// Whether a directory entry of this name exists. A symbolic link counts
    /// as existing even when its target does not.
    ///
    /// The default reads one byte, which treats a dangling link as absent;
    /// `StdFs` asks the directory entry itself.
    ///
    /// # Errors
    /// Any I/O error other than a missing entry.
    fn exists(&self, path: &Path) -> io::Result<bool> {
        match self.read(path, 1) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Creates the file, which must not exist, writes all bytes and syncs it to
    /// storage.
    ///
    /// # Errors
    /// `AlreadyExists` when the file exists, or any other I/O error. A failed
    /// write may leave a partial file, which the caller removes.
    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;

    /// Renames `from` to `to`, replacing `to` when it exists.
    ///
    /// # Errors
    /// Any I/O error.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Removes a file.
    ///
    /// # Errors
    /// Any I/O error.
    fn remove(&self, path: &Path) -> io::Result<()>;

    /// Creates `to` as a hard link of the file `from`.
    ///
    /// The default has no hard links, so the commit copies instead.
    ///
    /// # Errors
    /// Any I/O error, including a file system without hard links.
    fn hard_link(&self, _from: &Path, _to: &Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }

    /// Makes the entries of a directory durable. A file system without
    /// directory sync treats this as a no-op, which is the default.
    ///
    /// # Errors
    /// Any I/O error.
    fn sync_dir(&self, _dir: &Path) -> io::Result<()> {
        Ok(())
    }

    /// The paths of the entries of a directory, in sorted order.
    ///
    /// The default cannot list, so a Proposal_Store is unreadable.
    ///
    /// # Errors
    /// Any I/O error, including a missing directory.
    fn list_dir(&self, _dir: &Path) -> io::Result<Vec<PathBuf>> {
        Err(io::ErrorKind::Unsupported.into())
    }

    /// Takes an exclusive advisory lock on `path`, creating the file when
    /// needed. The lock is released when the returned value is dropped and is
    /// never waited for. There is no lock-file fallback (Req 26.30).
    ///
    /// The default has no advisory locks and reports `Unsupported`.
    ///
    /// # Errors
    /// `WouldBlock` when another holder has the lock, `Unsupported` when the
    /// file system or platform has no advisory locks (the file this call
    /// created is removed again), or any other I/O error.
    fn try_lock(&self, _path: &Path) -> io::Result<Box<dyn LockHeld>> {
        Err(io::ErrorKind::Unsupported.into())
    }

    /// Takes a shared advisory lock on an existing `path` without creating it,
    /// so that a reader never changes the directory and no writer can hold the
    /// exclusive lock while the reader holds this one. The default has no
    /// advisory locks and reports `Unsupported`.
    ///
    /// # Errors
    /// `NotFound` when the file does not exist, `WouldBlock` when a writer
    /// holds the exclusive lock, `Unsupported` when the file system or
    /// platform has no advisory locks, or any other I/O error.
    fn try_lock_shared(&self, _path: &Path) -> io::Result<Box<dyn LockHeld>> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

/// A held lock. Dropping the value releases it.
pub trait LockHeld {}

struct HeldFile {
    _file: std::fs::File,
}

impl LockHeld for HeldFile {}

/// The real file system.
#[derive(Clone, Copy, Debug, Default)]
pub struct StdFs;

impl Fs for StdFs {
    fn read(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        let file = std::fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.take(max_bytes).read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn exists(&self, path: &Path) -> io::Result<bool> {
        match std::fs::symlink_metadata(path) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::hard_link(from, to)
    }

    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        // Windows cannot open a directory for sync (design §3.8.3 limits).
        #[cfg(unix)]
        {
            std::fs::File::open(dir)?.sync_all()
        }
        #[cfg(not(unix))]
        {
            let _ = dir;
            Ok(())
        }
    }

    fn list_dir(&self, dir: &Path) -> io::Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            paths.push(entry?.path());
        }
        paths.sort();
        Ok(paths)
    }

    fn try_lock(&self, path: &Path) -> io::Result<Box<dyn LockHeld>> {
        let existed = self.exists(path)?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Box::new(HeldFile { _file: file })),
            Err(std::fs::TryLockError::WouldBlock) => Err(io::ErrorKind::WouldBlock.into()),
            Err(std::fs::TryLockError::Error(e)) => {
                drop(file);
                // A refusal leaves no new file behind.
                if !existed {
                    let _ = std::fs::remove_file(path);
                }
                Err(e)
            }
        }
    }

    fn try_lock_shared(&self, path: &Path) -> io::Result<Box<dyn LockHeld>> {
        let file = std::fs::File::open(path)?;
        match file.try_lock_shared() {
            Ok(()) => Ok(Box::new(HeldFile { _file: file })),
            Err(std::fs::TryLockError::WouldBlock) => Err(io::ErrorKind::WouldBlock.into()),
            Err(std::fs::TryLockError::Error(e)) => Err(e),
        }
    }
}

/// The path with `suffix` appended to its final component.
#[must_use]
pub(crate) fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// How many temp names are tried before giving up. A stray temp file of a
/// crashed run is never read and never removed here, so the next name is tried.
const TEMP_ATTEMPTS: u32 = 16;

fn temp_path(path: &Path, attempt: u32) -> PathBuf {
    let mut name: OsString = path.as_os_str().to_owned();
    if attempt == 0 {
        name.push(".tmp");
    } else {
        name.push(format!(".tmp{attempt}"));
    }
    PathBuf::from(name)
}

fn write_failure(path: &Path, what: &str, error: &io::Error) -> Box<Diagnostic> {
    Box::new(Diagnostic::new(
        Code::CliOutputWriteFailed,
        format!(
            "cannot write '{}': {what}: {error}; an earlier file of that name is unchanged",
            rt_foundation::escape_controls(&path.display().to_string())
        ),
        input_location(path),
    ))
}

/// The Input_Location of a file path. Control characters in the path are
/// escaped so that the identifier is valid. An empty path (refused by the
/// argument parser) has no location.
#[must_use]
pub fn input_location(path: &Path) -> Location {
    let text = rt_foundation::escape_controls(&path.display().to_string());
    match InputId::new(text) {
        Ok(input) => Location::Input(InputLocation {
            input,
            line: None,
            column: None,
            field: None,
        }),
        Err(_) => Location::None(Code::CliInputNotFound),
    }
}

fn create_temp(fs: &dyn Fs, path: &Path, bytes: &[u8]) -> Result<PathBuf, Box<Diagnostic>> {
    let mut last = None;
    for attempt in 0..TEMP_ATTEMPTS {
        let temp = temp_path(path, attempt);
        match fs.create_new(&temp, bytes) {
            Ok(()) => return Ok(temp),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => last = Some(e),
            Err(e) => {
                let _ = fs.remove(&temp);
                return Err(write_failure(path, "cannot create the temporary file", &e));
            }
        }
    }
    let error = last.unwrap_or_else(|| io::Error::other("no temporary name available"));
    Err(write_failure(path, "no free temporary file name", &error))
}

/// Writes every `(path, bytes)` pair: all temp files first, then the renames.
/// When any step fails, every temp file is removed and every target that was
/// already replaced is restored, so no target changes (Req 17.13). Restoration
/// is best effort and cannot itself fail the command further.
///
/// # Errors
/// `RT-CLI-0005` (exit 70) for the first failing step.
pub fn commit_files(fs: &dyn Fs, files: &[(&Path, &[u8])]) -> Result<(), Box<Diagnostic>> {
    let mut temps: Vec<PathBuf> = Vec::new();
    let cleanup = |temps: &[PathBuf]| {
        for t in temps {
            let _ = fs.remove(t);
        }
    };
    for (path, bytes) in files {
        match create_temp(fs, path, bytes) {
            Ok(t) => temps.push(t),
            Err(d) => {
                cleanup(&temps);
                return Err(d);
            }
        }
    }
    // Earlier content of each target, so a failed later rename can be undone.
    let mut previous: Vec<Option<Vec<u8>>> = Vec::new();
    for (path, _) in files {
        match fs.read(path, u64::MAX) {
            Ok(old) => previous.push(Some(old)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => previous.push(None),
            Err(e) => {
                cleanup(&temps);
                return Err(write_failure(path, "cannot inspect the existing file", &e));
            }
        }
    }
    for (index, ((path, _), temp)) in files.iter().zip(&temps).enumerate() {
        if let Err(e) = fs.rename(temp, path) {
            cleanup(temps.get(index..).unwrap_or(&[]));
            restore(fs, files, &previous, index);
            return Err(write_failure(path, "cannot rename the temporary file", &e));
        }
    }
    Ok(())
}

/// Puts back the first `count` targets: their earlier bytes, or no file when
/// none existed.
fn restore(fs: &dyn Fs, files: &[(&Path, &[u8])], previous: &[Option<Vec<u8>>], count: usize) {
    for ((path, _), old) in files.iter().zip(previous).take(count) {
        match old {
            None => {
                let _ = fs.remove(path);
            }
            Some(bytes) => {
                if let Ok(temp) = create_temp(fs, path, bytes) {
                    if fs.rename(&temp, path).is_err() {
                        let _ = fs.remove(&temp);
                    }
                }
            }
        }
    }
}

/// A scratch directory on the real file system, for tests of [`StdFs`]. It
/// lives here because `io.rs` is the only module that may touch files.
#[cfg(test)]
#[cfg(unix)]
pub(crate) mod scratch {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    pub(crate) struct Dir(pub(crate) PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Takes the first free `/tmp/rt-cli-<tag>-<n>`. `create_dir` fails when
    /// the directory exists, so two test processes never share one.
    pub(crate) fn dir(tag: &str) -> Dir {
        for n in 0_u32.. {
            let path = PathBuf::from(format!("/tmp/rt-cli-{tag}-{n}"));
            if std::fs::create_dir(&path).is_ok() {
                return Dir(path);
            }
        }
        unreachable!("the counter does not run out")
    }

    pub(crate) fn write_tree(dir: &Dir, files: &BTreeMap<PathBuf, Vec<u8>>) {
        for (path, bytes) in files {
            let target = dir.0.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).expect("a parent can be created");
            }
            std::fs::write(target, bytes).expect("a file can be written");
        }
    }

    pub(crate) fn read(dir: &Dir, name: &str) -> Vec<u8> {
        std::fs::read(dir.0.join(name)).expect("the file can be read")
    }

    pub(crate) fn exists(dir: &Dir, name: &str) -> bool {
        std::fs::symlink_metadata(dir.0.join(name)).is_ok()
    }

    /// Creates `name` as a symbolic link to a path that does not exist.
    pub(crate) fn dangling_link(dir: &Dir, name: &str) -> PathBuf {
        let aim = dir.0.join("nowhere");
        std::os::unix::fs::symlink(&aim, dir.0.join(name)).expect("a link can be created");
        aim
    }

    /// The target of the symbolic link `name`, or `None` when `name` is not
    /// one (or is gone).
    pub(crate) fn link_target(dir: &Dir, name: &str) -> Option<PathBuf> {
        std::fs::read_link(dir.0.join(name)).ok()
    }

    pub(crate) fn path(dir: &Dir, name: impl AsRef<Path>) -> std::ffi::OsString {
        dir.0.join(name).into_os_string()
    }
}
