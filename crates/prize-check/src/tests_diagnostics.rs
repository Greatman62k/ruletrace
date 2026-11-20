//! RN-38 / UD-40 section 3 and UD-41 for prize-check: an Indeterminate Outcome must
//! not conceal an Error Diagnostic of the evaluation. In Rule_IR version 1 a
//! missing input gives an Indeterminate Outcome together with real engine
//! Errors; the report built by the run lists those Errors in its Diagnostics
//! section beside the Indeterminate entry they belong to. This is a
//! current-language invariant, not a guarantee for later language versions.
//!
//! Count derived by hand from the fixture. `CSV_MISSING_AGE` leaves p2's `age`
//! empty. Only rule `elig.basic` (`age >= 16 and licence`) reads `age`;
//! `score.weighted` and `rank.standing` read `judges` and `technical`, which
//! are present. So exactly one (rule, participant, input) is missing, giving
//! exactly one `ExeMissingValue` Error, however many Questions depend on it.

use crate::tests_fixture::{full_request, run_with, sources, CSV_MISSING_AGE};
use crate::ResultClass;
use rt_engine::Outcome;
use rt_explain::report::{EligibilityEntry, Missing, ReportSection, Truth};
use rt_foundation::{Code, Diagnostic, Severity};

fn errors_of(diagnostics: &[Diagnostic]) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .collect()
}

fn missing_value_message(rule: &str) -> String {
    format!("rule {rule}: required input 'age' of participant 'p2' is a Missing_Value")
}

#[test]
fn the_report_lists_the_engine_error_beside_the_indeterminate_outcome() {
    let run = run_with(CSV_MISSING_AGE, None, &full_request(), &sources()).unwrap();
    let evaluation = run.evaluation();
    let errors = errors_of(evaluation.diagnostics.as_slice());
    assert_eq!(errors.len(), 1);
    assert!(errors.iter().all(|d| d.code() == Code::ExeMissingValue));
    assert_eq!(errors[0].message(), missing_value_message("elig.basic"));
    assert_eq!(errors[0].rules().len(), 1);
    assert_eq!(errors[0].rules()[0].rule.as_str(), "elig.basic");
    let doc = run.report().unwrap();
    let Some(ReportSection::Diagnostics(listed)) = doc.sections().last() else {
        panic!("the last section is not Diagnostics");
    };
    // The report lists exactly the evaluation's Errors, once each.
    assert_eq!(errors_of(listed), errors);
    assert_eq!(
        listed
            .iter()
            .filter(|d| d.code() == Code::ExeMissingValue)
            .count(),
        1
    );
    // The Indeterminate eligibility entry of the same report names the same
    // participant and input as the Diagnostic.
    let Some(ReportSection::Eligibility(entries)) = doc
        .sections()
        .iter()
        .find(|s| matches!(s, ReportSection::Eligibility(_)))
    else {
        panic!("no Eligibility section");
    };
    let open: Vec<&EligibilityEntry> = entries
        .iter()
        .filter(|e| matches!(e.result(), Truth::Indeterminate(_)))
        .collect();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].participant(), "p2");
    assert_eq!(open[0].target(), "eligible");
    assert_eq!(
        open[0].result(),
        &Truth::Indeterminate(vec![Missing::Input {
            participant: "p2".to_owned(),
            field: "age".to_owned()
        }])
    );
    assert!(evaluation
        .outcomes
        .iter()
        .any(|o| matches!(o, Outcome::Indeterminate(_))));
    assert_eq!(run.class(), ResultClass::Indeterminate);
}
