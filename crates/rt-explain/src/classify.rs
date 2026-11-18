//! The exit class of an executed evaluation, shared by `ruletrace evaluate`
//! and the application libraries (design D-12, Req 16.9, 16.11, 16.14, 16.17,
//! 16.23).
//!
//! The derivation follows the order of the stages: a Question the inputs
//! cannot answer, then an Indeterminate Outcome or an engine Error, then a
//! provenance Error. A Warning decides nothing.

use crate::Verification;
use rt_foundation::{Code, DiagnosticState};
use rt_trace::Outcome;

/// How an evaluation ended, before any failure to write or build output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvaluationClass {
    /// Exit code 0.
    Success,
    /// Exit code 4: an Indeterminate Outcome or an engine Error.
    Indeterminate,
    /// Exit code 2: a Question the inputs cannot answer, or a provenance
    /// Error.
    InvalidInput,
}

/// Whether the evaluation Diagnostics report a Question the inputs cannot
/// answer (`RT-EXE-0010`).
#[must_use]
pub fn has_question_fault(diagnostics: &DiagnosticState) -> bool {
    diagnostics
        .as_slice()
        .iter()
        .any(|d| d.code() == Code::QuestionInvalid)
}

/// Classifies an evaluation from its Outcomes, its own Diagnostics and the
/// verification of each trace.
#[must_use]
pub fn classify_evaluation(
    outcomes: &[Outcome],
    diagnostics: &DiagnosticState,
    verifications: &[Verification],
) -> EvaluationClass {
    let indeterminate = diagnostics.has_errors()
        || outcomes
            .iter()
            .any(|o| matches!(o, Outcome::Indeterminate(_)));
    let provenance = verifications.iter().any(|v| v.diagnostics().has_errors());
    if has_question_fault(diagnostics) {
        EvaluationClass::InvalidInput
    } else if indeterminate {
        EvaluationClass::Indeterminate
    } else if provenance {
        EvaluationClass::InvalidInput
    } else {
        EvaluationClass::Success
    }
}
