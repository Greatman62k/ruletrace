//! RN-38 / UD-40 section 3 for bracket-check: an Indeterminate Outcome must not
//! conceal an Error Diagnostic of the evaluation. The report lists the Error
//! in its Diagnostics section and the class is Indeterminate.
//!
//! The Outcomes and traces come from a real run with a missing input. The
//! Error Diagnostic is added to the evaluation Diagnostics, as the engine
//! reports it beside an Indeterminate Outcome.

use crate::tests_fixture::{compiled, full_request, run_with, sources, CSV_MISSING_JUDGES};
use crate::{present, PresentInputs, ResultClass};
use rt_engine::Outcome;
use rt_explain::report::ReportSection;
use rt_foundation::{Code, Diagnostic, DiagnosticState, Location, Severity};

fn error_diagnostic() -> Diagnostic {
    Diagnostic::new(
        Code::AggregateEmptyCollection,
        "an aggregate over an empty collection".to_owned(),
        Location::None(Code::AggregateEmptyCollection),
    )
}

#[test]
fn the_report_lists_an_error_diagnostic_beside_an_indeterminate_outcome() {
    let run = run_with(CSV_MISSING_JUDGES, None, &full_request(), &sources()).unwrap();
    let evaluation = run.evaluation();
    assert!(evaluation
        .outcomes
        .iter()
        .any(|o| matches!(o, Outcome::Indeterminate(_))));
    let error = error_diagnostic();
    assert_eq!(error.severity(), Severity::Error);
    let mut diagnostics = DiagnosticState::empty();
    diagnostics.push(error.clone());
    let rulebook = compiled();
    let doc = present(&PresentInputs {
        compiled: &rulebook,
        outcomes: &evaluation.outcomes,
        traces: &evaluation.traces,
        verifications: run.verifications(),
        diagnostics: &diagnostics,
        comparison: None,
        not_requested: &[],
        input_hashes: &[],
        statements: &[],
        synthetic_data: None,
    })
    .unwrap();
    match doc.sections().last() {
        Some(ReportSection::Diagnostics(list)) => assert!(list.contains(&error)),
        other => panic!("last section is not Diagnostics: {other:?}"),
    }
    assert_eq!(
        ResultClass::of(&evaluation.outcomes, &diagnostics, run.verifications()),
        ResultClass::Indeterminate
    );
}
