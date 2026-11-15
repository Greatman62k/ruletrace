//! Scratch directories and a linking gate for the tests that use the real
//! disk. The directories live under the workspace `target` directory, whose
//! location is fixed at compile time, so no test reads the environment to
//! find a place to write.

use crate::fs::{StdFs, StoreFile, StoreFs};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

const SCRATCH_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/test-scratch");

// An empty directory that is removed again when the guard drops, also
// when an assertion fails.
pub(crate) struct ScratchDir(PathBuf);

impl ScratchDir {
    pub(crate) fn new(name: &str) -> Self {
        let dir = Path::new(SCRATCH_ROOT).join(format!("{}-{name}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).expect("the scratch directory can be created");
        Self(dir)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

// The real file system with one forced interleaving: the first two
// `hard_link` calls wait for each other, so both runs have passed the
// existence check and written a temporary file before either links. The
// second link therefore always meets an existing target.
pub(crate) struct GateFs {
    arrivals: Mutex<usize>,
    both_here: Condvar,
    link_target_existed: AtomicUsize,
}

impl GateFs {
    pub(crate) fn new() -> Self {
        Self {
            arrivals: Mutex::new(0),
            both_here: Condvar::new(),
            link_target_existed: AtomicUsize::new(0),
        }
    }

    // How many `hard_link` calls failed with `AlreadyExists`.
    pub(crate) fn link_target_existed(&self) -> usize {
        self.link_target_existed.load(Ordering::SeqCst)
    }

    // Whether two calls were at the gate together. A bounded wait keeps a
    // run that never reaches the gate from hanging the test.
    fn rendezvous(&self) -> bool {
        let Ok(mut here) = self.arrivals.lock() else {
            return false;
        };
        if *here >= 2 {
            return true;
        }
        *here += 1;
        if *here == 2 {
            self.both_here.notify_all();
            return true;
        }
        match self
            .both_here
            .wait_timeout_while(here, Duration::from_secs(20), |n| *n < 2)
        {
            Ok((n, _)) => *n >= 2,
            Err(_) => false,
        }
    }
}

impl StoreFs for GateFs {
    fn exists(&self, path: &Path) -> io::Result<bool> {
        StdFs.exists(path)
    }

    fn list_dir(&self, path: &Path) -> io::Result<Vec<std::path::PathBuf>> {
        StdFs.list_dir(path)
    }

    fn read(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        StdFs.read(path, limit)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        StdFs.create_dir_all(path)
    }

    fn create_new(&self, path: &Path) -> io::Result<Box<dyn StoreFile>> {
        StdFs.create_new(path)
    }

    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        if !self.rendezvous() {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        let result = StdFs.hard_link(from, to);
        if matches!(&result, Err(e) if e.kind() == io::ErrorKind::AlreadyExists) {
            self.link_target_existed.fetch_add(1, Ordering::SeqCst);
        }
        result
    }

    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        StdFs.sync_dir(dir)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        StdFs.remove_file(path)
    }
}
