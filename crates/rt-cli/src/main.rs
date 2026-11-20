//! The `ruletrace` executable. It reads the process arguments, runs the command
//! against the real file system and exits with the command's code. All logic is
//! in the `rt_cli` library.
//!
//! With `--lifeline-stdin` as the first argument, one watcher thread blocks on
//! standard input and ends the process when the pipe closes (design §11.18.6).
#![forbid(unsafe_code)]

use rt_cli::{lifeline, run, StdFs};
use std::io::Write;

fn main() -> std::process::ExitCode {
    let all: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let (watched, args) = lifeline::strip(&all);
    if watched {
        std::thread::spawn(|| {
            let code = lifeline::watch(&mut std::io::stdin(), &mut std::io::stderr());
            std::process::exit(i32::from(code.value()));
        });
    }
    let outcome = run(args, &StdFs);
    // A failure to write to a closed pipe cannot be reported anywhere else.
    let _ = std::io::stdout().write_all(&outcome.stdout);
    let _ = std::io::stderr().write_all(outcome.stderr.as_bytes());
    std::process::ExitCode::from(outcome.code.value())
}
