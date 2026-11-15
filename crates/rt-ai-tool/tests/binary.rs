//! The built `ruletrace-ai` executable: argument failures reach the
//! process exit status and the standard streams. These runs name no file.

#![allow(clippy::unwrap_used)]

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ruletrace-ai"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn no_command_exits_2_and_writes_a_diagnostic_to_standard_error() {
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.starts_with("error RT-CLI-0002: "), "{stderr}");
}

#[test]
fn a_command_other_than_propose_exits_2() {
    let out = run(&["review", "list"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.starts_with("error RT-CLI-0001: "), "{stderr}");
    assert!(stderr.contains("command 'review'"), "{stderr}");
}

#[test]
fn propose_without_a_configuration_exits_2_as_ai_disabled() {
    let out = run(&["propose", "--dry-run"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.starts_with("error RT-AI-0001: "), "{stderr}");
}
