//! Tests of the fail-closed locking rule: a file system without advisory
//! locks makes every reader and every review command refuse (Req 26.30, 26.32;
//! design §3.8.3; user decision of 2026-11-14; policy sections 4, 6 and 10).
//!
//! The refusal must name the new code `RT-APR-0215` with exit code 70, must
//! not read any file, must not change any file and must never fall back to a
//! `create_new` lock file. A lock held by another process stays `RT-APR-0212`
//! for writers and `RT-APR-0214` for readers.

use super::commit_recovery_tests::readers;
use super::commit_tests::{
    adoption_fs, approve, clean_run, decide_args, fixture, journal_after_crash, position, probe,
    Fixture,
};
use super::fault_tests::{FaultFs, OpRecord};
use super::*;

const LOCK: &str = "ledger.jsonl.lock";

type Files = BTreeMap<PathBuf, Vec<u8>>;

fn is_lock_kind(op: &OpRecord) -> bool {
    op.kind == "try_lock" || op.kind == "try_lock_shared"
}

/// The refusal of one command: exact code, exit code and one diagnostic that
/// names the path and says why.
fn assert_unsupported_refusal(stderr: &str, code: u8, what: &str) {
    assert_eq!(code, 70, "{what}: {stderr}");
    assert!(stderr.starts_with("error RT-APR-0215:"), "{what}: {stderr}");
    assert_eq!(stderr.matches("RT-").count(), 1, "{what}: {stderr}");
    assert!(
        stderr.contains("advisory locks are required"),
        "{what}: {stderr}"
    );
    assert!(
        stderr.contains("ledger.jsonl.lock") || stderr.contains("rb.json"),
        "{what}: {stderr}"
    );
}

/// No file was read, written or listed, and no lock file was created.
fn assert_only_lock_attempts(fs: &FaultFs, before: &BTreeMap<PathBuf, Vec<u8>>, what: &str) {
    assert_eq!(&fs.snapshot(), before, "{what}: files changed");
    for op in fs.log() {
        assert!(is_lock_kind(&op), "{what}: unexpected operation {op:?}");
    }
}

fn reader_states(fx: &Fixture) -> Vec<(&'static str, BTreeMap<PathBuf, Vec<u8>>)> {
    let plain = adoption_fs(fx, true).snapshot();
    let mut with_lock = plain.clone();
    with_lock.insert(PathBuf::from(LOCK), Vec::new());
    vec![("no lock file", plain), ("lock file present", with_lock)]
}

/// Req 26.32: every reader refuses, whether or not the lock file exists.
#[test]
fn every_reader_refuses_when_advisory_locks_are_unsupported() {
    let fx = fixture();
    for (state, files) in reader_states(&fx) {
        for (name, args) in readers() {
            let fs = FaultFs::from_map(files.clone()).without_advisory_locks();
            let out = run(&args, &fs);
            let what = format!("{name}: {state}");
            assert_unsupported_refusal(&out.stderr, out.code.value(), &what);
            assert_only_lock_attempts(&fs, &files, &what);
        }
    }
}

/// A journal does not change the answer: the lock comes first, so nothing is
/// read, not even the journal.
#[test]
fn a_reader_refuses_for_missing_locks_before_it_looks_at_a_journal() {
    let fx = fixture();
    let (fs, _) = journal_after_crash(&fx, |o| o.is_rename("ledger.jsonl.new", "ledger.jsonl"));
    let plain = fs.snapshot();
    assert!(plain.contains_key(Path::new("ledger.jsonl.journal")));
    let mut with_lock = plain.clone();
    with_lock.insert(PathBuf::from(LOCK), Vec::new());
    for (state, files) in [("no lock file", plain), ("lock file present", with_lock)] {
        for (name, args) in readers() {
            let fs = FaultFs::from_map(files.clone()).without_advisory_locks();
            let out = run(&args, &fs);
            let what = format!("{name}: {state}");
            assert_unsupported_refusal(&out.stderr, out.code.value(), &what);
            assert_only_lock_attempts(&fs, &files, &what);
        }
    }
}

/// Approve, reject and the recovery of an interrupted commit all refuse.
#[test]
fn every_review_command_refuses_when_advisory_locks_are_unsupported() {
    let fx = fixture();
    let (crashed, _) =
        journal_after_crash(&fx, |o| o.is_rename("ledger.jsonl.new", "ledger.jsonl"));
    let journal_state = crashed.snapshot();
    let plain = adoption_fs(&fx, true).snapshot();
    let commands: Vec<(&str, Vec<OsString>, &Files)> = vec![
        ("approve", approve(&fx), &plain),
        (
            "reject",
            decide_args("reject", "d.b", &fx.hash, &[]),
            &plain,
        ),
        ("recovery by approve", approve(&fx), &journal_state),
        ("recovery by probe", probe(), &journal_state),
    ];
    for (name, args, files) in commands {
        let fs = FaultFs::from_map(files.clone()).without_advisory_locks();
        let out = run(&args, &fs);
        assert_unsupported_refusal(&out.stderr, out.code.value(), name);
        assert_only_lock_attempts(&fs, files, name);
        let log = fs.log();
        assert_eq!(log.len(), 1, "{name}: {log:?}");
        assert_eq!(log[0].kind, "try_lock", "{name}");
        assert_eq!(log[0].path, Path::new(LOCK), "{name}");
    }
}

/// The retired fallback must not run on any path: with working locks no
/// operation creates a lock file, and no lock file is left behind.
#[test]
fn no_run_ever_creates_a_lock_file_with_create_new() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    for op in &clean.log {
        assert!(
            !(op.kind == "create_new" && op.path.to_string_lossy().ends_with(".lock")),
            "{op:?}"
        );
    }
    assert!(position(&clean.log, |o| o.kind == "try_lock") > 0);
    let unsupported = adoption_fs(&fx, true).without_advisory_locks();
    let _ = run(&approve(&fx), &unsupported);
    assert!(unsupported
        .log()
        .iter()
        .all(|o| o.kind != "create_new" && o.kind != "remove"));
}

/// A lock held by another process keeps its own codes, so the two causes
/// stay distinguishable (RT-APR-0212 for writers, RT-APR-0214 for readers).
#[test]
fn a_held_lock_is_not_reported_as_unsupported() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let before = fs.snapshot();
    let guard = fs.hold_lock(LOCK);
    let out = run(&approve(&fx), &fs);
    assert_eq!(out.code.value(), 70, "{}", out.stderr);
    assert!(
        out.stderr.starts_with("error RT-APR-0212:"),
        "{}",
        out.stderr
    );
    assert!(!out.stderr.contains("RT-APR-0215"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), before);
    drop(guard);
    let mut with_lock = before.clone();
    with_lock.insert(PathBuf::from(LOCK), Vec::new());
    let fs = FaultFs::from_map(with_lock.clone());
    let _guard = fs.hold_lock(LOCK);
    for (name, args) in readers() {
        let out = run(&args, &fs);
        assert_eq!(out.code.value(), 2, "{name}: {}", out.stderr);
        assert!(
            out.stderr.starts_with("error RT-APR-0214:"),
            "{name}: {}",
            out.stderr
        );
        assert!(!out.stderr.contains("RT-APR-0215"), "{name}");
        assert_eq!(fs.snapshot(), with_lock, "{name}");
    }
}
