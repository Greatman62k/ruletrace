//! `DiagnosticState` and the staged result types (Req 22, Req 7.19).
//!
//! The only stored diagnostic state is one ordered list. `has_errors`, `no_errors` and the
//! outcome are computed from that list on every call. No constructor, field or method accepts
//! them, so a contradictory state cannot be built.

use crate::{Code, Diagnostic};

/// Result of validation, derived from the diagnostic list. Never stored or accepted as input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValidationOutcome {
    /// No blocking diagnostic exists.
    Passed,
    /// At least one Error, Ambiguity or Unsupported diagnostic exists.
    Failed,
}

/// An ordered list of diagnostics. The private list is the single source of truth.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticState {
    items: Vec<Diagnostic>,
}

impl DiagnosticState {
    /// A state with no diagnostics.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Appends a diagnostic, keeping insertion order.
    pub fn push(&mut self, d: Diagnostic) {
        self.items.push(d);
    }

    /// Appends all diagnostics of `other`, keeping their order. Nothing is merged or dropped.
    pub fn extend(&mut self, other: DiagnosticState) {
        self.items.extend(other.items);
    }

    /// Returns the state in the canonical order of Req 7.10 and 7.22.
    #[must_use]
    pub fn sorted(mut self) -> Self {
        self.items.sort();
        self
    }

    /// The diagnostics in stored order.
    #[must_use]
    pub fn as_slice(&self) -> &[Diagnostic] {
        &self.items
    }

    /// True when any diagnostic is an Error, Ambiguity or Unsupported. Computed, never stored.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity().is_blocking())
    }

    /// The negation of [`DiagnosticState::has_errors`]. Computed, never stored.
    #[must_use]
    pub fn no_errors(&self) -> bool {
        !self.has_errors()
    }

    /// `Passed` when there is no blocking diagnostic, otherwise `Failed`.
    #[must_use]
    pub fn outcome(&self) -> ValidationOutcome {
        if self.has_errors() {
            ValidationOutcome::Failed
        } else {
            ValidationOutcome::Passed
        }
    }

    /// True when any diagnostic has the code `APPROVAL_UNRESOLVED`.
    #[must_use]
    pub fn has_unresolved_approval(&self) -> bool {
        self.items
            .iter()
            .any(|d| d.code() == Code::ApprovalUnresolved)
    }
}

/// A successful stage value with its non-blocking diagnostics. Only Warnings can be held.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staged<T> {
    value: T,
    diagnostics: DiagnosticState,
}

impl<T> Staged<T> {
    /// A success with no diagnostics.
    #[must_use]
    pub fn new(value: T) -> Self {
        Self {
            value,
            diagnostics: DiagnosticState::empty(),
        }
    }

    /// The stage value.
    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }

    /// The Warnings that accompany the value.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticState {
        &self.diagnostics
    }

    /// Splits into the value and its Warnings.
    #[must_use]
    pub fn into_parts(self) -> (T, DiagnosticState) {
        (self.value, self.diagnostics)
    }
}

/// A failed stage: a diagnostic list with at least one blocking item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejected {
    diagnostics: DiagnosticState,
}

impl Rejected {
    /// Builds a rejection. A list with no blocking diagnostic is handed back unchanged.
    pub fn new(diagnostics: DiagnosticState) -> Result<Self, DiagnosticState> {
        if diagnostics.has_errors() {
            Ok(Self { diagnostics })
        } else {
            Err(diagnostics)
        }
    }

    /// The diagnostics, at least one of which is blocking.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticState {
        &self.diagnostics
    }

    /// Takes the diagnostics.
    #[must_use]
    pub fn into_diagnostics(self) -> DiagnosticState {
        self.diagnostics
    }
}

/// The result of a pipeline stage.
pub type StageResult<T> = Result<Staged<T>, Rejected>;

/// Builds a stage result from a value and its diagnostics. A blocking diagnostic anywhere makes
/// the result a `Rejected` and drops the value, so a value never accompanies a blocking item.
pub fn stage_result<T>(value: T, diagnostics: DiagnosticState) -> StageResult<T> {
    match Rejected::new(diagnostics) {
        Ok(rejected) => Err(rejected),
        Err(warnings) => Ok(Staged {
            value,
            diagnostics: warnings,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Location;

    fn d(code: Code) -> Diagnostic {
        Diagnostic::new(code, String::new(), Location::None(Code::InternalError))
    }

    fn state(codes: &[Code]) -> DiagnosticState {
        let mut s = DiagnosticState::empty();
        for c in codes {
            s.push(d(*c));
        }
        s
    }

    #[test]
    fn empty_state_passes() {
        let s = DiagnosticState::empty();
        assert!(s.no_errors() && !s.has_errors());
        assert_eq!(s.outcome(), ValidationOutcome::Passed);
        assert!(!s.has_unresolved_approval());
    }

    #[test]
    fn warning_never_changes_flags() {
        let s = state(&[Code::ExtractNoClauses]);
        assert!(s.no_errors());
        assert_eq!(s.outcome(), ValidationOutcome::Passed);
    }

    #[test]
    fn each_blocking_severity_fails() {
        for c in [
            Code::IngestNotUtf8,
            Code::ValAmbiguousInterpretation,
            Code::IrUnknownField,
        ] {
            let s = state(&[Code::ExtractNoClauses, c]);
            assert!(s.has_errors() && !s.no_errors());
            assert_eq!(s.outcome(), ValidationOutcome::Failed);
        }
    }

    #[test]
    fn unresolved_approval_detected_and_blocks() {
        let s = state(&[Code::ApprovalUnresolved]);
        assert!(s.has_unresolved_approval());
        assert!(s.has_errors());
    }

    #[test]
    fn extend_keeps_order_and_duplicates() {
        let mut a = state(&[Code::IngestNotUtf8]);
        a.extend(state(&[Code::IngestNotUtf8, Code::ExtractNoClauses]));
        assert_eq!(a.as_slice().len(), 3);
        assert_eq!(a.as_slice()[2].code(), Code::ExtractNoClauses);
    }

    #[test]
    fn sorted_is_canonical_for_any_order() {
        let x = state(&[Code::IngestNotUtf8, Code::AiDisabled]).sorted();
        let y = state(&[Code::AiDisabled, Code::IngestNotUtf8]).sorted();
        assert_eq!(x, y);
    }

    #[test]
    fn stage_result_keeps_warnings_and_rejects_blocking() {
        let ok = stage_result(5, state(&[Code::ExtractNoClauses])).unwrap();
        assert_eq!(*ok.value(), 5);
        assert_eq!(ok.diagnostics().as_slice().len(), 1);
        let (v, w) = ok.into_parts();
        assert_eq!((v, w.as_slice().len()), (5, 1));

        let err =
            stage_result(5, state(&[Code::ExtractNoClauses, Code::IngestNotUtf8])).unwrap_err();
        assert!(err.diagnostics().has_errors());
        assert_eq!(err.diagnostics().as_slice().len(), 2);
        assert_eq!(Staged::new(1).diagnostics().as_slice().len(), 0);
    }

    #[test]
    fn rejected_requires_a_blocking_item() {
        let back = Rejected::new(state(&[Code::ExtractNoClauses])).unwrap_err();
        assert_eq!(back.as_slice().len(), 1);
        assert!(Rejected::new(DiagnosticState::empty()).is_err());
        assert!(Rejected::new(state(&[Code::IngestNotUtf8])).is_ok());
    }
}
