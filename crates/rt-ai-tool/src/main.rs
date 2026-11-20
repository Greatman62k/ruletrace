//! The `ruletrace-ai` executable. It reads the process arguments, runs the
//! command against the real file system and exits with the command's code.
//! All logic is in the `rt_ai_tool` library. The tool reads no clock: the
//! time of a run is the `--created-at` argument.
//!
//! With `--lifeline-stdin` as the first argument, one watcher thread blocks on
//! standard input and ends the process when the pipe closes (design §11.18.6).
#![forbid(unsafe_code)]

use rt_ai_tool::{lifeline, run, Out, ProcessEnv, Redactor, StdFs};

fn main() -> std::process::ExitCode {
    let all: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let (watched, args) = lifeline::strip(&all);
    if watched {
        std::thread::spawn(|| {
            let code = lifeline::watch(&mut std::io::stdin(), &mut std::io::stderr());
            std::process::exit(i32::from(code.value()));
        });
    }
    let mut out = Out::new(
        std::io::stdout(),
        std::io::stderr(),
        Redactor::no_credential(),
    );
    let code = run(args, &ProcessEnv, &StdFs, &mut out);
    std::process::ExitCode::from(code.value())
}
