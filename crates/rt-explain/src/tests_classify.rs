//! The shared exit class against a truth table written from the
//! requirements, not from the production code (Req 16.9, 16.11, 16.14,
//! 16.17, 16.23 and the `ruletrace evaluate` module documentation):
//!
//! 1. a Question the inputs cannot answer is invalid input (exit 2);
//! 2. otherwise an Indeterminate Outcome or an engine Error is exit 4, and
//!    exit 4 outranks a provenance Error (Req 16.23);
//! 3. otherwise a provenance Error is exit 2;
//! 4. otherwise exit 0. Warnings never decide.

use crate::{classify_evaluation, has_question_fault, verify_trace, EvaluationClass, Verification};
use proptest::prelude::*;
use rt_foundation::{Code, Diagnostic, DiagnosticState, DocumentId, Location};
use rt_foundation::{RevisionNumber, RuleId};
use rt_numeric::parse_literal;
use rt_srcmodel::SourceModels;
use rt_trace::{
    DecisionTrace, DocumentHash, Outcome, QuestionKind, Reason, TraceParts, TraceQuestion,
    TraceRule, TraceSpan, TraceValue,
};

use EvaluationClass::{Indeterminate as Four, InvalidInput as Two, Success as Zero};

/// (question fault, indeterminate Outcome, engine Error, provenance Error)
/// and the class, all sixteen rows.
const TABLE: [(bool, bool, bool, bool, EvaluationClass); 16] = [
    (false, false, false, false, Zero),
    (false, false, false, true, Two),
    (false, false, true, false, Four),
    (false, false, true, true, Four),
    (false, true, false, false, Four),
    (false, true, false, true, Four),
    (false, true, true, false, Four),
    (false, true, true, true, Four),
    (true, false, false, false, Two),
    (true, false, false, true, Two),
    (true, false, true, false, Two),
    (true, false, true, true, Two),
    (true, true, false, false, Two),
    (true, true, false, true, Two),
    (true, true, true, false, Two),
    (true, true, true, true, Two),
];

fn expected(fault: bool, indeterminate: bool, engine: bool, provenance: bool) -> EvaluationClass {
    TABLE
        .iter()
        .find(|row| (row.0, row.1, row.2, row.3) == (fault, indeterminate, engine, provenance))
        .map(|row| row.4)
        .unwrap()
}

fn diagnostic(code: Code) -> Diagnostic {
    Diagnostic::new(code, "test".to_owned(), Location::None(code))
}

fn state(codes: &[Code]) -> DiagnosticState {
    let mut s = DiagnosticState::empty();
    for c in codes {
        s.push(diagnostic(*c));
    }
    s
}

fn decided() -> Outcome {
    Outcome::Decided(TraceValue::Number(parse_literal("1").unwrap()))
}

fn indeterminate() -> Outcome {
    Outcome::indeterminate(vec![Reason::Diagnosed(Code::ExeRankMissingValue)])
}

fn trace() -> DecisionTrace {
    let span = TraceSpan {
        document: DocumentId::new("rules.md").unwrap(),
        sha256: DocumentHash::new(&"ab".repeat(32)).unwrap(),
        start: 0,
        end: 7,
        line: 1,
        column: 1,
    };
    DecisionTrace::new(TraceParts {
        question: TraceQuestion {
            kind: QuestionKind::Value,
            target: "elig.age".to_owned(),
            participant: Some("p1".to_owned()),
        },
        outcome: decided(),
        rules: vec![TraceRule {
            rule: RuleId::new("elig.age").unwrap(),
            revision: RevisionNumber::new(1).unwrap(),
            spans: vec![span],
        }],
        entries: Vec::new(),
        not_applied: Vec::new(),
        missing: Vec::new(),
        diagnostics: DiagnosticState::empty(),
    })
    .unwrap()
}

/// A verification with the Warning of a missing Source_Model set, or with
/// the Error of a span whose document has no model.
fn verification(error: bool) -> Verification {
    if error {
        let mut other = SourceModels::new();
        other
            .insert(
                rt_srcmodel::SourceModel::new(
                    DocumentId::new("other.md").unwrap(),
                    rt_srcmodel::DocumentKind::Markdown,
                    "x".to_owned(),
                    Vec::new(),
                )
                .unwrap(),
            )
            .unwrap();
        verify_trace(&trace(), &other)
    } else {
        verify_trace(&trace(), &SourceModels::new())
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    fault: bool,
    indeterminate_outcome: bool,
    engine: bool,
    provenance: bool,
    warning: bool,
    decided_around: usize,
    error_position: usize,
) -> EvaluationClass {
    let mut outcomes: Vec<Outcome> = (0..decided_around).map(|_| decided()).collect();
    if indeterminate_outcome {
        outcomes.insert(error_position % (outcomes.len() + 1), indeterminate());
    }
    let mut codes = Vec::new();
    if fault {
        codes.push(Code::QuestionInvalid);
    }
    if engine {
        codes.push(Code::AggregateEmptyCollection);
    }
    if warning {
        codes.push(Code::ValConflictResolved);
    }
    let mut verifications: Vec<Verification> =
        (0..decided_around).map(|_| verification(false)).collect();
    if provenance {
        verifications.insert(
            error_position % (verifications.len() + 1),
            verification(true),
        );
    }
    classify_evaluation(&outcomes, &state(&codes), &verifications)
}

#[test]
fn the_fixture_diagnostics_have_the_severities_the_table_assumes() {
    assert!(state(&[Code::QuestionInvalid]).has_errors());
    assert!(state(&[Code::AggregateEmptyCollection]).has_errors());
    assert!(!state(&[Code::ValConflictResolved]).has_errors());
    assert!(verification(true).diagnostics().has_errors());
    assert!(!verification(false).diagnostics().as_slice().is_empty());
    assert!(!verification(false).diagnostics().has_errors());
}

#[test]
fn every_row_of_the_table_holds_with_and_without_a_warning() {
    for (fault, ind, eng, prov, class) in TABLE {
        for warning in [false, true] {
            assert_eq!(
                run(fault, ind, eng, prov, warning, 2, 1),
                class,
                "fault={fault} indeterminate={ind} engine={eng} provenance={prov} warning={warning}"
            );
        }
    }
}

#[test]
fn an_engine_error_is_exit_4_even_when_every_outcome_is_decided() {
    assert_eq!(run(false, false, true, false, false, 3, 0), Four);
    assert_eq!(run(false, false, true, true, false, 3, 0), Four);
}

#[test]
fn a_question_fault_is_detected_by_code_only() {
    assert!(has_question_fault(&state(&[
        Code::AggregateEmptyCollection,
        Code::QuestionInvalid
    ])));
    assert!(!has_question_fault(&state(&[
        Code::AggregateEmptyCollection,
        Code::ValConflictResolved
    ])));
    assert!(!has_question_fault(&DiagnosticState::empty()));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn the_class_equals_the_independent_table(
        fault in any::<bool>(),
        indeterminate_outcome in any::<bool>(),
        engine in any::<bool>(),
        provenance in any::<bool>(),
        warning in any::<bool>(),
        decided_around in 0usize..5,
        position in 0usize..8,
    ) {
        prop_assert_eq!(
            run(fault, indeterminate_outcome, engine, provenance, warning, decided_around, position),
            expected(fault, indeterminate_outcome, engine, provenance)
        );
    }
}
