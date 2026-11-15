//! The file operations of the Proposal_Store writer (design 3.7.5).
//!
//! The writer sees only [`StoreFs`], so a test double can fail each step
//! on the real error paths of the writer. [`StdFs`] is the production
//! implementation and the only one that touches the disk.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// A file that was created by [`StoreFs::create_new`].
pub trait StoreFile {
    /// Appends all of `bytes`.
    ///
    /// # Errors
    /// Any write failure. Part of the bytes may already be written.
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()>;

    /// Flushes the content and the metadata to stable storage.
    ///
    /// # Errors
    /// Any flush failure.
    fn sync_all(&mut self) -> io::Result<()>;
}

/// The operations the store writer uses. Every method takes `&self` so
/// that two runs can share one implementation in a test.
pub trait StoreFs {
    /// Whether `path` exists.
    ///
    /// # Errors
    /// Any failure other than a missing path.
    fn exists(&self, path: &Path) -> io::Result<bool>;

    /// The entries of the directory `path`, as full paths in ascending
    /// order.
    ///
    /// # Errors
    /// `NotFound` when the directory is missing, or any other failure.
    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;

    /// Reads at most `limit` bytes of the file `path`.
    ///
    /// # Errors
    /// Any read failure.
    fn read(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>>;

    /// Creates `path` and its missing parents.
    ///
    /// # Errors
    /// Any creation failure.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;

    /// Creates a new file and fails with `AlreadyExists` when one exists.
    ///
    /// # Errors
    /// Any creation failure.
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn StoreFile>>;

    /// Creates `to` as a second name of `from`, and fails with
    /// `AlreadyExists` when `to` exists. This is the only way a batch file
    /// appears, so a file at its final name is always complete.
    ///
    /// # Errors
    /// `AlreadyExists`, `Unsupported` or any other link failure.
    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Makes the entries of the directory durable.
    ///
    /// # Errors
    /// Any flush failure.
    fn sync_dir(&self, dir: &Path) -> io::Result<()>;

    /// Removes a file.
    ///
    /// # Errors
    /// Any removal failure.
    fn remove_file(&self, path: &Path) -> io::Result<()>;
}

/// The real file system.
#[derive(Clone, Copy, Debug, Default)]
pub struct StdFs;

struct DiskFile(File);

impl StoreFile for DiskFile {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.0.write_all(bytes)
    }

    fn sync_all(&mut self) -> io::Result<()> {
        self.0.flush()?;
        self.0.sync_all()
    }
}

impl StoreFs for StdFs {
    fn exists(&self, path: &Path) -> io::Result<bool> {
        path.try_exists()
    }

    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(path)? {
            found.push(entry?.path());
        }
        found.sort();
        Ok(found)
    }

    fn read(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        File::open(path)?.take(limit).read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn create_new(&self, path: &Path) -> io::Result<Box<dyn StoreFile>> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        Ok(Box::new(DiskFile(file)))
    }

    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::hard_link(from, to)
    }

    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        File::open(dir)?.sync_all()
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
}
