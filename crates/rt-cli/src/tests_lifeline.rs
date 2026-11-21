//! `--lifeline-stdin` (design §11.18.6, UD-44): the position rule, the
//! Diagnostic `RT-CLI-0007` and the watcher loop, without a process. The
//! built executable is tested in `tests/lifeline.rs`. Expected values are
//! written by hand from the catalogue row and the design.

use crate::lifeline::{
    closed_diagnostic, closed_text, exit_code, strip, wait_for_close, watch, OPTION,
};
use crate::{args, run, StdFs};
use rt_foundation::{CliLocation, Code, ExitCode, Location, Severity};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{self, ErrorKind, Read, Write};

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

// A reader that plays a script of results and counts the calls.
struct Script {
    steps: VecDeque<io::Result<usize>>,
    reads: usize,
}

impl Script {
    fn new(steps: Vec<io::Result<usize>>) -> Self {
        Self {
            steps: steps.into(),
            reads: 0,
        }
    }
}

impl Read for Script {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        match self.steps.pop_front() {
            Some(Ok(n)) => Ok(n.min(buf.len())),
            Some(Err(e)) => Err(e),
            None => panic!("read after the script ended"),
        }
    }
}

struct Broken;

impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(ErrorKind::BrokenPipe.into())
    }
}

#[test]
fn the_option_is_taken_only_as_the_first_argument() {
    let with = os(&["--lifeline-stdin", "validate", "--rulebook", "r.json"]);
    let (on, rest) = strip(&with);
    assert!(on);
    assert_eq!(rest, &with[1..]);

    let without = os(&["validate", "--lifeline-stdin"]);
    let (on, rest) = strip(&without);
    assert!(!on);
    assert_eq!(rest, &without[..]);

    let (on, rest) = strip(&[]);
    assert!(!on);
    assert!(rest.is_empty());
}

#[test]
fn only_the_exact_option_name_is_taken() {
    for name in [
        "--lifeline-stdin=1",
        "--lifeline-stdin ",
        "--lifeline",
        "-lifeline-stdin",
    ] {
        let args = os(&[name, "validate"]);
        let (on, rest) = strip(&args);
        assert!(!on, "{name}");
        assert_eq!(rest, &args[..], "{name}");
    }
}

#[test]
fn a_command_line_without_the_option_parses_and_runs_as_before() {
    let args = os(&["validate", "--no-such-option"]);
    let (_, rest) = strip(&args);
    assert_eq!(run(rest, &StdFs), run(&args, &StdFs));
    // With the option in front, what remains is byte-identical.
    let with = os(&["--lifeline-stdin", "validate", "--no-such-option"]);
    let (on, rest) = strip(&with);
    assert!(on);
    assert_eq!(run(rest, &StdFs), run(&args, &StdFs));
}

#[test]
fn a_misplaced_option_is_an_invalid_option_exit_2() {
    let problems = args::parse(&os(&["validate", OPTION])).unwrap_err();
    assert_eq!(problems[0].code(), Code::CliArgumentInvalid);
    assert_eq!(problems[0].code().exit_code(), ExitCode::InvalidInput);
}

#[test]
fn a_repeated_option_is_an_unknown_command_exit_2() {
    let given = os(&[OPTION, OPTION, "validate"]);
    let (_, rest) = strip(&given);
    let problems = args::parse(rest).unwrap_err();
    assert_eq!(problems[0].code(), Code::CliUnknownCommand);
    assert_eq!(problems[0].code().exit_code(), ExitCode::InvalidInput);
}

#[test]
fn the_option_alone_leaves_a_missing_command() {
    let given = os(&[OPTION]);
    let (on, rest) = strip(&given);
    assert!(on);
    let problems = args::parse(rest).unwrap_err();
    assert_eq!(problems[0].code(), Code::CliMissingArgument);
}

#[test]
fn the_diagnostic_is_rt_cli_0007_error_exit_70_at_the_option() {
    let d = closed_diagnostic();
    assert_eq!(d.code().id(), "RT-CLI-0007");
    assert_eq!(d.code().mnemonic(), "CLI_LIFELINE_CLOSED");
    assert_eq!(d.severity(), Severity::Error);
    assert_eq!(d.code().exit_code(), ExitCode::Internal);
    assert_eq!(d.code().exit_code().value(), 70);
    assert_eq!(exit_code().value(), 70);
    assert_eq!(
        d.primary(),
        &Location::Cli(CliLocation::Argument {
            position: 0,
            name: Some("--lifeline-stdin".to_owned())
        })
    );
    assert!(d.related().is_empty());
}

#[test]
fn the_text_is_the_standard_error_line_group_of_every_diagnostic() {
    assert_eq!(
        closed_text(),
        "error RT-CLI-0007: standard input of the lifeline closed, so the parent is taken as gone\n  at argument 0 (--lifeline-stdin)\n"
    );
}

#[test]
fn end_of_file_ends_the_wait_and_data_does_not() {
    let mut input = Script::new(vec![Ok(10), Ok(256), Ok(1), Ok(0)]);
    wait_for_close(&mut input);
    assert_eq!(input.reads, 4);
}

#[test]
fn an_interrupted_read_is_retried() {
    let mut input = Script::new(vec![
        Err(ErrorKind::Interrupted.into()),
        Err(ErrorKind::Interrupted.into()),
        Ok(3),
        Err(ErrorKind::Interrupted.into()),
        Ok(0),
    ]);
    wait_for_close(&mut input);
    assert_eq!(input.reads, 5);
}

#[test]
fn any_other_read_error_ends_the_wait() {
    for kind in [
        ErrorKind::BrokenPipe,
        ErrorKind::PermissionDenied,
        ErrorKind::WouldBlock,
        ErrorKind::Other,
    ] {
        let mut input = Script::new(vec![Ok(1), Err(kind.into())]);
        wait_for_close(&mut input);
        assert_eq!(input.reads, 2, "{kind:?}");
    }
}

#[test]
fn watch_writes_the_diagnostic_and_returns_70() {
    let mut input = Script::new(vec![Ok(0)]);
    let mut err = Vec::new();
    let code = watch(&mut input, &mut err);
    assert_eq!(code.value(), 70);
    assert_eq!(String::from_utf8(err).unwrap(), closed_text());
}

#[test]
fn a_failed_write_of_the_diagnostic_is_ignored() {
    let mut input = Script::new(vec![Err(ErrorKind::BrokenPipe.into())]);
    let code = watch(&mut input, &mut Broken);
    assert_eq!(code.value(), 70);
}
