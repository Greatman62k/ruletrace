//! Lifeline-pipe prototype for LT-6's sibling LT-7 (design §11.18.6, option L).
//!
//! Modes:
//! - `parent --pids FILE --children N [--gap-ms M] [--no-lifeline] [--tag T] [--sleep-secs S]`
//!   starts N stand-in children, each this binary in `child` mode with a fresh
//!   piped standard input whose write end the parent keeps. It appends each
//!   child PID to FILE, then sleeps. A gap of 0 starts the children back to back.
//! - `child --lifeline-stdin [--tag T] [--sleep-secs S]` starts a watcher thread
//!   that blocks reading standard input and exits with code 70 on end of file
//!   or a read error. The main thread sleeps, standing in for a long step.
//! - `child` without `--lifeline-stdin` has no watcher (control).
//!
//! The `--tag` value only labels the process command line, so that the crash
//! harness can tell its own children from an unrelated process that reuses a PID.

#![forbid(unsafe_code)]

use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::process::{exit, Child, ChildStdin, Command, ExitCode, Stdio};
use std::thread;
use std::time::Duration;

/// Exit code of a child whose lifeline closed (the `ruletrace` internal-error code).
const LIFELINE_EXIT: i32 = 70;

#[derive(Debug, Default)]
struct Options {
    pids: Option<String>,
    children: usize,
    gap_ms: u64,
    no_lifeline: bool,
    lifeline_stdin: bool,
    tag: String,
    sleep_secs: u64,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        sleep_secs: 120,
        ..Options::default()
    };
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        let mut value = |name: &str| -> Result<String, String> {
            i += 1;
            args.get(i)
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag {
            "--pids" => o.pids = Some(value("--pids")?),
            "--tag" => o.tag = value("--tag")?,
            "--children" => {
                o.children = value("--children")?
                    .parse()
                    .map_err(|_| "--children: number")?
            }
            "--gap-ms" => o.gap_ms = value("--gap-ms")?.parse().map_err(|_| "--gap-ms: number")?,
            "--sleep-secs" => {
                o.sleep_secs = value("--sleep-secs")?
                    .parse()
                    .map_err(|_| "--sleep-secs: number")?
            }
            "--no-lifeline" => o.no_lifeline = true,
            "--lifeline-stdin" => o.lifeline_stdin = true,
            other => return Err(format!("unknown argument {other}")),
        }
        i += 1;
    }
    Ok(o)
}

/// Blocks reading standard input; exits on end of file or a read error.
/// Bytes written to the pipe are discarded and do not end the process.
fn watch_lifeline() {
    let mut buf = [0u8; 256];
    let mut stdin = io::stdin();
    loop {
        match stdin.read(&mut buf) {
            Ok(0) => exit(LIFELINE_EXIT),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => exit(LIFELINE_EXIT),
        }
    }
}

fn run_child(o: &Options) -> ExitCode {
    if o.lifeline_stdin {
        thread::spawn(watch_lifeline);
    }
    thread::sleep(Duration::from_secs(o.sleep_secs));
    ExitCode::SUCCESS
}

fn spawn_child(o: &Options) -> io::Result<(Child, ChildStdin)> {
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.arg("child");
    if !o.no_lifeline {
        cmd.arg("--lifeline-stdin");
    }
    cmd.args(["--tag", &o.tag, "--sleep-secs", &o.sleep_secs.to_string()]);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = cmd.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("no piped stdin"))?;
    Ok((child, stdin))
}

fn run_parent(o: &Options) -> Result<(), String> {
    let path = o.pids.as_ref().ok_or("parent needs --pids")?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    // The write ends stay in this vector for the life of the process.
    let mut held: Vec<(Child, ChildStdin)> = Vec::new();
    for n in 0..o.children {
        if n > 0 && o.gap_ms > 0 {
            thread::sleep(Duration::from_millis(o.gap_ms));
        }
        let (child, stdin) = spawn_child(o).map_err(|e| e.to_string())?;
        writeln!(file, "{}", child.id()).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        held.push((child, stdin));
    }
    thread::sleep(Duration::from_secs(o.sleep_secs));
    drop(held);
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((mode, rest)) = args.split_first() else {
        eprintln!("usage: parent ... | child [--lifeline-stdin] ...");
        return ExitCode::from(2);
    };
    let options = match parse(rest) {
        Ok(o) => o,
        Err(m) => {
            eprintln!("error: {m}");
            return ExitCode::from(2);
        }
    };
    match mode.as_str() {
        "child" => run_child(&options),
        "parent" => match run_parent(&options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(m) => {
                eprintln!("error: {m}");
                ExitCode::from(2)
            }
        },
        other => {
            eprintln!("error: unknown mode {other}");
            ExitCode::from(2)
        }
    }
}
