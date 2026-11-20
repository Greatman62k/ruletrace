//! Tests of the result classification (Req 31.12). The order is the one of
//! Req 16.17: a Question the inputs cannot answer, then an Indeterminate
//! Outcome or an evaluation Error, then a provenance Error, and a Warning
//! decides nothing.

use crate::classify::class_of_run;
use crate::tests_fixture::{full_request, run_with, sources, CSV_FULL, CSV_MISSING_AGE};
use crate::ResultClass;
use rt_engine::{Outcome, Reason, TraceValue};
use rt_foundation::{Code, Diagnostic, DiagnosticState, DocumentId, Location, Severity};
use rt_srcmodel::{DocumentKind, SourceModel, SourceModels};

fn state(code: Code) -> DiagnosticState {
    let mut state = DiagnosticState::empty();
    state.push(Diagnostic::new(code, "x".to_owned(), Location::None(code)));
    state
}

fn decided() -> Outcome {
    Outcome::Decided(TraceValue::Bool(true))
}

fn indeterminate() -> Outcome {
    Outcome::indeterminate(vec![Reason::MissingInput {
        participant: "p1".to_owned(),
        field: "age".to_owned(),
    }])
}

#[test]
fn the_codes_used_below_have_the_severities_the_tests_assume() {
    let severity = |code| Diagnostic::new(code, String::new(), Location::None(code)).severity();
    assert_eq!(severity(Code::QuestionInvalid), Severity::Error);
    assert_eq!(severity(Code::ExecutionConflictDetected), Severity::Error);
    assert_eq!(severity(Code::ProvNotVerifiedNoSources), Severity::Warning);
}

#[test]
fn decided_outcomes_and_no_error_are_complete() {
    let none = DiagnosticState::empty();
    assert_eq!(
        ResultClass::of(&[decided(), decided()], &none, &[]),
        ResultClass::Complete
    );
}

#[test]
fn one_indeterminate_outcome_is_indeterminate() {
    let none = DiagnosticState::empty();
    assert_eq!(
        ResultClass::of(&[decided(), indeterminate()], &none, &[]),
        ResultClass::Indeterminate
    );
}

#[test]
fn an_evaluation_error_is_indeterminate_even_when_every_outcome_is_decided() {
    let found = state(Code::ExecutionConflictDetected);
    assert_eq!(
        ResultClass::of(&[decided()], &found, &[]),
        ResultClass::Indeterminate
    );
}

#[test]
fn a_warning_alone_does_not_change_the_class() {
    let found = state(Code::ProvNotVerifiedNoSources);
    assert_eq!(
        ResultClass::of(&[decided()], &found, &[]),
        ResultClass::Complete
    );
}

#[test]
fn a_question_the_inputs_cannot_answer_is_invalid_input_and_outranks_indeterminate() {
    let found = state(Code::QuestionInvalid);
    assert_eq!(
        ResultClass::of(&[indeterminate()], &found, &[]),
        ResultClass::InvalidInput
    );
}

fn models_without_the_cited_document() -> SourceModels {
    let model = SourceModel::new(
        DocumentId::new("other.md").unwrap(),
        DocumentKind::Markdown,
        "unrelated".to_owned(),
        Vec::new(),
    )
    .unwrap();
    let mut models = SourceModels::new();
    models.insert(model).unwrap();
    models
}

#[test]
fn a_provenance_error_is_invalid_input_and_still_carries_the_complete_report() {
    let run = run_with(
        CSV_FULL,
        None,
        &full_request(),
        &models_without_the_cited_document(),
    )
    .unwrap();
    assert!(run
        .verifications()
        .iter()
        .all(|v| v.diagnostics().has_errors()));
    assert_eq!(run.class(), ResultClass::InvalidInput);
    assert!(run.report().is_ok());
}

#[test]
fn an_indeterminate_outcome_outranks_a_provenance_error() {
    let run = run_with(
        CSV_MISSING_AGE,
        None,
        &full_request(),
        &models_without_the_cited_document(),
    )
    .unwrap();
    assert!(run
        .verifications()
        .iter()
        .any(|v| v.diagnostics().has_errors()));
    assert_eq!(run.class(), ResultClass::Indeterminate);
}

#[test]
fn a_report_that_could_not_be_built_is_an_internal_error() {
    let none = DiagnosticState::empty();
    assert_eq!(
        class_of_run(&[decided()], &none, &[], false),
        ResultClass::Internal
    );
    // An evaluation that was already Indeterminate also fails as internal,
    // because `ruletrace evaluate` writes nothing and exits 70 when its
    // results file cannot be built.
    assert_eq!(
        class_of_run(&[indeterminate()], &none, &[], false),
        ResultClass::Internal
    );
    assert_eq!(
        class_of_run(&[decided()], &none, &[], true),
        ResultClass::Complete
    );
}

#[test]
fn a_question_fault_stays_invalid_input_when_no_report_exists() {
    let found = state(Code::QuestionInvalid);
    assert_eq!(
        class_of_run(&[indeterminate()], &found, &[], false),
        ResultClass::InvalidInput
    );
}

#[test]
fn the_sources_of_the_fixture_verify_every_trace() {
    // Guards the premise of the other tests: with the committed Source_Model
    // there is no provenance error.
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    assert!(run
        .verifications()
        .iter()
        .all(|v| !v.diagnostics().has_errors()));
}
