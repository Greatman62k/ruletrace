//! Regression and guard tests of the journaled commit that need more than one
//! fault in a run or a third party editing files (design §3.8.3; Req 26.19,
//! 26.30, 26.31; policy sections 4, 6, 8 and 10).
//!
//! Each test drives the real `run` entry point over a `FaultFs`. Expected
//! files are either the state before the command or the state of a clean run,
//! compared byte for byte.

use super::commit_tests::{
    adoption_fs, approve, args_for, clean_run, fixture, journal_after_crash, pair, position, probe,
    refused_with_0208, with_journal_text, Clean, Fixture,
};
use super::fault_tests::{Fault, FaultFs};
use super::*;

const RESERVED: [&str; 5] = [
    "rb.json.old",
    "ledger.jsonl.old",
    "rb.json.new",
    "ledger.jsonl.new",
    "ledger.jsonl.journal.tmp",
];

fn with_file(fs: &FaultFs, name: &str, bytes: &[u8]) {
    let mut files = fs.snapshot();
    files.insert(PathBuf::from(name), bytes.to_vec());
    fs.replace_all(files);
}

/// Operation numbers of an in-process rollback: the `sync_dir` after the
/// Rulebook rename fails, so both renames are undone.
struct Rollback {
    sync: usize,
    ledger_restore: usize,
    rulebook_restore: usize,
}

fn rollback_positions(fx: &Fixture, clean: &Clean) -> Rollback {
    let rulebook = position(&clean.log, |o| o.is_rename("rb.json.new", "rb.json"));
    let sync = rulebook + 1;
    assert_eq!(clean.log[sync - 1].kind, "sync_dir");
    let fs = adoption_fs(fx, true);
    fs.set_fault(Fault::Fail(sync));
    let out = run(&approve(fx), &fs);
    assert_eq!(out.code.value(), 70, "{}", out.stderr);
    let log = fs.log();
    Rollback {
        sync,
        ledger_restore: position(&log, |o| o.is_rename("ledger.jsonl.old", "ledger.jsonl")),
        rulebook_restore: position(&log, |o| o.is_rename("rb.json.old", "rb.json")),
    }
}

#[test]
fn a_reserved_name_that_no_journal_accounts_for_is_refused_and_kept() {
    let fx = fixture();
    for name in RESERVED {
        let fs = adoption_fs(&fx, true);
        with_file(&fs, name, b"MY OWN FILE");
        let before = fs.snapshot();
        let out = run(&approve(&fx), &fs);
        assert_eq!(out.code.value(), 70, "{name}: {}", out.stderr);
        assert!(out.stderr.contains("RT-APR-0208"), "{name}: {}", out.stderr);
        assert!(out.stderr.contains(name), "{name}: {}", out.stderr);
        assert!(
            out.stderr.contains("no file changed"),
            "{name}: {}",
            out.stderr
        );
        assert_eq!(fs.snapshot(), before, "{name}: nothing changed");
    }
}

#[test]
fn a_reserved_name_is_refused_when_the_ledger_is_created_too() {
    let fx = fixture();
    let fs = adoption_fs(&fx, false);
    with_file(&fs, "ledger.jsonl.old", b"MY OWN FILE");
    let before = fs.snapshot();
    let out = run(&args_for(&fx, false), &fs);
    assert_eq!(out.code.value(), 70, "{}", out.stderr);
    assert!(out.stderr.contains("ledger.jsonl.old"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn leftovers_of_an_unpublished_attempt_are_replaced_by_the_next_attempt() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    let journal = position(&clean.log, |o| {
        o.is_rename("ledger.jsonl.journal.tmp", "ledger.jsonl.journal")
    });
    let rb_backup = position(&clean.log, |o| {
        o.kind == "hard_link" && o.path == Path::new("rb.json")
    });
    for crash in rb_backup..journal {
        let fs = adoption_fs(&fx, true);
        fs.set_fault(Fault::CrashAfter(crash));
        let _ = run(&approve(&fx), &fs);
        fs.revive();
        assert!(
            fs.snapshot()
                .contains_key(Path::new("ledger.jsonl.journal.tmp")),
            "crash at {crash} leaves the attempt's own marker"
        );
        let out = run(&approve(&fx), &fs);
        assert_eq!(out.code.value(), 0, "crash at {crash}: {}", out.stderr);
        assert_eq!(fs.snapshot(), clean.after, "crash at {crash}");
    }
}

#[test]
fn a_torn_attempt_marker_is_refused_and_kept() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    let marker = position(&clean.log, |o| {
        o.kind == "create_new" && o.path == Path::new("ledger.jsonl.journal.tmp")
    });
    let fs = adoption_fs(&fx, true);
    fs.set_fault(Fault::CrashPartial(marker));
    let _ = run(&approve(&fx), &fs);
    fs.revive();
    let torn = fs.snapshot();
    assert_eq!(pair(&torn), pair(&clean.before));
    let out = run(&approve(&fx), &fs);
    assert_eq!(out.code.value(), 70, "{}", out.stderr);
    assert!(
        out.stderr.contains("ledger.jsonl.journal.tmp"),
        "{}",
        out.stderr
    );
    assert_eq!(fs.snapshot(), torn);
}

#[test]
fn a_crash_between_the_two_restores_of_a_rollback_recovers_the_old_pair() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    let r = rollback_positions(&fx, &clean);
    let first = r.ledger_restore.min(r.rulebook_restore);
    let fs = adoption_fs(&fx, true);
    fs.set_fault(Fault::Fail(r.sync));
    fs.add_fault(Fault::CrashAfter(first));
    let _ = run(&approve(&fx), &fs);
    assert!(fs.is_dead());
    fs.revive();
    let out = run(&probe(), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("warning RT-APR-0211"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), clean.before);
}

#[test]
fn a_rollback_over_a_file_someone_else_changed_is_not_reported_as_done() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    let r = rollback_positions(&fx, &clean);
    let fs = adoption_fs(&fx, true);
    fs.set_fault(Fault::Fail(r.sync));
    fs.tamper_after(r.sync - 1, "ledger.jsonl", b"FOREIGN LEDGER");
    let out = run(&approve(&fx), &fs);
    assert_eq!(out.code.value(), 70, "{}", out.stderr);
    assert!(out.stderr.contains("the rollback failed"), "{}", out.stderr);
    assert!(out.stderr.contains("unexpected state"), "{}", out.stderr);
    let after = fs.snapshot();
    assert_eq!(
        after.get(Path::new("ledger.jsonl")).map(Vec::as_slice),
        Some(b"FOREIGN LEDGER".as_slice())
    );
    assert!(after.contains_key(Path::new("ledger.jsonl.journal")));
    assert!(after.contains_key(Path::new("ledger.jsonl.old")));
}

#[test]
fn recovery_does_not_roll_forward_over_a_rulebook_edited_after_the_crash() {
    let fx = fixture();
    let (fs, _) = journal_after_crash(&fx, |o| o.is_rename("ledger.jsonl.new", "ledger.jsonl"));
    with_file(&fs, "rb.json", b"EDITED BY SOMEONE ELSE");
    refused_with_0208(&fs, "Rulebook edited, ledger new");
    assert_eq!(
        fs.snapshot().get(Path::new("rb.json")).map(Vec::as_slice),
        Some(b"EDITED BY SOMEONE ELSE".as_slice())
    );
}

#[test]
fn recovery_does_not_roll_back_over_a_ledger_edited_after_the_crash() {
    let fx = fixture();
    let (fs, _) = journal_after_crash(&fx, |o| {
        o.is_rename("ledger.jsonl.journal.tmp", "ledger.jsonl.journal")
    });
    with_file(&fs, "ledger.jsonl", b"EDITED BY SOMEONE ELSE");
    refused_with_0208(&fs, "ledger edited, Rulebook old");
}

#[test]
fn recovery_does_not_roll_back_over_a_rulebook_edited_after_the_crash() {
    let fx = fixture();
    let (fs, _) = journal_after_crash(&fx, |o| {
        o.is_rename("ledger.jsonl.journal.tmp", "ledger.jsonl.journal")
    });
    with_file(&fs, "rb.json", b"EDITED BY SOMEONE ELSE");
    refused_with_0208(&fs, "Rulebook edited, ledger old");
}

#[test]
fn a_new_rulebook_with_a_damaged_backup_is_not_rolled_back() {
    let fx = fixture();
    let (fs, clean) = journal_after_crash(&fx, |o| {
        o.is_rename("ledger.jsonl.journal.tmp", "ledger.jsonl.journal")
    });
    with_file(&fs, "rb.json", &clean.after[Path::new("rb.json")]);
    with_file(&fs, "rb.json.old", b"DAMAGED BACKUP");
    refused_with_0208(&fs, "Rulebook new, backup damaged");
}

#[test]
fn a_journal_with_a_foreign_temp_name_is_refused() {
    let fx = fixture();
    let (fs, _) = journal_after_crash(&fx, |o| {
        o.is_rename("ledger.jsonl.journal.tmp", "ledger.jsonl.journal")
    });
    with_journal_text(&fs, |t| {
        t.replace("\"ledger.jsonl.new\"", "\"elsewhere.jsonl.new\"")
    });
    refused_with_0208(&fs, "foreign temp name");
}

#[test]
fn a_retained_journal_after_a_finished_commit_is_not_reported_as_unchanged() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    let drop_journal = position(&clean.log, |o| {
        o.kind == "remove" && o.path == Path::new("ledger.jsonl.journal")
    });
    let fs = adoption_fs(&fx, true);
    fs.set_fault(Fault::Fail(drop_journal));
    let out = run(&approve(&fx), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert!(out.stderr.contains("warning RT-APR-0211"), "{}", out.stderr);
    assert!(
        out.stderr.contains("the commit completed"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("leftover files remain"),
        "{}",
        out.stderr
    );
    assert!(!out.stderr.contains("changed nothing"), "{}", out.stderr);
}

#[test]
fn the_journal_is_published_durably_and_removed_after_every_other_leftover() {
    let fx = fixture();
    let log = clean_run(&fx, true).log;
    let journal = position(&log, |o| {
        o.is_rename("ledger.jsonl.journal.tmp", "ledger.jsonl.journal")
    });
    assert_eq!(log[journal].kind, "sync_dir");
    let drop_journal = position(&log, |o| {
        o.kind == "remove" && o.path == Path::new("ledger.jsonl.journal")
    });
    let last_other = log
        .iter()
        .rposition(|o| {
            o.kind == "remove"
                && (o.path == Path::new("ledger.jsonl.old") || o.path == Path::new("rb.json.old"))
        })
        .map(|i| i + 1)
        .unwrap();
    assert!(last_other < drop_journal);
}
