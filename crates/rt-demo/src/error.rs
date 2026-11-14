//! The failure type of the harness. Every failure names what failed and where.

use std::fmt;

/// A harness failure. `Display` gives the one-line report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DemoError {
    /// A manifest check failed for one subject (a document id or a data or
    /// script path).
    Manifest {
        /// The document identifier, or the path of the data file or script.
        subject: String,
        /// The field that is missing or wrong, as a dotted name.
        field: String,
        /// What is wrong with it.
        problem: String,
    },
    /// The runbook Markdown or its block is invalid.
    Runbook(String),
    /// A step could not be run, or ended with the wrong exit code.
    Step {
        /// The step identifier.
        step: String,
        /// What went wrong.
        problem: String,
    },
    /// A produced file differs from its golden file.
    Golden {
        /// The step identifier.
        step: String,
        /// The golden file, relative to the demonstration directory.
        golden: String,
        /// The first differing byte offset.
        offset: usize,
        /// Why the files differ (content, length or missing golden file).
        problem: String,
    },
    /// Bless mode was requested in an environment where it is refused.
    BlessRefused,
    /// A file operation of the harness failed.
    Io {
        /// The path concerned, as given to the harness.
        path: String,
        /// The operating system message.
        problem: String,
    },
}

impl fmt::Display for DemoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DemoError::Manifest {
                subject,
                field,
                problem,
            } => write!(f, "manifest: '{subject}' field '{field}': {problem}"),
            DemoError::Runbook(m) => write!(f, "runbook: {m}"),
            DemoError::Step { step, problem } => write!(f, "step '{step}': {problem}"),
            DemoError::Golden {
                step,
                golden,
                offset,
                problem,
            } => write!(
                f,
                "step '{step}': golden file '{golden}' differs at byte offset {offset}: {problem}"
            ),
            DemoError::BlessRefused => {
                f.write_str("RT_DEMO_BLESS=1 is refused because the CI variable is set")
            }
            DemoError::Io { path, problem } => write!(f, "file '{path}': {problem}"),
        }
    }
}

impl std::error::Error for DemoError {}
