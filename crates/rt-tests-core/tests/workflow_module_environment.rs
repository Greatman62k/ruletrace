//! Environment independence of the two Workflow_Modules (task 9.17,
//! Req 31.11).
//!
//! The libraries do no input or output, so the process test is a small
//! subprocess: this test binary starts itself (`std::env::current_exe`) with
//! the filter `child_entry`, an emptied environment (`env_clear`) and
//! explicit variables, and the child runs both libraries over the jobs of
//! `support/app_runs.rs` and prints the class, the results bytes, the human
//! report and the JSON report of each between two markers.
//!
//! Every variant differs from the baseline in one factor, in at least two
//! values. For every variant the child first reports what it sees (working
//! directory, every variable name, every value except credentials, and for a
//! credential only whether it equals a canary), and the test requires that
//! to equal what the variant intended. Then the printed bytes must equal the
//! baseline and the same runs made in this process:
//!
//! - working directory: an empty directory, and a directory that holds an
//!   AI_Configuration file `ai.toml` (the content of
//!   `demo/stand-in-ai/ai/mock-replay.toml`) and a file `ruletrace.toml`;
//! - locale: `LC_ALL` and `LANG` set to `C`, `de_DE.UTF-8` and `tr_TR.UTF-8`.
//!   Rust's standard formatting ignores the C locale, so these variants show
//!   only that the libraries read no locale variable;
//! - time zone: `TZ` set to `UTC` and `Pacific/Kiritimati`;
//! - wall clock: two runs at least 1.1 s apart. The test cannot set the
//!   clock, which needs privilege. It records the wall-clock second of each
//!   start and requires the two to differ. Limit: a library keyed on, for
//!   example, the parity of the second could pass one pair of starts with
//!   probability about 1/10;
//! - `ruletrace` and `ruletrace-ai` on `PATH`: stand-in files, not the built
//!   binaries (this package does not build them). On Unix each stand-in is an
//!   executable script that creates a marker file when started, and the test
//!   first proves that starting one creates the marker, then that no variant
//!   did;
//! - credential variables `MODEL_API_KEY`, `OPENAI_API_KEY` and
//!   `RULETRACE_OPENAI_API_KEY` (`tools/credential-variables.v1` does not
//!   exist) set to a canary, in two values. The canary must appear in no
//!   output of the child.
//!
//! Limit: the libraries are not given the executables or the configuration,
//! so the variants show that nothing in the process environment reaches the
//! reports. They do not show anything about the executables themselves.
//!
//! **Validates: Requirement 31.11**

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

#[path = "support/app_runs.rs"]
mod app_runs;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use app_runs::{find, framed_output, spawn_child, BEGIN, CHILD_MARKER, END};

const AI_CONFIGURATION: &str = include_str!("../../../demo/stand-in-ai/ai/mock-replay.toml");
const CREDENTIAL_VARIABLES: [&str; 3] = [
    "MODEL_API_KEY",
    "OPENAI_API_KEY",
    "RULETRACE_OPENAI_API_KEY",
];
const CANARY_ONE: &str = "RT-CANARY-5c1e7a90-NOT-A-REAL-KEY-ONE";
const CANARY_TWO: &str = "RT-CANARY-b04d2f68-NOT-A-REAL-KEY-TWO";

/// How a probe shows a value: a canary only as a label, never in full.
fn shown_value(value: &str) -> String {
    if value == CANARY_ONE {
        "<canary one>".to_owned()
    } else if value == CANARY_TWO {
        "<canary two>".to_owned()
    } else {
        value.to_owned()
    }
}

/// The child. Without the marker it only checks that the jobs run and give
/// the pinned class of the first job. With the marker `run` it prints the
/// bytes of the jobs. With the marker `probe` it prints what the process sees,
/// so the parent can check that a variant reached it.
#[test]
fn child_entry() {
    let rendered = app_runs::render_all();
    let text = String::from_utf8_lossy(&rendered);
    assert!(
        text.starts_with("job Prize-full class Complete\n"),
        "{text}"
    );
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    match std::env::var(CHILD_MARKER).as_deref() {
        Ok("run") => {
            writeln!(out, "{BEGIN}").unwrap();
            out.write_all(&rendered).unwrap();
            writeln!(out, "\n{END}").unwrap();
        }
        Ok("probe") => {
            let cwd = std::env::current_dir().unwrap();
            let has = |file: &str| cwd.join(file).is_file();
            writeln!(out, "{BEGIN}").unwrap();
            writeln!(out, "cwd={}", cwd.display()).unwrap();
            writeln!(out, "ai.toml={}", has("ai.toml")).unwrap();
            writeln!(out, "ruletrace.toml={}", has("ruletrace.toml")).unwrap();
            for (name, value) in std::env::vars_os() {
                let name = name.to_string_lossy().into_owned();
                if name == CHILD_MARKER || (cfg!(windows) && name == "SystemRoot") {
                    continue;
                }
                let shown = shown_value(&value.to_string_lossy());
                writeln!(out, "env {name}={shown}").unwrap();
            }
            writeln!(out, "\n{END}").unwrap();
        }
        _ => {}
    }
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("rt-wm-env-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn dir(&self, name: &str) -> PathBuf {
        let dir = self.0.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone)]
struct Variant {
    name: String,
    cwd: PathBuf,
    env: Vec<(String, OsString)>,
}

fn variant(name: &str, cwd: &Path, env: &[(&str, &str)]) -> Variant {
    Variant {
        name: name.to_owned(),
        cwd: cwd.to_path_buf(),
        env: env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
            .collect(),
    }
}

fn spawn(mode: &str, v: &Variant) -> Output {
    spawn_child(mode, &v.cwd, &v.env)
}

/// The bytes between the markers of a successful child, and every byte the
/// child wrote, for the canary check.
fn framed(output: &Output, v: &Variant) -> (Vec<u8>, Vec<u8>) {
    framed_output(output, &v.name)
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    find(haystack, needle.as_bytes()).is_some()
}

/// What this process produces for the same jobs, with its own environment.
fn in_process() -> Vec<u8> {
    app_runs::render_all()
}

/// Runs every variant and requires the printed bytes to equal `expected`
/// and to hold no canary.
fn assert_all_equal(expected: &[u8], variants: &[Variant]) {
    for v in variants {
        assert_probe_matches(v);
        let output = spawn("run", v);
        let (bytes, everything) = framed(&output, v);
        for canary in [CANARY_ONE, CANARY_TWO] {
            assert!(
                !contains(&everything, canary),
                "{}: canary in the output of the child",
                v.name
            );
        }
        assert!(
            bytes == expected,
            "{}: output differs from the baseline at byte {}",
            v.name,
            first_difference(expected, &bytes)
        );
    }
}

/// What the variant intends the child to see, in the form of the probe.
fn intended(v: &Variant) -> BTreeMap<String, String> {
    let mut seen = BTreeMap::new();
    seen.insert("cwd".to_owned(), canonical(&v.cwd));
    for file in ["ai.toml", "ruletrace.toml"] {
        seen.insert(file.to_owned(), v.cwd.join(file).is_file().to_string());
    }
    for (name, value) in &v.env {
        seen.insert(format!("env {name}"), shown_value(&value.to_string_lossy()));
    }
    seen
}

fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

/// Starts the child in probe mode and requires that it saw exactly the
/// working directory, files and variables of the variant: every variable it
/// holds is one the variant set, with the set value. A credential value is
/// shown only as a canary label, and the canary never appears in the probe
/// output otherwise.
fn assert_probe_matches(v: &Variant) {
    let output = spawn("probe", v);
    let (body, everything) = framed(&output, v);
    for canary in [CANARY_ONE, CANARY_TWO] {
        assert!(
            !contains(&everything, canary),
            "{}: canary in the probe output",
            v.name
        );
    }
    let mut got = BTreeMap::new();
    for line in String::from_utf8(body).unwrap().lines() {
        let (key, value) = line.split_once('=').unwrap();
        got.insert(key.to_owned(), value.to_owned());
    }
    if let Some(cwd) = got.get_mut("cwd") {
        *cwd = canonical(Path::new(cwd));
    }
    assert_eq!(got, intended(v), "{}: the child saw something else", v.name);
}

fn first_difference(a: &[u8], b: &[u8]) -> usize {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .unwrap_or_else(|| a.len().min(b.len()))
}

struct World {
    scratch: Scratch,
    empty: PathBuf,
    configured: PathBuf,
    bin: PathBuf,
}

/// Two working directories and a directory of stand-in executables.
fn world(label: &str) -> World {
    let scratch = Scratch::new(label);
    let empty = scratch.dir("empty");
    let configured = scratch.dir("configured");
    std::fs::write(configured.join("ai.toml"), AI_CONFIGURATION).unwrap();
    std::fs::write(configured.join("ruletrace.toml"), "name = \"stand-in\"\n").unwrap();
    let bin = scratch.dir("bin");
    for name in ["ruletrace", "ruletrace-ai"] {
        let file = bin.join(name);
        std::fs::write(&file, "#!/bin/sh\ntouch \"$(dirname \"$0\")/STARTED\"\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    World {
        scratch,
        empty,
        configured,
        bin,
    }
}

fn baseline(w: &World) -> Vec<u8> {
    let v = variant("baseline", &w.empty, &[]);
    let output = spawn("run", &v);
    let (bytes, _) = framed(&output, &v);
    bytes
}

#[test]
fn the_baseline_child_prints_what_this_process_computes() {
    let w = world("baseline");
    let bytes = baseline(&w);
    let local = in_process();
    assert!(
        bytes == local,
        "differs at byte {}",
        first_difference(&local, &bytes)
    );
    // Both modules and both classes are present, so equality is not vacuous.
    let text = String::from_utf8(bytes).unwrap();
    for line in [
        "job Prize-full class Complete\n",
        "job Prize-missing class Indeterminate\n",
        "job Bracket-full class Complete\n",
        "job Bracket-missing class Indeterminate\n",
    ] {
        assert!(text.contains(line), "{line}");
    }
}

#[test]
fn the_probe_reports_literal_values_and_never_a_credential() {
    // Control for the probe itself, with literals instead of the variant
    // mapping: a probe that printed nothing would otherwise match nothing.
    let w = world("probe");
    let v = variant(
        "rich",
        &w.configured,
        &[
            ("TZ", "Pacific/Kiritimati"),
            ("LC_ALL", "de_DE.UTF-8"),
            ("PATH", ""),
            ("MODEL_API_KEY", CANARY_ONE),
            ("OPENAI_API_KEY", CANARY_TWO),
        ],
    );
    let (body, _) = framed(&spawn("probe", &v), &v);
    let text = String::from_utf8(body).unwrap();
    for line in [
        "ai.toml=true\n",
        "ruletrace.toml=true\n",
        "env TZ=Pacific/Kiritimati\n",
        "env LC_ALL=de_DE.UTF-8\n",
        "env PATH=\n",
        "env MODEL_API_KEY=<canary one>\n",
        "env OPENAI_API_KEY=<canary two>\n",
    ] {
        assert!(text.contains(line), "{line:?} missing in {text}");
    }
    assert!(!text.contains("RT-CANARY"), "{text}");
    assert!(!text.contains("env RULETRACE_OPENAI_API_KEY"), "{text}");
    let plain = variant("plain", &w.empty, &[]);
    let (body, _) = framed(&spawn("probe", &plain), &plain);
    let text = String::from_utf8(body).unwrap();
    assert!(
        text.contains("ai.toml=false\nruletrace.toml=false\n"),
        "{text}"
    );
    assert!(!text.contains("env "), "{text}");
}

#[test]
fn working_directory_does_not_change_the_output() {
    let w = world("cwd");
    let base = baseline(&w);
    assert_all_equal(
        &base,
        &[
            variant("cwd empty", &w.empty, &[]),
            variant("cwd with ai.toml and ruletrace.toml", &w.configured, &[]),
            variant("cwd is the parent of both", w.scratch.0.as_path(), &[]),
        ],
    );
}

#[test]
fn locale_does_not_change_the_output() {
    let w = world("locale");
    let base = baseline(&w);
    let locales = ["C", "de_DE.UTF-8", "tr_TR.UTF-8"];
    let variants: Vec<Variant> = locales
        .iter()
        .map(|l| {
            variant(
                &format!("locale {l}"),
                &w.empty,
                &[("LC_ALL", l), ("LANG", l)],
            )
        })
        .collect();
    assert_all_equal(&base, &variants);
}

#[test]
fn time_zone_does_not_change_the_output() {
    let w = world("tz");
    let base = baseline(&w);
    assert_all_equal(
        &base,
        &[
            variant("TZ UTC", &w.empty, &[("TZ", "UTC")]),
            variant(
                "TZ Pacific/Kiritimati",
                &w.empty,
                &[("TZ", "Pacific/Kiritimati")],
            ),
        ],
    );
}

fn wall_clock_second() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn wall_clock_time_does_not_change_the_output() {
    // The clock itself cannot be set without privilege. Two starts more than
    // a second apart are the variation this test can produce.
    let w = world("clock");
    let v = variant("clock", &w.empty, &[]);
    let (first_second, first_instant) = (wall_clock_second(), Instant::now());
    let (first, _) = framed(&spawn("run", &v), &v);
    std::thread::sleep(Duration::from_millis(1100));
    let (second_second, second_instant) = (wall_clock_second(), Instant::now());
    let (second, _) = framed(&spawn("run", &v), &v);
    assert!(second_instant.duration_since(first_instant) >= Duration::from_millis(1100));
    assert!(
        second_second > first_second,
        "wall-clock second did not advance"
    );
    assert!(
        first == second,
        "output differs at byte {}",
        first_difference(&first, &second)
    );
    assert!(first == in_process());
}

#[cfg(unix)]
#[test]
fn a_stand_in_executable_records_that_it_was_started() {
    // Control for the PATH test: starting a stand-in must leave the marker,
    // otherwise "no marker" would prove nothing.
    let w = world("standin");
    for name in ["ruletrace", "ruletrace-ai"] {
        let marker = w.bin.join("STARTED");
        assert!(!marker.exists());
        let status = Command::new(w.bin.join(name)).status().unwrap();
        assert!(status.success());
        assert!(marker.exists(), "{name} left no marker");
        std::fs::remove_file(marker).unwrap();
    }
}

#[test]
fn executables_on_path_are_never_started_and_do_not_change_the_output() {
    let w = world("path");
    let base = baseline(&w);
    let bin = w.bin.to_str().unwrap().to_owned();
    assert_all_equal(
        &base,
        &[
            variant(
                "PATH is the stand-in directory",
                &w.empty,
                &[("PATH", &bin)],
            ),
            variant(
                "PATH has the stand-ins and the configured cwd",
                &w.configured,
                &[("PATH", &bin)],
            ),
            variant("PATH is empty", &w.empty, &[("PATH", "")]),
        ],
    );
    assert!(
        !w.bin.join("STARTED").exists(),
        "a stand-in executable was started by a library run"
    );
}

#[test]
fn credential_variables_do_not_change_the_output_and_never_appear_in_it() {
    let w = world("credentials");
    let base = baseline(&w);
    let mut variants = Vec::new();
    for canary in [CANARY_ONE, CANARY_TWO] {
        let all: Vec<(&str, &str)> = CREDENTIAL_VARIABLES.iter().map(|n| (*n, canary)).collect();
        variants.push(variant(
            &format!("all credentials = {canary}"),
            &w.empty,
            &all,
        ));
    }
    for name in CREDENTIAL_VARIABLES {
        variants.push(variant(
            &format!("only {name}"),
            &w.configured,
            &[(name, CANARY_ONE)],
        ));
    }
    assert_all_equal(&base, &variants);
    assert!(!contains(&base, CANARY_ONE) && !contains(&base, CANARY_TWO));
}

#[test]
fn every_factor_together_does_not_change_the_output() {
    let w = world("combined");
    let base = baseline(&w);
    let bin = w.bin.to_str().unwrap().to_owned();
    let mut env = vec![
        ("LC_ALL", "de_DE.UTF-8"),
        ("LANG", "de_DE.UTF-8"),
        ("TZ", "Pacific/Kiritimati"),
        ("PATH", bin.as_str()),
    ];
    for name in CREDENTIAL_VARIABLES {
        env.push((name, CANARY_TWO));
    }
    assert_all_equal(&base, &[variant("all factors", &w.configured, &env)]);
    assert!(!w.bin.join("STARTED").exists());
}
