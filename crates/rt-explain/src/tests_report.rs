//! Tests for the results serializer and the Proof_Report (Req 33.14, 13.11,
//! 13.19, 31.5).

use crate::report::{
    build_proof_report, render_proof_human, render_proof_json, ApprovalFacts, ApprovalView,
    ProofError, ProofInputs, PublishedFact,
};
use crate::results::{results_value, serialize_results, ResultsError, ResultsInput};
use crate::{verify_trace, Verification};
use proptest::prelude::*;
use rt_foundation::{DiagnosticState, DocumentId, RevisionNumber, RuleId};
use rt_numeric::parse_literal;
use rt_srcmodel::{DocumentKind, Element, ElementKind, SourceModel, SourceModels};
use rt_trace::{
    replay, CompareCriterion, CompareEntry, CriterionResult, DecisionEntry, DecisionOp,
    DecisionTrace, DecisionValue, DocumentHash, Entry, MissingInput, Outcome, Place, QuestionKind,
    RankDirection, RankPlace, ReadEntry, StepEntry, StepOp, StepResult, TraceParts, TraceQuestion,
    TraceRule, TraceSpan, TraceValue,
};

const CONTENT: &str = "Entrants must be 16.\x1b[0m\nSecond line.";

fn model() -> SourceModel {
    let len = u32::try_from(CONTENT.len()).unwrap();
    SourceModel::new(
        DocumentId::new("rules.md").unwrap(),
        DocumentKind::Markdown,
        CONTENT.to_owned(),
        vec![Element {
            id: 0,
            kind: ElementKind::Paragraph,
            level: None,
            label: None,
            start: 0,
            end: len,
            line: 1,
            column: 1,
            parent: None,
        }],
    )
    .unwrap()
}

fn models() -> SourceModels {
    let mut m = SourceModels::new();
    m.insert(model()).unwrap();
    m
}

fn span(start: u32, end: u32) -> TraceSpan {
    TraceSpan {
        document: DocumentId::new("rules.md").unwrap(),
        sha256: DocumentHash::new(model().sha256().as_str()).unwrap(),
        start,
        end,
        line: 1,
        column: 1,
    }
}

fn num(text: &str) -> TraceValue {
    TraceValue::Number(parse_literal(text).unwrap())
}

fn rule() -> RuleId {
    RuleId::new("score.total").unwrap()
}

/// `a + b` with two reads, one step and one decision, plus a missing input of
/// another field.
fn sum_trace(a: i64, b: i64, outcome: Outcome, spans: Vec<TraceSpan>) -> DecisionTrace {
    let sum = a + b;
    DecisionTrace::new(TraceParts {
        question: TraceQuestion {
            kind: QuestionKind::Value,
            target: "score.total".to_owned(),
            participant: Some("p\u{7}1".to_owned()),
        },
        outcome,
        rules: vec![TraceRule {
            rule: rule(),
            revision: RevisionNumber::new(2).unwrap(),
            spans,
        }],
        entries: vec![
            Entry::Read(ReadEntry {
                name: "a".to_owned(),
                participant: Some("p1".to_owned()),
                value: num(&a.to_string()),
                rule: rule(),
                default_spans: Vec::new(),
            }),
            Entry::Read(ReadEntry {
                name: "b".to_owned(),
                participant: Some("p1".to_owned()),
                value: num(&b.to_string()),
                rule: rule(),
                default_spans: Vec::new(),
            }),
            Entry::Step(StepEntry {
                op: StepOp::Add,
                operands: vec![num(&a.to_string()), num(&b.to_string())],
                result: StepResult::Value(num(&sum.to_string())),
                rule: rule(),
                field: None,
            }),
            Entry::Decision(DecisionEntry {
                rule: rule(),
                op: DecisionOp::Pass,
                value: DecisionValue::Known(num(&sum.to_string())),
                from: vec![3],
            }),
        ],
        not_applied: Vec::new(),
        missing: vec![MissingInput {
            participant: "p1".to_owned(),
            field: "bonus".to_owned(),
        }],
        diagnostics: DiagnosticState::empty(),
    })
    .unwrap()
}

fn decided(a: i64, b: i64) -> Outcome {
    Outcome::Decided(num(&(a + b).to_string()))
}

fn facts() -> ApprovalFacts {
    let mut f = ApprovalFacts::new();
    f.insert(
        rule(),
        RevisionNumber::new(2).unwrap(),
        ApprovalView {
            reviewer: "A. Reviewer".to_owned(),
            time: "2026-11-14T09:00:00Z".to_owned(),
            content_hash: "9f2c".to_owned(),
        },
    );
    f
}

fn report_bytes(
    traces: &[DecisionTrace],
    vs: &[Verification],
    published: &[PublishedFact],
) -> (String, Vec<u8>) {
    let f = facts();
    let r = build_proof_report(&ProofInputs {
        traces,
        verifications: vs,
        approvals: &f,
        published,
        rulebook_hash: "sha256:abc",
        synthetic_data: Some(true),
    })
    .unwrap();
    (render_proof_human(&r), render_proof_json(&r))
}

#[test]
fn proof_report_sections_follow_req_33_14_order() {
    let t = sum_trace(2, 3, decided(2, 3), vec![span(0, 20)]);
    let v = verify_trace(&t, &models());
    let (human, json) = report_bytes(
        std::slice::from_ref(&t),
        std::slice::from_ref(&v),
        &[PublishedFact::Differs {
            published: "6".to_owned(),
            computed: "5".to_owned(),
        }],
    );
    let labels = [
        "DECISION 1",
        "rules applied",
        "approval",
        "source",
        "inputs read",
        "calculation steps",
        "tie-breaking",
        "missing inputs",
        "not applied",
        "verification",
        "published",
    ];
    let mut at = 0;
    for l in labels {
        let found = human[at..]
            .find(l)
            .unwrap_or_else(|| panic!("{l} out of order"));
        at += found + l.len();
    }
    assert!(human.contains("score.total @ revision 2"));
    assert!(
        human.contains("reviewer \"A. Reviewer\", time 2026-11-14T09:00:00Z, content hash 9f2c")
    );
    assert!(human.contains("\"Entrants must be 16.\""));
    assert!(human.contains("3. add(2, 3) -> 5"));
    assert!(human.contains("differs: published 6, computed 5"));
    assert!(human.contains("p\\u{7}1"));
    assert!(!human.contains('\u{1b}') && !human.contains('\u{7}'));
    let json = String::from_utf8(json).unwrap();
    let keys = [
        "\"question\"",
        "\"outcome\"",
        "\"rules\"",
        "\"sources\"",
        "\"inputs_read\"",
        "\"calculation_steps\"",
        "\"tie_breaking\"",
        "\"missing_inputs\"",
        "\"not_applied\"",
        "\"verification\"",
        "\"published\"",
    ];
    let mut at = 0;
    for k in keys {
        at += json[at..]
            .find(k)
            .unwrap_or_else(|| panic!("{k} out of order"))
            + k.len();
    }
    assert!(json.ends_with('\n'));
}

#[test]
fn unverified_trace_shows_no_source_text_and_missing_approval_is_stated() {
    let t = sum_trace(1, 1, decided(1, 1), vec![span(0, 9999)]);
    let v = verify_trace(&t, &models());
    let r = build_proof_report(&ProofInputs {
        traces: std::slice::from_ref(&t),
        verifications: std::slice::from_ref(&v),
        approvals: &ApprovalFacts::new(),
        published: &[],
        rulebook_hash: "h",
        synthetic_data: None,
    })
    .unwrap();
    let human = render_proof_human(&r);
    assert!(!human.contains("Entrants"));
    assert!(human.contains("verification       not verified"));
    assert!(human.contains("unresolved: RT-PRV-0002") || human.contains("unresolved: RT-PRV-"));
    assert!(human.contains("no approval record supplied"));
    assert!(human.contains("not compared for this question"));
    assert!(human.contains("data: not stated"));
    assert_eq!(r.header.verified_traces, 0);
}

#[test]
fn tie_group_is_reported_with_the_no_meaning_note() {
    let place = RankPlace::new(2, vec!["p9".to_owned(), "p7".to_owned()]).unwrap();
    let t = DecisionTrace::new(TraceParts {
        question: TraceQuestion {
            kind: QuestionKind::Rank,
            target: "rank.total".to_owned(),
            participant: Some("p7".to_owned()),
        },
        outcome: Outcome::Decided(TraceValue::Rank(place.clone())),
        rules: vec![TraceRule {
            rule: RuleId::new("rank.total").unwrap(),
            revision: RevisionNumber::new(1).unwrap(),
            spans: vec![span(0, 20)],
        }],
        entries: vec![Entry::Decision(DecisionEntry {
            rule: RuleId::new("rank.total").unwrap(),
            op: DecisionOp::Recorded,
            value: DecisionValue::Known(TraceValue::Rank(place)),
            from: Vec::new(),
        })],
        not_applied: Vec::new(),
        missing: Vec::new(),
        diagnostics: DiagnosticState::empty(),
    })
    .unwrap();
    let v = verify_trace(&t, &models());
    let (human, json) = report_bytes(std::slice::from_ref(&t), std::slice::from_ref(&v), &[]);
    assert!(human.contains("tie group at position 2: {p7, p9}"));
    assert!(human.contains("no ranking meaning"));
    assert!(String::from_utf8(json)
        .unwrap()
        .contains("\"members\":[\"p7\",\"p9\"]"));
}

#[test]
fn compare_entries_list_criteria_and_values_in_order() {
    let rk = RuleId::new("rank.total").unwrap();
    let crit = |index: u32, l: &str, r: &str, result: CriterionResult| CompareCriterion {
        index,
        rule: rk.clone(),
        direction: RankDirection::Desc,
        left: num(l),
        right: num(r),
        result,
    };
    let places = vec![
        Place::new(1, vec!["p3".to_owned()]).unwrap(),
        Place::new(2, vec!["p7".to_owned(), "p9".to_owned()]).unwrap(),
    ];
    let t = DecisionTrace::new(TraceParts {
        question: TraceQuestion {
            kind: QuestionKind::Rank,
            target: "standings".to_owned(),
            participant: None,
        },
        outcome: Outcome::Decided(TraceValue::Standings(places.clone())),
        rules: vec![TraceRule {
            rule: rk.clone(),
            revision: RevisionNumber::new(1).unwrap(),
            spans: vec![span(0, 20)],
        }],
        entries: vec![
            Entry::Compare(CompareEntry {
                between: ("p3".to_owned(), "p7".to_owned()),
                criteria: vec![
                    crit(0, "10", "10", CriterionResult::Equal),
                    crit(1, "5", "4", CriterionResult::First),
                ],
                first_ahead: true,
            }),
            Entry::Place(places[0].clone()),
            Entry::Place(places[1].clone()),
            Entry::Decision(DecisionEntry {
                rule: rk,
                op: DecisionOp::Recorded,
                value: DecisionValue::Known(TraceValue::Standings(places)),
                from: Vec::new(),
            }),
        ],
        not_applied: Vec::new(),
        missing: Vec::new(),
        diagnostics: DiagnosticState::empty(),
    })
    .unwrap();
    let v = verify_trace(&t, &models());
    let (human, json) = report_bytes(std::slice::from_ref(&t), std::slice::from_ref(&v), &[]);
    let c0 = human
        .find("criterion 0 (desc, rule rank.total): 10 vs 10 -> equal")
        .unwrap();
    let c1 = human
        .find("criterion 1 (desc, rule rank.total): 5 vs 4 -> p3 ahead")
        .unwrap();
    assert!(c0 < c1);
    assert!(human.contains("compare p3 ahead of p7"));
    assert!(human.contains("tie group at position 2: {p7, p9}"));
    assert!(human.contains("standings [1: p3; 2: p7, p9]"));
    assert!(!human.contains("none recorded"));
    let json = String::from_utf8(json).unwrap();
    assert!(json.contains("\"kind\":\"compare\""));
    assert!(json.contains("\"kind\":\"tie_group\""));
}

#[test]
fn input_length_mismatch_is_an_error() {
    let t = sum_trace(1, 2, decided(1, 2), vec![span(0, 20)]);
    let f = ApprovalFacts::new();
    let err = build_proof_report(&ProofInputs {
        traces: std::slice::from_ref(&t),
        verifications: &[],
        approvals: &f,
        published: &[],
        rulebook_hash: "h",
        synthetic_data: None,
    });
    assert!(matches!(err, Err(ProofError::LengthMismatch { .. })));
    let d = DiagnosticState::empty();
    let err = serialize_results(&ResultsInput {
        outcomes: &[decided(1, 2)],
        traces: &[],
        diagnostics: &d,
        verifications: &[],
    });
    assert!(matches!(err, Err(ResultsError::LengthMismatch { .. })));
    let t2 = sum_trace(1, 2, decided(1, 2), vec![span(0, 20)]);
    let v = verify_trace(&t2, &models());
    let err = serialize_results(&ResultsInput {
        outcomes: &[decided(9, 9)],
        traces: &[t2],
        diagnostics: &d,
        verifications: &[v],
    });
    assert_eq!(err, Err(ResultsError::OutcomeTraceMismatch { index: 0 }));
}

#[test]
fn results_outcomes_and_traces_do_not_depend_on_verification() {
    let t = sum_trace(2, 3, decided(2, 3), vec![span(0, 20)]);
    let good = verify_trace(&t, &models());
    let bad = verify_trace(&t, &SourceModels::new());
    let d = DiagnosticState::empty();
    let o = [decided(2, 3)];
    let build = |v: Verification| {
        results_value(&ResultsInput {
            outcomes: &o,
            traces: std::slice::from_ref(&t),
            diagnostics: &d,
            verifications: &[v],
        })
        .unwrap()
    };
    let (a, b) = (build(good), build(bad));
    for key in ["format", "version", "outcomes", "traces", "diagnostics"] {
        assert_eq!(a.get(key), b.get(key), "{key}");
    }
    assert_ne!(a.get("verification"), b.get("verification"));
    let text = rt_json::write_compact(&a);
    assert!(
        !text.contains("Entrants"),
        "results file carries no source text"
    );
}

proptest! {
    /// Same inputs give identical bytes in both forms and every step replays.
    #[test]
    fn proof_report_is_deterministic_and_replays(a in -1000i64..1000, b in -1000i64..1000) {
        let t = sum_trace(a, b, decided(a, b), vec![span(0, 20)]);
        let v = verify_trace(&t, &models());
        let first = report_bytes(std::slice::from_ref(&t), std::slice::from_ref(&v), &[]);
        let second = report_bytes(std::slice::from_ref(&t), std::slice::from_ref(&v), &[]);
        prop_assert_eq!(first, second);
        let r = build_proof_report(&ProofInputs {
            traces: std::slice::from_ref(&t),
            verifications: std::slice::from_ref(&v),
            approvals: &facts(),
            published: &[],
            rulebook_hash: "h",
            synthetic_data: None,
        }).unwrap();
        prop_assert_eq!(replay(&r.decisions[0].trace), Ok(decided(a, b)));
        let d = DiagnosticState::empty();
        let o = [decided(a, b)];
        let input = ResultsInput { outcomes: &o, traces: std::slice::from_ref(&t), diagnostics: &d, verifications: std::slice::from_ref(&v) };
        prop_assert_eq!(serialize_results(&input), serialize_results(&input));
    }
}
