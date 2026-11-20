//! The built `ruletrace` executable with `--lifeline-stdin` (design §11.18.6,
//! UD-44; Req 5.12, 16.15, 17.9). No libtest harness: the binary is also the
//! parent process of the crash tests (see `lifeline_rig`).
//!
//! The long-running child is `evaluate --render` over a data file with many
//! participants. Its rendered traces go to a standard output pipe that the
//! test holds and never reads, so the child blocks in the write for as long
//! as the test keeps the read end. No hook or hidden mode is involved.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod lifeline_rig;

use lifeline_rig::{Scratch, Target};
use std::path::Path;

const EXE: &str = env!("CARGO_BIN_EXE_ruletrace");
const IMAGE: &str = "ruletrace.exe";
const FIXTURE: &str = "../../tests/fixtures/slice-basic";
const OPTION: &str = "--lifeline-stdin";

// Far above any pipe buffer (Linux 64 KiB, macOS 64 KiB, Windows 4 KiB).
const PARTICIPANTS: usize = 3000;
const MIN_RENDER_BYTES: usize = 1 << 20;

fn target(scratch: &Scratch, lifeline: bool, unread_stdout: bool, args: &[&str]) -> Target {
    let mut all: Vec<String> = Vec::new();
    if lifeline {
        all.push(OPTION.to_owned());
    }
    all.extend(args.iter().map(|a| (*a).to_owned()));
    Target {
        exe: EXE,
        image: IMAGE,
        args: all,
        cwd: scratch.path("."),
        env: Vec::new(),
        unread_stdout,
    }
}

fn data(count: usize) -> String {
    let rows: Vec<String> = (0..count)
        .map(|i| {
            format!(
                r#"{{"id":"p{i}","fields":{{"age":"{}","licence":{},"judges":"{}","technical":"{}"}},"collections":{{}}}}"#,
                14 + i % 20,
                i % 3 != 0,
                1 + i % 9,
                1 + i % 7
            )
        })
        .collect();
    format!(
        r#"{{"format":"ruletrace-data","version":1,"participants":[{}]}}"#,
        rows.join(",")
    )
}

/// A scratch directory with the validated and compiled `slice-basic`
/// fixture and a large data file. The fixture is only read.
fn prepared(name: &str) -> Scratch {
    let scratch = Scratch::new(name);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    for (from, to) in [
        ("rulebook.json", "rb.json"),
        ("ledger.jsonl", "ledger.jsonl"),
        ("expected/source-model.json", "model.json"),
    ] {
        scratch.write(to, &std::fs::read(fixture.join(from)).unwrap());
    }
    scratch.write("data.json", data(PARTICIPANTS).as_bytes());
    let validate = lifeline_rig::run_to_end(
        &target(
            &scratch,
            false,
            false,
            &[
                "validate",
                "--rulebook",
                "rb.json",
                "--ledger",
                "ledger.jsonl",
                "--source-model",
                "model.json",
                "--out",
                "validation.json",
            ],
        ),
        false,
    );
    assert_eq!(validate.code, Some(0), "{:?}", validate.stderr);
    let compile = lifeline_rig::run_to_end(
        &target(
            &scratch,
            false,
            false,
            &[
                "compile",
                "--validation",
                "validation.json",
                "--out",
                "compiled.json",
            ],
        ),
        false,
    );
    assert_eq!(compile.code, Some(0), "{:?}", compile.stderr);
    scratch
}

fn evaluate(scratch: &Scratch, lifeline: bool, unread_stdout: bool) -> Target {
    // Absolute paths put the scratch name on the command line, which the
    // liveness probe of the crash tests matches.
    let path = |rel: &str| scratch.arg(rel);
    let (compiled, data, model, out) = (
        path("compiled.json"),
        path("data.json"),
        path("model.json"),
        path("results.json"),
    );
    target(
        scratch,
        lifeline,
        unread_stdout,
        &[
            "evaluate",
            "--compiled",
            &compiled,
            "--data",
            &data,
            "--source-model",
            &model,
            "--out",
            &out,
            "--render",
        ],
    )
}

fn no_wait() {}

// (a) The watcher must not alter a normal run.
fn normal_run_is_unchanged_by_the_option() {
    let scratch = prepared("normal");
    let plain = lifeline_rig::run_to_end(&evaluate(&scratch, false, false), false);
    let plain_files = scratch.state();
    std::fs::remove_file(scratch.path("results.json")).unwrap();
    let watched = lifeline_rig::run_to_end(&evaluate(&scratch, true, false), true);
    let watched_files = scratch.state();
    assert_eq!(plain.code, Some(0), "{:?}", plain.stderr);
    assert!(
        plain.stdout.len() >= MIN_RENDER_BYTES,
        "the long-running child must exceed every pipe buffer: {} bytes",
        plain.stdout.len()
    );
    assert_eq!(watched.code, plain.code);
    assert_eq!(watched.stdout, plain.stdout);
    assert_eq!(watched.stderr, plain.stderr);
    assert_eq!(watched_files, plain_files);
    assert!(plain_files.contains_key("results.json"));
}

// A refused command line gives the same result with the option in front.
fn a_refused_command_is_unchanged_by_the_option() {
    let scratch = Scratch::new("refused");
    let bad = ["validate", "--no-such-option"];
    let plain = lifeline_rig::run_to_end(&target(&scratch, false, false, &bad), false);
    let watched = lifeline_rig::run_to_end(&target(&scratch, true, false, &bad), true);
    assert_eq!(plain.code, Some(2));
    assert_eq!(watched.code, plain.code);
    assert_eq!(watched.stdout, plain.stdout);
    assert_eq!(watched.stderr, plain.stderr);
}

// Position rule: the option is accepted only as the first argument.
fn a_misplaced_or_repeated_option_exits_2_like_any_unknown_option() {
    let scratch = Scratch::new("position");
    let misplaced =
        lifeline_rig::run_to_end(&target(&scratch, false, false, &["validate", OPTION]), true);
    assert_eq!(misplaced.code, Some(2));
    let text = String::from_utf8(misplaced.stderr).unwrap();
    assert!(text.contains("RT-CLI-0004"), "{text}");
    let repeated =
        lifeline_rig::run_to_end(&target(&scratch, true, false, &[OPTION, "validate"]), true);
    assert_eq!(repeated.code, Some(2));
    let text = String::from_utf8(repeated.stderr).unwrap();
    assert!(text.contains("RT-CLI-0001"), "{text}");
    assert!(!text.contains("RT-CLI-0007"), "{text}");
}

// (b) Orderly closure of the pipe.
fn closing_the_pipe_stops_the_child_with_70() {
    let scratch = prepared("closure");
    lifeline_rig::orderly_closure(&evaluate(&scratch, true, true), &no_wait);
}

// (c) The parent is killed.
fn killing_the_parent_leaves_no_child() {
    let scratch = prepared("crash");
    lifeline_rig::parent_crash(&evaluate(&scratch, true, true), &no_wait);
}

// (d) Control: without the option the blocked child outlives the bound.
fn without_the_option_the_child_outlives_the_parent() {
    let scratch = prepared("control");
    lifeline_rig::control_survives(&evaluate(&scratch, false, true), &no_wait);
}

fn main() {
    lifeline_rig::parent_mode_or_return();
    lifeline_rig::run_checks(vec![
        (
            "normal_run_is_unchanged_by_the_option",
            normal_run_is_unchanged_by_the_option,
        ),
        (
            "a_refused_command_is_unchanged_by_the_option",
            a_refused_command_is_unchanged_by_the_option,
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
