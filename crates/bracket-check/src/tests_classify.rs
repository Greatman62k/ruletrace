//! Tests of the result classification of the Tournament standings module
//! (Req 31.12). The derivation is `rt_explain::classify_evaluation`, tested in
//! `rt-explain`. These tests check the use made of it here: real runs, and the
//! class of a run whose report could not be built.

use crate::classify::class_of_run;
use crate::tests_fixture::{full_request, run_with, sources, CSV_FULL, CSV_MISSING_JUDGES};
use crate::ResultClass;
use rt_engine::{Outcome, Reason, TraceValue};
use rt_foundation::{Code, Diagnostic, DiagnosticState, DocumentId, Location};
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
        participant: "p2".to_owned(),
        field: "judges".to_owned(),
    }])
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
fn a_complete_run_is_complete() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    assert_eq!(run.class(), ResultClass::Complete);
}

#[test]
fn a_run_with_an_indeterminate_position_and_qualification_is_indeterminate() {
    let run = run_with(CSV_MISSING_JUDGES, None, &full_request(), &sources()).unwrap();
    assert_eq!(run.class(), ResultClass::Indeterminate);
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
        CSV_MISSING_JUDGES,
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
