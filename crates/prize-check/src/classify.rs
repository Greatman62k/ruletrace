//! The result classification of Req 31.12.
//!
//! The class exists for the Evaluate group and its tests, which compare it
//! with the exit code of `ruletrace evaluate` (0, 4, 2 and 70). The
//! Laboratory does not use it: it takes the class from the exit code of its
//! `ruletrace evaluate` subprocess (design §11.18.7).
//!
//! The derivation is `rt_explain::classify_evaluation`, the one function that
//! `ruletrace evaluate` also calls. This crate adds only `Internal`.

use rt_engine::Outcome;
use rt_explain::{classify_evaluation, has_question_fault, EvaluationClass, Verification};
use rt_foundation::DiagnosticState;

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
        match classify_evaluation(outcomes, diagnostics, verifications) {
            EvaluationClass::Success => Self::Complete,
            EvaluationClass::Indeterminate => Self::Indeterminate,
            EvaluationClass::InvalidInput => Self::InvalidInput,
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
