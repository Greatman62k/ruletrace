//! The built `ruletrace-ai` executable with `--lifeline-stdin` (design
//! §11.18.6, UD-44; Req 5.12, 16.16, 17.9). No libtest harness: the binary is
//! also the parent process of the crash tests (see `lifeline_rig`).
//!
//! The long-running child is `propose` against a loopback stub server that
//! accepts the connection and never answers. The configured attempt timeout is
//! 90 s, so the child waits far beyond the 5 s bound. No hook or hidden mode
//! is involved. The credential is a fixed dummy value; nothing leaves the
//! loopback interface.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../rt-cli/tests/lifeline_rig/mod.rs"]
mod lifeline_rig;

use lifeline_rig::{Scratch, Target};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const EXE: &str = env!("CARGO_BIN_EXE_ruletrace-ai");
const IMAGE: &str = "ruletrace-ai.exe";
const OPTION: &str = "--lifeline-stdin";
const FIXTURE: &str = "tests/fixtures/live-run1-prompt-2/inputs";
const DEMO: &str = "../../demo/stand-in-ai";
const RECORDING: &str = "provider/recorded-meta-muse-spark-1.3-prompt-2.json";
const CREATED_AT: &str = "2026-11-16T00:00:00Z";
const KEY_NAME: &str = "MODEL_API_KEY";
const KEY_VALUE: &str = "lifeline-test-dummy-value";
const LIMITS: &str = "[limits]\ntotal_deadline_secs = 120\nattempt_timeout_secs = 90\nmax_retries = 0\nmax_response_bytes = 1048576\n";

/// A loopback server that accepts connections and never answers.
struct Stalling {
    port: u16,
    count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Stalling {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let count = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (n, s) = (count.clone(), stop.clone());
        let held: Arc<Mutex<Vec<TcpStream>>> = Arc::default();
        let handle = thread::spawn(move || {
            while !s.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        held.lock().unwrap().push(stream);
                        n.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(_) => thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        Self {
            port,
            count,
            stop,
            handle: Some(handle),
        }
    }

    fn wait_for_connection(&self) {
        let start = Instant::now();
        while self.count.load(Ordering::SeqCst) == 0 {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "the child never connected to the stub"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Stalling {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            drop(h.join());
        }
    }
}

fn manifest(rel: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn private(scratch: &Scratch, rel: &str) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(scratch.path(rel), mode).unwrap();
    }
    #[cfg(not(unix))]
    let _ = (scratch, rel);
}

fn replay_config(scratch: &Scratch) -> String {
    let log = scratch.arg("tx.jsonl");
    let script = scratch.arg("recorded.json");
    format!(
        "schema_version = 2\nenabled = true\nscript = \"{script}\"\nallow_document_text = true\nai_identities = [\"mock-replay\", \"meta\", \"muse-spark-1.3\"]\ntransmission_log = \"{log}\"\nmax_clauses_per_request = 5\n\n[provider]\nid = \"mock-replay\"\nprotocol = \"mock-replay\"\nmodel = \"muse-spark-1.3\"\nreasoning_effort = \"low\"\n\n{LIMITS}"
    )
}

fn stub_config(scratch: &Scratch, endpoint: &str) -> String {
    let log = scratch.arg("tx.jsonl");
    format!(
        "schema_version = 2\nenabled = true\nallow_document_text = true\nai_identities = [\"meta\", \"muse-spark-1.3\"]\ntransmission_log = \"{log}\"\nmax_clauses_per_request = 5\n\n[provider]\nid = \"meta\"\nprotocol = \"openai-responses\"\nmodel = \"muse-spark-1.3\"\nendpoint = \"{endpoint}\"\ncredential_env_var = \"{KEY_NAME}\"\nreasoning_effort = \"low\"\n\n{LIMITS}"
    )
}

/// Inputs of the first live run, copied; the originals are only read.
/// `endpoint` is `None` for the replay provider.
fn prepared(name: &str, endpoint: Option<&str>) -> Scratch {
    let scratch = Scratch::new(name);
    let fixture = manifest(FIXTURE);
    for file in ["candidates.json", "source-model.json"] {
        scratch.write(file, &std::fs::read(fixture.join(file)).unwrap());
    }
    let demo = manifest(DEMO);
    scratch.write(
        "seed.json",
        &std::fs::read(demo.join("rulebook/seed.json")).unwrap(),
    );
    scratch.write(
        "recorded.json",
        &std::fs::read(demo.join(RECORDING)).unwrap(),
    );
    let config = match endpoint {
        None => replay_config(&scratch),
        Some(endpoint) => stub_config(&scratch, endpoint),
    };
    scratch.write("config.toml", config.as_bytes());
    private(&scratch, "config.toml");
    scratch
}

fn propose(scratch: &Scratch, lifeline: bool, dry_run: bool) -> Target {
    let mut args: Vec<String> = Vec::new();
    if lifeline {
        args.push(OPTION.to_owned());
    }
    args.push("propose".to_owned());
    if dry_run {
        args.push("--dry-run".to_owned());
    }
    for (name, value) in [
        ("--config", scratch.arg("config.toml")),
        ("--rulebook", scratch.arg("seed.json")),
        ("--source-model", scratch.arg("source-model.json")),
        ("--candidates", scratch.arg("candidates.json")),
        ("--store", scratch.arg("store")),
        ("--created-at", CREATED_AT.to_owned()),
        ("--consent", "stand-in-rules.md".to_owned()),
    ] {
        args.push(name.to_owned());
        args.push(value);
    }
    Target {
        exe: EXE,
        image: IMAGE,
        args,
        cwd: scratch.path("."),
        env: vec![(KEY_NAME.to_owned(), KEY_VALUE.to_owned())],
        unread_stdout: false,
    }
}

fn stalled(name: &str) -> (Scratch, Stalling) {
    let stub = Stalling::start();
    let scratch = prepared(name, Some(&format!("http://127.0.0.1:{}", stub.port)));
    (scratch, stub)
}

// The two runs name their own scratch directory in what they print.
fn neutral(bytes: &[u8], dir: &Scratch) -> String {
    String::from_utf8_lossy(bytes).replace(&dir.arg("."), "<dir>")
}

// (a) The watcher must not alter a normal run. The replay provider stores a
// batch and writes the Transmission_Log, so files are compared too.
fn normal_run_is_unchanged_by_the_option() {
    let plain_dir = prepared("normal-plain", None);
    let plain = lifeline_rig::run_to_end(&propose(&plain_dir, false, false), false);
    let watched_dir = prepared("normal-watched", None);
    let watched = lifeline_rig::run_to_end(&propose(&watched_dir, true, false), true);
    let stderr = String::from_utf8_lossy(&plain.stderr).into_owned();
    assert_eq!(plain.code, Some(0), "{stderr}");
    let files = plain_dir.state();
    assert!(files.keys().any(|k| k.starts_with("store/batches/")));
    assert!(files.contains_key("tx.jsonl"));
    assert_eq!(watched.code, plain.code);
    assert_eq!(
        neutral(&watched.stdout, &watched_dir),
        neutral(&plain.stdout, &plain_dir)
    );
    assert_eq!(
        neutral(&watched.stderr, &watched_dir),
        neutral(&plain.stderr, &plain_dir)
    );
    // The configuration and log name their own directory; compare the store.
    let store = |dir: &Scratch| -> Vec<(String, Vec<u8>)> {
        dir.state()
            .into_iter()
            .filter(|(k, _)| k.starts_with("store/"))
            .collect()
    };
    assert_eq!(store(&watched_dir), store(&plain_dir));
}

// A dry run and a refused command line give the same result with the option.
fn dry_and_refused_runs_are_unchanged_by_the_option() {
    let dir = prepared("dry", None);
    let plain = lifeline_rig::run_to_end(&propose(&dir, false, true), false);
    let watched = lifeline_rig::run_to_end(&propose(&dir, true, true), true);
    let stderr = String::from_utf8_lossy(&plain.stderr).into_owned();
    assert_eq!(plain.code, Some(0), "{stderr}");
    assert_eq!(watched.code, plain.code);
    assert_eq!(watched.stdout, plain.stdout);
    assert_eq!(watched.stderr, plain.stderr);
    let mut bad = propose(&dir, false, false);
    bad.args = vec!["propose".into(), "--no-such-option".into()];
    let plain = lifeline_rig::run_to_end(&bad, false);
    bad.args.insert(0, OPTION.to_owned());
    let watched = lifeline_rig::run_to_end(&bad, true);
    assert_eq!(plain.code, Some(2));
    assert_eq!(watched.code, plain.code);
    assert_eq!(watched.stdout, plain.stdout);
    assert_eq!(watched.stderr, plain.stderr);
}

// Position rule: the option is accepted only as the first argument.
fn a_misplaced_or_repeated_option_exits_2_like_any_unknown_option() {
    let dir = prepared("position", None);
    let mut target = propose(&dir, false, false);
    target.args = vec!["propose".into(), OPTION.into()];
    let misplaced = lifeline_rig::run_to_end(&target, true);
    assert_eq!(misplaced.code, Some(2));
    let text = String::from_utf8(misplaced.stderr).unwrap();
    assert!(text.contains("RT-CLI-0004"), "{text}");
    target.args = vec![OPTION.into(), OPTION.into(), "propose".into()];
    let repeated = lifeline_rig::run_to_end(&target, true);
    assert_eq!(repeated.code, Some(2));
    let text = String::from_utf8(repeated.stderr).unwrap();
    assert!(text.contains("RT-CLI-0001"), "{text}");
    assert!(!text.contains("RT-CLI-0007"), "{text}");
}

// (b) Orderly closure of the pipe.
fn closing_the_pipe_stops_the_child_with_70() {
    let (dir, stub) = stalled("closure");
    lifeline_rig::orderly_closure(&propose(&dir, true, false), &|| stub.wait_for_connection());
}

// (c) The parent is killed.
fn killing_the_parent_leaves_no_child() {
    let (dir, stub) = stalled("crash");
    lifeline_rig::parent_crash(&propose(&dir, true, false), &|| stub.wait_for_connection());
}

// (d) Control: without the option the waiting child outlives the bound.
fn without_the_option_the_child_outlives_the_parent() {
    let (dir, stub) = stalled("control");
    lifeline_rig::control_survives(&propose(&dir, false, false), &|| stub.wait_for_connection());
}

fn main() {
    lifeline_rig::parent_mode_or_return();
    lifeline_rig::run_checks(vec![
        (
            "normal_run_is_unchanged_by_the_option",
            normal_run_is_unchanged_by_the_option,
        ),
        (
            "dry_and_refused_runs_are_unchanged_by_the_option",
            dry_and_refused_runs_are_unchanged_by_the_option,
        ),
        (
            "a_misplaced_or_repeated_option_exits_2_like_any_unknown_option",
            a_misplaced_or_repeated_option_exits_2_like_any_unknown_option,
        ),
        (
            "closing_the_pipe_stops_the_child_with_70",
            closing_the_pipe_stops_the_child_with_70,
        ),
        (
            "killing_the_parent_leaves_no_child",
            killing_the_parent_leaves_no_child,
        ),
        (
            "without_the_option_the_child_outlives_the_parent",
            without_the_option_the_child_outlives_the_parent,
        ),
    ]);
}
