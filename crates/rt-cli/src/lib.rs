//! The `ruletrace` command-line driver (design §3.9.1, §6.1, §6.2.4).
//!
//! This is the only crate of the core that reads and writes files. [`run`] takes the arguments
//! and an [`Fs`] and returns what the process must print and its exit code, so every step of
//! every command can be tested without starting a process. A command runs its stages in the
//! order of design §6.1: arguments, input reading, the pipeline stages, then writing. The first
//! failing step decides the exit code. A failed command writes no output file and leaves an
//! existing one byte-identical.
#![forbid(unsafe_code)]

pub mod args;
pub mod io;
pub mod render;

mod compile;
mod evaluate;
mod ingest;
mod review;
mod validate;

use rt_foundation::{Diagnostic, DiagnosticState, ExitCode};
use std::ffi::OsString;

pub use io::{Fs, StdFs};

/// What the process does after a command: bytes for standard output, text for standard error and
/// the exit status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Standard output. Empty for `ingest`, whose result goes to its output files.
    pub stdout: Vec<u8>,
    /// Standard error: the rendered Diagnostics in canonical order.
    pub stderr: String,
    /// The exit code.
    pub code: ExitCode,
}

/// Runs one command line. `args` excludes the program name.
#[must_use]
pub fn run(args: &[OsString], fs: &dyn Fs) -> Outcome {
    let command = match args::parse(args) {
        Ok(command) => command,
        Err(problems) => return failure(problems),
    };
    match command {
        args::Invocation::Ingest(ingest_args) => match ingest::run(&ingest_args, fs) {
            Ok(warnings) => Outcome {
                stdout: Vec::new(),
                stderr: render::diagnostics(&warnings),
                code: ExitCode::Success,
            },
            Err(state) => failure(state.as_slice().to_vec()),
        },
        args::Invocation::Validate(validate_args) => {
            let done = validate::run(&validate_args, fs);
            Outcome {
                stdout: Vec::new(),
                stderr: match validate_args.output_format {
                    args::OutputFormat::Human => render::diagnostics(&done.state),
                    args::OutputFormat::Json => render::diagnostics_json(&done.state),
                },
                code: done.code,
            }
        }
        args::Invocation::Compile(compile_args) => {
            let done = compile::run(&compile_args, fs);
            let mut stderr = render::diagnostics(&done.state);
            for text in &done.stored {
                stderr.push_str(text);
            }
            Outcome {
                stdout: Vec::new(),
                stderr,
                code: done.code,
            }
        }
        args::Invocation::Review(review_args) => {
            let done = review::run(&review_args, fs);
            Outcome {
                stdout: done.stdout,
                stderr: render::diagnostics(&done.state),
                code: done.code,
            }
        }
        args::Invocation::Evaluate(evaluate_args) => {
            let done = evaluate::run(&evaluate_args, fs);
            Outcome {
                stdout: done.stdout,
                stderr: match evaluate_args.output_format {
                    args::OutputFormat::Human => render::diagnostics(&done.state),
                    args::OutputFormat::Json => render::diagnostics_json(&done.state),
                },
                code: done.code,
            }
        }
    }
}

/// The outcome of a failed command: the earliest failing stage has already been chosen by the
/// caller, so the first blocking Diagnostic of the sorted list decides the code. Warnings never
/// decide.
fn failure(problems: Vec<Diagnostic>) -> Outcome {
    let mut state = DiagnosticState::empty();
    for d in problems {
        state.push(d);
    }
    let state = state.sorted();
    let code = state
        .as_slice()
        .iter()
        .find(|d| d.severity().is_blocking())
        .map_or(ExitCode::Internal, |d| d.code().exit_code());
    Outcome {
        stdout: Vec::new(),
        stderr: render::diagnostics(&state),
        code,
    }
}

#[cfg(test)]
mod tests;
