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
//! values, and the printed bytes must equal the baseline and the same runs
//! made in this process:
//!
//! - working directory: an empty directory, and a directory that holds an
//!   AI_Configuration file `ai.toml` (the content of
//!   `demo/stand-in-ai/ai/mock-replay.toml`) and a file `ruletrace.toml`;
//! - locale: `LC_ALL` and `LANG` set to `C`, `de_DE.UTF-8` and `tr_TR.UTF-8`;
//! - time zone: `TZ` set to `UTC` and `Pacific/Kiritimati`;
//! - wall clock: two runs at least 1.1 s apart. The test cannot set the
//!   clock, which needs privilege. It records the wall-clock second of each
//!   start and requires the two to differ;
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

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CHILD_MARKER: &str = "RT_WORKFLOW_MODULE_CHILD";
const BEGIN: &str = "<<<RT-CHILD-BEGIN>>>";
const END: &str = "<<<RT-CHILD-END>>>";
const AI_CONFIGURATION: &str = include_str!("../../../demo/stand-in-ai/ai/mock-replay.toml");
const CREDENTIAL_VARIABLES: [&str; 3] = [
    "MODEL_API_KEY",
    "OPENAI_API_KEY",
    "RULETRACE_OPENAI_API_KEY",
];
const CANARY_ONE: &str = "RT-CANARY-5c1e7a90-NOT-A-REAL-KEY-ONE";
const CANARY_TWO: &str = "RT-CANARY-b04d2f68-NOT-A-REAL-KEY-TWO";

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
            let value = |name: &str| std::env::var(name).unwrap_or_else(|_| "(unset)".to_owned());
            let cwd = std::env::current_dir().unwrap();
            let has = |file: &str| cwd.join(file).is_file();
            writeln!(out, "{BEGIN}").unwrap();
            for name in ["TZ", "LC_ALL", "LANG", "PATH"] {
                writeln!(out, "{name}={}", value(name)).unwrap();
            }
            writeln!(out, "cwd={}", cwd.display()).unwrap();
            writeln!(
                out,
                "ai.toml={} ruletrace.toml={}",
                has("ai.toml"),
                has("ruletrace.toml")
            )
            .unwrap();
            for name in CREDENTIAL_VARIABLES {
                writeln!(out, "{name}={}", value(name)).unwrap();
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
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env_clear()
        .env(CHILD_MARKER, mode)
        .current_dir(&v.cwd);
    // The Windows loader needs the system root; it carries no test input.
    if cfg!(windows) {
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", root);
        }
    }
    for (key, value) in &v.env {
        command.env(key, value);
    }
    command.output().unwrap()
}

/// The bytes between the markers of a successful child, and every byte the
/// child wrote, for the canary check.
fn framed(output: &Output, v: &Variant) -> (Vec<u8>, Vec<u8>) {
    let mut everything = output.stdout.clone();
    everything.extend(&output.stderr);
    assert!(
        output.status.success(),
        "{}: child failed: {}",
        v.name,
        String::from_utf8_lossy(&everything)
    );
    let begin = format!("{BEGIN}\n");
    let start = find(&output.stdout, begin.as_bytes())
        .unwrap_or_else(|| panic!("{}: no begin marker", v.name))
        + begin.len();
    let end = find(&output.stdout, format!("\n{END}\n").as_bytes())
        .unwrap_or_else(|| panic!("{}: no end marker", v.name));
    (output.stdout[start..end].to_vec(), everything)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
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
fn the_probe_shows_that_each_variant_reaches_the_child() {
    let w = world("probe");
    let probe = |v: &Variant| -> String {
        let output = spawn("probe", v);
        String::from_utf8(framed(&output, v).0).unwrap()
    };
    let plain = probe(&variant("plain", &w.empty, &[]));
    assert!(plain.contains("TZ=(unset)\n"), "{plain}");
    assert!(
        plain.contains("ai.toml=false ruletrace.toml=false"),
        "{plain}"
    );
    let rich = probe(&variant(
        "rich",
        &w.configured,
        &[
            ("TZ", "Pacific/Kiritimati"),
            ("LC_ALL", "de_DE.UTF-8"),
            ("LANG", "tr_TR.UTF-8"),
            ("OPENAI_API_KEY", CANARY_ONE),
        ],
    ));
    assert!(rich.contains("TZ=Pacific/Kiritimati\n"), "{rich}");
    assert!(rich.contains("LC_ALL=de_DE.UTF-8\n"), "{rich}");
    assert!(rich.contains("LANG=tr_TR.UTF-8\n"), "{rich}");
    assert!(rich.contains("ai.toml=true ruletrace.toml=true"), "{rich}");
    assert!(
        rich.contains(&format!("OPENAI_API_KEY={CANARY_ONE}\n")),
        "{rich}"
    );
    assert!(rich.contains("MODEL_API_KEY=(unset)\n"), "{rich}");
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
