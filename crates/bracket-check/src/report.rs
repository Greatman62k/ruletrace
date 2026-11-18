//! The Present group: the Tournament standings report from a
//! Compiled_Rulebook and the Outcomes, Decision_Traces and verifications of an
//! evaluation (design §8.5, §8.7, Req 32.1, 32.2, 32.6, 31.7, 31.13, 31.14).
//!
//! The inputs are typed values. `Evaluate` passes what the engine returned,
//! and the results-file reader of the Laboratory passes what it parsed. The
//! module copies every value into the report model of `rt-explain` and
//! computes, compares and orders nothing. Entries follow the order of the
//! Outcomes.

use crate::mapping::target_kinds_by_name;
use rt_compiled::CompiledRulebook;
use rt_data::tabular::{TargetKind, UnusedPublished};
use rt_data::PublishedValue;
use rt_engine::{Comparison, Computed, DecisionTrace, Difference, Outcome, TraceValue};
use rt_explain::report::{
    DifferenceFacts, PublishedDifference, PublishedSection, PublishedUnused, QualificationEntry,
    ReportApplication, ReportDoc, ReportError, ReportHeader, ReportSection, Sha256Hex,
    StandingsTable, StatementId, TraceRef,
};
use rt_explain::Verification;
use rt_foundation::{Diagnostic, DiagnosticState};
use rt_numeric::to_canonical;
use std::collections::BTreeSet;
use std::fmt;

/// Everything Present reads.
#[derive(Clone, Copy, Debug)]
pub struct PresentInputs<'a> {
    /// The Compiled_Rulebook the evaluation ran on. It gives the report its
    /// hash and the kind of each Target.
    pub compiled: &'a CompiledRulebook,
    /// One Outcome per Question, in Question order.
    pub outcomes: &'a [Outcome],
    /// The Decision_Trace of each Outcome, in the same order.
    pub traces: &'a [DecisionTrace],
    /// The Explainer's verification of each trace, in the same order.
    pub verifications: &'a [Verification],
    /// The Diagnostics of the evaluation itself.
    pub diagnostics: &'a DiagnosticState,
    /// The comparison with the Published_Result, when one was supplied.
    pub comparison: Option<&'a Comparison>,
    /// Published rows for Targets that were not requested.
    pub not_requested: &'a [UnusedPublished],
    /// The SHA-256 of each input by role.
    pub input_hashes: &'a [(&'static str, Sha256Hex)],
    /// The statements the header shows.
    pub statements: &'a [StatementId],
    /// Whether the data is synthetic, as the caller states it. A label only.
    pub synthetic_data: Option<bool>,
}

/// Why no report could be built. It is an internal error of the caller or
/// the engine, never a property of the user's input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresentError {
    /// The Outcomes, traces and verifications are not one list each of the
    /// same length.
    CountMismatch {
        /// Number of Outcomes.
        outcomes: usize,
        /// Number of Decision_Traces.
        traces: usize,
        /// Number of verifications.
        verifications: usize,
    },
    /// An Outcome differs from the Outcome inside its trace.
    OutcomeNotInTrace {
        /// Index of the trace.
        trace: usize,
    },
    /// The trace answers a Target that the Compiled_Rulebook does not declare
    /// with a reportable kind.
    TargetNotReportable {
        /// Index of the trace.
        trace: usize,
    },
    /// A published difference carries a computed value that is neither a
    /// number nor a Boolean.
    ComputedValueNotComparable {
        /// Index of the Question.
        question: usize,
    },
    /// The Compiled_Rulebook hash is not a valid digest.
    RulebookHash,
    /// The report model refused an entry or the document.
    Report(ReportError),
}

impl fmt::Display for PresentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CountMismatch {
                outcomes,
                traces,
                verifications,
            } => write!(
                f,
                "{outcomes} Outcomes, {traces} traces and {verifications} verifications are not one list"
            ),
            Self::OutcomeNotInTrace { trace } => {
                write!(f, "the Outcome of trace {trace} differs from its trace")
            }
            Self::TargetNotReportable { trace } => {
                write!(f, "the Target of trace {trace} cannot be reported")
            }
            Self::ComputedValueNotComparable { question } => {
                write!(f, "the computed value of Question {question} is not a number or a Boolean")
            }
            Self::RulebookHash => f.write_str("the Compiled_Rulebook hash is not a valid digest"),
            Self::Report(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PresentError {}

impl From<ReportError> for PresentError {
    fn from(e: ReportError) -> Self {
        Self::Report(e)
    }
}

fn published_text(value: &PublishedValue) -> String {
    match value {
        PublishedValue::Bool(b) => b.to_string(),
        PublishedValue::Number(n) => to_canonical(n.value()),
    }
}

fn computed_text(difference: &Difference) -> Result<Option<String>, PresentError> {
    match &difference.computed {
        Computed::Indeterminate => Ok(None),
        Computed::Value(TraceValue::Number(n)) => Ok(Some(to_canonical(n))),
        Computed::Value(TraceValue::Bool(b)) => Ok(Some(b.to_string())),
        Computed::Value(_) => Err(PresentError::ComputedValueNotComparable {
            question: difference.question,
        }),
    }
}

fn published_section(
    comparison: Option<&Comparison>,
    not_requested: &[UnusedPublished],
) -> Result<PublishedSection, PresentError> {
    let Some(comparison) = comparison else {
        return Ok(PublishedSection::NotSupplied);
    };
    let mut differences = Vec::new();
    for d in &comparison.differences {
        let facts = DifferenceFacts {
            participant: d.participant.as_str().to_owned(),
            target: d.target.as_str().to_owned(),
            published: published_text(&d.published),
            computed: computed_text(d)?,
            trace: d.question,
            entry: d.entry,
            non_terminating: d.non_terminating,
            group: d.group.iter().map(|p| p.as_str().to_owned()).collect(),
        };
        differences.push(PublishedDifference::new(d.kind.as_str(), facts)?);
    }
    let engine_unused = comparison.unused.iter().map(|(p, t)| PublishedUnused {
        participant: p.as_str().to_owned(),
        target: t.as_str().to_owned(),
    });
    let file_unused = not_requested.iter().map(|u| PublishedUnused {
        participant: u.participant.as_str().to_owned(),
        target: u.target.as_str().to_owned(),
    });
    Ok(PublishedSection::Compared {
        differences,
        unused: engine_unused.chain(file_unused).collect(),
    })
}

fn run_diagnostics(inputs: &PresentInputs<'_>) -> Vec<Diagnostic> {
    let mut all: BTreeSet<Diagnostic> = inputs.diagnostics.as_slice().iter().cloned().collect();
    for v in inputs.verifications {
        all.extend(v.diagnostics().as_slice().iter().cloned());
    }
    all.into_iter().collect()
}

/// Builds the Tournament standings report.
///
/// Each rank trace becomes one Standings table with its tie-breaking
/// comparisons, and each value trace of an eligibility Target becomes one
/// Qualification entry. Entries stay in the order of the Outcomes. The header
/// states provenance as verified only when the Explainer verified every
/// trace, and states that nothing was verified when no Source_Model was
/// supplied.
///
/// # Errors
/// [`PresentError`] when the lists are inconsistent, a trace answers a Target
/// that cannot be reported, or the report model refuses an entry. No partial
/// report is returned.
pub fn present(inputs: &PresentInputs<'_>) -> Result<ReportDoc, PresentError> {
    let (n_out, n_trace, n_ver) = (
        inputs.outcomes.len(),
        inputs.traces.len(),
        inputs.verifications.len(),
    );
    if n_out != n_trace || n_trace != n_ver {
        return Err(PresentError::CountMismatch {
            outcomes: n_out,
            traces: n_trace,
            verifications: n_ver,
        });
    }
    let kinds = target_kinds_by_name(inputs.compiled);
    let mut standings = Vec::new();
    let mut qualification = Vec::new();
    let rows = inputs.traces.iter().zip(inputs.verifications);
    for (index, ((trace, verification), outcome)) in rows.zip(inputs.outcomes).enumerate() {
        if outcome != trace.outcome() {
            return Err(PresentError::OutcomeNotInTrace { trace: index });
        }
        let at = TraceRef::new(index, trace, verification)?;
        match kinds.get(&trace.question().target) {
            Some(TargetKind::Ranking) => standings.push(StandingsTable::from_trace(&at, true)?),
            Some(TargetKind::Eligibility) => {
                qualification.push(QualificationEntry::from_trace(&at)?);
            }
            Some(TargetKind::Derived | TargetKind::Score | TargetKind::Other) | None => {
                return Err(PresentError::TargetNotReportable { trace: index });
            }
        }
    }
    let mut sections = Vec::new();
    if !standings.is_empty() {
        sections.push(ReportSection::Standings(standings));
    }
    if !qualification.is_empty() {
        sections.push(ReportSection::Qualification(qualification));
    }
    sections.push(ReportSection::Published(published_section(
        inputs.comparison,
        inputs.not_requested,
    )?));
    sections.push(ReportSection::Diagnostics(run_diagnostics(inputs)));
    let header = ReportHeader {
        application: ReportApplication::BracketCheck,
        statements: inputs.statements.to_vec(),
        rulebook_hash: Sha256Hex::new(inputs.compiled.rulebook_hash().as_str())
            .ok_or(PresentError::RulebookHash)?,
        input_hashes: inputs.input_hashes.to_vec(),
        synthetic_data: inputs.synthetic_data,
    };
    Ok(ReportDoc::new(header, inputs.verifications, sections)?)
}
