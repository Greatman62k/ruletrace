//! Property 25: Trace completeness and replay.
//!
//! Feature: ruletrace-compiler, Property 25: Trace completeness and replay
//!
//! **Validates: Requirements 13.1, 13.2, 13.3, 13.4, 13.6, 13.11, 13.21, 10.11,
//! 12.7**
//!
//! A Compiled_Rulebook is built with `CompiledRulebook::from_parts` (the
//! Compiler is not used). It has derived, eligibility, weighted-score,
//! rounding, aggregate, ranking, conflicting and precedence-ordered rules. For
//! generated Competition_Data (missing values, zero divisors, empty and
//! unsupplied collections included) every Question is evaluated and the
//! expectations below are computed from the generated integers (cents) by this
//! file, not by the engine.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;
use proptest::prelude::*;
use proptest_floor::property_tests;
use rt_compiled::{
    CType, CollectionIdx, CompiledBody, CompiledCollection, CompiledDocument, CompiledField,
    CompiledInput, CompiledLiteral, CompiledNode, CompiledParts, CompiledPrecedence, CompiledRule,
    CompiledRulebook, CompiledSpan, CompiledTerm, FieldIdx, InputIdx, NodeIdx, NodeKind, RuleIdx,
    TargetIdx,
};
use rt_data::{
    CompetitionData, DataValue, Element, Participant, ParticipantId, Question, TargetName,
};
use rt_engine::{evaluate, Evaluation};
use rt_foundation::{Code, DocumentId, RevisionNumber, RuleId};
use rt_hash::sha256;
use rt_ir::{AggFn, BinaryOp, CmpOp, Direction, IrVersion, Name, Places, Rounding, Ties};
use rt_numeric::{parse_canonical, parse_literal, RoundingMode};
use rt_trace::{
    replay, DecisionTrace, DecisionValue, Entry, NotAppliedReason, Outcome, Reason, TraceSpan,
    TraceValue,
};
use std::collections::{BTreeMap, BTreeSet};

const NUM: CType = CType::Number;
const BOOL: CType = CType::Boolean;
const DOC: &str = "rules.md";

/// Value Targets in ascending byte order: the position is the `TargetIdx`.
const TARGETS: [&str; 9] = [
    "best",
    "clash",
    "elig",
    "pick",
    "ratio",
    "score",
    "standings",
    "sum",
    "tot",
];
const VALUE_TARGETS: [&str; 8] = [
    "best", "clash", "elig", "pick", "ratio", "score", "sum", "tot",
];

fn name(s: &str) -> Name {
    Name::new(s).unwrap()
}

fn target_idx(t: &str) -> TargetIdx {
    TargetIdx::new(u32::try_from(TARGETS.iter().position(|x| *x == t).unwrap()).unwrap())
}

fn span(start: u32) -> CompiledSpan {
    CompiledSpan {
        document: DocumentId::new(DOC).unwrap(),
        sha256: sha256(b"the source document"),
        start,
        end: start + 10,
        line: 3,
        column: 1,
    }
}

#[derive(Default)]
struct Nodes(Vec<CompiledNode>);

impl Nodes {
    fn add(&mut self, ty: CType, kind: NodeKind) -> NodeIdx {
        self.0.push(CompiledNode { ty, kind });
        NodeIdx::new(u32::try_from(self.0.len() - 1).unwrap())
    }
    fn num(&mut self, text: &str) -> NodeIdx {
        self.add(
            NUM,
            NodeKind::Literal(CompiledLiteral::Number(parse_canonical(text).unwrap())),
        )
    }
    fn field(&mut self, input: u32) -> NodeIdx {
        self.add(NUM, NodeKind::Field(InputIdx::new(input)))
    }
    fn bin(&mut self, op: BinaryOp, left: NodeIdx, right: NodeIdx) -> NodeIdx {
        self.add(NUM, NodeKind::Binary { op, left, right })
    }
    fn cmp(&mut self, op: CmpOp, left: NodeIdx, right: NodeIdx) -> NodeIdx {
        self.add(BOOL, NodeKind::Cmp { op, left, right })
    }
    fn agg(&mut self, func: AggFn, field: Option<u32>) -> NodeIdx {
        self.add(
            NUM,
            NodeKind::Agg {
                func,
                collection: CollectionIdx::new(0),
                field: field.map(FieldIdx::new),
            },
        )
    }
}

fn rule(id: &str, target: &str, ty: CType, nodes: Nodes, body: CompiledBody) -> CompiledRule {
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

fn derived(id: &str, target: &str, nodes: Nodes, root: NodeIdx) -> CompiledRule {
    rule(id, target, NUM, nodes, CompiledBody::Derived { expr: root })
}

/// `a + 1` or `a`, for the clash and pick rules.
fn a_plus(id: &str, target: &str, plus_one: bool) -> CompiledRule {
    let mut n = Nodes::default();
    let a = n.field(0);
    let root = if plus_one {
        let one = n.num("1");
        n.bin(BinaryOp::Add, a, one)
    } else {
        a
    };
    derived(id, target, n, root)
}

fn rulebook() -> CompiledRulebook {
    let mut rules = Vec::new();
    // sum = a + b
    let mut n = Nodes::default();
    let (a, b) = (n.field(0), n.field(1));
    let root = n.bin(BinaryOp::Add, a, b);
    rules.push(derived("r1.sum", "sum", n, root));
    // ratio = round(a / b, 2, half_even)
    let mut n = Nodes::default();
    let (a, b) = (n.field(0), n.field(1));
    let div = n.bin(BinaryOp::Div, a, b);
    let root = n.add(
        NUM,
        NodeKind::Round {
            arg: div,
            rounding: Rounding {
                mode: RoundingMode::HalfEven,
                places: Places::new(2).unwrap(),
            },
        },
    );
    rules.push(derived("r2.ratio", "ratio", n, root));
    // score = 0.6 * a + 0.4 * b, rounded to 1 place
    let mut n = Nodes::default();
    let (w1, a) = (n.num("0.6"), n.field(0));
    let (w2, b) = (n.num("0.4"), n.field(1));
    rules.push(rule(
        "r3.score",
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
            rounding: Some(Rounding {
                mode: RoundingMode::HalfAwayFromZero,
                places: Places::new(1).unwrap(),
            }),
        },
    ));
    // elig = sum >= 10 and not (a < 0)
    let mut n = Nodes::default();
    let sum = n.add(NUM, NodeKind::Ref(target_idx("sum")));
    let ten = n.num("10");
    let ge = n.cmp(CmpOp::Ge, sum, ten);
    let (a, zero) = (n.field(0), n.num("0"));
    let lt = n.cmp(CmpOp::Lt, a, zero);
    let not = n.add(BOOL, NodeKind::Not(lt));
    let root = n.add(BOOL, NodeKind::And(vec![ge, not]));
    rules.push(rule(
        "r4.elig",
        "elig",
        BOOL,
        n,
        CompiledBody::Eligibility { expr: root },
    ));
    // tot = sum(results.place) + count(results)
    let mut n = Nodes::default();
    let (s, c) = (n.agg(AggFn::Sum, Some(0)), n.agg(AggFn::Count, None));
    let root = n.bin(BinaryOp::Add, s, c);
    rules.push(derived("r5.tot", "tot", n, root));
    // best = min(results.place)
    let mut n = Nodes::default();
    let root = n.agg(AggFn::Min, Some(0));
    rules.push(derived("r6.best", "best", n, root));
    // standings: a descending
    let mut n = Nodes::default();
    let key = n.field(0);
    rules.push(rule(
        "r7.rank",
        "standings",
        CType::Ranking,
        n,
        CompiledBody::Ranking {
            key,
            direction: Direction::Desc,
            tie_breakers: Vec::new(),
            ties: Ties::Group,
        },
    ));
    // clash: two rules, no precedence declaration
    rules.push(a_plus("r8.clash.a", "clash", false));
    rules.push(a_plus("r8.clash.b", "clash", true));
    // pick: two rules, the first listed by a precedence declaration
    rules.push(a_plus("r9.pick.a", "pick", false));
    rules.push(a_plus("r9.pick.b", "pick", true));

    rules.sort_by(|x, y| x.id.cmp(&y.id));
    let pos = |id: &str| {
        RuleIdx::new(
            u32::try_from(rules.iter().position(|r| r.id.as_str() == id).unwrap()).unwrap(),
        )
    };
    let precedence = vec![CompiledPrecedence {
        id: RuleId::new("prec.pick").unwrap(),
        target: target_idx("pick"),
        order: vec![pos("r9.pick.a"), pos("r9.pick.b")],
        spans: vec![span(100)],
    }];
    let n = u32::try_from(rules.len()).unwrap();
    CompiledRulebook::from_parts(CompiledParts {
        compiler_version: "0.1.0".into(),
        ir_version: IrVersion::V1,
        rulebook_hash: sha256(b"rulebook"),
        documents: vec![CompiledDocument {
            id: DocumentId::new(DOC).unwrap(),
            sha256: sha256(b"the source document"),
        }],
        inputs: ["a", "b"]
            .iter()
            .map(|n| CompiledInput {
                name: name(n),
                ty: NUM,
            })
            .collect(),
        collections: vec![CompiledCollection {
            name: name("results"),
            fields: vec![CompiledField {
                name: name("place"),
                ty: NUM,
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

/// Cents as a canonical decimal literal, for example `-1234` is `-12.34`.
fn cents(v: i64) -> String {
    let sign = if v < 0 { "-" } else { "" };
    format!(
        "{sign}{}.{:02}",
        v.unsigned_abs() / 100,
        v.unsigned_abs() % 100
    )
}

fn number(v: i64) -> TraceValue {
    TraceValue::Number(parse_literal(&cents(v)).unwrap())
}

#[derive(Clone, Debug)]
struct Person {
    a: Option<i64>,
    b: Option<i64>,
    results: Option<Vec<i64>>,
}

fn person_strategy() -> impl Strategy<Value = Person> {
    let value =
        || prop::option::weighted(0.8, prop_oneof![Just(0_i64), Just(100), -2000_i64..2000]);
    (
        value(),
        value(),
        prop::option::weighted(0.8, prop::collection::vec(-500_i64..500, 0..4)),
    )
        .prop_map(|(a, b, results)| Person { a, b, results })
}

fn pid(i: usize) -> String {
    format!("p{i}")
}

fn competition(people: &[Person]) -> CompetitionData {
    CompetitionData::new(
        people
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut fields = BTreeMap::new();
                for (k, v) in [("a", p.a), ("b", p.b)] {
                    if let Some(v) = v {
                        fields.insert(k.to_owned(), DataValue::Text(cents(v)));
                    }
                }
                let collections = p
                    .results
                    .iter()
                    .map(|r| {
                        let elements: Vec<Element> = r
                            .iter()
                            .map(|v| {
                                BTreeMap::from([("place".to_owned(), DataValue::Text(cents(*v)))])
                            })
                            .collect();
                        ("results".to_owned(), elements)
                    })
                    .collect();
                (
                    ParticipantId::new(pid(i)).unwrap(),
                    Participant::new(fields, collections),
                )
            })
            .collect(),
    )
}

fn questions(count: usize) -> Vec<Question> {
    let mut out = Vec::new();
    for i in 0..count {
        for t in VALUE_TARGETS {
            out.push(Question::Value {
                target: TargetName::new(t).unwrap(),
                participant: ParticipantId::new(pid(i)).unwrap(),
            });
        }
    }
    out.push(Question::Rank {
        target: TargetName::new("standings").unwrap(),
        participants: None,
    });
    out
}

fn same_span(c: &CompiledSpan, t: &TraceSpan) -> bool {
    c.document == t.document
        && c.sha256.as_str() == t.sha256.as_str()
        && (c.start, c.end, c.line, c.column) == (t.start, t.end, t.line, t.column)
}

/// Req 13.2: the trace lists each rule with exactly the spans and hash the
/// rulebook retains.
fn check_rules(rb: &CompiledRulebook, trace: &DecisionTrace, target: &str) {
    let rules = trace.rules();
    assert!(!rules.is_empty(), "trace names no rule");
    for tr in rules {
        let rule = rb.rules().iter().find(|r| r.id == tr.rule).unwrap();
        assert_eq!(rule.revision, tr.revision);
        assert_eq!(
            rule.spans.len(),
            tr.spans.len(),
            "span count of {}",
            tr.rule
        );
        for (c, t) in rule.spans.iter().zip(&tr.spans) {
            assert!(same_span(c, t), "span of {} altered", tr.rule);
            let doc = rb.documents().iter().find(|d| d.id == t.document).unwrap();
            assert_eq!(doc.sha256.as_str(), t.sha256.as_str());
        }
    }
    let ids: BTreeSet<&str> = rules.iter().map(|r| r.rule.as_str()).collect();
    let wanted: Vec<&str> = rb
        .rules()
        .iter()
        .filter(|r| r.target.as_str() == target)
        .map(|r| r.id.as_str())
        .collect();
    // Under a precedence declaration only the first-listed rule applies.
    if target == "pick" {
        assert!(ids.contains("r9.pick.a"));
        assert!(!ids.contains("r9.pick.b"));
    } else {
        for w in wanted {
            assert!(ids.contains(w), "rule {w} of Target {target} is not listed");
        }
    }
    assert!(!trace.spans().is_empty());
}

/// Req 13.4, 13.21: one reason per not-applied rule, with the rule's spans.
fn check_not_applied(rb: &CompiledRulebook, trace: &DecisionTrace) {
    let mut seen = BTreeSet::new();
    for na in trace.not_applied() {
        assert!(seen.insert((na.rule.clone(), na.revision)), "two reasons");
        let rule = rb.rules().iter().find(|r| r.id == na.rule).unwrap();
        assert!(!na.spans.is_empty());
        assert_eq!(rule.spans.len(), na.spans.len());
        for (c, t) in rule.spans.iter().zip(&na.spans) {
            assert!(same_span(c, t));
        }
        match &na.reason {
            NotAppliedReason::ConditionNotSatisfied { condition } => assert!(!condition.is_empty()),
            NotAppliedReason::MissingInput { input } => assert!(!input.is_empty()),
            NotAppliedReason::Precedence { id } => assert_eq!(id, "prec.pick"),
        }
    }
}

fn check_value_question(
    rb: &CompiledRulebook,
    people: &[Person],
    target: &str,
    who: usize,
    outcome: &Outcome,
    trace: &DecisionTrace,
) {
    let p = &people[who];
    let part = pid(who);
    check_rules(rb, trace, target);
    check_not_applied(rb, trace);
    // Req 13.3: every read names a declared input and carries the exact data
    // value.
    for e in trace.entries() {
        match e {
            Entry::Read(r) => {
                assert!(r.default_spans.is_empty(), "no default is declared");
                assert_eq!(r.participant.as_deref(), Some(part.as_str()));
                let expected = match r.name.as_str() {
                    "a" => p.a,
                    "b" => p.b,
                    other => panic!("read of undeclared input {other}"),
                };
                assert_eq!(r.value, number(expected.expect("read of a missing input")));
                assert!(rb.rules().iter().any(|x| x.id == r.rule));
            }
            Entry::Step(s) => assert!(rb.rules().iter().any(|x| x.id == s.rule)),
            Entry::Decision(d) => assert!(rb.rules().iter().any(|x| x.id == d.rule)),
            Entry::Compare(_) | Entry::Place(_) => panic!("ranking entry in a value trace"),
        }
    }
    // The last entry is a decision, and an Indeterminate Outcome has an Unknown
    // decision.
    let Some(Entry::Decision(last)) = trace.entries().last() else {
        panic!("no final decision");
    };
    assert_eq!(
        matches!(outcome, Outcome::Decided(_)),
        matches!(last.value, DecisionValue::Known(_))
    );
    // Req 13.5: missing inputs are recorded by name, never read, never
    // defaulted.
    for m in trace.missing() {
        assert_eq!(m.participant, part);
        assert!(!trace.entries().iter().any(|e| matches!(e,
            Entry::Read(r) if r.name == m.field && r.participant.as_deref() == Some(&m.participant))));
    }
    match target {
        "sum" => {
            let missing: Vec<&str> = [("a", p.a), ("b", p.b)]
                .iter()
                .filter(|(_, v)| v.is_none())
                .map(|(k, _)| *k)
                .collect();
            let recorded: Vec<&str> = trace.missing().iter().map(|m| m.field.as_str()).collect();
            assert_eq!(recorded, missing);
            match (p.a, p.b) {
                (Some(a), Some(b)) => {
                    assert_eq!(outcome, &Outcome::Decided(number(a + b)));
                    // Req 13.3: the step has the read values as operands and
                    // the exact result.
                    assert!(trace.entries().iter().any(|e| matches!(e,
                        Entry::Step(s) if s.operands == vec![number(a), number(b)]
                            && s.result == rt_trace::StepResult::Value(number(a + b)))));
                }
                _ => {
                    let Outcome::Indeterminate(reasons) = outcome else {
                        panic!("missing input gave a value");
                    };
                    assert!(reasons
                        .iter()
                        .all(|r| matches!(r, Reason::MissingInput { .. })));
                    assert_eq!(reasons.len(), missing.len());
                }
            }
        }
        "clash" => {
            if p.a.is_some() {
                // Req 13.6: conflict recorded, both rules listed, no winner
                // selected.
                let Outcome::Indeterminate(reasons) = outcome else {
                    panic!("conflict gave a value");
                };
                assert!(reasons.contains(&Reason::Diagnosed(Code::ExecutionConflictDetected)));
                assert!(trace
                    .diagnostics()
                    .as_slice()
                    .iter()
                    .any(|d| d.code() == Code::ExecutionConflictDetected));
                let ids: Vec<&str> = trace.rules().iter().map(|r| r.rule.as_str()).collect();
                assert_eq!(ids, ["r8.clash.a", "r8.clash.b"]);
                assert!(trace.entries().iter().all(|e| match e {
                    Entry::Decision(d) => d.value == DecisionValue::Unknown,
                    _ => true,
                }));
            }
        }
        "pick" => {
            if let Some(a) = p.a {
                assert_eq!(outcome, &Outcome::Decided(number(a)));
                let na = trace.not_applied();
                assert_eq!(na.len(), 1);
                assert_eq!(na[0].rule.as_str(), "r9.pick.b");
                assert_eq!(
                    na[0].reason,
                    NotAppliedReason::Precedence {
                        id: "prec.pick".to_owned()
                    }
                );
            }
        }
        _ => {}
    }
}

/// Req 12.7 and 13.3 for the ranking question.
fn check_rank_question(people: &[Person], outcome: &Outcome, trace: &DecisionTrace) {
    let Outcome::Decided(TraceValue::Standings(places)) = outcome else {
        assert!(
            people.iter().any(|p| p.a.is_none()),
            "ranking without cause"
        );
        return;
    };
    let compares: Vec<_> = trace
        .entries()
        .iter()
        .filter_map(|e| match e {
            Entry::Compare(c) => Some(c),
            _ => None,
        })
        .collect();
    let members: usize = places.iter().map(|p| p.members().len()).sum();
    assert_eq!(members, people.len());
    assert_eq!(compares.len(), people.len().saturating_sub(1));
    for c in compares {
        let value = |who: &str| {
            let i: usize = who[1..].parse().unwrap();
            number(people[i].a.unwrap())
        };
        let first = c.criteria.first().unwrap();
        assert_eq!(first.left, value(&c.between.0));
        assert_eq!(first.right, value(&c.between.1));
        let (l, r) = (
            people[c.between.0[1..].parse::<usize>().unwrap()]
                .a
                .unwrap(),
            people[c.between.1[1..].parse::<usize>().unwrap()]
                .a
                .unwrap(),
        );
        // Descending: the participant that is ahead has the larger value; tied
        // means equal.
        assert_eq!(c.first_ahead, l > r);
        assert!(c.first_ahead || l == r);
    }
}

fn check_all(people: &[Person]) {
    let rb = rulebook();
    let data = competition(people);
    let qs = questions(people.len());
    let eval: Evaluation = evaluate(&rb, &data, &qs, None);
    // Req 13.1: one trace per Outcome, each with its own Outcome.
    assert_eq!(eval.outcomes.len(), qs.len());
    assert_eq!(eval.traces.len(), qs.len());
    // Req 13.3 and 10.11: same inputs, same trace, in the same order.
    assert_eq!(evaluate(&rb, &data, &qs, None), eval);
    for ((q, outcome), trace) in qs.iter().zip(&eval.outcomes).zip(&eval.traces) {
        assert_eq!(trace.outcome(), outcome);
        // Req 13.1: entries are in evaluation order, entry n has sequence
        // number n, and each decision refers only to earlier entries
        // (contiguous, 1-based, no forward reference).
        for (i, e) in trace.entries().iter().enumerate() {
            if let Entry::Decision(d) = e {
                let seq = u32::try_from(i + 1).unwrap();
                assert!(d.from.iter().all(|&to| (1..seq).contains(&to)));
            }
        }
        // Req 13.11: replay reproduces the Outcome exactly.
        assert_eq!(replay(trace).as_ref(), Ok(outcome), "replay of {q:?}");
        match q {
            Question::Value {
                target,
                participant,
            } => {
                let who: usize = participant.as_str()[1..].parse().unwrap();
                check_value_question(&rb, people, target.as_str(), who, outcome, trace);
            }
            Question::Rank { target, .. } => {
                check_rules(&rb, trace, target.as_str());
                check_rank_question(people, outcome, trace);
            }
        }
    }
}

property_tests! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Feature: ruletrace-compiler, Property 25: Trace completeness and replay
    #[test]
    fn trace_is_complete_and_replays(people in prop::collection::vec(person_strategy(), 1..5)) {
        check_all(&people);
    }
}

#[test]
fn slice_with_all_values_present_replays() {
    check_all(&[
        Person {
            a: Some(1050),
            b: Some(0),
            results: Some(vec![3, 1]),
        },
        Person {
            a: Some(1050),
            b: Some(250),
            results: Some(Vec::new()),
        },
    ]);
}
