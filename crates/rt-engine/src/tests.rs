use crate::*;
use proptest::prelude::*;
use rt_compiled::{
    CType, CollectionIdx, CompiledBody, CompiledCollection, CompiledDocument, CompiledField,
    CompiledInput, CompiledLiteral, CompiledNode, CompiledParts, CompiledRule, CompiledSpan,
    CompiledTerm, FieldIdx, InputIdx, NodeIdx, NodeKind, RuleIdx, TargetIdx,
};
use rt_data::{
    DataValue, Element, Participant, ParticipantId, PublishedResult, Question, TargetName,
};
use rt_foundation::{DocumentId, Location, RevisionNumber, RuleId, Severity};
use rt_hash::sha256;
use rt_ir::{AggFn, BinaryOp, CmpOp, IrVersion, Name, Places, Rounding};
use rt_numeric::{parse_canonical, parse_literal, ExactNumber, RoundingMode};
use rt_trace::Tri;
use std::collections::BTreeMap;

// ----- builders -----

pub(crate) fn name(s: &str) -> Name {
    Name::new(s).unwrap()
}

pub(crate) fn doc_sha() -> rt_hash::Sha256Hex {
    sha256(b"the source document")
}

pub(crate) fn span(start: u32) -> CompiledSpan {
    CompiledSpan {
        document: DocumentId::new("rules.md").unwrap(),
        sha256: doc_sha(),
        start,
        end: start + 10,
        line: 3,
        column: 1,
    }
}

#[derive(Default)]
pub(crate) struct Nodes(pub(crate) Vec<CompiledNode>);

impl Nodes {
    pub(crate) fn add(&mut self, ty: CType, kind: NodeKind) -> NodeIdx {
        self.0.push(CompiledNode { ty, kind });
        NodeIdx::new(u32::try_from(self.0.len() - 1).unwrap())
    }
    pub(crate) fn num(&mut self, text: &str) -> NodeIdx {
        self.add(
            CType::Number,
            NodeKind::Literal(CompiledLiteral::Number(parse_canonical(text).unwrap())),
        )
    }
    pub(crate) fn field(&mut self, ty: CType, input: u32) -> NodeIdx {
        self.add(ty, NodeKind::Field(InputIdx::new(input)))
    }
    pub(crate) fn bin(&mut self, op: BinaryOp, left: NodeIdx, right: NodeIdx) -> NodeIdx {
        self.add(CType::Number, NodeKind::Binary { op, left, right })
    }
    pub(crate) fn cmp(&mut self, op: CmpOp, left: NodeIdx, right: NodeIdx) -> NodeIdx {
        self.add(CType::Boolean, NodeKind::Cmp { op, left, right })
    }
    pub(crate) fn agg(&mut self, func: AggFn, field: Option<u32>) -> NodeIdx {
        self.add(
            CType::Number,
            NodeKind::Agg {
                func,
                collection: CollectionIdx::new(0),
                field: field.map(FieldIdx::new),
            },
        )
    }
}

pub(crate) fn rule(
    id: &str,
    target: &str,
    ty: CType,
    nodes: Nodes,
    body: CompiledBody,
) -> CompiledRule {
    CompiledRule {
        id: RuleId::new(id).unwrap(),
        revision: RevisionNumber::new(1).unwrap(),
        target: name(target),
        ty,
        spans: vec![span(20), span(60)],
        nodes: nodes.0,
        body,
    }
}

pub(crate) fn derived(id: &str, target: &str, nodes: Nodes, root: NodeIdx) -> CompiledRule {
    rule(
        id,
        target,
        CType::Number,
        nodes,
        CompiledBody::Derived { expr: root },
    )
}

pub(crate) fn eligibility(id: &str, target: &str, nodes: Nodes, root: NodeIdx) -> CompiledRule {
    rule(
        id,
        target,
        CType::Boolean,
        nodes,
        CompiledBody::Eligibility { expr: root },
    )
}

pub(crate) fn default_rule(id: &str, input: &str, value: bool) -> CompiledRule {
    rule(
        id,
        input,
        CType::Boolean,
        Nodes::default(),
        CompiledBody::Default {
            value: CompiledLiteral::Boolean(value),
        },
    )
}

pub(crate) fn build(inputs: &[(&str, CType)], rules: Vec<CompiledRule>) -> CompiledRulebook {
    build_with(inputs, rules, &[])
}

/// `prec` lists (declaration id, Target, rule identifiers in order of
/// precedence).
pub(crate) fn build_with(
    inputs: &[(&str, CType)],
    mut rules: Vec<CompiledRule>,
    prec: &[(&str, &str, &[&str])],
) -> CompiledRulebook {
    rules.sort_by(|a, b| a.id.cmp(&b.id));
    let targets = rt_compiled::targets_of(&rules);
    let precedence = prec
        .iter()
        .map(|(id, target, order)| rt_compiled::CompiledPrecedence {
            id: RuleId::new(*id).unwrap(),
            target: TargetIdx::new(
                u32::try_from(targets.iter().position(|t| t.as_str() == *target).unwrap()).unwrap(),
            ),
            order: order
                .iter()
                .map(|r| {
                    RuleIdx::new(
                        u32::try_from(rules.iter().position(|x| x.id.as_str() == *r).unwrap())
                            .unwrap(),
                    )
                })
                .collect(),
            spans: vec![span(100)],
        })
        .collect();
    let n = u32::try_from(rules.len()).unwrap();
    CompiledRulebook::from_parts(CompiledParts {
        compiler_version: "0.1.0".into(),
        ir_version: IrVersion::V1,
        rulebook_hash: sha256(b"rulebook"),
        documents: vec![CompiledDocument {
            id: DocumentId::new("rules.md").unwrap(),
            sha256: doc_sha(),
        }],
        inputs: inputs
            .iter()
            .map(|(n, ty)| CompiledInput {
                name: name(n),
                ty: *ty,
            })
            .collect(),
        collections: vec![CompiledCollection {
            name: name("results"),
            fields: vec![CompiledField {
                name: name("place"),
                ty: CType::Number,
            }],
        }],
        enums: Vec::new(),
        sets: Vec::new(),
        order: (0..n).map(RuleIdx::new).collect(),
        rules,
        precedence,
    })
    .unwrap()
}

pub(crate) fn text(s: &str) -> DataValue {
    DataValue::Text(s.to_owned())
}

pub(crate) fn element(place: &str) -> Element {
    BTreeMap::from([("place".to_owned(), text(place))])
}

pub(crate) fn person(fields: &[(&str, DataValue)], results: Option<Vec<Element>>) -> Participant {
    let collections = results
        .map(|r| BTreeMap::from([("results".to_owned(), r)]))
        .unwrap_or_default();
    Participant::new(
        fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
        collections,
    )
}

pub(crate) fn data(people: Vec<(&str, Participant)>) -> CompetitionData {
    CompetitionData::new(
        people
            .into_iter()
            .map(|(id, p)| (ParticipantId::new(id).unwrap(), p))
            .collect(),
    )
}

pub(crate) fn ask(target: &str, participant: &str) -> Question {
    Question::Value {
        target: TargetName::new(target).unwrap(),
        participant: ParticipantId::new(participant).unwrap(),
    }
}

pub(crate) fn number(text: &str) -> ExactNumber {
    parse_literal(text).unwrap()
}

pub(crate) fn decided_number(text: &str) -> Outcome {
    Outcome::Decided(rt_trace::TraceValue::Number(number(text)))
}

pub(crate) fn decided_bool(b: bool) -> Outcome {
    Outcome::Decided(rt_trace::TraceValue::Bool(b))
}

/// Evaluates one question and checks that every trace replays to its Outcome.
pub(crate) fn eval_one(
    rb: &CompiledRulebook,
    d: &CompetitionData,
    q: Question,
) -> (Outcome, DecisionTrace, Evaluation) {
    let e = evaluate(rb, d, std::slice::from_ref(&q), None);
    assert_eq!(e.outcomes.len(), 1);
    assert_eq!(e.traces.len(), 1);
    for (o, t) in e.outcomes.iter().zip(&e.traces) {
        assert_eq!(t.outcome(), o);
        assert_eq!(rt_trace::replay(t).as_ref(), Ok(o), "trace must replay");
    }
    (e.outcomes[0].clone(), e.traces[0].clone(), e)
}

pub(crate) fn codes(e: &Evaluation) -> Vec<Code> {
    e.diagnostics
        .as_slice()
        .iter()
        .map(Diagnostic::code)
        .collect()
}

pub(crate) const NUM: CType = CType::Number;
pub(crate) const BOOL: CType = CType::Boolean;

// ----- exact arithmetic -----

#[test]
fn weighted_score_with_decimal_weights_is_the_exact_sum() {
    let mut n = Nodes::default();
    let (w1, a) = (n.num("0.6"), n.field(NUM, 0));
    let (w2, b) = (n.num("0.4"), n.field(NUM, 1));
    let score = rule(
        "score.total",
        "score",
        NUM,
        n,
        CompiledBody::WeightedScore {
            terms: vec![
                CompiledTerm {
                    weight: w1,
                    value: a,
                },
                CompiledTerm {
                    weight: w2,
                    value: b,
                },
            ],
            rounding: None,
        },
    );
    let rb = build(&[("a", NUM), ("b", NUM)], vec![score]);
    let d = data(vec![(
        "p1",
        person(&[("a", text("8.5")), ("b", text("7.25"))], None),
    )]);
    let (outcome, trace, e) = eval_one(&rb, &d, ask("score", "p1"));
    // 0.6 * 8.5 + 0.4 * 7.25 = 5.1 + 2.9
    assert_eq!(outcome, decided_number("8"));
    assert!(e.diagnostics.as_slice().is_empty());
    assert_eq!(trace.rules().len(), 1);
    assert_eq!(trace.rules()[0].spans.len(), 2);
}

#[test]
fn one_tenth_plus_two_tenths_equals_three_tenths() {
    let mut n = Nodes::default();
    let (a, b) = (n.num("0.1"), n.num("0.2"));
    let sum = n.bin(BinaryOp::Add, a, b);
    let c = n.num("0.3");
    let eq = n.cmp(CmpOp::Eq, sum, c);
    let rb = build(&[("age", NUM)], vec![eligibility("elig.eq", "same", n, eq)]);
    let d = data(vec![("p1", person(&[], None))]);
    assert_eq!(eval_one(&rb, &d, ask("same", "p1")).0, decided_bool(true));
}

#[test]
fn rounding_applies_only_where_declared() {
    let mk = |mode: RoundingMode, places: u64, value: &str| {
        let mut n = Nodes::default();
        let v = n.num(value);
        let r = n.add(
            NUM,
            NodeKind::Round {
                arg: v,
                rounding: Rounding {
                    mode,
                    places: Places::new(places).unwrap(),
                },
            },
        );
        let rb = build(&[], vec![derived("calc.r", "r", n, r)]);
        let d = data(vec![("p1", person(&[], None))]);
        eval_one(&rb, &d, ask("r", "p1")).0
    };
    assert_eq!(mk(RoundingMode::HalfEven, 0, "2.5"), decided_number("2"));
    assert_eq!(
        mk(RoundingMode::HalfAwayFromZero, 0, "2.5"),
        decided_number("3")
    );
    assert_eq!(
        mk(RoundingMode::TowardZero, 0, "-2.5"),
        decided_number("-2")
    );
    assert_eq!(
        mk(RoundingMode::HalfAwayFromZero, 2, "0.125"),
        decided_number("0.13")
    );
    assert_eq!(
        mk(RoundingMode::HalfEven, 2, "0.125"),
        decided_number("0.12")
    );
}

#[test]
fn division_without_rounding_keeps_the_exact_fraction() {
    let mut n = Nodes::default();
    let (a, b) = (n.num("7"), n.num("3"));
    let q = n.bin(BinaryOp::Div, a, b);
    let rb = build(&[], vec![derived("calc.q", "q", n, q)]);
    let d = data(vec![("p1", person(&[], None))]);
    let (outcome, ..) = eval_one(&rb, &d, ask("q", "p1"));
    assert_eq!(
        outcome,
        Outcome::Decided(rt_trace::TraceValue::Number(
            parse_canonical("7/3").unwrap()
        ))
    );
}

#[test]
fn division_by_zero_is_a_fault_for_that_question_only() {
    let mut n = Nodes::default();
    let (a, b) = (n.field(NUM, 0), n.field(NUM, 1));
    let q = n.bin(BinaryOp::Div, a, b);
    let rb = build(
        &[("a", NUM), ("b", NUM)],
        vec![derived("calc.q", "q", n, q)],
    );
    let d = data(vec![
        ("p1", person(&[("a", text("1")), ("b", text("0"))], None)),
        ("p2", person(&[("a", text("6")), ("b", text("3"))], None)),
    ]);
    let e = evaluate(&rb, &d, &[ask("q", "p1"), ask("q", "p2")], None);
    assert_eq!(
        e.outcomes[0],
        Outcome::indeterminate(vec![Reason::Diagnosed(Code::ExeDivisionByZero)])
    );
    assert_eq!(e.outcomes[1], decided_number("2"));
    for (o, t) in e.outcomes.iter().zip(&e.traces) {
        assert_eq!(rt_trace::replay(t).as_ref(), Ok(o));
    }
    let diag = &e.diagnostics.as_slice()[0];
    assert_eq!(diag.code(), Code::ExeDivisionByZero);
    assert!(matches!(diag.primary(), Location::Source(s) if s.start().get() == 20));
    assert!(diag.message().contains("calc.q"));
}

#[test]
fn overflow_is_reported_and_never_wraps() {
    let big = "99999999999999999999999999999999999999";
    let mut n = Nodes::default();
    let (a, b) = (n.num(big), n.num(big));
    let m = n.bin(BinaryOp::Mul, a, b);
    let rb = build(&[], vec![derived("calc.m", "m", n, m)]);
    let d = data(vec![("p1", person(&[], None))]);
    let (outcome, _, e) = eval_one(&rb, &d, ask("m", "p1"));
    assert_eq!(
        outcome,
        Outcome::indeterminate(vec![Reason::Diagnosed(Code::ExeNumericOverflow)])
    );
    assert_eq!(codes(&e), vec![Code::ExeNumericOverflow]);
}

#[test]
fn data_values_that_fail_the_declared_type_are_refused_per_value() {
    let mut n = Nodes::default();
    let (a, one) = (n.field(NUM, 0), n.num("1"));
    let s = n.bin(BinaryOp::Add, a, one);
    let mut m = Nodes::default();
    let l = m.field(BOOL, 1);
    let rb = build(
        &[("age", NUM), ("licence", BOOL)],
        vec![
            derived("calc.age", "next", n, s),
            eligibility("elig.l", "ok", m, l),
        ],
    );
    let d = data(vec![(
        "p1",
        person(&[("age", text("1e3")), ("licence", text("yes"))], None),
    )]);
    let e = evaluate(&rb, &d, &[ask("next", "p1"), ask("ok", "p1")], None);
    assert_eq!(
        e.outcomes[0],
        Outcome::indeterminate(vec![Reason::Diagnosed(Code::ExeNumericValueInvalid)])
    );
    assert_eq!(
        e.outcomes[1],
        Outcome::indeterminate(vec![Reason::Diagnosed(Code::ExeInputTypeMismatch)])
    );
    let found = e.diagnostics.as_slice();
    assert!(found
        .iter()
        .all(|d| matches!(d.primary(), Location::Input(i) if i.field.is_some())));
    assert!(found
        .iter()
        .any(|d| d.message().contains("participant 'p1'")));
}

// ----- missing information and three-valued logic -----

pub(crate) fn licence_rule(op_and: bool) -> CompiledRulebook {
    let mut n = Nodes::default();
    let a = n.field(NUM, 0);
    let sixteen = n.num("16");
    let adult = n.cmp(CmpOp::Ge, a, sixteen);
    let l = n.field(BOOL, 1);
    let root = n.add(
        BOOL,
        if op_and {
            NodeKind::And(vec![adult, l])
        } else {
            NodeKind::Or(vec![adult, l])
        },
    );
    build(
        &[("age", NUM), ("licence", BOOL)],
        vec![eligibility("elig.main", "eligible", n, root)],
    )
}

#[test]
fn a_missing_input_gives_indeterminate_naming_the_input() {
    let rb = licence_rule(true);
    let d = data(vec![("p1", person(&[("age", text("20"))], None))]);
    let (outcome, trace, e) = eval_one(&rb, &d, ask("eligible", "p1"));
    assert_eq!(
        outcome,
        Outcome::indeterminate(vec![Reason::MissingInput {
            participant: "p1".into(),
            field: "licence".into()
        }])
    );
    assert_eq!(trace.missing().len(), 1);
    assert_eq!(trace.missing()[0].field, "licence");
    let diag = &e.diagnostics.as_slice()[0];
    assert_eq!(diag.code(), Code::ExeMissingValue);
    assert_eq!(diag.severity(), Severity::Error);
    assert!(diag.message().contains("licence") && diag.message().contains("elig.main"));
    assert!(matches!(diag.primary(), Location::Source(_)));
}

#[test]
fn kleene_absorption_decides_despite_a_missing_operand() {
    let rb = licence_rule(true);
    let young = data(vec![("p1", person(&[("age", text("10"))], None))]);
    let (outcome, trace, e) = eval_one(&rb, &young, ask("eligible", "p1"));
    assert_eq!(outcome, decided_bool(false));
    assert!(trace.missing().is_empty());
    assert!(e.diagnostics.as_slice().is_empty());

    let rb = licence_rule(false);
    let adult = data(vec![("p1", person(&[("age", text("30"))], None))]);
    assert_eq!(
        eval_one(&rb, &adult, ask("eligible", "p1")).0,
        decided_bool(true)
    );
    let minor = data(vec![("p1", person(&[("age", text("3"))], None))]);
    assert!(matches!(
        eval_one(&rb, &minor, ask("eligible", "p1")).0,
        Outcome::Indeterminate(_)
    ));
}

#[test]
fn zero_false_and_empty_text_are_present_and_a_default_fills_only_a_missing_field() {
    let mut n = Nodes::default();
    let l = n.field(BOOL, 0);
    let rb = build(
        &[("licence", BOOL)],
        vec![
            eligibility("elig.l", "ok", n, l),
            default_rule("default.licence", "licence", true),
        ],
    );
    let present = data(vec![(
        "p1",
        person(&[("licence", DataValue::Bool(false))], None),
    )]);
    let (outcome, trace, _) = eval_one(&rb, &present, ask("ok", "p1"));
    assert_eq!(outcome, decided_bool(false));
    assert!(trace.spans().len() == trace.rules().iter().map(|r| r.spans.len()).sum::<usize>());

    let absent = data(vec![("p1", person(&[], None))]);
    let (outcome, trace, e) = eval_one(&rb, &absent, ask("ok", "p1"));
    assert_eq!(outcome, decided_bool(true));
    assert!(e.diagnostics.as_slice().is_empty());
    let read = trace
        .entries()
        .iter()
        .find_map(|x| match x {
            Entry::Read(r) => Some(r),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        read.default_spans.len(),
        2,
        "the default's spans are recorded"
    );
    assert_eq!(
        trace.rules().len(),
        2,
        "the default rule is listed as applied"
    );
}

#[test]
fn a_missing_input_does_not_stop_other_questions() {
    let rb = licence_rule(true);
    let d = data(vec![
        ("p1", person(&[("age", text("20"))], None)),
        (
            "p2",
            person(
                &[("age", text("20")), ("licence", DataValue::Bool(true))],
                None,
            ),
        ),
    ]);
    let e = evaluate(
        &rb,
        &d,
        &[ask("eligible", "p1"), ask("eligible", "p2")],
        None,
    );
    assert!(matches!(e.outcomes[0], Outcome::Indeterminate(_)));
    assert_eq!(e.outcomes[1], decided_bool(true));
}

#[test]
fn a_rule_that_reads_another_rule_depends_on_its_result() {
    let mut n = Nodes::default();
    let a = n.field(NUM, 0);
    let base = derived("calc.base", "base", n, a);
    let mut m = Nodes::default();
    let r = m.add(NUM, NodeKind::Ref(TargetIdx::new(0)));
    let two = m.num("2");
    let t = m.bin(BinaryOp::Mul, r, two);
    let double = derived("calc.double", "double", m, t);
    let rb = build(&[("a", NUM)], vec![base, double]);
    let d = data(vec![
        ("p1", person(&[("a", text("4"))], None)),
        ("p2", person(&[], None)),
    ]);
    let (outcome, trace, _) = eval_one(&rb, &d, ask("double", "p1"));
    assert_eq!(outcome, decided_number("8"));
    assert_eq!(trace.rules().len(), 2);
    let (outcome, trace, _) = eval_one(&rb, &d, ask("double", "p2"));
    assert!(matches!(outcome, Outcome::Indeterminate(_)));
    assert_eq!(trace.missing().len(), 1);
}

// ----- aggregation -----

fn aggregate_rule(func: AggFn, add_five: bool) -> CompiledRulebook {
    let mut n = Nodes::default();
    let field = (func != AggFn::Count).then_some(0);
    let mut root = n.agg(func, field);
    if add_five {
        let five = n.num("5");
        root = n.bin(BinaryOp::Add, root, five);
    }
    build(&[("age", NUM)], vec![derived("calc.agg", "agg", n, root)])
}

#[test]
fn count_and_sum_of_an_empty_collection_are_zero_without_a_diagnostic() {
    let d = data(vec![("p1", person(&[], Some(vec![])))]);
    for func in [AggFn::Count, AggFn::Sum] {
        let (outcome, trace, e) = eval_one(&aggregate_rule(func, false), &d, ask("agg", "p1"));
        assert_eq!(outcome, decided_number("0"));
        assert!(e.diagnostics.as_slice().is_empty());
        assert!(trace.entries().iter().any(|x| matches!(
            x,
            Entry::Step(s) if s.operands.is_empty() && s.field.as_deref() == Some("results")
        )));
    }
    let (outcome, ..) = eval_one(&aggregate_rule(AggFn::Sum, true), &d, ask("agg", "p1"));
    assert_eq!(outcome, decided_number("5"));
}

#[test]
fn count_sum_min_and_max_of_a_collection_are_exact() {
    let d = data(vec![(
        "p1",
        person(&[], Some(vec![element("3"), element("0.5"), element("2")])),
    )]);
    let run = |f| eval_one(&aggregate_rule(f, false), &d, ask("agg", "p1")).0;
    assert_eq!(run(AggFn::Count), decided_number("3"));
    assert_eq!(run(AggFn::Sum), decided_number("5.5"));
    assert_eq!(run(AggFn::Min), decided_number("0.5"));
    assert_eq!(run(AggFn::Max), decided_number("3"));
}

#[test]
fn min_of_an_empty_collection_is_indeterminate_with_the_empty_aggregate_diagnostic() {
    let d = data(vec![("p1", person(&[], Some(vec![])))]);
    for func in [AggFn::Min, AggFn::Max] {
        let (outcome, trace, e) = eval_one(&aggregate_rule(func, false), &d, ask("agg", "p1"));
        assert_eq!(
            outcome,
            Outcome::indeterminate(vec![Reason::EmptyAggregate {
                field: "results".into()
            }])
        );
        assert!(trace.missing().is_empty());
        let diag = &e.diagnostics.as_slice()[0];
        assert_eq!(diag.code(), Code::AggregateEmptyCollection);
        assert!(diag.message().contains("calc.agg") && diag.message().contains("results"));
        assert!(matches!(diag.primary(), Location::Source(s) if s.start().get() == 20));
    }
}

#[test]
fn an_outcome_decided_without_the_empty_minimum_has_no_diagnostic() {
    let mut n = Nodes::default();
    let age = n.field(NUM, 0);
    let hundred = n.num("100");
    let old = n.cmp(CmpOp::Ge, age, hundred);
    let min = n.agg(AggFn::Min, Some(0));
    let three = n.num("3");
    let good = n.cmp(CmpOp::Gt, min, three);
    let root = n.add(BOOL, NodeKind::And(vec![old, good]));
    let rb = build(&[("age", NUM)], vec![eligibility("elig.min", "e", n, root)]);
    let d = data(vec![("p1", person(&[("age", text("5"))], Some(vec![])))]);
    let (outcome, _, e) = eval_one(&rb, &d, ask("e", "p1"));
    assert_eq!(outcome, decided_bool(false));
    assert!(e.diagnostics.as_slice().is_empty());
}

#[test]
fn a_collection_the_data_does_not_supply_is_missing_not_empty() {
    let d = data(vec![("p1", person(&[], None))]);
    for func in [AggFn::Count, AggFn::Sum, AggFn::Min] {
        let (outcome, _, e) = eval_one(&aggregate_rule(func, false), &d, ask("agg", "p1"));
        assert_eq!(
            outcome,
            Outcome::indeterminate(vec![Reason::MissingInput {
                participant: "p1".into(),
                field: "results".into()
            }])
        );
        assert_eq!(codes(&e), vec![Code::ExeMissingValue]);
    }
}

#[test]
fn an_element_without_the_aggregated_field_is_missing_information() {
    let d = data(vec![(
        "p1",
        person(&[], Some(vec![element("1"), BTreeMap::new()])),
    )]);
    let (outcome, ..) = eval_one(&aggregate_rule(AggFn::Sum, false), &d, ask("agg", "p1"));
    assert_eq!(
        outcome,
        Outcome::indeterminate(vec![Reason::MissingInput {
            participant: "p1".into(),
            field: "results.place".into()
        }])
    );
}

// ----- questions and conflicts -----

#[test]
fn a_question_naming_an_unknown_target_or_participant_is_invalid() {
    let rb = licence_rule(true);
    let d = data(vec![("p1", person(&[], None))]);
    let e = evaluate(
        &rb,
        &d,
        &[ask("nope", "p1"), ask("eligible", "ghost")],
        None,
    );
    assert_eq!(e.outcomes.len(), 2);
    assert_eq!(e.traces.len(), 2);
    assert!(codes(&e).iter().all(|c| *c == Code::QuestionInvalid));
    assert_eq!(codes(&e).len(), 2);
}

#[test]
fn several_rules_for_one_target_select_no_result() {
    let mk = |id: &str, v: &str| {
        let mut n = Nodes::default();
        let r = n.num(v);
        derived(id, "same", n, r)
    };
    let rb = build(&[], vec![mk("calc.b", "2"), mk("calc.a", "1")]);
    let d = data(vec![("p1", person(&[], None))]);
    let (outcome, trace, e) = eval_one(&rb, &d, ask("same", "p1"));
    assert_eq!(
        outcome,
        Outcome::indeterminate(vec![Reason::Diagnosed(Code::ExecutionConflictDetected)])
    );
    let diag = &e.diagnostics.as_slice()[0];
    assert_eq!(diag.code(), Code::ExecutionConflictDetected);
    assert_eq!(diag.rules()[0].rule.as_str(), "calc.a");
    assert!(matches!(diag.primary(), Location::Source(s) if s.start().get() == 20));
    assert_eq!(diag.related().len(), 3);
    assert_eq!(trace.rules().len(), 2);
}

#[test]
fn the_published_result_is_compared_and_does_not_change_the_outcomes() {
    let rb = licence_rule(true);
    let d = data(vec![("p1", person(&[("age", text("10"))], None))]);
    let q = [ask("eligible", "p1")];
    let published = PublishedResult::default();
    let with = evaluate(&rb, &d, &q, Some(&published));
    let without = evaluate(&rb, &d, &q, None);
    assert_eq!(with.outcomes, without.outcomes);
    assert_eq!(with.traces, without.traces);
    assert_eq!(with.diagnostics, without.diagnostics);
    assert_eq!(without.comparison, None);
    assert!(with.comparison.unwrap().is_equal());
}

#[test]
fn two_rulebooks_evaluated_in_any_interleaving_do_not_affect_each_other() {
    let a = licence_rule(true);
    let b = licence_rule(false);
    let d = data(vec![
        ("p1", person(&[("age", text("20"))], None)),
        (
            "p2",
            person(
                &[("age", text("2")), ("licence", DataValue::Bool(true))],
                None,
            ),
        ),
    ]);
    let qs = [ask("eligible", "p1"), ask("eligible", "p2")];
    let alone_a = evaluate(&a, &d, &qs, None);
    let alone_b = evaluate(&b, &d, &qs, None);
    let first_b = evaluate(&b, &d, &qs, None);
    let then_a = evaluate(&a, &d, &qs, None);
    assert_eq!(alone_a, then_a);
    assert_eq!(alone_b, first_b);
    assert_ne!(alone_a, alone_b);
}

// ----- properties -----

fn hundredths(n: u32) -> String {
    format!("{}.{:02}", n / 100, n % 100)
}

proptest! {
    /// **Validates: Requirements 8.1, 8.10, 8.12, 10.2, 13.11** A weighted
    /// score of exact decimals is the exact sum of the exact products.
    #[test]
    fn weighted_scores_equal_an_independent_integer_model(
        pairs in proptest::collection::vec((0u32..100_000, 0u32..100_000), 1..5)
    ) {
        let mut n = Nodes::default();
        let mut terms = Vec::new();
        let mut inputs = Vec::new();
        let names: Vec<String> = (0..pairs.len()).map(|i| format!("v{i}")).collect();
        for (i, (w, _)) in pairs.iter().enumerate() {
            let weight = n.num(&hundredths(*w));
            let value = n.field(NUM, u32::try_from(i).unwrap());
            terms.push(CompiledTerm { weight, value });
            inputs.push((names[i].as_str(), NUM));
        }
        let rb = build(
            &inputs,
            vec![rule("score.total", "score", NUM, n, CompiledBody::WeightedScore { terms, rounding: None })],
        );
        let fields: Vec<(&str, DataValue)> = pairs
            .iter()
            .enumerate()
            .map(|(i, (_, v))| (names[i].as_str(), text(&hundredths(*v))))
            .collect();
        let d = data(vec![("p1", person(&fields, None))]);
        let (outcome, ..) = eval_one(&rb, &d, ask("score", "p1"));
        let total: u64 = pairs.iter().map(|(w, v)| u64::from(*w) * u64::from(*v)).sum();
        let expected = format!("{}.{:04}", total / 10_000, total % 10_000);
        prop_assert_eq!(outcome, decided_number(&expected));
        prop_assert_eq!(evaluate(&rb, &d, &[ask("score", "p1")], None), evaluate(&rb, &d, &[ask("score", "p1")], None));
    }

    /// **Validates: Requirements 11.1, 11.4, 11.5, 11.6** Boolean combinations
    /// follow Kleene logic for every mix of true, false and missing inputs.
    #[test]
    fn boolean_rules_follow_kleene_three_valued_logic(
        s in proptest::collection::vec(0u8..3, 3)
    ) {
        let tri = |v: u8| match v { 0 => Tri::False, 1 => Tri::True, _ => Tri::Unknown };
        let (a, b, c) = (tri(s[0]), tri(s[1]), tri(s[2]));
        let mut n1 = Nodes::default();
        let (x, y, z) = (n1.field(BOOL, 0), n1.field(BOOL, 1), n1.field(BOOL, 2));
        let all = n1.add(BOOL, NodeKind::And(vec![x, y, z]));
        let mut n2 = Nodes::default();
        let (x, y) = (n2.field(BOOL, 0), n2.field(BOOL, 1));
        let ny = n2.add(BOOL, NodeKind::Not(y));
        let any = n2.add(BOOL, NodeKind::Or(vec![x, ny]));
        let mut n3 = Nodes::default();
        let (x, y, z) = (n3.field(BOOL, 0), n3.field(BOOL, 1), n3.field(BOOL, 2));
        let o = n3.add(BOOL, NodeKind::Or(vec![x, y]));
        let nz = n3.add(BOOL, NodeKind::Not(z));
        let mixed = n3.add(BOOL, NodeKind::And(vec![o, nz]));
        let rb = build(
            &[("a", BOOL), ("b", BOOL), ("c", BOOL)],
            vec![
                eligibility("e.all", "all", n1, all),
                eligibility("e.any", "any", n2, any),
                eligibility("e.mixed", "mixed", n3, mixed),
            ],
        );
        let fields: Vec<(&str, DataValue)> = [("a", a), ("b", b), ("c", c)]
            .iter()
            .filter_map(|(k, t)| t.to_bool().map(|v| (*k, DataValue::Bool(v))))
            .collect();
        let d = data(vec![("p1", person(&fields, None))]);
        let expect = [a.and(b).and(c), a.or(!b), a.or(b).and(!c)];
        for (target, want) in ["all", "any", "mixed"].iter().zip(expect) {
            let (outcome, ..) = eval_one(&rb, &d, ask(target, "p1"));
            match want.to_bool() {
                Some(v) => prop_assert_eq!(outcome, decided_bool(v)),
                None => prop_assert!(matches!(outcome, Outcome::Indeterminate(_))),
            }
        }
    }
}
