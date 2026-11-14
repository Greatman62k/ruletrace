//! Tests of the reader window of the journaled commit: a reader must never
//! consume a Rulebook and ledger of two different states (Req 26.30, 26.32;
//! design §3.8.3 (g); policy sections 4, 6 and 10).
//!
//! The files of a commit and of its rollback are replaced between two reads of
//! the reader with `tamper_after_read`, so no test depends on how many file
//! operations the reader makes. The two states are the byte contents before
//! and after a clean run, which the commit tests check independently.

use super::commit_recovery_tests::readers;
use super::commit_tests::{clean_run, fixture, Clean};
use super::fault_tests::{Fault, FaultFs};
use super::pipeline_tests;
use super::*;

const JOURNAL: &str = "ledger.jsonl.journal";
const LOCK: &str = "ledger.jsonl.lock";

fn bytes<'a>(state: &'a BTreeMap<PathBuf, Vec<u8>>, name: &str) -> &'a [u8] {
    state
        .get(Path::new(name))
        .unwrap_or_else(|| panic!("{name} is in the fixture"))
}

/// The two files of one state, as `tamper_after_read` changes.
fn pair_of(state: &BTreeMap<PathBuf, Vec<u8>>) -> [(&'static str, &[u8]); 2] {
    [
        ("rb.json", bytes(state, "rb.json")),
        ("ledger.jsonl", bytes(state, "ledger.jsonl")),
    ]
}

/// The reader refused with one RT-APR-0214 Error and changed nothing but what
/// the test replaced itself.
fn assert_refused(
    fs: &FaultFs,
    name: &str,
    args: &[OsString],
    at: &str,
    expected: &BTreeMap<PathBuf, Vec<u8>>,
) {
    let out = run(args, fs);
    let what = format!("{name}: {at}: {} {}", out.code.value(), out.stderr);
    assert_eq!(out.code.value(), 2, "{what}");
    assert!(out.stderr.starts_with("error RT-APR-0214:"), "{what}");
    assert_eq!(out.stderr.matches("RT-").count(), 1, "{what}");
    assert_eq!(&fs.snapshot(), expected, "{what}");
}

fn with_lock_file(state: &BTreeMap<PathBuf, Vec<u8>>) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut with = state.clone();
    with.insert(PathBuf::from(LOCK), Vec::new());
    with
}

fn clean() -> Clean {
    clean_run(&fixture(), true)
}

/// A commit fails after the reader read the Rulebook; the rollback restores
/// both files before the reader reads the ledger. Without the fix the reader
/// holds the adopted Rulebook and the old ledger and exits normally.
#[test]
fn a_rollback_between_the_two_reads_is_refused_by_every_reader() {
    let c = clean();
    for lock_file in [false, true] {
        for (name, args) in readers() {
            let start = if lock_file {
                with_lock_file(&c.after)
            } else {
                c.after.clone()
            };
            let fs = FaultFs::from_map(start);
            fs.tamper_after_read("rb.json", 1, &pair_of(&c.before));
            let mut expected = c.before.clone();
            if lock_file {
                expected = with_lock_file(&expected);
            }
            assert_refused(
                &fs,
                name,
                &args,
                &format!("rollback, lock file {lock_file}"),
                &expected,
            );
        }
    }
}

/// The reader read the old Rulebook; the commit then replaces both files. The
/// reader would hold the old Rulebook and the new ledger.
#[test]
fn a_commit_between_the_two_reads_is_refused_by_every_reader() {
    let c = clean();
    for lock_file in [false, true] {
        for (name, args) in readers() {
            let start = if lock_file {
                with_lock_file(&c.before)
            } else {
                c.before.clone()
            };
            let fs = FaultFs::from_map(start);
            fs.tamper_after_read("rb.json", 1, &pair_of(&c.after));
            let mut expected = c.after.clone();
            if lock_file {
                expected = with_lock_file(&expected);
            }
            assert_refused(
                &fs,
                name,
                &args,
                &format!("commit, lock file {lock_file}"),
                &expected,
            );
        }
    }
}

/// Each read of the pair is consistent, but the pair differs between the two
/// reads, so the reader cannot tell which state is current.
#[test]
fn a_pair_that_changes_between_two_consistent_reads_is_refused() {
    let c = clean();
    for (name, args) in readers() {
        let fs = FaultFs::from_map(c.before.clone());
        fs.tamper_after_read("ledger.jsonl", 1, &pair_of(&c.after));
        assert_refused(&fs, name, &args, "after the first pair", &c.after);
    }
}

/// A journal that appears after the first check, with the files untouched
/// until then, is found by the second check.
#[test]
fn a_journal_published_between_the_two_checks_is_refused() {
    let c = clean();
    for (name, args) in readers() {
        let fs = FaultFs::from_map(c.before.clone());
        fs.tamper_after_read("ledger.jsonl", 1, &[(JOURNAL, b"not read".as_slice())]);
        let mut expected = c.before.clone();
        expected.insert(PathBuf::from(JOURNAL), b"not read".to_vec());
        assert_refused(&fs, name, &args, "journal after the first read", &expected);
    }
}

/// With no lock file at the start, a lock file that exists afterwards means
/// that a review command ran during the reads.
#[test]
fn a_lock_file_created_during_the_reads_is_refused() {
    let c = clean();
    for (name, args) in readers() {
        let fs = FaultFs::from_map(c.before.clone());
        fs.tamper_after_read("ledger.jsonl", 1, &[(LOCK, b"".as_slice())]);
        let mut expected = c.before.clone();
        expected.insert(PathBuf::from(LOCK), Vec::new());
        assert_refused(&fs, name, &args, "lock file created", &expected);
        assert!(fs.log().iter().any(|o| o.kind == "try_lock_shared"));
    }
}

/// A writer that holds the exclusive lock excludes every reader, with no read.
#[test]
fn a_reader_refuses_while_a_writer_holds_the_lock() {
    let c = clean();
    let start = with_lock_file(&c.before);
    for (name, args) in readers() {
        let fs = FaultFs::from_map(start.clone());
        let _writer = fs.hold_lock(LOCK);
        assert_refused(&fs, name, &args, "writer holds the lock", &start);
        assert!(
            fs.log().iter().all(|o| o.kind == "try_lock_shared"),
            "{name}: no file is read"
        );
    }
}

/// A shared lock keeps a writer out for as long as a reader holds it.
#[test]
fn a_held_shared_lock_excludes_the_writer() {
    let c = clean();
    let fs = FaultFs::from_map(with_lock_file(&c.before));
    let held = fs.try_lock_shared(Path::new(LOCK));
    assert!(held.is_ok());
    let writer = fs.try_lock(Path::new(LOCK));
    assert_eq!(
        writer.err().map(|e| e.kind()),
        Some(io::ErrorKind::WouldBlock)
    );
    drop(held);
    assert!(fs.try_lock(Path::new(LOCK)).is_ok());
}

/// With no journal and no tampering, each reader reads each of the two files
/// twice, takes the shared lock when the lock file exists, and gives the
/// result of the files themselves.
#[test]
fn an_undisturbed_reader_reads_the_pair_twice_and_succeeds() {
    let c = clean();
    for lock_file in [false, true] {
        let start = if lock_file {
            with_lock_file(&c.before)
        } else {
            c.before.clone()
        };
        let all = readers();
        for (index, expected_code) in [(0, 3), (1, 0)] {
            let fs = FaultFs::from_map(start.clone());
            let out = run(&all[index].1, &fs);
            let what = format!("{}: {}", all[index].0, out.stderr);
            assert_eq!(out.code.value(), expected_code, "{what}");
            assert!(!out.stderr.contains("RT-APR-0214"), "{what}");
            let log = fs.log();
            for file in ["rb.json", "ledger.jsonl"] {
                let reads = log
                    .iter()
                    .filter(|o| o.kind == "read" && o.path == Path::new(file))
                    .count();
                assert_eq!(reads, 2, "{what}: reads of {file}");
            }
            let journal_checks = log
                .iter()
                .filter(|o| o.kind == "read" && o.path == Path::new(JOURNAL))
                .count();
            assert_eq!(journal_checks, 2, "{what}");
            let locks = log.iter().filter(|o| o.kind == "try_lock_shared").count();
            // A missing lock file is followed by one support probe on the
            // Rulebook.
            let expected_locks = if lock_file { 1 } else { 2 };
            assert_eq!(locks, expected_locks, "{what}");
        }
    }
}

/// A journal check that cannot run blocks the reader (fail closed). The first
/// and the second check are each made to fail for every reader.
#[test]
fn a_failed_journal_check_blocks_every_reader() {
    let c = clean();
    for (name, args) in readers() {
        for check in [1_usize, 2] {
            let fs = FaultFs::from_map(c.before.clone());
            // Without a lock file the operations are: lock, support probe,
            // check, rulebook, ledger, check, rulebook, ledger, lock file
            // existence.
            let op = if check == 1 { 3 } else { 6 };
            fs.set_fault(Fault::Fail(op));
            let out = run(&args, &fs);
            let what = format!("{name}: check {check}: {} {}", out.code.value(), out.stderr);
            let failed = fs.log()[op - 1].clone();
            assert_eq!(failed.kind, "read", "{what}");
            assert_eq!(failed.path, Path::new(JOURNAL), "{what}");
            assert_eq!(out.code.value(), 2, "{what}");
            assert!(out.stderr.starts_with("error RT-APR-0214:"), "{what}");
            assert!(
                out.stderr
                    .contains("the commit journal cannot be inspected"),
                "{what}"
            );
            assert_eq!(out.stderr.matches("RT-").count(), 1, "{what}");
            assert_eq!(fs.snapshot(), c.before, "{what}");
        }
    }
}

/// A lock that cannot be taken for a reason other than a missing file blocks
/// the reader.
#[test]
fn a_failed_shared_lock_blocks_every_reader() {
    let c = clean();
    let start = with_lock_file(&c.before);
    for (name, args) in readers() {
        let fs = FaultFs::from_map(start.clone());
        fs.set_fault(Fault::Fail(1));
        let out = run(&args, &fs);
        let what = format!("{name}: {} {}", out.code.value(), out.stderr);
        assert_eq!(out.code.value(), 2, "{what}");
        assert!(out.stderr.starts_with("error RT-APR-0214:"), "{what}");
        assert!(out.stderr.contains("cannot be taken for reading"), "{what}");
        assert_eq!(fs.snapshot(), start, "{what}");
    }
}

/// A change of one file alone also refuses: each file is compared on its own.
#[test]
fn a_change_of_only_one_file_between_the_reads_is_refused() {
    let c = clean();
    let after_ledger = bytes(&c.after, "ledger.jsonl");
    let after_rulebook = bytes(&c.after, "rb.json");
    for (name, args) in readers() {
        let only_ledger = FaultFs::from_map(c.before.clone());
        only_ledger.tamper_after_read("ledger.jsonl", 1, &[("ledger.jsonl", after_ledger)]);
        let mut expected = c.before.clone();
        expected.insert(PathBuf::from("ledger.jsonl"), after_ledger.to_vec());
        assert_refused(&only_ledger, name, &args, "ledger only", &expected);

        let only_rulebook = FaultFs::from_map(c.before.clone());
        only_rulebook.tamper_after_read("ledger.jsonl", 1, &[("rb.json", after_rulebook)]);
        let mut expected = c.before.clone();
        expected.insert(PathBuf::from("rb.json"), after_rulebook.to_vec());
        assert_refused(&only_rulebook, name, &args, "Rulebook only", &expected);
    }
}

/// A reader parses the bytes that passed both checks and never reads the pair
/// a third time: files replaced after its last read change nothing it reports.
#[test]
fn every_reader_uses_the_bytes_it_verified() {
    let c = clean();
    for (name, args) in readers().into_iter().take(2) {
        let calm = FaultFs::from_map(c.before.clone());
        let expected = run(&args, &calm);
        let busy = FaultFs::from_map(c.before.clone());
        busy.tamper_after_read(
            "ledger.jsonl",
            2,
            &[("rb.json", bytes(&c.after, "rb.json"))],
        );
        let got = run(&args, &busy);
        assert_eq!(got, expected, "{name}");
        let artifact = |fs: &FaultFs| fs.snapshot().get(Path::new("validation.json")).cloned();
        assert_eq!(artifact(&busy), artifact(&calm), "{name}");
        assert_eq!(
            bytes(&busy.snapshot(), "rb.json"),
            bytes(&c.after, "rb.json"),
            "{name}: the tamper took effect"
        );
    }
}

/// The same for `evaluate --proof-report`, on the fixture of the pipeline
/// tests. The replaced ledger would change the proof report.
#[test]
fn evaluate_builds_its_proof_report_from_the_verified_pair() {
    let state = || pipeline_tests::compiled_fs().files.borrow().clone();
    let args = os(&pipeline_tests::evaluate_args(
        "data-complete.json",
        &pipeline_tests::PROOF,
    ));
    let empty = format!("{}\n", rt_ledger::HEADER_LINE).into_bytes();
    let report = |fs: &FaultFs| fs.snapshot().get(Path::new("proof.txt")).cloned();

    let calm = FaultFs::from_map(state());
    let expected = run(&args, &calm);
    assert_eq!(expected.code.value(), 0, "{}", expected.stderr);
    let expected_report = report(&calm).expect("the calm run wrote a proof report");

    let mut replaced = state();
    replaced.insert(PathBuf::from("ledger.jsonl"), empty.clone());
    let other = FaultFs::from_map(replaced);
    let _ = run(&args, &other);
    assert_ne!(
        report(&other),
        Some(expected_report.clone()),
        "the replaced ledger would change the report"
    );

    let busy = FaultFs::from_map(state());
    busy.tamper_after_read("ledger.jsonl", 2, &[("ledger.jsonl", empty.as_slice())]);
    let got = run(&args, &busy);
    assert_eq!(got, expected);
    assert_eq!(report(&busy), Some(expected_report));
}
