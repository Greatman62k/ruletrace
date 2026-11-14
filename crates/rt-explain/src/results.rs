//! The results-file serializer (design §8.10, decision D-12, Req 31.5, 13.19).
//!
//! One function serves `ruletrace evaluate` and both applications, so byte
//! identity between them holds by construction. The file has three parts:
//!
//! * `outcomes` and `traces`, one entry per Question in Question order, taken
//!   from the engine unchanged;
//! * `diagnostics`, the Diagnostics of the evaluation;
//! * `verification`, one entry per trace with the status and the per-span
//!   result of the Explainer. It is a separate section: removing it leaves the
//!   bytes of the other parts as they would be without verification (Req
//!   13.19). It carries no source text.
//!
//! No timestamp, path, process identifier or environment value is written.

use crate::verify::{SpanResolution, Verification, VerificationStatus};
use rt_foundation::{CliLocation, Diagnostic, DiagnosticState, Location, RuleRef, Severity};
use rt_json::Value;
use rt_trace::{DecisionTrace, Outcome};
use std::fmt;

type Json = Value<()>;

/// Format identifier written at the top of every results file.
pub const RESULTS_FORMAT: &str = "ruletrace-results";
/// Version of the results format.
pub const RESULTS_VERSION: u32 = 1;

/// Everything the serializer reads. `outcomes[i]`, `traces[i]` and
/// `verifications[i]` answer the same Question. The slices are borrowed; the
/// engine's `Evaluation` supplies the first three.
#[derive(Clone, Copy, Debug)]
pub struct ResultsInput<'a> {
    /// One Outcome per Question.
    pub outcomes: &'a [Outcome],
    /// One Decision_Trace per Outcome.
    pub traces: &'a [DecisionTrace],
    /// The Diagnostics of the evaluation.
    pub diagnostics: &'a DiagnosticState,
    /// One verification per trace, from `verify_trace`.
    pub verifications: &'a [Verification],
}

/// Why the input cannot be serialized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultsError {
    /// The slices have different lengths.
    LengthMismatch {
        /// Number of Outcomes.
        outcomes: usize,
        /// Number of traces.
        traces: usize,
        /// Number of verifications.
        verifications: usize,
    },
    /// An Outcome differs from the Outcome recorded in the trace at the same
    /// index.
    OutcomeTraceMismatch {
        /// Index of the Question.
        index: usize,
    },
}

impl fmt::Display for ResultsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResultsError::LengthMismatch {
                outcomes,
                traces,
                verifications,
            } => write!(
                f,
                "results input has {outcomes} outcomes, {traces} traces and {verifications} verifications"
            ),
            ResultsError::OutcomeTraceMismatch { index } => {
                write!(f, "outcome {index} differs from the outcome in its trace")
            }
        }
    }
}

impl std::error::Error for ResultsError {}

/// Builds an object. Every call site uses distinct literal keys, so the
/// duplicate-key error cannot occur; a test would show a `null` if it did.
pub(crate) fn obj(members: Vec<(&str, Json)>) -> Json {
    Value::object(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
    .unwrap_or_else(|_| Value::null())
}

pub(crate) fn text(s: &str) -> Json {
    Value::string(s)
}

pub(crate) fn opt_text(s: Option<&str>) -> Json {
    s.map_or_else(Value::null, Value::string)
}

fn opt_int(n: Option<u32>) -> Json {
    n.map_or_else(Value::null, Value::integer)
}

fn location(l: &Location) -> Json {
    match l {
        Location::Source(s) => obj(vec![
            ("kind", text("source")),
            ("document", text(s.doc().as_str())),
            ("start", Value::integer(s.start().get())),
            ("end", Value::integer(s.end().get())),
            ("line", Value::integer(s.line())),
            ("column", Value::integer(s.column())),
        ]),
        Location::Document(d) => obj(vec![
            ("kind", text("document")),
            ("document", text(d.as_str())),
        ]),
        Location::Input(i) => obj(vec![
            ("kind", text("input")),
            ("input", text(i.input.as_str())),
            ("line", opt_int(i.line)),
            ("column", opt_int(i.column)),
            ("field", opt_text(i.field.as_deref())),
        ]),
        Location::Cli(CliLocation::Argument { position, name }) => obj(vec![
            ("kind", text("cli_argument")),
            ("position", Value::integer(*position)),
            ("name", opt_text(name.as_deref())),
        ]),
        Location::Cli(CliLocation::Command(c)) => {
            obj(vec![("kind", text("cli_command")), ("command", text(c))])
        }
        Location::None(code) => obj(vec![("kind", text("none")), ("reason", text(code.id()))]),
    }
}

fn rule_ref(r: &RuleRef) -> Json {
    obj(vec![
        ("rule", text(r.rule.as_str())),
        ("revision", opt_int(r.revision.map(|v| v.get()))),
    ])
}

pub(crate) fn severity_name(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Ambiguity => "ambiguity",
        Severity::Unsupported => "unsupported",
    }
}

fn diagnostic(d: &Diagnostic) -> Json {
    obj(vec![
        ("code", text(d.code().id())),
        ("severity", text(severity_name(d.severity()))),
        ("message", text(d.message())),
        ("primary", location(d.primary())),
        (
            "related",
            Value::array(d.related().iter().map(location).collect()),
        ),
        (
            "rules",
            Value::array(d.rules().iter().map(rule_ref).collect()),
        ),
    ])
}

pub(crate) fn diagnostics(state: &DiagnosticState) -> Json {
    Value::array(state.as_slice().iter().map(diagnostic).collect())
}

pub(crate) fn status_name(v: &Verification) -> &'static str {
    match v.status() {
        VerificationStatus::Verified => "verified",
        VerificationStatus::NotVerified => "not_verified",
    }
}

fn verification(index: usize, v: &Verification) -> Json {
    let spans = v
        .spans()
        .iter()
        .map(|s| {
            let (resolved, code) = match &s.resolution {
                SpanResolution::Resolved(_) => (true, Value::null()),
                SpanResolution::Unresolved(c) => (false, text(c.id())),
            };
            obj(vec![
                ("rule", text(s.rule.as_str())),
                ("revision", opt_int(s.revision.map(|r| r.get()))),
                ("document", text(s.span.document.as_str())),
                ("sha256", text(s.span.sha256.as_str())),
                ("start", Value::integer(s.span.start)),
                ("end", Value::integer(s.span.end)),
                ("line", Value::integer(s.span.line)),
                ("column", Value::integer(s.span.column)),
                ("resolved", Value::boolean(resolved)),
                ("code", code),
            ])
        })
        .collect();
    obj(vec![
        (
            "trace",
            Value::integer(u32::try_from(index).unwrap_or(u32::MAX)),
        ),
        ("status", text(status_name(v))),
        ("spans", Value::array(spans)),
        ("diagnostics", diagnostics(v.diagnostics())),
    ])
}

/// The canonical RT-JSON value of the results file. Keys: `format`, `version`,
/// `outcomes`, `traces`, `diagnostics`, `verification`.
///
/// # Errors
/// [`ResultsError`] when the slices disagree in length or an Outcome differs
/// from its trace.
pub fn results_value(input: &ResultsInput<'_>) -> Result<Value<()>, ResultsError> {
    let n = input.outcomes.len();
    if input.traces.len() != n || input.verifications.len() != n {
        return Err(ResultsError::LengthMismatch {
            outcomes: n,
            traces: input.traces.len(),
            verifications: input.verifications.len(),
        });
    }
    for (index, (o, t)) in input.outcomes.iter().zip(input.traces).enumerate() {
        if o.normalised() != *t.outcome() {
            return Err(ResultsError::OutcomeTraceMismatch { index });
        }
    }
    Ok(obj(vec![
        ("format", text(RESULTS_FORMAT)),
        ("version", Value::integer(RESULTS_VERSION)),
        (
            "outcomes",
            Value::array(input.outcomes.iter().map(Outcome::to_value).collect()),
        ),
        (
            "traces",
            Value::array(input.traces.iter().map(DecisionTrace::to_value).collect()),
        ),
        ("diagnostics", diagnostics(input.diagnostics)),
        (
            "verification",
            Value::array(
                input
                    .verifications
                    .iter()
                    .enumerate()
                    .map(|(i, v)| verification(i, v))
                    .collect(),
            ),
        ),
    ]))
}

/// The results file as compact canonical RT-JSON bytes with a trailing line
/// feed.
///
/// # Errors
/// See [`results_value`].
pub fn serialize_results(input: &ResultsInput<'_>) -> Result<Vec<u8>, ResultsError> {
    let mut s = rt_json::write_compact(&results_value(input)?);
    s.push('\n');
    Ok(s.into_bytes())
}

/// The results file as pretty canonical RT-JSON bytes (two-space indent,
/// trailing line feed).
///
/// # Errors
/// See [`results_value`].
pub fn serialize_results_pretty(input: &ResultsInput<'_>) -> Result<Vec<u8>, ResultsError> {
    Ok(rt_json::write_pretty(&results_value(input)?).into_bytes())
}
