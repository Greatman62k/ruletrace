//! Trace verification (Req 13.15 to 13.18, 20.5, 16.2): the Explainer runs over
//! every trace of every executable fixture with the Source_Models preloaded.
//! Each trace is verified, each span resolves to the exact bytes of the clause,
//! and without the models the same traces are not verified and no text is
//! invented.

use crate::load::{load, EXECUTABLE};
use rt_engine::evaluate;
use rt_explain::{verify_trace, SpanResolution, VerificationStatus};
use rt_srcmodel::SourceModels;

const CLAUSE_1: &str = "1. A competitor is eligible if age is at least 16 and holds a licence.";
const CLAUSE_2: &str =
    "2. The score is 0.6 times the judges score plus 0.4 times the technical score.";

#[test]
fn every_trace_of_every_fixture_verifies_against_the_preloaded_models() {
    for name in EXECUTABLE {
        let f = load(name);
        let evaluation = evaluate(&f.compiled, &f.data, &f.questions, None);
        assert_eq!(evaluation.traces.len(), f.questions.len(), "{name}");
        for (n, trace) in evaluation.traces.iter().enumerate() {
            let v = verify_trace(trace, &f.sources);
            assert_eq!(v.status(), VerificationStatus::Verified, "{name} trace {n}");
            assert!(v.diagnostics().as_slice().is_empty(), "{name} trace {n}");
            assert!(!v.spans().is_empty(), "{name} trace {n} has no span");
            for verdict in v.spans() {
                let text = verdict.text().expect("every span resolves");
                let expected = match verdict.rule.as_str() {
                    "elig.basic" => CLAUSE_1,
                    _ => CLAUSE_2,
                };
                assert_eq!(text, expected, "{name} trace {n} rule {}", verdict.rule);
            }
        }
    }
}

#[test]
fn without_models_the_same_traces_are_not_verified_and_carry_no_text() {
    let f = load("slice-basic");
    let evaluation = evaluate(&f.compiled, &f.data, &f.questions, None);
    let none = SourceModels::new();
    for trace in &evaluation.traces {
        let v = verify_trace(trace, &none);
        assert_eq!(v.status(), VerificationStatus::NotVerified);
        assert!(!v.diagnostics().as_slice().is_empty());
        for verdict in v.spans() {
            assert!(verdict.text().is_none());
            assert!(matches!(verdict.resolution, SpanResolution::Unresolved(_)));
        }
    }
}
