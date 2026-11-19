//! Shared harness of Property 42: runs `ruletrace evaluate` (the real
//! `rt_cli::run` over an in-memory file system) for the canonical bytes an
//! application's Evaluate group produced, and compares the results file and
//! the exit code with what that group reports.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use rt_cli::{run, Fs};
use rt_srcmodel::SourceModels;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

/// What an Evaluate group returned for one run, as bytes and a class code.
#[derive(Clone)]
pub struct Observed {
    pub data: Vec<u8>,
    pub questions: Vec<u8>,
    pub published: Option<Vec<u8>>,
    /// The classification mapped to the exit code of Req 31.12.
    pub class_code: u8,
    /// The results file the library's values serialise to, through the writer
    /// `ruletrace evaluate` uses.
    pub results: Vec<u8>,
    /// Published differences the library found, by kind: value differs, tie
    /// split, tie not computed. `ruletrace evaluate` reports them as Warnings
    /// on standard error, not in the results file.
    pub differences: [usize; 3],
}

/// The codes of Req 13.12, in the order of [`Observed::differences`], written
/// from the catalogue and not read from the libraries.
const DIFFERENCE_CODES: [&str; 3] = ["RT-EXE-0011", "RT-EXE-0012", "RT-EXE-0013"];

#[derive(Default)]
struct MemFs {
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
}

struct NoLock;

impl rt_cli::io::LockHeld for NoLock {}

impl MemFs {
    fn put(&self, name: &str, bytes: &[u8]) {
        self.files
            .borrow_mut()
            .insert(PathBuf::from(name), bytes.to_vec());
    }

    fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.files.borrow().get(Path::new(name)).cloned()
    }
}

impl Fs for MemFs {
    fn try_lock(&self, _path: &Path) -> io::Result<Box<dyn rt_cli::io::LockHeld>> {
        Ok(Box::new(NoLock))
    }

    fn try_lock_shared(&self, path: &Path) -> io::Result<Box<dyn rt_cli::io::LockHeld>> {
        if self.exists(path)? {
            Ok(Box::new(NoLock))
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn len(&self, path: &Path) -> io::Result<u64> {
        let files = self.files.borrow();
        let bytes = files.get(path).ok_or(io::ErrorKind::NotFound)?;
        Ok(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
    }

    fn read(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        let files = self.files.borrow();
        let bytes = files.get(path).ok_or(io::ErrorKind::NotFound)?;
        Ok(bytes
            .iter()
            .take(usize::try_from(max_bytes).unwrap_or(usize::MAX))
            .copied()
            .collect())
    }

    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut files = self.files.borrow_mut();
        if files.contains_key(path) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        files.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut files = self.files.borrow_mut();
        let bytes = files.remove(from).ok_or(io::ErrorKind::NotFound)?;
        files.insert(to.to_path_buf(), bytes);
        Ok(())
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        self.files
            .borrow_mut()
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }
}

/// An in-memory file system for running several `ruletrace` commands in
/// sequence through `rt_cli::run`, for tests that build a Compiled_Rulebook.
#[derive(Default)]
pub struct Session {
    fs: MemFs,
}

impl Session {
    pub fn put(&self, name: &str, bytes: &[u8]) {
        self.fs.put(name, bytes);
    }

    pub fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.fs.get(name)
    }

    /// Runs one command line. Returns the exit code and standard error.
    pub fn run(&self, args: &[&str]) -> (u8, String) {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let outcome = run(&args, &self.fs);
        (outcome.code.value(), outcome.stderr)
    }
}

/// Runs `ruletrace evaluate` over the given files. Returns the exit code and
/// the results file, if one was written.
pub fn ruletrace_evaluate(
    compiled: &[u8],
    observed: &Observed,
    sources: Option<&SourceModels>,
) -> (u8, Option<Vec<u8>>, String) {
    let fs = MemFs::default();
    fs.put("compiled.json", compiled);
    fs.put("data.json", &observed.data);
    fs.put("questions.json", &observed.questions);
    let mut args: Vec<String> = [
        "evaluate",
        "--compiled",
        "compiled.json",
        "--data",
        "data.json",
        "--questions",
        "questions.json",
        "--out",
        "results.json",
    ]
    .map(str::to_owned)
    .to_vec();
    if let Some(bytes) = &observed.published {
        fs.put("published.json", bytes);
        args.extend(["--published".to_owned(), "published.json".to_owned()]);
    }
    if let Some(models) = sources {
        for (i, m) in models.iter().enumerate() {
            let name = format!("model{i}.json");
            fs.put(&name, m.print().as_bytes());
            args.extend(["--source-model".to_owned(), name]);
        }
    }
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let outcome = run(&args, &fs);
    (outcome.code.value(), fs.get("results.json"), outcome.stderr)
}

/// Property 42 for one run: the same exit code, the same results bytes and the
/// same published differences.
/// Returns the exit code on success.
pub fn identical(
    compiled: &[u8],
    observed: &Observed,
    sources: Option<&SourceModels>,
) -> Result<u8, String> {
    let (code, results, stderr) = ruletrace_evaluate(compiled, observed, sources);
    if code != observed.class_code {
        return Err(format!(
            "exit code: ruletrace {code}, application class {}",
            observed.class_code
        ));
    }
    if results.as_deref() != Some(observed.results.as_slice()) {
        return Err(format!(
            "results bytes differ\nruletrace:\n{}\napplication:\n{}",
            results.as_deref().map_or_else(
                || "(none)".to_owned(),
                |b| String::from_utf8_lossy(b).into_owned()
            ),
            String::from_utf8_lossy(&observed.results)
        ));
    }
    let reported = DIFFERENCE_CODES.map(|c| stderr.matches(c).count());
    if reported != observed.differences {
        return Err(format!(
            "published differences: ruletrace reported {reported:?}, application found {:?}\n{stderr}",
            observed.differences
        ));
    }
    Ok(code)
}
