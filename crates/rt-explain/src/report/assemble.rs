//! The report assembly that every Workflow_Module shares: consistency checks,
//! trace references, the Published and Diagnostics sections and the header.
//!
//! The functions take plain values only. Which section an entry belongs to
//! is the application's choice and arrives as a per-trace function.

use crate::report::model::{
    DifferenceFacts, EligibilityEntry, PublishedDifference, PublishedSection, PublishedUnused,
    QualificationEntry, ReportApplication, ReportDoc, ReportError, ReportHeader, ReportSection,
    ScoreEntry, Sha256Hex, StandingsTable, StatementId, TraceRef,
};
use crate::Verification;
use rt_foundation::{Diagnostic, DiagnosticState};
use rt_trace::{DecisionTrace, Outcome};
use std::collections::BTreeSet;
use std::fmt;

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

/// The entry an application built for one trace. The variant decides the
/// section; sections appear in a fixed order whatever the order of entries.
#[derive(Clone, Debug)]
pub enum SectionEntry {
    /// An eligibility predicate.
    Eligibility(EligibilityEntry),
    /// A derived value or weighted score.
    Score(ScoreEntry),
    /// A ranking.
    Standings(StandingsTable),
    /// A Qualification_Predicate.
    Qualification(QualificationEntry),
}

/// What the assembler reads, as plain values.
#[derive(Clone, Copy, Debug)]
pub struct AssembleInputs<'a> {
    /// The application the report belongs to.
    pub application: ReportApplication,
    /// One Outcome per Question, in Question order.
    pub outcomes: &'a [Outcome],
    /// The Decision_Trace of each Outcome, in the same order.
    pub traces: &'a [DecisionTrace],
    /// The Explainer's verification of each trace, in the same order.
    pub verifications: &'a [Verification],
    /// The Diagnostics of the evaluation itself.
    pub diagnostics: &'a DiagnosticState,
    /// The Compiled_Rulebook hash, as the text of its digest.
    pub rulebook_hash: &'a str,
    /// The SHA-256 of each input by role.
    pub input_hashes: &'a [(&'static str, Sha256Hex)],
    /// The statements the header shows.
    pub statements: &'a [StatementId],
    /// Whether the data is synthetic, as the caller states it. A label only.
    pub synthetic_data: Option<bool>,
}

/// The evaluation and verification Diagnostics of a run, de-duplicated and in
/// canonical order. An Error Diagnostic is listed whatever the Outcomes are.
fn run_diagnostics(
    diagnostics: &DiagnosticState,
    verifications: &[Verification],
) -> Vec<Diagnostic> {
    let mut all: BTreeSet<Diagnostic> = diagnostics.as_slice().iter().cloned().collect();
    for v in verifications {
        all.extend(v.diagnostics().as_slice().iter().cloned());
    }
    all.into_iter().collect()
}

/// Builds the Published section of a compared Published_Result from the
/// facts of each difference (with the name of its kind) and the unused rows
/// in the order they are to appear.
///
/// # Errors
/// [`PresentError::Report`] when a kind name is not in the table of kinds.
pub fn compared_section(
    differences: Vec<(&str, DifferenceFacts)>,
    unused: Vec<PublishedUnused>,
) -> Result<PublishedSection, PresentError> {
    let mut built = Vec::new();
    for (kind, facts) in differences {
        built.push(PublishedDifference::new(kind, facts)?);
    }
    Ok(PublishedSection::Compared {
        differences: built,
        unused,
    })
}

/// Assembles the report of a run.
///
/// `entry_of` receives the index, the trace and its [`TraceRef`] and returns
/// the entry of the application's section for that trace. `published` builds
/// the Published section after every trace was handled. Entries stay in the
/// order of the Outcomes. The header states provenance as verified only when
/// the Explainer verified every trace.
///
/// # Errors
/// [`PresentError`] when the lists are inconsistent, `entry_of` or
/// `published` fails, the rulebook hash is not a digest, or the report model
/// refuses an entry. No partial report is returned.
pub fn assemble_report<E, P>(
    inputs: &AssembleInputs<'_>,
    mut entry_of: E,
    published: P,
) -> Result<ReportDoc, PresentError>
where
    E: FnMut(usize, &DecisionTrace, &TraceRef<'_>) -> Result<SectionEntry, PresentError>,
    P: FnOnce() -> Result<PublishedSection, PresentError>,
{
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
    let mut eligibility = Vec::new();
    let mut scores = Vec::new();
    let mut standings = Vec::new();
    let mut qualification = Vec::new();
    let rows = inputs.traces.iter().zip(inputs.verifications);
    for (index, ((trace, verification), outcome)) in rows.zip(inputs.outcomes).enumerate() {
        if outcome != trace.outcome() {
            return Err(PresentError::OutcomeNotInTrace { trace: index });
        }
        let at = TraceRef::new(index, trace, verification)?;
        match entry_of(index, trace, &at)? {
            SectionEntry::Eligibility(e) => eligibility.push(e),
            SectionEntry::Score(e) => scores.push(e),
            SectionEntry::Standings(e) => standings.push(e),
            SectionEntry::Qualification(e) => qualification.push(e),
        }
    }
    let mut sections = Vec::new();
    if !eligibility.is_empty() {
        sections.push(ReportSection::Eligibility(eligibility));
    }
    if !scores.is_empty() {
        sections.push(ReportSection::Scores(scores));
    }
    if !standings.is_empty() {
        sections.push(ReportSection::Standings(standings));
    }
    if !qualification.is_empty() {
        sections.push(ReportSection::Qualification(qualification));
    }
    sections.push(ReportSection::Published(published()?));
    sections.push(ReportSection::Diagnostics(run_diagnostics(
        inputs.diagnostics,
        inputs.verifications,
    )));
    let header = ReportHeader {
        application: inputs.application,
        statements: inputs.statements.to_vec(),
        rulebook_hash: Sha256Hex::new(inputs.rulebook_hash).ok_or(PresentError::RulebookHash)?,
        input_hashes: inputs.input_hashes.to_vec(),
        synthetic_data: inputs.synthetic_data,
    };
    Ok(ReportDoc::new(header, inputs.verifications, sections)?)
}
