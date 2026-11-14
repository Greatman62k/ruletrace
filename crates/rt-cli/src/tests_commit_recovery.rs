//! Fault-injection tests of rollback and recovery, and of the readers that must
//! refuse while a commit journal exists (design §3.8.3; Req 26.19, 26.30,
//! 26.32 to 26.36; policy sections 4, 6, 8 and 10).
//!
//! Expected states are derived from the byte contents of the files, never from
//! the production recovery code: the old pair is the state before the clean
//! run, the new pair is the state after it. A rulebook that holds the adopted
//! revision while the ledger lacks its record is never an acceptable state in
//! any sweep (Req 26.30).

use super::commit_tests::{
    adoption_fs, approve, args_for, clean_run, fixture, journal_after_crash, pair, position, probe,
    refused_with_0208, Clean, Fixture,
};
use super::fault_tests::{Fault, FaultFs};
use super::*;

const JOURNAL: &str = "ledger.jsonl.journal";
const MARKER: &str = "ledger.jsonl.journal.tmp";

pub(super) fn readers() -> Vec<(&'static str, Vec<OsString>)> {
    vec![
        (
            "validate",
            os(&[
                "validate",
                "--rulebook",
                "rb.json",
                "--ledger",
                "ledger.jsonl",
                "--source-model",
                "model.json",
                "--out",
                "validation.json",
            ]),
        ),
        (
            "review list",
            os(&[
                "review",
                "list",
                "--rulebook",
                "rb.json",
                "--ledger",
                "ledger.jsonl",
                "--source-model",
                "model.json",
            ]),
        ),
        (
            "evaluate with a proof report",
            os(&[
                "evaluate",
                "--compiled",
                "compiled.json",
                "--data",
                "data.json",
                "--source-model",
                "model.json",
                "--out",
                "results.json",
                "--rulebook",
                "rb.json",
                "--ledger",
                "ledger.jsonl",
                "--proof-report",
                "proof.txt",
            ]),
        ),
    ]
}

/// Every reader refuses with one Error RT-APR-0214 (exit 2), writes nothing and
/// touches no file but the journal.
fn assert_readers_refuse(fs: &FaultFs, label: &str) {
    let frozen = fs.snapshot();
    for (name, args) in readers() {
        fs.revive();
        let out = run(&args, fs);
        let what = format!("{label}: {name}: {}", out.stderr);
        assert_eq!(out.code.value(), 2, "{what}");
        assert!(out.stderr.starts_with("error RT-APR-0214:"), "{what}");
        assert_eq!(out.stderr.matches("RT-").count(), 1, "{what}");
        assert_eq!(fs.snapshot(), frozen, "{what}");
        for op in fs.log() {
            let lock_probe = op.kind == "try_lock_shared"
                && (op.path == Path::new("ledger.jsonl.lock") || op.path == Path::new("rb.json"));
            let journal_read = op.kind == "read" && op.path == Path::new(JOURNAL);
            assert!(lock_probe || journal_read, "{what}: {op:?}");
        }
    }
}

/// `validate` still runs (the fixture's first rule stays Proposed, exit 3) and
/// `review list` succeeds: neither refuses.
fn assert_readers_run(fs: &FaultFs, label: &str) {
    let all = readers();
    let validate = run(&all[0].1, fs);
    assert_eq!(validate.code.value(), 3, "{label}: {}", validate.stderr);
    assert!(validate.stderr.contains("RT-APR-0001"), "{label}");
    assert!(!validate.stderr.contains("RT-APR-0214"), "{label}");
    let list = run(&all[1].1, fs);
    assert_eq!(list.code.value(), 0, "{label}: {}", list.stderr);
    assert!(!list.stderr.contains("RT-APR-0214"), "{label}");
}

fn has_journal(snapshot: &BTreeMap<PathBuf, Vec<u8>>) -> bool {
    snapshot.contains_key(Path::new(JOURNAL))
}

fn file<'a>(snapshot: &'a BTreeMap<PathBuf, Vec<u8>>, name: &str) -> Option<&'a Vec<u8>> {
    snapshot.get(Path::new(name))
}

/// Whether recovery has to end in the new pair, derived from file bytes alone:
/// the Rulebook is new, or the ledger is new and the new Rulebook file is
/// still there to be renamed. A Rulebook holding the adopted revision without
/// its record is never acceptable (Req 26.30).
fn is_after(state: &BTreeMap<PathBuf, Vec<u8>>, clean: &Clean) -> bool {
    let ledger_new = file(state, "ledger.jsonl") == file(&clean.after, "ledger.jsonl");
    let rulebook_new = file(state, "rb.json") == file(&clean.after, "rb.json");
    assert!(
        !(rulebook_new && !ledger_new),
        "the adopted revision is observable without its record"
    );
    rulebook_new || (ledger_new && state.contains_key(Path::new("rb.json.new")))
}

fn target<'a>(
    state: &BTreeMap<PathBuf, Vec<u8>>,
    clean: &'a Clean,
) -> &'a BTreeMap<PathBuf, Vec<u8>> {
    if is_after(state, clean) {
        &clean.after
    } else {
        &clean.before
    }
}

struct Setup {
    fx: Fixture,
    with_ledger: bool,
    clean: Clean,
}

fn setup(with_ledger: bool) -> Setup {
    let fx = fixture();
    let clean = clean_run(&fx, with_ledger);
    Setup {
        fx,
        with_ledger,
        clean,
    }
}

impl Setup {
    fn fs(&self) -> FaultFs {
        adoption_fs(&self.fx, self.with_ledger)
    }

    fn args(&self) -> Vec<OsString> {
        args_for(&self.fx, self.with_ledger)
    }

    fn probe(&self) -> Vec<OsString> {
        let mut args = probe();
        if !self.with_ledger {
            args.push("--create-ledger".into());
        }
        args
    }

    /// Operation numbers where an in-process failure starts a rollback: from
    /// the publication of the journal to the sync after the Rulebook rename.
    fn forward_failures(&self) -> Vec<usize> {
        let publish = position(&self.clean.log, |o| o.is_rename(MARKER, JOURNAL));
        let rulebook = position(&self.clean.log, |o| o.is_rename("rb.json.new", "rb.json"));
        (publish..=rulebook + 1).collect()
    }

    /// The operation numbers of the rollback that follows a failure at `f`.
    fn rollback_ops(&self, f: usize) -> std::ops::RangeInclusive<usize> {
        let fs = self.fs();
        fs.set_fault(Fault::Fail(f));
        let out = run(&self.args(), &fs);
        assert_eq!(out.code.value(), 70, "{}", out.stderr);
        f + 1..=fs.log().len()
    }

    /// Every journal-bearing state a single crash or a rollback interrupted by
    /// a second fault can leave, without duplicates.
    fn states(&self) -> Vec<(String, BTreeMap<PathBuf, Vec<u8>>)> {
        let mut found: Vec<(String, BTreeMap<PathBuf, Vec<u8>>)> = Vec::new();
        let mut add = |label: String, fs: &FaultFs| {
            let snapshot = fs.snapshot();
            if has_journal(&snapshot) && !found.iter().any(|(_, s)| *s == snapshot) {
                found.push((label, snapshot));
            }
        };
        let publish = position(&self.clean.log, |o| o.is_rename(MARKER, JOURNAL));
        for n in publish..=self.clean.log.len() {
            let fs = self.fs();
            fs.set_fault(Fault::CrashAfter(n));
            let _ = run(&self.args(), &fs);
            add(format!("crash after {n}"), &fs);
        }
        for f in self.forward_failures() {
            for k in self.rollback_ops(f) {
                for second in [Fault::Fail(k), Fault::CrashAfter(k)] {
                    let fs = self.fs();
                    fs.set_fault(Fault::Fail(f));
                    fs.add_fault(second);
                    let _ = run(&self.args(), &fs);
                    add(format!("fail {f} then {second:?}"), &fs);
                }
            }
        }
        found
    }
}

#[test]
fn every_reader_refuses_in_every_journal_state_and_never_reads_the_files() {
    for with_ledger in [true, false] {
        let s = setup(with_ledger);
        let states = s.states();
        assert!(states.len() >= 8, "the sweep reaches distinct states");
        for (label, state) in states {
            let fs = FaultFs::from_map(state);
            assert_readers_refuse(&fs, &format!("ledger {with_ledger}: {label}"));
        }
    }
}

#[test]
fn readers_work_when_no_journal_exists() {
    let s = setup(true);
    assert_readers_run(&s.fs(), "before any commit");
}

#[test]
fn rollback_after_a_failed_commit_restores_the_rulebook_before_the_ledger() {
    let s = setup(true);
    let sync = position(&s.clean.log, |o| o.is_rename("rb.json.new", "rb.json")) + 1;
    let fs = s.fs();
    fs.set_fault(Fault::Fail(sync));
    let out = run(&s.args(), &fs);
    assert_eq!(out.code.value(), 70, "{}", out.stderr);
    let log = fs.log();
    let rulebook = position(&log, |o| o.is_rename("rb.json.old", "rb.json"));
    let ledger = position(&log, |o| o.is_rename("ledger.jsonl.old", "ledger.jsonl"));
    assert!(rulebook < ledger, "forward order is reversed on rollback");
}

#[test]
fn case_a_a_successful_rollback_restores_both_files_byte_for_byte() {
    let s = setup(true);
    let sync = position(&s.clean.log, |o| o.is_rename("rb.json.new", "rb.json")) + 1;
    let fs = s.fs();
    fs.set_fault(Fault::Fail(sync));
    let out = run(&s.args(), &fs);
    assert_eq!(out.code.value(), 70, "{}", out.stderr);
    assert_eq!(out.stderr.matches("RT-").count(), 1, "{}", out.stderr);
    assert!(
        out.stderr.starts_with("error RT-APR-0208:"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr
            .contains("the ledger and the Rulebook are unchanged"),
        "{}",
        out.stderr
    );
    assert_eq!(fs.snapshot(), s.clean.before);
    assert_readers_run(&fs, "after a rolled back commit");
}

#[test]
fn case_b_a_failed_rulebook_restore_keeps_the_complete_new_pair_and_says_so() {
    let s = setup(true);
    let sync = position(&s.clean.log, |o| o.is_rename("rb.json.new", "rb.json")) + 1;
    let restore = {
        let fs = s.fs();
        fs.set_fault(Fault::Fail(sync));
        let _ = run(&s.args(), &fs);
        position(&fs.log(), |o| o.is_rename("rb.json.old", "rb.json"))
    };
    let fs = s.fs();
    fs.set_fault(Fault::Fail(sync));
    fs.add_fault(Fault::Fail(restore));
    let out = run(&s.args(), &fs);
    assert_eq!(out.code.value(), 70, "{}", out.stderr);
    assert_eq!(out.stderr.matches("RT-").count(), 1, "{}", out.stderr);
    assert!(
        out.stderr.starts_with("error RT-APR-0208:"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("the rollback failed"), "{}", out.stderr);
    assert!(
        out.stderr
            .contains("the decision is committed: both files carry the new state"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains(JOURNAL), "{}", out.stderr);
    assert!(!out.stderr.contains("unchanged"), "{}", out.stderr);
    assert!(!out.stderr.contains("intermediate"), "{}", out.stderr);
    let stuck = fs.snapshot();
    assert_eq!(pair(&stuck), pair(&s.clean.after));
    assert!(has_journal(&stuck));
    assert_readers_refuse(&fs, "case b");
    fs.revive();
    let next = run(&probe(), &fs);
    assert_eq!(next.code.value(), 2, "{}", next.stderr);
    assert!(
        next.stderr.contains("warning RT-APR-0211"),
        "{}",
        next.stderr
    );
    assert_eq!(fs.snapshot(), s.clean.after);
}

#[test]
fn case_c_a_failed_ledger_restore_is_an_intermediate_state_no_reader_may_use() {
    let s = setup(true);
    let sync = position(&s.clean.log, |o| o.is_rename("rb.json.new", "rb.json")) + 1;
    let restore = {
        let fs = s.fs();
        fs.set_fault(Fault::Fail(sync));
        let _ = run(&s.args(), &fs);
        position(&fs.log(), |o| {
            o.is_rename("ledger.jsonl.old", "ledger.jsonl")
        })
    };
    let fs = s.fs();
    fs.set_fault(Fault::Fail(sync));
    fs.add_fault(Fault::Fail(restore));
    let out = run(&s.args(), &fs);
    assert_eq!(out.code.value(), 70, "{}", out.stderr);
    assert_eq!(out.stderr.matches("RT-").count(), 1, "{}", out.stderr);
    assert!(
        out.stderr.starts_with("error RT-APR-0208:"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("the rollback failed"), "{}", out.stderr);
    assert!(
        out.stderr
            .contains("the files are in an intermediate state and must not be used"),
        "{}",
        out.stderr
    );
    assert!(!out.stderr.contains("unchanged"), "{}", out.stderr);
    assert!(!out.stderr.contains("committed"), "{}", out.stderr);
    assert!(
        out.stderr.contains(
            "until the next 'review approve' or 'review reject' either completes the commit or undoes it"
        ),
        "{}",
        out.stderr
    );
    let stuck = fs.snapshot();
    // New ledger, old Rulebook: the only intermediate pair the protocol makes.
    assert_eq!(
        file(&stuck, "ledger.jsonl"),
        file(&s.clean.after, "ledger.jsonl")
    );
    assert_eq!(file(&stuck, "rb.json"), file(&s.clean.before, "rb.json"));
    assert!(has_journal(&stuck));
    assert_readers_refuse(&fs, "case c");
    fs.revive();
    let next = run(&probe(), &fs);
    assert_eq!(next.code.value(), 2, "{}", next.stderr);
    assert!(
        next.stderr.contains("warning RT-APR-0211"),
        "{}",
        next.stderr
    );
    assert_eq!(fs.snapshot(), s.clean.before);
}

#[test]
fn case_c_a_failed_ledger_restore_after_a_failed_rulebook_rename_is_completed_by_the_next_command()
{
    let s = setup(true);
    let rename = position(&s.clean.log, |o| o.is_rename("rb.json.new", "rb.json"));
    let restore = {
        let fs = s.fs();
        fs.set_fault(Fault::Fail(rename));
        let _ = run(&s.args(), &fs);
        position(&fs.log(), |o| {
            o.is_rename("ledger.jsonl.old", "ledger.jsonl")
        })
    };
    let fs = s.fs();
    fs.set_fault(Fault::Fail(rename));
    fs.add_fault(Fault::Fail(restore));
    let out = run(&s.args(), &fs);
    assert_eq!(out.code.value(), 70, "{}", out.stderr);
    assert!(
        out.stderr
            .contains("the files are in an intermediate state and must not be used"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr
            .contains("either completes the commit or undoes it"),
        "{}",
        out.stderr
    );
    let stuck = fs.snapshot();
    assert_eq!(
        file(&stuck, "ledger.jsonl"),
        file(&s.clean.after, "ledger.jsonl")
    );
    assert_eq!(file(&stuck, "rb.json"), file(&s.clean.before, "rb.json"));
    assert!(has_journal(&stuck));
    fs.revive();
    let next = run(&probe(), &fs);
    assert!(
        next.stderr.contains("the commit was completed"),
        "{}",
        next.stderr
    );
    assert_eq!(fs.snapshot(), s.clean.after);
}

#[test]
fn case_c_a_crash_inside_the_rollback_is_recovered_before_any_load() {
    let s = setup(true);
    let sync = position(&s.clean.log, |o| o.is_rename("rb.json.new", "rb.json")) + 1;
    let restore = {
        let fs = s.fs();
        fs.set_fault(Fault::Fail(sync));
        let _ = run(&s.args(), &fs);
        position(&fs.log(), |o| o.is_rename("rb.json.old", "rb.json"))
    };
    let fs = s.fs();
    fs.set_fault(Fault::Fail(sync));
    fs.add_fault(Fault::CrashAfter(restore));
    let _ = run(&s.args(), &fs);
    assert!(fs.is_dead());
    fs.revive();
    assert_readers_refuse(&fs, "crash after the Rulebook restore");
    let next = run(&probe(), &fs);
    assert_eq!(next.code.value(), 2, "{}", next.stderr);
    assert_eq!(fs.snapshot(), s.clean.before);
}

#[test]
fn a_recovery_runs_before_the_review_command_loads_anything() {
    let s = setup(true);
    let (fs, _) = journal_after_crash(&s.fx, |o| o.is_rename("ledger.jsonl.new", "ledger.jsonl"));
    let out = run(&probe(), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    let log = fs.log();
    let recovered = position(&log, |o| o.kind == "remove" && o.path == Path::new(JOURNAL));
    let first_load = position(&log, |o| {
        o.kind == "read" && o.path == Path::new("model.json")
    });
    assert!(recovered < first_load, "recovery precedes the first load");
}

#[test]
fn a_completed_commit_with_a_failed_cleanup_exits_0_and_blocks_readers_until_recovered() {
    let s = setup(true);
    let drop_journal = position(&s.clean.log, |o| {
        o.kind == "remove" && o.path == Path::new(JOURNAL)
    });
    let fs = s.fs();
    fs.set_fault(Fault::Fail(drop_journal));
    let out = run(&s.args(), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert!(
        out.stderr.starts_with("warning RT-APR-0211:"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("the commit completed"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("every other command refuses"),
        "{}",
        out.stderr
    );
    assert_eq!(pair(&fs.snapshot()), pair(&s.clean.after));
    assert_readers_refuse(&fs, "cleanup failed");
    fs.revive();
    let next = run(&probe(), &fs);
    assert_eq!(next.code.value(), 2, "{}", next.stderr);
    assert_eq!(fs.snapshot(), s.clean.after);
    assert_readers_run(&fs, "after recovery");
}

/// The message class an in-process failure of the rollback must report, from
/// the bytes the files have afterwards.
fn expected_phrase(state: &BTreeMap<PathBuf, Vec<u8>>, clean: &Clean) -> &'static str {
    let both_old = pair(state) == pair(&clean.before);
    let both_new = pair(state) == pair(&clean.after);
    match (both_old, both_new, has_journal(state)) {
        (true, _, false) => "the ledger and the Rulebook are unchanged",
        (true, _, true) => "restored the ledger and the Rulebook to their earlier bytes",
        (_, true, _) => "the decision is committed",
        _ => "the files are in an intermediate state and must not be used",
    }
}

#[test]
fn every_single_and_double_fault_of_the_rollback_ends_in_a_recoverable_state() {
    for with_ledger in [true, false] {
        let s = setup(with_ledger);
        let mut runs = 0;
        for f in s.forward_failures() {
            for k in s.rollback_ops(f) {
                for second in [Fault::Fail(k), Fault::CrashAfter(k)] {
                    let label = format!("ledger {with_ledger}: fail {f} then {second:?}");
                    let fs = s.fs();
                    fs.set_fault(Fault::Fail(f));
                    fs.add_fault(second);
                    let out = run(&s.args(), &fs);
                    let state = fs.snapshot();
                    let _ = is_after(&state, &s.clean);
                    if matches!(second, Fault::Fail(_)) {
                        assert!(!fs.is_dead(), "{label}");
                        assert_eq!(out.code.value(), 70, "{label}: {}", out.stderr);
                        assert!(
                            out.stderr.starts_with("error RT-APR-0208:"),
                            "{label}: {}",
                            out.stderr
                        );
                        assert_eq!(out.stderr.matches("RT-").count(), 1, "{label}");
                        let phrase = expected_phrase(&state, &s.clean);
                        assert!(
                            out.stderr.contains(phrase),
                            "{label}: {phrase}: {}",
                            out.stderr
                        );
                        if phrase.contains("intermediate") {
                            assert!(
                                out.stderr
                                    .contains("either completes the commit or undoes it"),
                                "{label}: {}",
                                out.stderr
                            );
                        }
                    } else {
                        assert!(fs.is_dead(), "{label}");
                    }
                    fs.revive();
                    if has_journal(&state) {
                        assert_readers_refuse(&fs, &label);
                        let next = run(&s.probe(), &fs);
                        assert_eq!(next.code.value(), 2, "{label}: {}", next.stderr);
                        assert!(
                            next.stderr.contains("warning RT-APR-0211"),
                            "{label}: {}",
                            next.stderr
                        );
                        assert_eq!(&fs.snapshot(), target(&state, &s.clean), "{label}");
                    } else {
                        // No journal: an unpublished attempt, old pair.
                        assert_eq!(pair(&state), pair(&s.clean.before), "{label}");
                        let again = run(&s.args(), &fs);
                        assert_eq!(again.code.value(), 0, "{label}: {}", again.stderr);
                        assert_eq!(fs.snapshot(), s.clean.after, "{label}");
                    }
                    runs += 1;
                }
            }
        }
        assert!(runs >= 20, "the sweep is not empty: {runs}");
    }
}

#[test]
fn every_single_and_double_fault_of_the_recovery_converges() {
    for with_ledger in [true, false] {
        let s = setup(with_ledger);
        let mut runs = 0;
        for (label, state) in s.states() {
            let want = target(&state, &s.clean);
            let reference = FaultFs::from_map(state.clone());
            let out = run(&s.probe(), &reference);
            assert_eq!(out.code.value(), 2, "{label}: {}", out.stderr);
            assert_eq!(&reference.snapshot(), want, "{label}");
            let log = reference.log();
            let dropped = position(&log, |o| o.kind == "remove" && o.path == Path::new(JOURNAL));
            for k in 1..=dropped + 1 {
                for second in [Fault::Fail(k), Fault::CrashAfter(k)] {
                    let what = format!(
                        "ledger {with_ledger}: {label}: {second:?} on {:?}",
                        log[k - 1]
                    );
                    let fs = FaultFs::from_map(state.clone());
                    fs.set_fault(second);
                    let out = run(&s.probe(), &fs);
                    if matches!(second, Fault::Fail(_)) {
                        assert_eq!(out.code.value(), 70, "{what}: {}", out.stderr);
                        let code = if log[k - 1].kind == "try_lock" {
                            "RT-APR-0212"
                        } else {
                            "RT-APR-0208"
                        };
                        assert!(
                            out.stderr.starts_with(&format!("error {code}:")),
                            "{what}: {}",
                            out.stderr
                        );
                        assert_eq!(out.stderr.matches("RT-").count(), 1, "{what}");
                    }
                    let mid = fs.snapshot();
                    // Whatever the fault left is never the forbidden pair.
                    let _ = is_after(&mid, &s.clean);
                    if has_journal(&mid) {
                        assert_readers_refuse(&fs, &what);
                    }
                    fs.revive();
                    let done = run(&s.probe(), &fs);
                    assert_eq!(done.code.value(), 2, "{what}: {}", done.stderr);
                    assert_eq!(&fs.snapshot(), want, "{what}");
                    runs += 1;
                }
            }
        }
        assert!(runs >= 100, "the sweep is not empty: {runs}");
    }
}

#[test]
fn an_old_ledger_with_the_new_rulebook_is_refused_because_the_protocol_never_makes_it() {
    let s = setup(true);
    let (fs, clean) = journal_after_crash(&s.fx, |o| o.is_rename(MARKER, JOURNAL));
    let mut files = fs.snapshot();
    files.insert(
        PathBuf::from("rb.json"),
        clean.after[Path::new("rb.json")].clone(),
    );
    fs.replace_all(files);
    refused_with_0208(&fs, "old ledger with the new Rulebook");
    assert_readers_refuse(&fs, "old ledger with the new Rulebook");
}

#[test]
fn an_edited_ledger_with_the_new_rulebook_is_refused_and_nothing_changes() {
    let s = setup(true);
    let (fs, clean) = journal_after_crash(&s.fx, |o| o.is_rename(MARKER, JOURNAL));
    let mut files = fs.snapshot();
    files.insert(
        PathBuf::from("rb.json"),
        clean.after[Path::new("rb.json")].clone(),
    );
    files.insert(
        PathBuf::from("ledger.jsonl"),
        b"EDITED BY SOMEONE ELSE".to_vec(),
    );
    fs.replace_all(files);
    refused_with_0208(&fs, "edited ledger with the new Rulebook");
}

fn interrupted_rollback(s: &Setup) -> FaultFs {
    let sync = position(&s.clean.log, |o| o.is_rename("rb.json.new", "rb.json")) + 1;
    let restore = {
        let fs = s.fs();
        fs.set_fault(Fault::Fail(sync));
        let _ = run(&s.args(), &fs);
        position(&fs.log(), |o| o.is_rename("rb.json.old", "rb.json"))
    };
    let fs = s.fs();
    fs.set_fault(Fault::Fail(sync));
    fs.add_fault(Fault::CrashAfter(restore));
    let _ = run(&s.args(), &fs);
    fs.revive();
    fs
}

#[test]
fn a_new_ledger_with_a_damaged_backup_is_not_restored() {
    let s = setup(true);
    let fs = interrupted_rollback(&s);
    let mut files = fs.snapshot();
    files.insert(
        PathBuf::from("ledger.jsonl.old"),
        b"DAMAGED BACKUP".to_vec(),
    );
    fs.replace_all(files);
    refused_with_0208(&fs, "ledger backup damaged");
}

#[test]
fn a_new_ledger_without_its_backup_is_not_restored() {
    let s = setup(true);
    let fs = interrupted_rollback(&s);
    let mut files = fs.snapshot();
    files.remove(Path::new("ledger.jsonl.old"));
    fs.replace_all(files);
    refused_with_0208(&fs, "ledger backup missing");
}

#[test]
fn an_adopted_ledger_that_was_created_is_removed_when_the_rollback_was_interrupted() {
    let s = setup(false);
    let fs = interrupted_rollback(&s);
    assert!(fs.snapshot().contains_key(Path::new("ledger.jsonl")));
    let out = run(&s.probe(), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert_eq!(fs.snapshot(), s.clean.before);
}

#[cfg(unix)]
mod dangling {
    use super::*;
    use crate::io::scratch;

    #[test]
    fn a_dangling_symbolic_link_at_a_reserved_name_is_refused_and_kept() {
        let fx = fixture();
        let before = adoption_fs(&fx, true).snapshot();
        let dir = scratch::dir("dangling");
        scratch::write_tree(&dir, &before);
        let aim = scratch::dangling_link(&dir, "rb.json.new");
        let args: Vec<OsString> = approve(&fx)
            .into_iter()
            .map(|a| match a.to_str() {
                Some("rb.json" | "ledger.jsonl" | "model.json" | "store") => scratch::path(&dir, a),
                _ => a,
            })
            .collect();
        let out = run(&args, &crate::StdFs);
        assert_eq!(out.code.value(), 70, "{}", out.stderr);
        assert!(
            out.stderr.starts_with("error RT-APR-0208:"),
            "{}",
            out.stderr
        );
        assert!(out.stderr.contains("rb.json.new"), "{}", out.stderr);
        assert!(out.stderr.contains("no file changed"), "{}", out.stderr);
        assert_eq!(
            scratch::link_target(&dir, "rb.json.new"),
            Some(aim),
            "the user's link is still there"
        );
        assert_eq!(scratch::read(&dir, "rb.json"), before[Path::new("rb.json")]);
        assert_eq!(
            scratch::read(&dir, "ledger.jsonl"),
            before[Path::new("ledger.jsonl")]
        );
        assert!(!scratch::exists(&dir, "ledger.jsonl.journal"));
        assert!(!scratch::exists(&dir, "ledger.jsonl.journal.tmp"));
    }
}
