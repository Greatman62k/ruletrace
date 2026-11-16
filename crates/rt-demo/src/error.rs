//! The failure type of the harness. Every failure names what failed and where.

use std::fmt;

/// Why the live-step guard refused a step (design §8.11.6, Req 33.10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A step that is not live was given to the live runner.
    NotLive,
    /// A step declared or detected as live was given to the offline runner.
    LiveStepInOfflineRunner,
    /// The live runner accepts only `ruletrace-ai`.
    NotRuletraceAi,
    /// The arguments are not in the form the guard can read.
    UnreadableArguments(String),
    /// The step names no document by `--consent`.
    NoConsent,
    /// The step names no `--store`, so no output location can be checked.
    NoStore,
    /// The configuration cannot be read or is not in the form the guard
    /// reads.
    UnreadableConfig(String),
    /// An output location lies where a live run must never write.
    ForbiddenOutput {
        /// The option or configuration field that names the location.
        option: String,
        /// The location as written.
        path: String,
        /// The protected location it resolves to or below.
        protected: String,
    },
    /// An output location cannot be resolved, for example a dangling or
    /// looping symbolic link, so it cannot be shown to lie outside the
    /// protected directories.
    UnresolvablePath {
        /// The option or configuration field that names the location.
        option: String,
        /// The location as written.
        path: String,
        /// The operating system message.
        problem: String,
    },
    /// The configuration field still holds the placeholder model of the
    /// example configuration.
    PlaceholderModel(String),
    /// An environment variable other than the configured credential variable.
    EnvironmentNotAllowed(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::NotLive => f.write_str("the step is not a live step"),
            Refusal::LiveStepInOfflineRunner => f.write_str(
                "the step is live (or its configuration is not the offline mock-replay \
                 provider) and the offline runner never runs it",
            ),
            Refusal::NotRuletraceAi => f.write_str("a live step must run ruletrace-ai"),
            Refusal::UnreadableArguments(p) => write!(f, "arguments cannot be read: {p}"),
            Refusal::NoConsent => {
                f.write_str("the step names no document with --consent; nothing is transmitted")
            }
            Refusal::NoStore => f.write_str("the step names no --store"),
            Refusal::UnreadableConfig(p) => write!(f, "configuration cannot be checked: {p}"),
            Refusal::ForbiddenOutput {
                option,
                path,
                protected,
            } => write!(
                f,
                "{option} '{path}' lies under '{protected}', which a live run never writes"
            ),
            Refusal::UnresolvablePath {
                option,
                path,
                problem,
            } => write!(f, "{option} '{path}' cannot be resolved: {problem}"),
            Refusal::PlaceholderModel(field) => write!(
                f,
                "field '{field}' is the placeholder REPLACE_WITH_YOUR_MODEL_ID, not a model"
            ),
            Refusal::EnvironmentNotAllowed(n) => write!(
                f,
                "environment variable '{n}' is not the credential variable of the configuration"
            ),
        }
    }
}

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
    /// The live-step guard refused a step before any process was started.
    LiveRefused {
        /// The step identifier.
        step: String,
        /// The reason.
        refusal: Refusal,
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
            DemoError::LiveRefused { step, refusal } => {
                write!(f, "step '{step}': refused before it started: {refusal}")
            }
            DemoError::BlessRefused => {
                f.write_str("RT_DEMO_BLESS=1 is refused because the CI variable is set")
            }
            DemoError::Io { path, problem } => write!(f, "file '{path}': {problem}"),
        }
    }
}

impl std::error::Error for DemoError {}
