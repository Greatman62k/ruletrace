//! The result classification of Req 31.12.
//!
//! `ruletrace-lab` maps the four classes to the exit codes 0, 4, 2 and 70.
//! The derivation follows the order of `ruletrace evaluate`: a Question the
//! inputs cannot answer, then an Indeterminate Outcome or an engine Error,
//! then a provenance Error. A Warning decides nothing.

use rt_engine::Outcome;
use rt_explain::Verification;
use rt_foundation::{Code, DiagnosticState};

/// How a run ended. It has no stored validity flag: it is derived from the
/// Outcomes and Diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultClass {
    /// The report is complete and no Outcome is Indeterminate (exit code 0).
    Complete,
    /// At least one Outcome is Indeterminate, or the evaluation reported an
    /// Error (exit code 4).
    Indeterminate,
    /// Invalid input, or a provenance Error of the Explainer (exit code 2).
    InvalidInput,
    /// An internal error (exit code 70).
    Internal,
}

/// Whether the evaluation reports a Question that the inputs cannot answer.
pub(crate) fn has_question_fault(diagnostics: &DiagnosticState) -> bool {
    diagnostics
        .as_slice()
        .iter()
        .any(|d| d.code() == Code::QuestionInvalid)
}

impl ResultClass {
    /// Classifies an evaluation from its Outcomes, its own Diagnostics and
    /// the Explainer's verifications. Never `Internal`: that class comes from
    /// a failure to build the report.
    #[must_use]
    pub fn of(
        outcomes: &[Outcome],
        diagnostics: &DiagnosticState,
        verifications: &[Verification],
    ) -> Self {
        let indeterminate = diagnostics.has_errors()
            || outcomes
                .iter()
                .any(|o| matches!(o, Outcome::Indeterminate(_)));
        let provenance = verifications.iter().any(|v| v.diagnostics().has_errors());
        if has_question_fault(diagnostics) {
            Self::InvalidInput
        } else if indeterminate {
            Self::Indeterminate
        } else if provenance {
            Self::InvalidInput
        } else {
            Self::Complete
        }
    }
}

/// The classification of a run that reached the engine. A report that could
/// not be built is an internal error, except for a Question the inputs cannot
/// answer, which is invalid input and writes nothing, as `ruletrace evaluate`
/// does.
pub(crate) fn class_of_run(
    outcomes: &[Outcome],
    diagnostics: &DiagnosticState,
    verifications: &[Verification],
    report_built: bool,
) -> ResultClass {
    if !report_built && !has_question_fault(diagnostics) {
        ResultClass::Internal
    } else {
        ResultClass::of(outcomes, diagnostics, verifications)
    }
}
