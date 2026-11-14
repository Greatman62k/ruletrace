//! The `ruletrace` executable. It reads the process arguments, runs the command
//! against the real file system and exits with the command's code. All logic is
//! in the `rt_cli` library.
#![forbid(unsafe_code)]

use rt_cli::{run, StdFs};
use std::io::Write;

fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let outcome = run(&args, &StdFs);
    // A failure to write to a closed pipe cannot be reported anywhere else.
    let _ = std::io::stdout().write_all(&outcome.stdout);
    let _ = std::io::stderr().write_all(outcome.stderr.as_bytes());
    std::process::ExitCode::from(outcome.code.value())
}
