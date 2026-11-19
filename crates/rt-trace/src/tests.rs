//! Unit and property tests for rt-trace.

use super::*;
use crate::proptest_floor::property_tests;
use proptest::prelude::*;
use rt_foundation::{
    Code, Diagnostic, DiagnosticState, DocumentId, Location, RevisionNumber, RuleId,
};
use rt_numeric::{parse_literal, ExactNumber, RoundingMode};

fn num(text: &str) -> TraceValue {
    TraceValue::Number(parse_literal(text).unwrap())
}

fn rule_id(s: &str) -> RuleId {
    RuleId::new(s).unwrap()
}

fn span() -> TraceSpan {
    TraceSpan {
        document: DocumentId::new("rules.md").unwrap(),
        sha256: DocumentHash::new(&"ab".repeat(32)).unwrap(),
        start: 120,
        end: 180,
        line: 4,
        column: 1,
    }
}

fn question() -> TraceQuestion {
    TraceQuestion {
        kind: QuestionKind::Value,
        target: "elig.age".to_owned(),
        participant: Some("p1".to_owned()),
    }
}

fn read(name: &str, value: TraceValue) -> Entry {
    Entry::Read(ReadEntry {
        name: name.to_owned(),
        participant: Some("p1".to_owned()),
        value,
        rule: rule_id("elig.age"),
        default_spans: Vec::new(),
    })
}

fn step(op: StepOp, operands: Vec<TraceValue>, result: StepResult) -> Entry {
    Entry::Step(StepEntry {
        op,
        operands,
        result,
        rule: rule_id("elig.age"),
        field: None,
    })
}

fn decision(op: DecisionOp, value: DecisionValue, from: Vec<u32>) -> Entry {
    Entry::Decision(DecisionEntry {
        rule: rule_id("elig.age"),
        op,
        value,
        from,
    })
}

fn parts(entries: Vec<Entry>, outcome: Outcome) -> TraceParts {
    TraceParts {
        question: question(),
        outcome,
        rules: vec![TraceRule {
            rule: rule_id("elig.age"),
            revision: RevisionNumber::new(1).unwrap(),
            spans: vec![span()],
        }],
        entries,
        not_applied: Vec::new(),
        missing: Vec::new(),
        diagnostics: DiagnosticState::empty(),
    }
}

/// age 17, `17 >= 18` is false, decision false; weighted part 0.6 * 8.5 = 5.1.
fn hand_built() -> TraceParts {
    parts(
        vec![
            read("age", num("17")),
            step(
                StepOp::Ge,
                vec![num("17"), num("18")],
                StepResult::Value(TraceValue::Bool(false)),
            ),
            step(
                StepOp::Mul,
                vec![num("0.6"), num("8.5")],
                StepResult::Value(num("5.1")),
            ),
            decision(
                DecisionOp::Pass,
                DecisionValue::Known(TraceValue::Bool(false)),
                vec![2],
            ),
        ],
        Outcome::Decided(TraceValue::Bool(false)),
    )
}

fn build(p: TraceParts) -> DecisionTrace {
    DecisionTrace::new(p).unwrap()
}

#[test]
fn kleene_tables_match_design() {
    use Tri::{False as F, True as T, Unknown as U};
    let and = [
        (T, T, T),
        (T, F, F),
        (T, U, U),
        (F, T, F),
        (F, F, F),
        (F, U, F),
        (U, T, U),
        (U, F, F),
        (U, U, U),
    ];
    let or = [
        (T, T, T),
        (T, F, T),
        (T, U, T),
        (F, T, T),
        (F, F, F),
        (F, U, U),
        (U, T, T),
        (U, F, U),
        (U, U, U),
    ];
    for (a, b, r) in and {
        assert_eq!(a.and(b), r, "{a:?} AND {b:?}");
    }
    for (a, b, r) in or {
        assert_eq!(a.or(b), r, "{a:?} OR {b:?}");
    }
    assert_eq!((!T, !F, !U), (F, T, U));
    assert_eq!(Tri::from_bool(true).to_bool(), Some(true));
    assert_eq!(U.to_bool(), None);
}

fn tri() -> impl Strategy<Value = Tri> {
    prop_oneof![Just(Tri::True), Just(Tri::False), Just(Tri::Unknown)]
}

property_tests! {
    /// **Validates: Requirements 11.4**
    #[test]
    fn kleene_laws(a in tri(), b in tri(), c in tri()) {
        prop_assert_eq!(a.and(b), b.and(a));
        prop_assert_eq!(a.or(b), b.or(a));
        prop_assert_eq!(a.and(b).and(c), a.and(b.and(c)));
        prop_assert_eq!(a.or(b).or(c), a.or(b.or(c)));
        prop_assert_eq!(!(a.and(b)), (!a).or(!b));
        prop_assert_eq!(!(a.or(b)), (!a).and(!b));
        prop_assert_eq!(!!a, a);
        prop_assert_eq!(Tri::False.and(a), Tri::False);
        prop_assert_eq!(Tri::True.or(a), Tri::True);
    }
}

#[test]
fn replay_of_hand_built_trace_equals_recorded() {
    let trace = build(hand_built());
    assert_eq!(
        replay(&trace),
        Ok(Outcome::Decided(TraceValue::Bool(false)))
    );
}

#[test]
fn altered_operand_fails_replay() {
    let mut p = hand_built();
    if let Some(Entry::Step(s)) = p.entries.get_mut(1) {
        s.operands = vec![num("19"), num("18")];
    }
    match replay(&build(p)) {
        Err(ReplayError::StepMismatch { seq: 2, .. }) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn altered_decision_and_outcome_fail_replay() {
    let mut p = hand_built();
    if let Some(Entry::Decision(d)) = p.entries.get_mut(3) {
        d.value = DecisionValue::Known(TraceValue::Bool(true));
    }
    assert!(matches!(
        replay(&build(p)),
        Err(ReplayError::DecisionMismatch { seq: 4, .. })
    ));
    let mut p = hand_built();
    p.outcome = Outcome::Decided(TraceValue::Bool(true));
    assert!(matches!(
        replay(&build(p)),
        Err(ReplayError::OutcomeMismatch { .. })
    ));
}

#[test]
fn step_faults_and_empty_aggregates_replay() {
    let mut empty_min = StepEntry {
        op: StepOp::Min,
        operands: Vec::new(),
        result: StepResult::Undefined,
        rule: rule_id("r"),
        field: Some("scores".to_owned()),
    };
    let p = parts(
        vec![
            step(
                StepOp::Div,
                vec![num("1"), num("0")],
                StepResult::Fault(StepFault::DivisionByZero),
            ),
            Entry::Step(empty_min.clone()),
            step(StepOp::Sum, Vec::new(), StepResult::Value(num("0"))),
            step(StepOp::Count, Vec::new(), StepResult::Value(num("0"))),
            step(
                StepOp::Round {
                    places: 1,
                    mode: RoundingMode::HalfEven,
                },
                vec![num("0.25")],
                StepResult::Value(num("0.2")),
            ),
            decision(DecisionOp::Pass, DecisionValue::Unknown, vec![2]),
        ],
        Outcome::indeterminate(vec![Reason::EmptyAggregate {
            field: "scores".to_owned(),
        }]),
    );
    assert!(replay(&build(p.clone())).is_ok());
    empty_min.field = None;
    let mut bad = p;
    bad.entries[1] = Entry::Step(empty_min);
    assert_eq!(
        DecisionTrace::new(bad),
        Err(TraceError::UndefinedWithoutField { seq: 2 })
    );
}

#[test]
fn indeterminate_needs_supported_reasons() {
    let missing = MissingInput {
        participant: "p1".to_owned(),
        field: "age".to_owned(),
    };
    let reason = Reason::MissingInput {
        participant: "p1".to_owned(),
        field: "age".to_owned(),
    };
    let mut ok = parts(
        vec![decision(
            DecisionOp::Recorded,
            DecisionValue::Unknown,
            vec![],
        )],
        Outcome::indeterminate(vec![reason.clone()]),
    );
    ok.missing = vec![missing];
    assert!(replay(&build(ok.clone())).is_ok());

    let mut unsupported = ok.clone();
    unsupported.missing.clear();
    assert_eq!(
        replay(&build(unsupported)),
        Err(ReplayError::UnsupportedReason { reason })
    );
    let mut none = ok;
    none.outcome = Outcome::Indeterminate(Vec::new());
    assert_eq!(replay(&build(none)), Err(ReplayError::NoReason));
}

#[test]
fn diagnosed_reason_uses_trace_diagnostics() {
    let mut p = parts(
        vec![decision(
            DecisionOp::Recorded,
            DecisionValue::Unknown,
            vec![],
        )],
        Outcome::indeterminate(vec![Reason::Diagnosed(Code::ExeDivisionByZero)]),
    );
    assert!(replay(&build(p.clone())).is_err());
    let mut d = DiagnosticState::empty();
    d.push(Diagnostic::new(
        Code::ExeDivisionByZero,
        "division by zero".to_owned(),
        Location::Document(DocumentId::new("rules.md").unwrap()),
    ));
    p.diagnostics = d;
    assert!(replay(&build(p)).is_ok());
}

#[test]
fn decisions_follow_kleene_without_short_circuit() {
    // false AND unknown = false; unknown OR unknown = unknown; NOT unknown =
    // unknown.
    let p = parts(
        vec![
            read("flag", TraceValue::Bool(false)),
            decision(DecisionOp::Recorded, DecisionValue::Unknown, vec![]),
            decision(
                DecisionOp::And,
                DecisionValue::Known(TraceValue::Bool(false)),
                vec![1, 2],
            ),
            decision(DecisionOp::Or, DecisionValue::Unknown, vec![2, 2]),
            decision(DecisionOp::Not, DecisionValue::Unknown, vec![2]),
            decision(
                DecisionOp::Or,
                DecisionValue::Known(TraceValue::Bool(false)),
                vec![1, 3],
            ),
        ],
        Outcome::Decided(TraceValue::Bool(false)),
    );
    assert!(replay(&build(p.clone())).is_ok());
    let mut wrong = p;
    wrong.entries[2] = decision(DecisionOp::And, DecisionValue::Unknown, vec![1, 2]);
    assert!(matches!(
        replay(&build(wrong)),
        Err(ReplayError::DecisionMismatch { seq: 3, .. })
    ));
}

#[test]
fn construction_rejects_bad_structure() {
    let mut p = hand_built();
    p.entries[3] = decision(DecisionOp::Pass, DecisionValue::Unknown, vec![4]);
    assert_eq!(
        DecisionTrace::new(p),
        Err(TraceError::BadReference { seq: 4, to: 4 })
    );
    let mut p = hand_built();
    p.missing = vec![MissingInput {
        participant: "p1".to_owned(),
        field: "age".to_owned(),
    }];
    assert_eq!(
        DecisionTrace::new(p),
        Err(TraceError::ReadOfMissingInput { seq: 1 })
    );
}

#[test]
fn construction_orders_unordered_parts() {
    let mut p = hand_built();
    let m = |f: &str| MissingInput {
        participant: "p1".to_owned(),
        field: f.to_owned(),
    };
    p.entries.remove(0);
    p.entries.insert(0, read("score", num("1")));
    p.missing = vec![m("b"), m("a"), m("b")];
    let t = build(p);
    assert_eq!(t.missing(), &[m("a"), m("b")]);
}

#[test]
fn canonical_form_is_stable_and_has_no_text_fields() {
    let trace = build(hand_built());
    let compact = trace.to_canonical_compact();
    let expected = concat!(
        r#"{"question":{"kind":"value","target":"elig.age","participant":"p1"},"#,
        r#""outcome":{"state":"decided","value":{"type":"bool","value":false}},"#,
        r#""rules":[{"rule":"elig.age","revision":1,"spans":[{"document":"rules.md","#,
        r#""sha256":"abababababababababababababababababababababababababababababababab","#,
        r#""start":120,"end":180,"line":4,"column":1}]}],"#,
        r#""entries":[{"seq":1,"kind":"read","name":"age","participant":"p1","#,
        r#""value":{"type":"number","value":"17"},"rule":"elig.age","default_spans":[]},"#,
        r#"{"seq":2,"kind":"step","op":"ge","operands":[{"type":"number","value":"17"},"#,
        r#"{"type":"number","value":"18"}],"result":{"type":"bool","value":false},"#,
        r#""rule":"elig.age","field":null},"#,
        r#"{"seq":3,"kind":"step","op":"mul","operands":[{"type":"number","value":"0.6"},"#,
        r#"{"type":"number","value":"8.5"}],"result":{"type":"number","value":"5.1"},"#,
        r#""rule":"elig.age","field":null},"#,
        r#"{"seq":4,"kind":"decision","rule":"elig.age","op":"pass","#,
        r#""value":{"type":"bool","value":false},"from":[2]}],"#,
        r#""not_applied":[],"missing":[],"diagnostics":[]}"#,
    );
    assert_eq!(compact, expected);
    assert_eq!(build(hand_built()).to_canonical_compact(), compact);
    assert!(trace.to_canonical_pretty().ends_with("}\n"));
}

#[test]
fn fractions_are_written_in_canonical_exact_form() {
    let third =
        rt_numeric::div(&parse_literal("7").unwrap(), &parse_literal("3").unwrap()).unwrap();
    let v = Outcome::Decided(TraceValue::Number(third)).to_value();
    assert_eq!(
        rt_json::write_compact(&v),
        r#"{"state":"decided","value":{"type":"number","value":"7/3"}}"#
    );
}

#[test]
fn rank_place_sorts_group_and_refuses_zero() {
    assert!(RankPlace::new(0, Vec::new()).is_none());
    let r = RankPlace::new(2, vec!["p7".into(), "p3".into(), "p3".into()]).unwrap();
    assert_eq!(r.group(), ["p3".to_owned(), "p7".to_owned()]);
}

fn compare_entry(left: &str, right: &str, result: CriterionResult, first_ahead: bool) -> Entry {
    Entry::Compare(CompareEntry {
        between: ("p3".to_owned(), "p7".to_owned()),
        criteria: vec![CompareCriterion {
            index: 0,
            rule: rule_id("rank.total"),
            direction: RankDirection::Desc,
            left: num(left),
            right: num(right),
            result,
        }],
        first_ahead,
    })
}

fn ranking_parts(compare: Entry) -> TraceParts {
    let places = vec![
        Place::new(1, vec!["p3".to_owned()]).unwrap(),
        Place::new(2, vec!["p7".to_owned()]).unwrap(),
    ];
    let mut entries = vec![compare];
    entries.extend(places.iter().cloned().map(Entry::Place));
    entries.push(decision(
        DecisionOp::Recorded,
        DecisionValue::Known(TraceValue::Standings(places.clone())),
        Vec::new(),
    ));
    let mut p = parts(entries, Outcome::Decided(TraceValue::Standings(places)));
    p.question = TraceQuestion {
        kind: QuestionKind::Rank,
        target: "standings".to_owned(),
        participant: None,
    };
    p
}

#[test]
fn replay_rederives_the_criteria_of_a_comparison() {
    let good = DecisionTrace::new(ranking_parts(compare_entry(
        "7",
        "2",
        CriterionResult::First,
        true,
    )))
    .unwrap();
    assert_eq!(replay(&good).as_ref(), Ok(good.outcome()));
    let bad = DecisionTrace::new(ranking_parts(compare_entry(
        "2",
        "7",
        CriterionResult::First,
        true,
    )))
    .unwrap();
    assert_eq!(replay(&bad), Err(ReplayError::CompareMismatch { seq: 1 }));
    let tied = DecisionTrace::new(ranking_parts(compare_entry(
        "5",
        "5",
        CriterionResult::Equal,
        true,
    )))
    .unwrap();
    assert_eq!(replay(&tied), Err(ReplayError::CompareMismatch { seq: 1 }));
}

#[test]
fn comparisons_and_places_have_a_canonical_form() {
    let t = DecisionTrace::new(ranking_parts(compare_entry(
        "5",
        "5",
        CriterionResult::Equal,
        false,
    )))
    .unwrap();
    let text = t.to_canonical_compact();
    assert!(text.contains(
        "\"kind\":\"compare\",\"between\":[\"p3\",\"p7\"],\"criteria\":[{\"index\":0,\"rule\":\"rank.total\",\"direction\":\"desc\""
    ));
    assert!(text.contains("\"result\":\"tied\""));
    assert!(text.contains("\"kind\":\"place\",\"position\":1,\"members\":[\"p3\"]"));
    assert_eq!(text, t.to_canonical_compact());
    let tie = Place::new(1, vec!["p9".to_owned(), "p2".to_owned()]).unwrap();
    assert!(tie.is_tie_group());
    assert_eq!(tie.members(), ["p2".to_owned(), "p9".to_owned()]);
    assert!(Place::new(0, vec!["p".to_owned()]).is_none());
    assert!(Place::new(1, Vec::new()).is_none());
}

#[test]
fn spans_lists_every_span_once_in_order() {
    let mut p = hand_built();
    p.not_applied = vec![NotApplied {
        rule: rule_id("other"),
        revision: RevisionNumber::new(1).unwrap(),
        spans: vec![span(), span()],
        reason: NotAppliedReason::Precedence {
            id: "prec.score".to_owned(),
        },
    }];
    assert_eq!(build(p).spans().len(), 3);
}

fn small_number() -> impl Strategy<Value = ExactNumber> {
    (-1000i32..1000, 0u32..4).prop_filter_map("literal", |(n, places)| {
        let text = if places == 0 {
            n.to_string()
        } else {
            let scale = 10i32.pow(places);
            format!(
                "{}.{:0width$}",
                n / scale,
                (n % scale).abs(),
                width = places as usize
            )
        };
        parse_literal(&text).ok()
    })
}

property_tests! {
    /// **Validates: Requirements 13.11**
    #[test]
    fn replay_of_derived_steps_always_matches(a in small_number(), b in small_number()) {
        let ops = [StepOp::Add, StepOp::Sub, StepOp::Mul, StepOp::Div, StepOp::Lt, StepOp::Ge, StepOp::Eq];
        let mut entries = Vec::new();
        for op in ops {
            let operands = vec![TraceValue::Number(a.clone()), TraceValue::Number(b.clone())];
            let derived = match op {
                StepOp::Add => rt_numeric::add(&a, &b),
                StepOp::Sub => rt_numeric::sub(&a, &b),
                StepOp::Mul => rt_numeric::mul(&a, &b),
                _ => rt_numeric::div(&a, &b),
            };
            let result = match op {
                StepOp::Lt => StepResult::Value(TraceValue::Bool(rt_numeric::cmp(&a, &b).is_lt())),
                StepOp::Ge => StepResult::Value(TraceValue::Bool(rt_numeric::cmp(&a, &b).is_ge())),
                StepOp::Eq => StepResult::Value(TraceValue::Bool(rt_numeric::cmp(&a, &b).is_eq())),
                _ => match derived {
                    Ok(n) => StepResult::Value(TraceValue::Number(n)),
                    Err(_) => StepResult::Fault(StepFault::DivisionByZero),
                },
            };
            entries.push(step(op, operands, result));
        }
        entries.push(decision(DecisionOp::Recorded, DecisionValue::Known(TraceValue::Bool(true)), vec![]));
        let trace = build(parts(entries, Outcome::Decided(TraceValue::Bool(true))));
        prop_assert!(replay(&trace).is_ok());
        prop_assert_eq!(trace.to_canonical_compact(), trace.clone().to_canonical_compact());
    }

    /// **Validates: Requirements 13.11**
    #[test]
    fn altering_an_add_operand_fails_replay(a in small_number(), b in small_number(), d in 1i32..50) {
        let sum = rt_numeric::add(&a, &b).unwrap();
        let shifted = rt_numeric::add(&a, &parse_literal(&d.to_string()).unwrap()).unwrap();
        let entries = vec![
            step(
                StepOp::Add,
                vec![TraceValue::Number(shifted), TraceValue::Number(b)],
                StepResult::Value(TraceValue::Number(sum)),
            ),
            decision(DecisionOp::Recorded, DecisionValue::Known(TraceValue::Bool(true)), vec![]),
        ];
        let trace = build(parts(entries, Outcome::Decided(TraceValue::Bool(true))));
        let is_step_mismatch = matches!(replay(&trace), Err(ReplayError::StepMismatch { .. }));
        prop_assert!(is_step_mismatch);
    }
}
