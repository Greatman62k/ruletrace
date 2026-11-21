//! Process rig for the `--lifeline-stdin` tests of `ruletrace` and
//! `ruletrace-ai` (design §11.18.6, UD-44). Standard library only.
//!
//! The test binary has no libtest harness: it is also the parent process of
//! the crash tests, started with `__parent` as its first argument. The parent
//! starts the executable under test with a piped standard input whose write
//! end it keeps, records the child PID in a file and sleeps. Killing it with
//! `Child::kill` (SIGKILL on Unix, TerminateProcess on Windows) is the crash.
//!
//! Liveness of a child that the test did not start is read without `unsafe`:
//! - Unix: `kill -0` through `sh`, then `ps -o stat=,args= -p`. A zombie is
//!   terminated but not yet reaped; it is reported as such and does not count
//!   as gone, because Req 37.17 requires every child to be reaped. The command
//!   line must contain the scratch name of the test, so that an unrelated
//!   process that reuses a PID counts as gone.
//! - Windows: `tasklist /FI "PID eq <pid>"`; the image name must match.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// The acceptance bound of UD-44: from the kill of the parent to the absence
/// of every child.
pub const BOUND: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(5);
const PARENT_MODE: &str = "__parent";

/// The text of `RT-CLI-0007` as both executables render it, written by hand
/// from the catalogue row and the Cli_Location rule of Req 19.5.
pub const CLOSED_TEXT: &str = "error RT-CLI-0007: standard input of the lifeline closed, so the parent is taken as gone\n  at argument 0 (--lifeline-stdin)\n";

/// A scratch directory under the workspace `target` directory, removed on
/// drop unless the thread is panicking (a failed test keeps its files).
pub struct Scratch {
    root: PathBuf,
}

impl Scratch {
    pub fn new(name: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch/lifeline")
            .join(format!("{}-{name}", std::process::id()));
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(&root).unwrap();
        Self {
            root: std::fs::canonicalize(root).unwrap(),
        }
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    pub fn write(&self, rel: &str, bytes: &[u8]) {
        let path = self.path(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// The path as an argument, with forward slashes on every platform.
    pub fn arg(&self, rel: &str) -> String {
        self.path(rel).to_string_lossy().replace('\\', "/")
    }

    /// A word of the directory name that no other test shares.
    pub fn tag(&self) -> String {
        self.root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    /// Every file under the directory, keyed by relative path with forward
    /// slashes. The `pids` file of the crash tests is left out.
    pub fn state(&self) -> std::collections::BTreeMap<String, Vec<u8>> {
        fn walk(dir: &Path, root: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else {
                    let rel = path.strip_prefix(root).unwrap();
                    out.insert(
                        rel.to_string_lossy().replace('\\', "/"),
                        std::fs::read(&path).unwrap(),
                    );
                }
            }
        }
        let mut out = std::collections::BTreeMap::new();
        walk(&self.root, &self.root, &mut out);
        out
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !thread::panicking() {
            drop(std::fs::remove_dir_all(&self.root));
        }
    }
}

/// An executable under test and how to start it.
#[derive(Clone)]
pub struct Target {
    pub exe: &'static str,
    /// Image name on Windows, for the liveness probe.
    pub image: &'static str,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    /// Standard output goes to a pipe that the test holds and never reads, so
    /// a command with a large result blocks writing it.
    pub unread_stdout: bool,
}

impl Target {
    fn command(&self) -> Command {
        let mut cmd = Command::new(self.exe);
        cmd.args(&self.args).current_dir(&self.cwd);
        cmd.env_clear();
        for (key, value) in self.env.iter().chain(&system_env()) {
            cmd.env(key, value);
        }
        cmd
    }
}

// A fixed environment keeps runs comparable; these two variables are the
// ones a process needs to start on every platform.
fn system_env() -> Vec<(String, String)> {
    ["PATH", "SystemRoot"]
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| ((*k).to_owned(), v)))
        .collect()
}

/// The result of a complete run.
pub struct Ran {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Runs the target to its end. `stdin_open` keeps a piped, idle standard
/// input open for the whole run (the lifeline), otherwise standard input is
/// null.
pub fn run_to_end(target: &Target, stdin_open: bool) -> Ran {
    let mut cmd = target.command();
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.stdin(if stdin_open {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = cmd.spawn().unwrap();
    let held = child.stdin.take();
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let stderr = thread::spawn(move || {
        let mut buf = Vec::new();
        drop(err.read_to_end(&mut buf));
        buf
    });
    let mut stdout = Vec::new();
    drop(out.read_to_end(&mut stdout));
    let status = child.wait().unwrap();
    drop(held);
    Ran {
        code: status.code(),
        stdout,
        stderr: stderr.join().unwrap(),
    }
}

/// A child started directly by the test, with its lifeline and drained
/// standard error.
pub struct Direct {
    pub child: Child,
    pub lifeline: Option<ChildStdin>,
    stderr: Option<JoinHandle<Vec<u8>>>,
    _stdout: Option<ChildStdout>,
}

pub fn spawn_direct(target: &Target) -> Direct {
    let mut cmd = target.command();
    cmd.stdin(Stdio::piped())
        .stdout(if target.unread_stdout {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let lifeline = child.stdin.take();
    let mut err = child.stderr.take().unwrap();
    let stderr = thread::spawn(move || {
        let mut buf = Vec::new();
        drop(err.read_to_end(&mut buf));
        buf
    });
    Direct {
        lifeline,
        stderr: Some(stderr),
        _stdout: child.stdout.take(),
        child,
    }
}

impl Direct {
    /// The exit status, or `None` when the child still runs after `limit`.
    pub fn wait_within(&mut self, limit: Duration) -> Option<ExitStatus> {
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return Some(status);
            }
            if start.elapsed() >= limit {
                return None;
            }
            thread::sleep(POLL);
        }
    }

    pub fn running(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
    }

    /// Standard error of an exited child.
    pub fn stderr(&mut self) -> String {
        let bytes = self.stderr.take().unwrap().join().unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

impl Drop for Direct {
    fn drop(&mut self) {
        drop(self.child.kill());
        drop(self.child.wait());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Absent,
    #[cfg(unix)]
    Zombie,
    Running,
}

#[cfg(unix)]
pub fn state(pid: u32, tag: &str, _image: &str) -> State {
    let pid = pid.to_string();
    let probe = Command::new("sh")
        .args(["-c", "kill -0 \"$1\" 2>/dev/null", "sh", &pid])
        .status()
        .unwrap();
    if !probe.success() {
        return State::Absent;
    }
    let ps = Command::new("ps")
        .args(["-o", "stat=,args=", "-p", &pid])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&ps.stdout).trim().to_string();
    if !ps.status.success() || text.is_empty() {
        State::Absent
    } else if text.starts_with('Z') {
        State::Zombie
    } else if text.contains(tag) {
        State::Running
    } else {
        State::Absent
    }
}

#[cfg(windows)]
pub fn state(pid: u32, _tag: &str, image: &str) -> State {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines().filter(|l| l.starts_with('"')) {
        let fields: Vec<&str> = line.trim().trim_matches('"').split("\",\"").collect();
        if let (Some(name), Some(found)) = (fields.first(), fields.get(1)) {
            if *found == pid.to_string() && name.eq_ignore_ascii_case(image) {
                return State::Running;
            }
        }
    }
    State::Absent
}

#[cfg(unix)]
pub fn force_kill(pid: u32) {
    drop(
        Command::new("sh")
            .args(["-c", "kill -9 \"$1\" 2>/dev/null", "sh", &pid.to_string()])
            .status(),
    );
}

#[cfg(windows)]
pub fn force_kill(pid: u32) {
    drop(
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output(),
    );
}

/// A started parent process and the child it recorded.
pub struct Crash {
    parent: Child,
    pub child_pid: u32,
    tag: String,
    image: &'static str,
    // Kept open and unread so that a blocked write stays blocked after the
    // parent is gone.
    _stdout: Option<ChildStdout>,
}

impl Crash {
    /// Starts `current_exe` in parent mode for `target` and waits for the
    /// child PID. `tag` is a word of the child's command line.
    pub fn start(target: &Target, tag: &str) -> Self {
        let pids = target.cwd.join("pids");
        drop(std::fs::remove_file(&pids));
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.arg(PARENT_MODE)
            .arg(&pids)
            .arg(&target.cwd)
            .arg(if target.unread_stdout {
                "inherit"
            } else {
                "null"
            })
            .arg(target.exe)
            .args(&target.args);
        cmd.env_clear();
        for (key, value) in target.env.iter().chain(&system_env()) {
            cmd.env(key, value);
        }
        cmd.stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(if target.unread_stdout {
                Stdio::piped()
            } else {
                Stdio::null()
            });
        let mut parent = cmd.spawn().unwrap();
        let held = parent.stdout.take();
        let start = Instant::now();
        let child_pid = loop {
            let text = std::fs::read_to_string(&pids).unwrap_or_default();
            if text.ends_with('\n') {
                break text.trim().parse().unwrap();
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "the parent recorded no child PID"
            );
            thread::sleep(POLL);
        };
        Self {
            parent,
            child_pid,
            tag: tag.to_owned(),
            image: target.image,
            _stdout: held,
        }
    }

    pub fn child_state(&self) -> State {
        state(self.child_pid, &self.tag, self.image)
    }

    /// Kills the parent and returns the time until the child is absent, that
    /// is terminated and reaped, or `None` when it is not absent after `limit`.
    pub fn kill_and_time(&mut self, limit: Duration) -> Option<Duration> {
        self.parent.kill().unwrap();
        let killed = Instant::now();
        self.parent.wait().unwrap();
        loop {
            if self.child_state() == State::Absent {
                return Some(killed.elapsed());
            }
            if killed.elapsed() >= limit {
                return None;
            }
            thread::sleep(POLL);
        }
    }
}

impl Drop for Crash {
    fn drop(&mut self) {
        drop(self.parent.kill());
        drop(self.parent.wait());
        force_kill(self.child_pid);
    }
}

/// Runs the parent mode if the first argument asks for it. Returns only when
/// the process is an ordinary test run.
pub fn parent_mode_or_return() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.first().and_then(|a| a.to_str()) != Some(PARENT_MODE) {
        return;
    }
    let [_, pids, cwd, stdout, exe, rest @ ..] = args.as_slice() else {
        std::process::exit(64);
    };
    let mut cmd = Command::new(exe);
    cmd.args(rest).current_dir(cwd).stdin(Stdio::piped());
    cmd.stdout(if stdout.to_str() == Some("inherit") {
        Stdio::inherit()
    } else {
        Stdio::null()
    });
    cmd.stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        std::process::exit(65);
    };
    // The write end stays here for the life of the process.
    let _lifeline = child.stdin.take();
    let line = format!("{}\n", child.id());
    if std::fs::write(pids, line).is_err() {
        std::process::exit(66);
    }
    drop(std::io::stdout().flush());
    thread::sleep(Duration::from_secs(120));
    std::process::exit(0);
}

// The AI crates forbid the printing macros, so the rig writes lines itself.
fn say(line: &str) {
    drop(writeln!(std::io::stdout(), "{line}"));
}

/// Runs the named checks one after another, each on its own thread so that a
/// panic is caught, prints one line per check and exits with status 1 when any
/// failed. One at a time, because design §11.18.6 starts at most one child at a
/// time on Windows (handle inheritance between children started together).
pub fn run_checks(checks: Vec<(&'static str, fn())>) -> ! {
    let mut failed = 0;
    for (name, check) in checks {
        match thread::spawn(check).join() {
            Ok(()) => say(&format!("test {name} ... ok")),
            Err(_) => {
                say(&format!("test {name} ... FAILED"));
                failed += 1;
            }
        }
    }
    let verdict = if failed == 0 { "ok" } else { "FAILED" };
    say(&format!("\ntest result: {verdict} ({failed} failed)"));
    std::process::exit(i32::from(failed != 0));
}

// The four checks shared by both executables.

/// (b) Orderly closure: the child still runs when the pipe is closed and
/// then exits with 70 and the Diagnostic within the bound.
pub fn orderly_closure(target: &Target, ready: &dyn Fn()) {
    let mut run = spawn_direct(target);
    ready();
    assert!(run.running(), "the child ended before the pipe was closed");
    drop(run.lifeline.take());
    let closed = Instant::now();
    let status = run.wait_within(BOUND);
    say(&format!(
        "lifeline: {} exited {:?} after the close",
        target.image,
        closed.elapsed()
    ));
    let measured = status.is_some();
    assert!(measured, "the child still ran {BOUND:?} after the close");
    assert_eq!(status.and_then(|s| s.code()), Some(70));
    let stderr = run.stderr();
    assert!(stderr.ends_with(CLOSED_TEXT), "{stderr}");
}

/// (c) Crash of the parent: the child is gone within the bound.
pub fn parent_crash(target: &Target, ready: &dyn Fn()) -> Duration {
    let mut crash = Crash::start(target, &target_tag(target));
    ready();
    assert_eq!(crash.child_state(), State::Running, "child not running");
    let gone = crash.kill_and_time(BOUND);
    assert!(
        gone.is_some(),
        "the child still ran {BOUND:?} after the kill"
    );
    let gone = gone.unwrap();
    say(&format!(
        "lifeline: {} gone {gone:?} after the kill",
        target.image
    ));
    gone
}

/// (d) Control without the option: the child still runs at the end of the
/// bound.
pub fn control_survives(target: &Target, ready: &dyn Fn()) {
    let mut crash = Crash::start(target, &target_tag(target));
    ready();
    assert_eq!(crash.child_state(), State::Running, "child not running");
    let gone = crash.kill_and_time(BOUND);
    assert!(gone.is_none(), "the control child ended after {gone:?}");
}

fn target_tag(target: &Target) -> String {
    target
        .cwd
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}
