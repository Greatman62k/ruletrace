//! RN-38 / UD-40 section 3 for bracket-check: an Indeterminate Outcome must not
//! conceal an Error Diagnostic of the evaluation. In Rule_IR version 1 a
//! missing input gives an Indeterminate Outcome together with a real engine
//! Error; the report built by the run lists that Error in its Diagnostics
//! section and the class is Indeterminate. This is a current-language
//! invariant, not a guarantee for later language versions.

use crate::tests_fixture::{full_request, run_with, sources, CSV_MISSING_JUDGES};
use crate::ResultClass;
use rt_engine::Outcome;
use rt_explain::report::ReportSection;
use rt_foundation::Severity;

#[test]
fn the_report_lists_the_engine_error_beside_the_indeterminate_outcome() {
    let run = run_with(CSV_MISSING_JUDGES, None, &full_request(), &sources()).unwrap();
    let evaluation = run.evaluation();
    assert!(evaluation
        .outcomes
        .iter()
        .any(|o| matches!(o, Outcome::Indeterminate(_))));
    let errors: Vec<_> = evaluation
        .diagnostics
        .as_slice()
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .cloned()
        .collect();
    assert!(
        !errors.is_empty(),
        "the missing input must give an engine Error"
    );
    let doc = run.report().unwrap();
    let Some(ReportSection::Diagnostics(listed)) = doc.sections().last() else {
        panic!("the last section is not Diagnostics");
    };
    for error in &errors {
        assert!(listed.contains(error), "{error:?} is not listed");
    }
    assert_eq!(run.class(), ResultClass::Indeterminate);
}
