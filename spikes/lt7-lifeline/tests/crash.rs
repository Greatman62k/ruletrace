//! Crash tests for the lifeline pipe (design §11.18.6, option L), standard library only.
//!
//! The harness starts the `parent` mode of the prototype, waits until the
//! child PIDs are recorded, kills the parent with `Child::kill` (SIGKILL on
//! Unix, TerminateProcess on Windows), and polls the recorded PIDs until no
//! child is left, within 5 s of the kill.
//!
//! Liveness without `unsafe` or dependencies:
//! - Unix: `kill -0 <pid>` through `sh`, then `ps -o stat=,args= -p <pid>`.
//!   A zombie (state `Z`) is terminated but not yet reaped; it is reported
//!   separately, and the strict check requires the PID to disappear, because
//!   the requirement says "terminated and reaped". The command line must
//!   contain the harness's unique tag, so that an unrelated process that
//!   reuses a PID counts as absent.
//! - Windows: `tasklist /FI "PID eq <pid>" /FO CSV /NH`; the image name must
//!   be `lt7-lifeline.exe`, which guards against PID reuse.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_lt7-lifeline");
const BOUND: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Absent,
    // Only the Unix probe can see a zombie; Windows has no such state.
    #[cfg(unix)]
    Zombie,
    Running,
}

#[cfg(unix)]
fn state(pid: u32, tag: &str) -> State {
    let pid = pid.to_string();
    let probe = Command::new("sh")
        .args(["-c", "kill -0 \"$1\" 2>/dev/null", "sh", &pid])
        .status()
        .expect("sh runs");
    if !probe.success() {
        return State::Absent;
    }
    let ps = Command::new("ps")
        .args(["-o", "stat=,args=", "-p", &pid])
        .output()
        .expect("ps runs");
    let text = String::from_utf8_lossy(&ps.stdout).trim().to_string();
    if !ps.status.success() || text.is_empty() {
        return State::Absent;
    }
    if text.starts_with('Z') {
        State::Zombie
    } else if text.contains(tag) {
        State::Running
    } else {
        State::Absent
    }
}

#[cfg(windows)]
fn state(pid: u32, _tag: &str) -> State {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .expect("tasklist runs");
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines().filter(|l| l.starts_with('"')) {
        let fields: Vec<&str> = line.trim().trim_matches('"').split("\",\"").collect();
        if fields.len() > 1
            && fields[1] == pid.to_string()
            && fields[0].eq_ignore_ascii_case("lt7-lifeline.exe")
        {
            return State::Running;
        }
    }
    State::Absent
}

#[cfg(unix)]
fn force_kill(pid: u32) {
    let _ = Command::new("sh")
        .args(["-c", "kill -9 \"$1\" 2>/dev/null", "sh", &pid.to_string()])
        .status();
}

#[cfg(windows)]
fn force_kill(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .output();
}

fn unique_tag() -> String {
    static N: AtomicU32 = AtomicU32::new(0);
    format!(
        "lt7-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    )
}

/// A started `parent` process and its recorded children; cleans up on drop.
struct Run {
    parent: Child,
    pids: Vec<u32>,
    tag: String,
    file: PathBuf,
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = self.parent.kill();
        let _ = self.parent.wait();
        for pid in &self.pids {
            force_kill(*pid);
        }
        let _ = fs::remove_file(&self.file);
    }
}

fn start_parent(children: usize, gap_ms: u64, no_lifeline: bool) -> Run {
    let tag = unique_tag();
    let file = std::env::temp_dir().join(format!("{tag}.pids"));
    let _ = fs::remove_file(&file);
    let mut cmd = Command::new(BIN);
    cmd.args(["parent", "--pids"])
        .arg(&file)
        .args([
            "--children",
            &children.to_string(),
            "--gap-ms",
            &gap_ms.to_string(),
        ])
        .args(["--tag", &tag, "--sleep-secs", "120"]);
    if no_lifeline {
        cmd.arg("--no-lifeline");
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let parent = cmd.spawn().expect("parent starts");
    let mut run = Run {
        parent,
        pids: Vec::new(),
        tag,
        file,
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let text = fs::read_to_string(&run.file).unwrap_or_default();
        let pids: Vec<u32> = text.lines().filter_map(|l| l.trim().parse().ok()).collect();
        if pids.len() >= children {
            run.pids = pids;
            return run;
        }
        assert!(
            Instant::now() < deadline,
            "parent recorded {} of {children} pids",
            pids.len()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Measured {
    no_running: Option<Duration>,
    absent: Option<Duration>,
}

/// Kills the parent, then polls until no child runs and until every child is absent.
fn kill_and_measure(run: &mut Run, limit: Duration) -> Measured {
    let start = Instant::now();
    run.parent.kill().expect("kill parent");
    run.parent.wait().expect("reap parent");
    let mut m = Measured {
        no_running: None,
        absent: None,
    };
    loop {
        let states: Vec<State> = run.pids.iter().map(|p| state(*p, &run.tag)).collect();
        let elapsed = start.elapsed();
        if m.no_running.is_none() && states.iter().all(|s| *s != State::Running) {
            m.no_running = Some(elapsed);
        }
        if states.iter().all(|s| *s == State::Absent) {
            m.absent = Some(elapsed);
            return m;
        }
        if elapsed > limit {
            return m;
        }
        std::thread::sleep(POLL);
    }
}

fn report(test: &str, m: &Measured) {
    let ms = |d: Option<Duration>| d.map_or("none".to_string(), |d| d.as_millis().to_string());
    println!(
        "LT7-MEASURE os={} test={test} kill_to_no_running_ms={} kill_to_absent_ms={}",
        std::env::consts::OS,
        ms(m.no_running),
        ms(m.absent)
    );
}

fn assert_all_running(run: &Run) {
    for pid in &run.pids {
        assert_eq!(
            state(*pid, &run.tag),
            State::Running,
            "child {pid} before the crash"
        );
    }
}

fn crash_test(name: &str, children: usize, gap_ms: u64, settle_ms: u64) {
    let mut run = start_parent(children, gap_ms, false);
    assert_eq!(run.pids.len(), children);
    assert_all_running(&run);
    std::thread::sleep(Duration::from_millis(settle_ms));
    let m = kill_and_measure(&mut run, BOUND);
    report(name, &m);
    assert!(
        m.no_running.is_some(),
        "a child still runs {BOUND:?} after the crash"
    );
    assert!(
        m.absent.is_some(),
        "a child was terminated but not reaped within {BOUND:?}"
    );
}

#[test]
fn a_one_child_crash_leaves_no_child_within_5s() {
    crash_test("one_child", 1, 0, 500);
}

#[test]
fn b_two_steps_back_to_back_crash_leaves_no_child_within_5s() {
    crash_test("two_back_to_back", 2, 0, 0);
}

#[test]
fn b_two_steps_with_a_gap_crash_leaves_no_child_within_5s() {
    crash_test("two_with_gap", 2, 300, 0);
}

#[test]
fn b_five_steps_back_to_back_crash_leaves_no_child_within_5s() {
    crash_test("five_back_to_back", 5, 0, 0);
}

#[test]
fn c_control_children_without_lifeline_survive_the_crash() {
    let mut run = start_parent(2, 0, true);
    assert_all_running(&run);
    let m = kill_and_measure(&mut run, BOUND);
    report("control_no_lifeline", &m);
    assert!(
        m.no_running.is_none(),
        "control children ended after the crash"
    );
    assert!(m.absent.is_none());
    for pid in &run.pids {
        assert_eq!(
            state(*pid, &run.tag),
            State::Running,
            "control child {pid} after 5 s"
        );
    }
    // Clean up and show the liveness probe also detects the end of a process.
    for pid in run.pids.clone() {
        force_kill(pid);
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while run
        .pids
        .iter()
        .any(|p| state(*p, &run.tag) != State::Absent)
    {
        assert!(Instant::now() < deadline, "control children not cleaned up");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Kills and reaps the wrapped child on drop, so a failed assertion leaks nothing.
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_child_directly(lifeline: bool, tag: &str) -> KillOnDrop {
    let mut cmd = Command::new(BIN);
    cmd.arg("child");
    if lifeline {
        cmd.arg("--lifeline-stdin");
    }
    cmd.args(["--tag", tag, "--sleep-secs", "120"]);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    KillOnDrop(cmd.spawn().expect("child starts"))
}

fn wait_exit(child: &mut Child, limit: Duration) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return Some(status);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

#[test]
fn d_closing_the_pipe_ends_the_child_with_exit_code_70() {
    let mut guard = spawn_child_directly(true, &unique_tag());
    let child = &mut guard.0;
    let mut pipe = child.stdin.take().unwrap();
    // Data on the pipe is not a shutdown signal; only end of file is.
    pipe.write_all(b"data that must be ignored\n").unwrap();
    pipe.flush().unwrap();
    assert!(
        wait_exit(child, Duration::from_secs(1)).is_none(),
        "child ended on data"
    );
    let start = Instant::now();
    drop(pipe);
    let status = wait_exit(child, BOUND).expect("child exits after the pipe closes");
    println!(
        "LT7-MEASURE os={} test=orderly_close close_to_exit_ms={}",
        std::env::consts::OS,
        start.elapsed().as_millis()
    );
    assert_eq!(status.code(), Some(70));
}

#[test]
fn d_child_without_the_watcher_ignores_a_closed_pipe() {
    let mut guard = spawn_child_directly(false, &unique_tag());
    let child = &mut guard.0;
    drop(child.stdin.take());
    let ended = wait_exit(child, Duration::from_secs(2));
    assert!(
        ended.is_none(),
        "a child without --lifeline-stdin exited: {ended:?}"
    );
}

/// A child that has exited but is not yet waited for is a zombie on Unix.
#[cfg(unix)]
#[test]
fn e_liveness_probe_distinguishes_running_zombie_and_absent() {
    let tag = unique_tag();
    let mut short = Command::new(BIN);
    short.args(["child", "--tag", &tag, "--sleep-secs", "0"]);
    short
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = short.spawn().expect("short child starts");
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(5);
    while state(pid, &tag) != State::Zombie {
        assert!(
            Instant::now() < deadline,
            "exited child never seen as a zombie"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait().unwrap();
    assert_eq!(state(pid, &tag), State::Absent);

    let long = spawn_child_directly(false, &tag);
    assert_eq!(state(long.0.id(), &tag), State::Running);
    assert_eq!(
        state(long.0.id(), "a-tag-the-process-does-not-carry"),
        State::Absent
    );
}
