//! The `--lifeline-stdin` option (design §11.18.6 mechanism L, UD-44).
//!
//! The option is accepted only as the first argument, before the command. It
//! is removed before [`crate::run`] sees the arguments, so every parse result,
//! usage text and output of a command line without it is unchanged. A repeated
//! or misplaced option reaches the ordinary parser and is refused there with
//! `RT-CLI-0001` or `RT-CLI-0004` (exit 2).
//!
//! The watcher thread and the process exit are in `main`; this module holds
//! the parts that tests can reach without a process.
//!
//! A pipe that closes after the command has finished but before `main` has
//! written its output and returned still stops the process with `RT-CLI-0007`
//! and exit 70, and standard output can then be incomplete. The parent that
//! closed the pipe is taken as gone, so no reader of that output remains.

use crate::out::render_diagnostic;
use rt_foundation::{CliLocation, Code, Diagnostic, ExitCode, Location};
use std::ffi::OsString;
use std::io::{self, ErrorKind, Read};

/// The option name.
pub const OPTION: &str = "--lifeline-stdin";

/// Splits a leading `--lifeline-stdin` from `args`. Returns whether it was
/// present and the arguments that remain.
#[must_use]
pub fn strip(args: &[OsString]) -> (bool, &[OsString]) {
    match args.split_first() {
        Some((first, rest)) if first.to_str() == Some(OPTION) => (true, rest),
        _ => (false, args),
    }
}

/// Reads and discards `input` until end of file or a read error that is not
/// `ErrorKind::Interrupted`, then returns.
pub fn wait_for_close<R: Read>(input: &mut R) {
    let mut buf = [0u8; 256];
    loop {
        match input.read(&mut buf) {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

/// The exit code of a process whose lifeline closed.
#[must_use]
pub fn exit_code() -> ExitCode {
    ExitCode::Internal
}

/// The `RT-CLI-0007` Diagnostic, located at the option (argument 0).
#[must_use]
pub fn closed_diagnostic() -> Diagnostic {
    Diagnostic::new(
        Code::CliLifelineClosed,
        "standard input of the lifeline closed, so the parent is taken as gone".to_owned(),
        Location::Cli(CliLocation::Argument {
            position: 0,
            name: Some(OPTION.to_owned()),
        }),
    )
}

/// The Diagnostic as written to standard error.
#[must_use]
pub fn closed_text() -> String {
    render_diagnostic(&closed_diagnostic())
}

/// Blocks on `input`, then writes the Diagnostic to `err`, ignoring a write
/// failure because the parent may be gone, and returns the exit code.
pub fn watch<R: Read, W: io::Write>(input: &mut R, err: &mut W) -> ExitCode {
    wait_for_close(input);
    let _ = err.write_all(closed_text().as_bytes());
    let _ = err.flush();
    exit_code()
}
