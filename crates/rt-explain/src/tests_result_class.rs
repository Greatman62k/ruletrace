//! Tests of `ResultClass` and `class_of_run`, from the exit codes of
//! Req 31.12 (0, 4, 2 and 70) and the order of `classify_evaluation`.
//! RN-38 / UD-40 section 3: an Error Diagnostic is never hidden by an
//! Indeterminate Outcome, and the class is Indeterminate.

use crate::{class_of_run, ResultClass, Verification};
use rt_foundation::{Code, Diagnostic, DiagnosticState, Location};
use rt_numeric::parse_literal;
use rt_trace::{Outcome, Reason, TraceValue};

fn state(codes: &[Code]) -> DiagnosticState {
    let mut s = DiagnosticState::empty();
    for c in codes {
        s.push(Diagnostic::new(*c, "test".to_owned(), Location::None(*c)));
    }
    s
}

fn decided() -> Outcome {
    Outcome::Decided(TraceValue::Number(parse_literal("1").unwrap()))
}

fn indeterminate() -> Outcome {
    Outcome::indeterminate(vec![Reason::Diagnosed(Code::ExeRankMissingValue)])
}

const NO_VERIFICATIONS: &[Verification] = &[];

#[test]
fn a_clean_run_is_complete() {
    assert_eq!(
        ResultClass::of(&[decided()], &DiagnosticState::empty(), NO_VERIFICATIONS),
        ResultClass::Complete
    );
}

#[test]
fn an_indeterminate_outcome_is_indeterminate() {
    assert_eq!(
        ResultClass::of(
            &[decided(), indeterminate()],
            &DiagnosticState::empty(),
            NO_VERIFICATIONS
        ),
        ResultClass::Indeterminate
    );
}

#[test]
fn an_error_diagnostic_beside_an_indeterminate_outcome_is_indeterminate() {
    let errors = state(&[Code::AggregateEmptyCollection]);
    assert!(errors.has_errors());
    assert_eq!(
        ResultClass::of(&[indeterminate()], &errors, NO_VERIFICATIONS),
        ResultClass::Indeterminate
    );
}

#[test]
fn an_error_diagnostic_alone_is_indeterminate_and_a_warning_alone_is_complete() {
    assert_eq!(
        ResultClass::of(
            &[decided()],
            &state(&[Code::AggregateEmptyCollection]),
            NO_VERIFICATIONS
        ),
        ResultClass::Indeterminate
    );
    assert_eq!(
        ResultClass::of(
            &[decided()],
            &state(&[Code::ValConflictResolved]),
            NO_VERIFICATIONS
        ),
        ResultClass::Complete
    );
}

#[test]
fn a_question_fault_is_invalid_input_even_beside_an_indeterminate_outcome() {
    assert_eq!(
        ResultClass::of(
            &[indeterminate()],
            &state(&[Code::QuestionInvalid]),
            NO_VERIFICATIONS
        ),
        ResultClass::InvalidInput
    );
}

#[test]
fn of_never_returns_internal() {
    for outcomes in [vec![], vec![decided()], vec![indeterminate()]] {
        for codes in [
            vec![],
            vec![Code::QuestionInvalid],
            vec![Code::AggregateEmptyCollection],
        ] {
            assert_ne!(
                ResultClass::of(&outcomes, &state(&codes), NO_VERIFICATIONS),
                ResultClass::Internal
            );
        }
    }
}

#[test]
fn a_report_that_was_not_built_is_internal() {
    assert_eq!(
        class_of_run(
            &[decided()],
            &DiagnosticState::empty(),
            NO_VERIFICATIONS,
            false
        ),
        ResultClass::Internal
    );
    assert_eq!(
        class_of_run(
            &[indeterminate()],
            &state(&[Code::AggregateEmptyCollection]),
            NO_VERIFICATIONS,
            false
        ),
        ResultClass::Internal
    );
}

#[test]
fn a_report_that_was_not_built_for_a_question_fault_is_invalid_input() {
    assert_eq!(
        class_of_run(
            &[],
            &state(&[Code::QuestionInvalid]),
            NO_VERIFICATIONS,
            false
        ),
        ResultClass::InvalidInput
    );
}

#[test]
fn a_built_report_takes_the_class_of_its_outcomes_and_diagnostics() {
    assert_eq!(
        class_of_run(
            &[decided()],
            &DiagnosticState::empty(),
            NO_VERIFICATIONS,
            true
        ),
        ResultClass::Complete
    );
    assert_eq!(
        class_of_run(
            &[indeterminate()],
            &state(&[Code::AggregateEmptyCollection]),
            NO_VERIFICATIONS,
            true
        ),
        ResultClass::Indeterminate
    );
    assert_eq!(
        class_of_run(
            &[],
            &state(&[Code::QuestionInvalid]),
            NO_VERIFICATIONS,
            true
        ),
        ResultClass::InvalidInput
    );
}
