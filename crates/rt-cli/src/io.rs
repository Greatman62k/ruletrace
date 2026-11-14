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

    /// Takes an exclusive advisory lock on `path`, creating the file when
    /// needed. `Ok(None)` means that this file system has no such lock, and the
    /// caller uses the lock-file fallback of [`acquire_lock`]. The lock is
    /// released when the returned value is dropped.
    ///
    /// # Errors
    /// `WouldBlock` when another holder has the lock, or any other I/O error.
    fn try_lock(&self, _path: &Path) -> io::Result<Option<Box<dyn LockHeld>>> {
        Ok(None)
    }
}

/// A held lock. Dropping the value releases it.
pub trait LockHeld {}

/// An exclusive lock on one file of the file system, released on drop.
pub struct Lock<'a> {
    _native: Option<Box<dyn LockHeld>>,
    fallback: Option<(&'a dyn Fs, PathBuf)>,
}

impl Drop for Lock<'_> {
    fn drop(&mut self) {
        if let Some((fs, path)) = self.fallback.take() {
            let _ = fs.remove(&path);
        }
    }
}

/// Takes the exclusive lock on `path` (Req 26.19, design §3.8.3 step 1):
/// `File::try_lock` where the file system has it, otherwise a lock file created
/// with `create_new` that is removed on release. The lock is never waited for.
///
/// # Errors
/// Any I/O error. `AlreadyExists` (fallback) and `WouldBlock` (native) mean
/// another holder.
pub fn acquire_lock<'a>(fs: &'a dyn Fs, path: &Path) -> io::Result<Lock<'a>> {
    match fs.try_lock(path)? {
        Some(held) => Ok(Lock {
            _native: Some(held),
            fallback: None,
        }),
        None => {
            fs.create_new(path, b"")?;
            Ok(Lock {
                _native: None,
                fallback: Some((fs, path.to_path_buf())),
            })
        }
    }
}

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

    fn try_lock(&self, path: &Path) -> io::Result<Option<Box<dyn LockHeld>>> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Box::new(HeldFile { _file: file }))),
            Err(std::fs::TryLockError::WouldBlock) => Err(io::ErrorKind::WouldBlock.into()),
            Err(std::fs::TryLockError::Error(e)) if e.kind() == io::ErrorKind::Unsupported => {
                // No advisory locks here: the caller falls back to a lock file,
                // which needs a name that does not exist yet.
                drop(file);
                let _ = std::fs::remove_file(path);
                Ok(None)
            }
            Err(std::fs::TryLockError::Error(e)) => Err(e),
        }
    }
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
