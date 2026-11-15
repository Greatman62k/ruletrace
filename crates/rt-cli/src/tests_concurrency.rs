//! Tests of the adoption commit on the real file system (`StdFs`): recovery
//! of a crashed commit, two review commands that race on one ledger, and a
//! reader that races a writer (design §3.8.3; Req 26.19, 26.30 to 26.32,
//! 26.35; policy sections 4, 6 and 8).
//!
//! Expected states are the byte contents of the files before and after a
//! clean run on the in-memory double, never the output of the recovery code.
//! Directories live under the cargo target directory, not under `/tmp`.

use super::commit_recovery_tests::readers;
use super::commit_tests::{
    adoption_fs, approve, args_for, clean_run, decide_args, fixture, pair, probe, Clean, Fixture,
};
use super::fault_tests::{Fault, FaultFs};
use super::*;
use crate::io::target_scratch::Dir;
use std::sync::atomic::Ordering;
use std::sync::{Barrier, Mutex};

const JOURNAL: &str = "ledger.jsonl.journal";
const NAMES: [&str; 4] = ["rb.json", "ledger.jsonl", "model.json", "store"];

impl Dir {
    /// The arguments with the file names of the fixture placed in this
    /// directory.
    fn args(&self, args: &[OsString]) -> Vec<OsString> {
        args.iter()
            .map(|a| match a.to_str() {
                Some(s) if NAMES.contains(&s) => self.0.join(s).into_os_string(),
                // Outputs and absent inputs stay inside the scratch directory.
                Some(
                    s @ ("validation.json" | "results.json" | "compiled.json" | "data.json"
                    | "proof.txt"),
                ) => self.0.join(format!("out-{s}")).into_os_string(),
                _ => a.clone(),
            })
            .collect()
    }

    fn run(&self, args: &[OsString]) -> Outcome {
        run(&self.args(args), &StdFs)
    }
}

/// The real file system with pauses: after every read of the Rulebook, so
/// that a reader stays between its reads, and in every directory sync, as a
/// slow disk would, so that a commit stays in its intermediate states.
struct Slow {
    after_rulebook_read: std::time::Duration,
    in_dir_sync: std::time::Duration,
}

const READER: Slow = Slow {
    after_rulebook_read: std::time::Duration::from_millis(20),
    in_dir_sync: std::time::Duration::ZERO,
};
const WRITER: Slow = Slow {
    after_rulebook_read: std::time::Duration::ZERO,
    in_dir_sync: std::time::Duration::from_millis(8),
};

impl Fs for Slow {
    fn len(&self, path: &Path) -> std::io::Result<u64> {
        StdFs.len(path)
    }

    fn read(&self, path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
        let bytes = StdFs.read(path, max_bytes);
        if path.ends_with("rb.json") {
            std::thread::sleep(self.after_rulebook_read);
        }
        bytes
    }

    fn exists(&self, path: &Path) -> std::io::Result<bool> {
        StdFs.exists(path)
    }

    fn create_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        StdFs.create_new(path, bytes)
    }

    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        StdFs.rename(from, to)
    }

    fn remove(&self, path: &Path) -> std::io::Result<()> {
        StdFs.remove(path)
    }

    fn hard_link(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        StdFs.hard_link(from, to)
    }

    fn sync_dir(&self, dir: &Path) -> std::io::Result<()> {
        std::thread::sleep(self.in_dir_sync);
        StdFs.sync_dir(dir)
    }

    fn list_dir(&self, dir: &Path) -> std::io::Result<Vec<PathBuf>> {
        StdFs.list_dir(dir)
    }

    fn try_lock(&self, path: &Path) -> std::io::Result<Box<dyn crate::io::LockHeld>> {
        StdFs.try_lock(path)
    }

    fn try_lock_shared(&self, path: &Path) -> std::io::Result<Box<dyn crate::io::LockHeld>> {
        StdFs.try_lock_shared(path)
    }
}

fn leftovers(state: &BTreeMap<PathBuf, Vec<u8>>) -> Vec<String> {
    super::commit_tests::commit_leftovers(state)
}

fn assert_whole(dir: &Dir, clean: &Clean, expected: &BTreeMap<PathBuf, Vec<u8>>, label: &str) {
    let state = dir.state();
    assert_eq!(leftovers(&state), Vec::<String>::new(), "{label}");
    assert_eq!(pair(&state), pair(expected), "{label}");
    assert_ne!(pair(&clean.before), pair(&clean.after), "fixture is inert");
}

/// A crash at every operation of the commit leaves files that the next review
/// command on the real file system resolves to the old pair or the new pair.
#[test]
fn a_crash_after_every_operation_is_resolved_by_the_next_command_on_the_real_file_system() {
    let fx = fixture();
    for with_ledger in [true, false] {
        let clean = clean_run(&fx, with_ledger);
        let mut recovered_forward = 0;
        let mut recovered_back = 0;
        let mut finished = 0;
        for n in 1..=clean.log.len() {
            let label = format!("with_ledger={with_ledger} crash after {n}");
            let dir = Dir::new("crash");
            // The attempt marker records absolute names, so the crashed run
            // uses the same absolute names as the run that recovers it.
            let start = adoption_fs(&fx, with_ledger).snapshot();
            let named: Vec<(String, Vec<u8>)> = start
                .iter()
                .map(|(k, v)| (dir.0.join(k).display().to_string(), v.clone()))
                .collect();
            let files: Vec<(&str, &[u8])> = named
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_slice()))
                .collect();
            let mem = FaultFs::with(&files);
            mem.set_fault(Fault::CrashAfter(n));
            let _ = run(&dir.args(&args_for(&fx, with_ledger)), &mem);
            assert!(mem.is_dead(), "{label}: the crash happened");
            dir.load(&mem.snapshot());
            let crashed = dir.state();

            // A reader on the real file system refuses while the journal
            // stands, and never reads a mixed pair.
            if crashed.contains_key(Path::new(JOURNAL)) {
                for (name, args) in readers() {
                    let out = dir.run(&args);
                    assert_eq!(out.code.value(), 2, "{label} {name}: {}", out.stderr);
                    assert!(out.stderr.contains("RT-APR-0214"), "{label} {name}");
                }
            }

            // The recovering command: a refused probe, so that only the
            // recovery can have changed the files.
            let mut probe_args = probe();
            if !with_ledger {
                probe_args.push("--create-ledger".into());
            }
            let out = dir.run(&probe_args);
            let state = dir.state();
            let journal_was_there = crashed.contains_key(Path::new(JOURNAL));
            if journal_was_there {
                // Design table: a new ledger with a new Rulebook, or with a
                // matching `.new` Rulebook, is the decided pair; every other
                // journal state resolves to the earlier pair.
                let ledger_new = pair(&crashed).0 != pair(&clean.before).0;
                let rulebook_new = pair(&crashed).1 != pair(&clean.before).1;
                let expected = if ledger_new
                    && (rulebook_new || crashed.contains_key(Path::new("rb.json.new")))
                {
                    &clean.after
                } else {
                    &clean.before
                };
                assert_eq!(out.code.value(), 2, "{label}: {}", out.stderr);
                assert_whole(&dir, &clean, expected, &label);
                if std::ptr::eq(expected, &clean.after) {
                    recovered_forward += 1;
                } else {
                    recovered_back += 1;
                }
            } else {
                // No journal: either nothing was decided yet (old pair, and a
                // normal run finishes the adoption) or the commit had already
                // finished and removed its journal (new pair, decision final).
                let out = dir.run(&args_for(&fx, with_ledger));
                if pair(&state) == pair(&clean.after) {
                    assert_eq!(out.code.value(), 3, "{label}: {}", out.stderr);
                    finished += 1;
                } else {
                    assert_eq!(pair(&state), pair(&clean.before), "{label}");
                    assert_eq!(out.code.value(), 0, "{label}: {}", out.stderr);
                }
                assert_whole(&dir, &clean, &clean.after, &label);
            }
        }
        assert!(
            recovered_forward > 0 && recovered_back > 0 && finished > 0,
            "sweep must reach every outcome: {recovered_forward} {recovered_back} {finished}"
        );
    }
}

/// Runs `approve` from `racers` threads released together. Returns each exit
/// code and standard error.
fn race(dir: &Dir, racers: usize, fx: &Fixture) -> Vec<(u8, String)> {
    let barrier = Barrier::new(racers);
    let results = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..racers {
            scope.spawn(|| {
                barrier.wait();
                let out = dir.run(&approve(fx));
                results
                    .lock()
                    .unwrap()
                    .push((out.code.value(), out.stderr.clone()));
            });
        }
    });
    results.into_inner().unwrap()
}

#[test]
fn a_second_approval_after_the_first_is_refused_and_changes_nothing() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    let dir = Dir::new("serial");
    dir.load(&clean.before);
    assert_eq!(dir.run(&approve(&fx)).code.value(), 0);
    let after = dir.state();
    assert_eq!(pair(&after), pair(&clean.after));

    let out = dir.run(&approve(&fx));
    assert_eq!(out.code.value(), 3, "{}", out.stderr);
    assert!(out.stderr.contains("RT-APR-0204"), "{}", out.stderr);
    let wrong = decide_args("approve", "d.b", &"1".repeat(64), &[]);
    let out = dir.run(&wrong);
    assert_eq!(out.code.value(), 3, "{}", out.stderr);
    assert!(out.stderr.contains("RT-APR-0204"), "{}", out.stderr);
    assert_eq!(dir.state(), after);
    assert_eq!(dir.state(), after);
}

#[test]
fn a_command_that_finds_the_ledger_lock_held_refuses_with_0212_on_the_real_file_system() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    let dir = Dir::new("held");
    dir.load(&clean.before);
    let before = dir.state();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.0.join("ledger.jsonl.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let out = dir.run(&approve(&fx));
    assert_eq!(out.code.value(), 70, "{}", out.stderr);
    assert!(out.stderr.contains("RT-APR-0212"), "{}", out.stderr);
    assert_eq!(dir.state(), before);

    drop(lock);
    let out = dir.run(&approve(&fx));
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert_whole(&dir, &clean, &clean.after, "after release");
}

/// A reader that finds the ledger lock held exclusively refuses with
/// RT-APR-0214 and changes nothing. The lock is taken through `std`, so this
/// fails if `StdFs` stops taking the shared lock.
#[test]
fn readers_refuse_with_0214_while_the_ledger_lock_is_held_on_the_real_file_system() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    let dir = Dir::new("readers-held");
    dir.load(&clean.before);
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.0.join("ledger.jsonl.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let before = dir.state();
    let names = dir.names();
    for (name, args) in readers() {
        let out = dir.run(&args);
        assert_eq!(out.code.value(), 2, "{name}: {}", out.stderr);
        assert!(out.stderr.starts_with("error RT-APR-0214:"), "{name}");
        assert_eq!(out.stderr.matches("RT-").count(), 1, "{name}");
        assert_eq!(dir.state(), before, "{name}");
        assert_eq!(dir.names(), names, "{name}");
    }

    drop(lock);
    let all = readers();
    let validate = dir.run(&all[0].1);
    assert_eq!(validate.code.value(), 3, "{}", validate.stderr);
    assert!(
        !validate.stderr.contains("RT-APR-0214"),
        "{}",
        validate.stderr
    );
    let list = dir.run(&all[1].1);
    assert_eq!(list.code.value(), 0, "{}", list.stderr);
}

/// Two commands race on one ledger. Exactly one adopts; the other is refused
/// either because it met the lock (RT-APR-0212, exit 70) or because it ran
/// after the first and recomputed under the lock (RT-APR-0204, exit 3). No
/// round ends in a mixed pair or with leftovers.
#[test]
fn two_racing_approvals_adopt_once_and_the_loser_is_refused_cleanly() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    for round in 0..40 {
        let dir = Dir::new("race");
        dir.load(&clean.before);
        let results = race(&dir, 2, &fx);
        let winners = results.iter().filter(|(c, _)| *c == 0).count();
        assert_eq!(winners, 1, "round {round}: {results:?}");
        for (code, text) in results.iter().filter(|(c, _)| *c != 0) {
            let by_lock = *code == 70 && text.contains("RT-APR-0212");
            let by_recheck = *code == 3 && text.contains("RT-APR-0204");
            assert!(by_lock || by_recheck, "round {round}: {code} {text}");
        }
        assert_whole(&dir, &clean, &clean.after, &format!("round {round}"));
    }
}

/// Validators that run while a writer adopts see either the old pair or the new
/// pair, or are refused with RT-APR-0214. A writer that meets a reader's
/// shared lock gets RT-APR-0212 and retries with the files unchanged. Across
/// the rounds at least one reader must have been refused, which shows that a
/// reader overlapped a commit; the test does not claim to hit any particular
/// interleaving.
#[test]
fn readers_racing_a_writer_never_see_a_mixed_pair() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    let validate_args = readers()
        .into_iter()
        .find(|(name, _)| *name == "validate")
        .unwrap()
        .1;
    // What a reader returns: its exit code and the artifact it wrote.
    let view = |dir: &Dir, tag: usize| -> (u8, String, Vec<u8>) {
        let mut args = dir.args(&validate_args);
        let out_path = dir.0.join(format!("out-{tag}.json"));
        let at = args
            .iter()
            .position(|a| a.to_string_lossy().ends_with("out-validation.json"))
            .unwrap();
        args[at] = out_path.clone().into_os_string();
        let out = run(&args, &READER);
        let artifact = std::fs::read(&out_path).unwrap_or_default();
        (out.code.value(), out.stderr, artifact)
    };
    let old_view = {
        let dir = Dir::new("view-old");
        dir.load(&clean.before);
        view(&dir, 0)
    };
    let new_view = {
        let dir = Dir::new("view-new");
        dir.load(&clean.after);
        view(&dir, 0)
    };
    assert!(!old_view.2.is_empty() && !new_view.2.is_empty());
    assert_ne!(old_view, new_view, "views must differ");
    {
        let mut mixed = clean.before.clone();
        mixed.insert(
            "ledger.jsonl".into(),
            clean.after[Path::new("ledger.jsonl")].clone(),
        );
        let dir = Dir::new("view-mixed");
        dir.load(&mixed);
        let v = view(&dir, 0);
        // The detector must tell a mixed pair from both whole pairs.
        assert!(v != old_view && v != new_view, "{}", v.1);
    }

    let mut refused_total = 0;
    let deadline = std::time::Duration::from_secs(120);
    for round in 0..15 {
        let dir = Dir::new("window");
        dir.load(&clean.before);
        // The lock file exists, as after any earlier review command, so that
        // every reader takes the shared lock.
        std::fs::write(dir.0.join("ledger.jsonl.lock"), b"").unwrap();
        let done = std::sync::atomic::AtomicBool::new(false);
        let seen = Mutex::new((0usize, 0usize, 0usize));
        std::thread::scope(|scope| {
            for tag in 1..=3 {
                let (dir, view, old_view, new_view, done, seen) =
                    (&dir, &view, &old_view, &new_view, &done, &seen);
                scope.spawn(move || loop {
                    let finished = done.load(Ordering::SeqCst);
                    let got = view(dir, tag);
                    let mut seen = seen.lock().unwrap();
                    if got == *old_view {
                        seen.0 += 1;
                    } else if got == *new_view {
                        seen.1 += 1;
                    } else {
                        assert_eq!(got.0, 2, "round {round}: mixed view {:?}", got.1);
                        assert!(got.1.contains("RT-APR-0214"), "round {round}: {}", got.1);
                        seen.2 += 1;
                    }
                    drop(seen);
                    if finished {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(tag as u64));
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(60));
            let started = std::time::Instant::now();
            loop {
                let out = run(&dir.args(&approve(&fx)), &WRITER);
                if out.code.value() == 0 {
                    break;
                }
                let retryable = out.code.value() == 70 && out.stderr.contains("RT-APR-0212");
                let starved = started.elapsed() >= deadline;
                if !retryable || starved {
                    // Readers must stop first, or the scope never joins.
                    done.store(true, Ordering::SeqCst);
                    panic!("writer refused (starved: {starved}): {}", out.stderr);
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            done.store(true, Ordering::SeqCst);
        });
        assert_whole(&dir, &clean, &clean.after, &format!("round {round}"));
        assert_eq!(view(&dir, 9), new_view, "round {round}");
        let (old, new, refused) = seen.into_inner().unwrap();
        assert!(old + new + refused > 0, "round {round}: no reader ran");
        refused_total += refused;
    }
    assert!(
        refused_total > 0,
        "no reader was refused: no reader overlapped a commit in any round"
    );
}
