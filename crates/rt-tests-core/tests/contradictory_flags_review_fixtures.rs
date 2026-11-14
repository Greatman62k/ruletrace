//! Property 5 (remaining input kinds): Review_Decision and committed fixtures.
//!
//! A Review_Decision has no file form today. The reviewer, the time and the
//! decision are command-line arguments of `ruletrace review approve` and
//! `reject`, so the flag and lifecycle keys cannot arrive as JSON members.
//! The review tests below therefore check the command line: a flag-like or
//! lifecycle-like option is an unknown option (`RT-CLI-0004`, exit code 2),
//! and the command touches no file. This does not exercise the flag-key
//! check of `rt_json`, because no decision document exists to carry the keys.
//!
//! The fixture test scans every committed JSON and JSON Lines input under
//! `tests/fixtures/` and `demo/` so that no stored input can carry a flag or
//! lifecycle key.
//!
//! **Validates: Requirements 22.16, 22.17, 22.22, 4.10**

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use rt_cli::{run, Fs};
use rt_foundation::{Code, ExitCode, InputId};
use rt_json::{check_flag_keys, parse, ParseLimits, FLAG_KEYS, LIFECYCLE_KEYS};
use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

const HASH: &str = "eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a";

/// A file system with no files that records every operation attempted.
struct Recording(RefCell<Vec<String>>);

impl Recording {
    fn new() -> Self {
        Self(RefCell::new(Vec::new()))
    }

    fn log(&self, what: &str, path: &Path) {
        self.0
            .borrow_mut()
            .push(format!("{what} {}", path.display()));
    }

    fn operations(&self) -> Vec<String> {
        self.0.borrow().clone()
    }
}

impl Fs for Recording {
    fn read(&self, path: &Path, _: u64) -> io::Result<Vec<u8>> {
        self.log("read", path);
        Err(io::ErrorKind::NotFound.into())
    }

    fn create_new(&self, path: &Path, _: &[u8]) -> io::Result<()> {
        self.log("create_new", path);
        Err(io::Error::other("read-only"))
    }

    fn rename(&self, from: &Path, _: &Path) -> io::Result<()> {
        self.log("rename", from);
        Err(io::Error::other("read-only"))
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        self.log("remove", path);
        Err(io::Error::other("read-only"))
    }
}

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn decision(verb: &str, extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        "review",
        verb,
        "--rulebook",
        "rulebook.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "model.json",
        "--rule",
        "elig.basic",
        "--revision",
        "1",
        "--content-hash",
        HASH,
        "--reviewer",
        "alice",
        "--time",
        "2026-01-01T00:00:00Z",
    ];
    if verb == "reject" {
        args.extend(["--reason", "not supported"]);
    }
    args.extend(extra);
    args.into_iter().map(str::to_owned).collect()
}

fn run_decision(args: &[String]) -> (rt_cli::Outcome, Vec<String>) {
    let fs = Recording::new();
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let outcome = run(&os(&refs), &fs);
    (outcome, fs.operations())
}

#[test]
fn a_decision_command_line_without_a_flag_option_reaches_the_file_system() {
    // Control for the test below: the same command lines with no flag-like
    // option are accepted by the parser and stop at the read-only lock.
    for verb in ["approve", "reject"] {
        let (outcome, operations) = run_decision(&decision(verb, &[]));
        assert_eq!(
            outcome.code,
            ExitCode::Internal,
            "{verb}: {}",
            outcome.stderr
        );
        assert!(
            outcome.stderr.contains(Code::ReviewLockUnavailable.id()),
            "{verb}: {}",
            outcome.stderr
        );
        assert!(
            !outcome.stderr.contains(Code::CliArgumentInvalid.id()),
            "{verb}: {}",
            outcome.stderr
        );
        assert!(!operations.is_empty(), "{verb} touched no file");
    }
}

#[test]
fn flag_and_lifecycle_options_are_unknown_options_of_review_decisions() {
    // Written out here, not derived from the production key lists.
    let names: Vec<String> = [
        "--has-errors",
        "--no-errors",
        "--validation-status",
        "--validation-result",
        "--passed",
        "--approval-status",
        "--is-approved",
        "--is-rejected",
        "--is-proposed",
        "--approval-record",
    ]
    .map(str::to_owned)
    .to_vec();
    for verb in ["approve", "reject"] {
        for name in &names {
            for tail in [&[][..], &["true"][..]] {
                let mut extra: Vec<&str> = vec![name.as_str()];
                extra.extend_from_slice(tail);
                let (outcome, operations) = run_decision(&decision(verb, &extra));
                let context = format!("{verb} {name} {tail:?}: {}", outcome.stderr);
                assert_eq!(outcome.code, ExitCode::InvalidInput, "{context}");
                assert!(
                    outcome.stderr.contains(Code::CliArgumentInvalid.id()),
                    "{context}"
                );
                assert!(
                    outcome.stderr.contains(&format!("unknown option '{name}'")),
                    "{context}"
                );
                assert!(
                    !outcome.stderr.contains(Code::CliInputNotFound.id()),
                    "{context}"
                );
                assert_eq!(operations, Vec::<String>::new(), "{context}");
            }
        }
    }
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("fixture directory")
        .map(|e| e.expect("entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("json" | "jsonl")
        ) {
            out.push(path);
        }
    }
}

/// Every document of a committed input: the file, or each line of JSON Lines.
fn documents(path: &Path) -> Vec<(String, Vec<u8>)> {
    let bytes = std::fs::read(path).expect("fixture file");
    let name = path.display().to_string();
    if path.extension().and_then(|e| e.to_str()) == Some("json") {
        return vec![(name, bytes)];
    }
    bytes
        .split(|b| *b == b'\n')
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(i, line)| (format!("{name}:{}", i + 1), line.to_vec()))
        .collect()
}

#[test]
fn no_committed_input_carries_a_flag_or_lifecycle_key() {
    let root = workspace();
    let mut files = Vec::new();
    collect(&root.join("tests/fixtures"), &mut files);
    collect(&root.join("demo"), &mut files);
    // The scan must not pass vacuously when a directory moves.
    assert!(files.len() >= 50, "only {} files found", files.len());
    assert!(files
        .iter()
        .any(|p| p.ends_with("slice-basic/rulebook.json")));
    assert!(files
        .iter()
        .any(|p| p.ends_with("stand-in-hand/golden/artifacts/ledger.jsonl")));
    let limits = ParseLimits::new(64 << 20, 128);
    let mut scanned = 0usize;
    let mut offenders = Vec::new();
    for path in &files {
        for (name, bytes) in documents(path) {
            let id = InputId::new(name.clone()).expect("input id");
            let value = parse(&id, &bytes, limits)
                .unwrap_or_else(|d| panic!("{name} is not valid RT-JSON: {d:?}"));
            scanned += 1;
            if let Err(found) = check_flag_keys(&id, &value) {
                offenders.push((
                    name,
                    found.iter().map(|d| d.code().id()).collect::<Vec<_>>(),
                ));
            }
        }
    }
    assert!(scanned >= files.len());
    assert_eq!(offenders, Vec::<(String, Vec<&str>)>::new());
}

#[test]
fn the_fixture_scan_detects_every_flag_and_lifecycle_key() {
    let id = InputId::new("probe.json").expect("input id");
    let limits = ParseLimits::new(1 << 20, 64);
    let keys = [
        ("has_errors", Code::DiagnosticStateContradiction),
        ("no_errors", Code::DiagnosticStateContradiction),
        ("validation_status", Code::DiagnosticStateContradiction),
        ("validation_result", Code::DiagnosticStateContradiction),
        ("passed", Code::DiagnosticStateContradiction),
        ("approval_status", Code::LifecycleFlagContradiction),
        ("is_approved", Code::LifecycleFlagContradiction),
        ("is_rejected", Code::LifecycleFlagContradiction),
        ("is_proposed", Code::LifecycleFlagContradiction),
        ("approval_record", Code::LifecycleFlagContradiction),
    ];
    // The probe list must be the whole production list.
    assert_eq!(keys.len(), FLAG_KEYS.len() + LIFECYCLE_KEYS.len());
    for (key, code) in keys {
        let text = format!(r#"{{"a":[{{"{key}":false}}]}}"#);
        let value = parse(&id, text.as_bytes(), limits).expect("valid json");
        let found = check_flag_keys(&id, &value).expect_err("must be rejected");
        assert_eq!(found.len(), 1, "{text}");
        assert_eq!(found[0].code(), code, "{text}");
    }
}
