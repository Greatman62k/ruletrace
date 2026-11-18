//! RN-38 / UD-40 section 3 and UD-41 for bracket-check: an Indeterminate Outcome must
//! not conceal an Error Diagnostic of the evaluation. In Rule_IR version 1 a
//! missing input gives an Indeterminate Outcome together with real engine
//! Errors; the report built by the run lists those Errors in its Diagnostics
//! section beside the Indeterminate entry they belong to. This is a
//! current-language invariant, not a guarantee for later language versions.
//!
//! Count derived by hand from the fixture. `CSV_MISSING_JUDGES` leaves p2's
//! `judges` empty. `score.weighted` reads `judges`, so p2's score is unknown:
//! one (rule, participant, input) gives exactly one `ExeMissingValue` Error,
//! shared by the ranking and p2's qualification Question. The ranking key
//! (score) of p2 is then missing, which the engine reports as exactly one
//! further Error of a different code, `ExeRankMissingValue`, from
//! `rank.standing`. So two Errors in all, one of them `ExeMissingValue`.

use crate::tests_fixture::{full_request, run_with, sources, CSV_MISSING_JUDGES};
use crate::ResultClass;
use rt_engine::Outcome;
use rt_explain::report::{Missing, QualificationEntry, ReportSection, Truth};
use rt_foundation::{Code, Diagnostic, Severity};

fn errors_of(diagnostics: &[Diagnostic]) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .collect()
}

fn missing_value_message(rule: &str) -> String {
    format!("rule {rule}: required input 'judges' of participant 'p2' is a Missing_Value")
}

#[test]
fn the_report_lists_the_engine_error_beside_the_indeterminate_outcome() {
    let run = run_with(CSV_MISSING_JUDGES, None, &full_request(), &sources()).unwrap();
    let evaluation = run.evaluation();
    let errors = errors_of(evaluation.diagnostics.as_slice());
    assert_eq!(errors.len(), 2);
    let missing: Vec<&&Diagnostic> = errors
        .iter()
        .filter(|d| d.code() == Code::ExeMissingValue)
        .collect();
    assert_eq!(missing.len(), 1);
    assert_eq!(
        missing[0].message(),
        missing_value_message("score.weighted")
    );
    assert_eq!(missing[0].rules().len(), 1);
    assert_eq!(missing[0].rules()[0].rule.as_str(), "score.weighted");
    let others: Vec<Code> = errors
        .iter()
        .map(|d| d.code())
        .filter(|c| *c != Code::ExeMissingValue)
        .collect();
    assert_eq!(others, vec![Code::ExeRankMissingValue]);
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
    // The Indeterminate qualification entry of the same report names the same
    // participant and input as the Diagnostic.
    let Some(ReportSection::Qualification(entries)) = doc
        .sections()
        .iter()
        .find(|s| matches!(s, ReportSection::Qualification(_)))
    else {
        panic!("no Qualification section");
    };
    let open: Vec<&QualificationEntry> = entries
        .iter()
        .filter(|e| matches!(e.result(), Truth::Indeterminate(_)))
        .collect();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].participant(), "p2");
    assert_eq!(open[0].target(), "qualified");
    assert_eq!(
        open[0].result(),
        &Truth::Indeterminate(vec![Missing::Input {
            participant: "p2".to_owned(),
            field: "judges".to_owned()
        }])
    );
    assert!(evaluation
        .outcomes
        .iter()
        .any(|o| matches!(o, Outcome::Indeterminate(_))));
    assert_eq!(run.class(), ResultClass::Indeterminate);
}
