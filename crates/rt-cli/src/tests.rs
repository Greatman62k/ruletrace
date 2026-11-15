#[path = "tests_adoption_property.rs"]
mod adoption_property_tests;
#[path = "tests_commit_faults.rs"]
mod commit_fault_tests;
#[path = "tests_commit_recovery.rs"]
mod commit_recovery_tests;
#[path = "tests_commit.rs"]
mod commit_tests;
#[path = "tests_concurrency.rs"]
mod concurrency_tests;
#[path = "tests_fault.rs"]
mod fault_tests;
#[path = "tests_lock_required.rs"]
mod lock_required_tests;
#[path = "tests_pipeline.rs"]
mod pipeline_tests;
#[path = "tests_reader_window.rs"]
mod reader_window_tests;
#[path = "tests_review_adopt.rs"]
mod review_adopt_tests;
#[path = "tests_review_list_missing.rs"]
mod review_list_missing_tests;
#[path = "tests_review_oversize.rs"]
mod review_oversize_tests;
#[path = "tests_review.rs"]
mod review_tests;
#[path = "tests_validate.rs"]
mod validate_tests;

use super::*;
use rt_srcmodel::{default_load_limits, SourceModel};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

/// An in-memory file system that can fail chosen operations.
#[derive(Default)]
struct MemFs {
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
    /// Fail the n-th (1-based) `rename` call.
    fail_rename: Cell<Option<u32>>,
    renames: Cell<u32>,
    /// Fail every `create_new`.
    fail_create: Cell<bool>,
    /// Another process holds the exclusive review lock.
    lock_held: Cell<bool>,
    /// Fail every `len`.
    fail_len: Cell<bool>,
}

/// A lock that holds nothing in memory: `MemFs` has one lock state only.
struct NoGuard;

impl crate::io::LockHeld for NoGuard {}

impl MemFs {
    fn with(files: &[(&str, &[u8])]) -> Self {
        let fs = MemFs::default();
        for (p, b) in files {
            fs.files.borrow_mut().insert(PathBuf::from(p), b.to_vec());
        }
        fs
    }
    fn snapshot(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        self.files.borrow().clone()
    }
    fn get(&self, p: &str) -> Option<Vec<u8>> {
        self.files.borrow().get(Path::new(p)).cloned()
    }
}

impl Fs for MemFs {
    fn len(&self, path: &Path) -> io::Result<u64> {
        if self.fail_len.get() {
            return Err(io::Error::other("injected len failure"));
        }
        match self.files.borrow().get(path) {
            Some(b) => Ok(u64::try_from(b.len()).unwrap_or(u64::MAX)),
            None => Err(io::ErrorKind::NotFound.into()),
        }
    }
    fn read(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        match self.files.borrow().get(path) {
            Some(b) => Ok(b
                .iter()
                .take(usize::try_from(max_bytes).unwrap_or(usize::MAX))
                .copied()
                .collect()),
            None => Err(io::ErrorKind::NotFound.into()),
        }
    }
    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        if self.fail_create.get() {
            return Err(io::Error::other("injected create failure"));
        }
        let mut files = self.files.borrow_mut();
        if files.contains_key(path) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        files.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let n = self.renames.get() + 1;
        self.renames.set(n);
        if self.fail_rename.get() == Some(n) {
            return Err(io::Error::other("injected rename failure"));
        }
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
    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut files = self.files.borrow_mut();
        let bytes = files.get(from).cloned().ok_or(io::ErrorKind::NotFound)?;
        if files.contains_key(to) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        files.insert(to.to_path_buf(), bytes);
        Ok(())
    }
    fn sync_dir(&self, _dir: &Path) -> io::Result<()> {
        Ok(())
    }
    fn try_lock(&self, _path: &Path) -> io::Result<Box<dyn crate::io::LockHeld>> {
        if self.lock_held.get() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(Box::new(NoGuard))
    }
    fn try_lock_shared(&self, path: &Path) -> io::Result<Box<dyn crate::io::LockHeld>> {
        if !self.files.borrow().contains_key(path) {
            return Err(io::ErrorKind::NotFound.into());
        }
        if self.lock_held.get() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(Box::new(NoGuard))
    }
    fn list_dir(&self, dir: &Path) -> io::Result<Vec<PathBuf>> {
        Ok(self
            .files
            .borrow()
            .keys()
            .filter(|p| p.parent() == Some(dir))
            .cloned()
            .collect())
    }
}

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

const DOC: &[u8] = b"1. Eligibility\n\n1.1 A participant must be at least 18 years old.\n";

fn ingest_args<'a>(extra: &[&'a str]) -> Vec<&'a str> {
    let mut a = vec!["ingest", "--document", "rules.txt", "--out", "model.json"];
    a.extend_from_slice(extra);
    a
}

#[test]
fn ingest_writes_model_and_candidates() {
    let fs = MemFs::with(&[("rules.txt", DOC)]);
    let out = run(&os(&ingest_args(&["--candidates", "cands.json"])), &fs);
    assert_eq!(out.code, ExitCode::Success);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "");
    let model_bytes = fs.get("model.json").unwrap();
    let id = rt_foundation::InputId::new("model.json").unwrap();
    let model = SourceModel::load(&id, &model_bytes, default_load_limits()).unwrap();
    assert_eq!(model.document().as_str(), "rules.txt");
    assert_eq!(model.content().as_bytes(), DOC);
    let cands = String::from_utf8(fs.get("cands.json").unwrap()).unwrap();
    assert!(cands.contains("\"1.1 A participant must be at least 18 years old.\""));
    // Only the three files exist: no temporary file remains.
    assert_eq!(fs.snapshot().len(), 3);
}

#[test]
fn ingest_is_deterministic() {
    let a = MemFs::with(&[("rules.txt", DOC)]);
    let b = MemFs::with(&[("rules.txt", DOC)]);
    let args = os(&ingest_args(&["--candidates", "c.json"]));
    assert_eq!(run(&args, &a), run(&args, &b));
    assert_eq!(a.snapshot(), b.snapshot());
}

#[test]
fn ingest_without_candidates_runs_no_extraction() {
    let fs = MemFs::with(&[("rules.txt", b"no numbered clause here\n")]);
    let out = run(&os(&ingest_args(&[])), &fs);
    assert_eq!(out.code, ExitCode::Success);
    assert_eq!(out.stderr, "");
    assert_eq!(fs.snapshot().len(), 2);
}

#[test]
fn no_clauses_is_a_warning_and_exit_zero() {
    let fs = MemFs::with(&[("rules.txt", b"no numbered clause here\n")]);
    let out = run(&os(&ingest_args(&["--candidates", "c.json"])), &fs);
    assert_eq!(out.code, ExitCode::Success);
    assert!(
        out.stderr.starts_with("warning RT-ING-0005:"),
        "{}",
        out.stderr
    );
    assert!(fs.get("c.json").is_some());
}

#[test]
fn empty_document_is_accepted() {
    let fs = MemFs::with(&[("rules.txt", b"")]);
    let out = run(&os(&ingest_args(&[])), &fs);
    assert_eq!(out.code, ExitCode::Success);
    assert!(fs.get("model.json").is_some());
}

#[test]
fn missing_input_exits_2_and_writes_nothing() {
    let fs = MemFs::default();
    let out = run(&os(&ingest_args(&["--candidates", "c.json"])), &fs);
    assert_eq!(out.code, ExitCode::InvalidInput);
    assert!(
        out.stderr.starts_with("error RT-CLI-0003:"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("at input 'rules.txt'"));
    assert!(fs.snapshot().is_empty());
}

#[test]
fn failure_leaves_existing_outputs_byte_identical() {
    let fs = MemFs::with(&[
        ("rules.txt", b"\xff\xfe not utf-8"),
        ("model.json", b"old model"),
        ("c.json", b"old candidates"),
    ]);
    let before = fs.snapshot();
    let out = run(&os(&ingest_args(&["--candidates", "c.json"])), &fs);
    assert_eq!(out.code, ExitCode::InvalidInput);
    assert!(
        out.stderr.starts_with("error RT-ING-0001:"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("at document 'rules.txt'"));
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn oversize_document_uses_the_max_bytes_limit() {
    let fs = MemFs::with(&[("rules.txt", DOC)]);
    let out = run(&os(&ingest_args(&["--max-bytes", "10"])), &fs);
    assert_eq!(out.code, ExitCode::InvalidInput);
    assert!(
        out.stderr.starts_with("error RT-LIM-0001:"),
        "{}",
        out.stderr
    );
    assert!(fs.get("model.json").is_none());
    // A document exactly at the limit is accepted (Req 19.9).
    let size = DOC.len().to_string();
    let out = run(&os(&ingest_args(&["--max-bytes", &size])), &fs);
    assert_eq!(out.code, ExitCode::Success, "{}", out.stderr);
}

#[test]
fn unsupported_format_exits_2() {
    let fs = MemFs::with(&[("rules.pdf", b"%PDF-1.7 data")]);
    let out = run(
        &os(&["ingest", "--document", "rules.pdf", "--out", "m.json"]),
        &fs,
    );
    assert_eq!(out.code, ExitCode::InvalidInput);
    assert!(
        out.stderr.starts_with("unsupported RT-ING-0002:"),
        "{}",
        out.stderr
    );
    assert!(fs.get("m.json").is_none());
}

#[test]
fn document_id_defaults_to_the_path_with_forward_slashes() {
    let fs = MemFs::with(&[("dir\\rules.txt", DOC)]);
    let out = run(
        &os(&["ingest", "--document", "dir\\rules.txt", "--out", "m.json"]),
        &fs,
    );
    assert_eq!(out.code, ExitCode::Success, "{}", out.stderr);
    let text = String::from_utf8(fs.get("m.json").unwrap()).unwrap();
    assert!(text.contains("\"dir/rules.txt\""));
    let out = run(
        &os(&[
            "ingest",
            "--document",
            "dir\\rules.txt",
            "--document-id",
            "rb-2024",
            "--format",
            "markdown",
            "--out",
            "m2.json",
        ]),
        &fs,
    );
    assert_eq!(out.code, ExitCode::Success, "{}", out.stderr);
    let text = String::from_utf8(fs.get("m2.json").unwrap()).unwrap();
    assert!(text.contains("\"rb-2024\"") && text.contains("\"markdown\""));
}

fn assert_cli_failure(args: &[&str], code: &str, location: &str) {
    let fs = MemFs::with(&[("rules.txt", DOC)]);
    let before = fs.snapshot();
    let out = run(&os(args), &fs);
    assert_eq!(out.code, ExitCode::InvalidInput, "{args:?}");
    assert!(
        out.stderr.contains(&format!(" {code}: ")) && out.stderr.contains(location),
        "{args:?}: {}",
        out.stderr
    );
    assert_eq!(fs.snapshot(), before, "{args:?}");
}

#[test]
fn argument_errors_name_the_argument_and_touch_nothing() {
    assert_cli_failure(&[], "RT-CLI-0002", "at argument 0");
    assert_cli_failure(&["frobnicate"], "RT-CLI-0001", "at command 'frobnicate'");
    assert_cli_failure(&["review"], "RT-CLI-0001", "at command 'review'");
    assert_cli_failure(
        &["ingest", "--out", "m.json"],
        "RT-CLI-0002",
        "at argument 3 (--document)",
    );
    assert_cli_failure(
        &["ingest", "--document", "rules.txt"],
        "RT-CLI-0002",
        "(--out)",
    );
    assert_cli_failure(&ingest_args(&["--bogus", "1"]), "RT-CLI-0004", "(--bogus)");
    assert_cli_failure(
        &ingest_args(&["stray"]),
        "RT-CLI-0004",
        "at argument 5 (stray)",
    );
    assert_cli_failure(&ingest_args(&["--"]), "RT-CLI-0004", "(--)");
    assert_cli_failure(
        &ingest_args(&["--out", "other.json"]),
        "RT-CLI-0004",
        "more than once",
    );
    assert_cli_failure(
        &ingest_args(&["--format", "pdf"]),
        "RT-CLI-0004",
        "(--format)",
    );
    assert_cli_failure(
        &ingest_args(&["--max-bytes", "0"]),
        "RT-CLI-0004",
        "(--max-bytes)",
    );
    assert_cli_failure(
        &ingest_args(&["--max-bytes", "+5"]),
        "RT-CLI-0004",
        "(--max-bytes)",
    );
    assert_cli_failure(
        &ingest_args(&["--max-bytes"]),
        "RT-CLI-0004",
        "needs a value",
    );
    assert_cli_failure(
        &ingest_args(&["--candidates", "model.json"]),
        "RT-CLI-0004",
        "same as",
    );
    assert_cli_failure(
        &ingest_args(&["--candidates", "rules.txt"]),
        "RT-CLI-0004",
        "input document",
    );
    assert_cli_failure(
        &ingest_args(&["--document-id", ""]),
        "RT-CLI-0004",
        "empty value",
    );
}

#[test]
fn non_utf8_argument_is_refused() {
    // A lone surrogate is the Windows counterpart of the byte 0xff.
    #[cfg(unix)]
    let bad = {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(vec![0x66, 0xff])
    };
    #[cfg(windows)]
    let bad = {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[0x66, 0xd800])
    };
    let fs = MemFs::default();
    let args = vec![OsString::from("ingest"), OsString::from("--document"), bad];
    let out = run(&args, &fs);
    assert_eq!(out.code, ExitCode::InvalidInput);
    assert!(out.stderr.contains("RT-CLI-0004") && out.stderr.contains("argument 2"));
}

#[test]
fn stderr_has_no_control_characters_other_than_line_feed() {
    let fs = MemFs::default();
    let out = run(&os(&["ingest\u{1b}[31m"]), &fs);
    assert_eq!(out.code, ExitCode::InvalidInput);
    assert!(out.stderr.chars().all(|c| !c.is_control() || c == '\n'));
    assert!(out.stderr.contains("\\u{1b}"));
}

#[test]
fn create_failure_exits_70_and_keeps_the_old_output() {
    let fs = MemFs::with(&[("rules.txt", DOC), ("model.json", b"old")]);
    fs.fail_create.set(true);
    let before = fs.snapshot();
    let out = run(&os(&ingest_args(&[])), &fs);
    assert_eq!(out.code, ExitCode::Internal);
    assert!(
        out.stderr.starts_with("error RT-CLI-0005:"),
        "{}",
        out.stderr
    );
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn rename_failure_restores_earlier_outputs() {
    // Existing model, no candidates file: the second rename fails after the
    // first succeeded.
    let fs = MemFs::with(&[("rules.txt", DOC), ("model.json", b"old model")]);
    fs.fail_rename.set(Some(2));
    let before = fs.snapshot();
    let out = run(&os(&ingest_args(&["--candidates", "c.json"])), &fs);
    assert_eq!(out.code, ExitCode::Internal);
    assert!(
        out.stderr.starts_with("error RT-CLI-0005:"),
        "{}",
        out.stderr
    );
    assert_eq!(fs.snapshot(), before);

    // First rename fails: nothing changed, and when no earlier file existed
    // none is created.
    let fs = MemFs::with(&[("rules.txt", DOC)]);
    fs.fail_rename.set(Some(1));
    let before = fs.snapshot();
    let out = run(&os(&ingest_args(&["--candidates", "c.json"])), &fs);
    assert_eq!(out.code, ExitCode::Internal);
    assert_eq!(fs.snapshot(), before);

    // Second rename fails with no earlier files: the first output is removed
    // again.
    let fs = MemFs::with(&[("rules.txt", DOC)]);
    fs.fail_rename.set(Some(2));
    let before = fs.snapshot();
    let out = run(&os(&ingest_args(&["--candidates", "c.json"])), &fs);
    assert_eq!(out.code, ExitCode::Internal);
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn a_stray_temp_file_is_ignored_and_kept() {
    let fs = MemFs::with(&[("rules.txt", DOC), ("model.json.tmp", b"stray")]);
    let out = run(&os(&ingest_args(&[])), &fs);
    assert_eq!(out.code, ExitCode::Success, "{}", out.stderr);
    assert_eq!(fs.get("model.json.tmp").unwrap(), b"stray");
    assert!(fs.get("model.json").is_some());
}

/// Every command line built from these words has exit code 0, 2 or 70, and a
/// nonzero exit code never changes any file (Req 16.15, 17.4, 17.13).
#[test]
fn exit_codes_are_closed_and_failures_change_nothing() {
    let words = [
        "ingest",
        "--document",
        "--out",
        "--candidates",
        "--format",
        "--max-bytes",
        "--document-id",
        "rules.txt",
        "model.json",
        "bad.bin",
        "missing.txt",
        "text",
        "markdown",
        "7",
        "--zzz",
    ];
    let mut checked = 0u32;
    for a in words {
        for b in words {
            for c in words {
                for d in words {
                    let fs = MemFs::with(&[
                        ("rules.txt", DOC),
                        ("bad.bin", b"\xff\xff"),
                        ("model.json", b"old"),
                    ]);
                    let before = fs.snapshot();
                    let out = run(&os(&[a, b, c, d]), &fs);
                    let value = out.code.value();
                    assert!([0, 2, 70].contains(&value), "{a} {b} {c} {d}");
                    if value != 0 {
                        assert_eq!(fs.snapshot(), before, "{a} {b} {c} {d}");
                        assert!(!out.stderr.is_empty());
                    }
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 15 * 15 * 15 * 15);
}
