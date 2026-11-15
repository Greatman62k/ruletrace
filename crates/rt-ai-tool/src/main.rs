//! The `ruletrace-ai` executable. It reads the process arguments, runs the
//! command against the real file system and exits with the command's code.
//! All logic is in the `rt_ai_tool` library. The tool reads no clock: the
//! time of a run is the `--created-at` argument.
#![forbid(unsafe_code)]

use rt_ai_tool::{run, Out, ProcessEnv, Redactor, StdFs};

fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let mut out = Out::new(
        std::io::stdout(),
        std::io::stderr(),
        Redactor::no_credential(),
    );
    let code = run(&args, &ProcessEnv, &StdFs, &mut out);
    std::process::ExitCode::from(code.value())
}
