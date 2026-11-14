//! Property 24: Input order independence (task 5.22).
//!
//! Feature: ruletrace-compiler, Property 24: Input order independence
//!
//! Permuting the participants, the fields and the elements of every collection of the
//! Competition_Data (and the order in which the participant set of a rank Question is written)
//! leaves every Outcome, Decision_Trace, Diagnostic and the comparison byte-identical.
//!
//! **Validates: Requirements 10.4, 10.3, 10.1**

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

use proptest::prelude::*;
use rt_compiled::{
    CType, CollectionIdx, CompiledBody, CompiledCollection, CompiledDocument, CompiledField,
    CompiledInput, CompiledNode, CompiledParts, CompiledRule, CompiledRulebook, CompiledSpan,
    CompiledTieBreaker, FieldIdx, InputIdx, NodeIdx, NodeKind, RuleIdx,
};
use rt_data::{
    serialize_data, CompetitionData, DataValue, Element, Participant, ParticipantId, Question,
    TargetName,
};
use rt_foundation::{DocumentId, RevisionNumber, RuleId};
use rt_hash::sha256;
use rt_ir::{AggFn, BinaryOp, Direction, IrVersion, Name, Ties};
use std::collections::{BTreeMap, BTreeSet};

const NUM: CType = CType::Number;

fn name(s: &str) -> Name {
    Name::new(s).unwrap()
}

fn span(start: u32) -> CompiledSpan {
    CompiledSpan {
        document: DocumentId::new("rules.md").unwrap(),
        sha256: sha256(b"the source document"),
        start,
        end: start + 10,
        line: 3,
        column: 1,
    }
}

fn node(nodes: &mut Vec<CompiledNode>, ty: CType, kind: NodeKind) -> NodeIdx {
    nodes.push(CompiledNode { ty, kind });
    NodeIdx::new(u32::try_from(nodes.len() - 1).unwrap())
}

fn rule(
    id: &str,
    target: &str,
    ty: CType,
    nodes: Vec<CompiledNode>,
    body: CompiledBody,
) -> CompiledRule {
    CompiledRule {
        id: RuleId::new(id).unwrap(),
        revision: RevisionNumber::new(1).unwrap(),
        target: name(target),
        ty,
        spans: vec![span(20), span(60)],
        nodes,
        body,
    }
}

fn agg(nodes: &mut Vec<CompiledNode>, func: AggFn) -> NodeIdx {
    let field = (func != AggFn::Count).then(|| FieldIdx::new(0));
    node(
        nodes,
        NUM,
        NodeKind::Agg {
            func,
            collection: CollectionIdx::new(0),
            field,
        },
    )
}

/// Inputs `a` and `b`, a collection `results` with the field `place`, four aggregate Targets
/// over it, a derived Target that adds an input, and a ranking on `a` then `b`.
fn rulebook() -> CompiledRulebook {
    let mut rules = Vec::new();
    for (id, target, func) in [
        ("calc.count", "n", AggFn::Count),
        ("calc.sum", "total", AggFn::Sum),
        ("calc.min", "best", AggFn::Min),
        ("calc.max", "worst", AggFn::Max),
    ] {
        let mut nodes = Vec::new();
        let root = agg(&mut nodes, func);
        rules.push(rule(
            id,
            target,
            NUM,
            nodes,
            CompiledBody::Derived { expr: root },
        ));
    }
    let mut nodes = Vec::new();
    let sum = agg(&mut nodes, AggFn::Sum);
    let a = node(&mut nodes, NUM, NodeKind::Field(InputIdx::new(0)));
    let root = node(
        &mut nodes,
        NUM,
        NodeKind::Binary {
            op: BinaryOp::Add,
            left: sum,
            right: a,
        },
    );
    rules.push(rule(
        "calc.mix",
        "mix",
        NUM,
        nodes,
        CompiledBody::Derived { expr: root },
    ));
    let mut nodes = Vec::new();
    let key = node(&mut nodes, NUM, NodeKind::Field(InputIdx::new(0)));
    let tb = node(&mut nodes, NUM, NodeKind::Field(InputIdx::new(1)));
    rules.push(rule(
        "rank.standings",
        "standings",
        CType::Ranking,
        nodes,
        CompiledBody::Ranking {
            key,
            direction: Direction::Desc,
            tie_breakers: vec![CompiledTieBreaker {
                expr: tb,
                direction: Direction::Asc,
            }],
            ties: Ties::Group,
        },
    ));
    rules.sort_by(|x, y| x.id.cmp(&y.id));
    let n = u32::try_from(rules.len()).unwrap();
    CompiledRulebook::from_parts(CompiledParts {
        compiler_version: "0.1.0".into(),
        ir_version: IrVersion::V1,
        rulebook_hash: sha256(b"rulebook"),
        documents: vec![CompiledDocument {
            id: DocumentId::new("rules.md").unwrap(),
            sha256: sha256(b"the source document"),
        }],
        inputs: vec![
            CompiledInput {
                name: name("a"),
                ty: NUM,
            },
            CompiledInput {
                name: name("b"),
                ty: NUM,
            },
        ],
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
        precedence: Vec::new(),
    })
    .unwrap()
}

/// One participant before it is placed in any order.
#[derive(Clone, Debug)]
struct Spec {
    id: String,
    a: Option<String>,
    b: Option<String>,
    places: Vec<String>,
}

fn number_text() -> impl Strategy<Value = String> {
    prop_oneof![
        8 => (0u8..4).prop_map(|n| n.to_string()),
        2 => (0u8..4).prop_map(|n| format!("{n}.5")),
        1 => Just("not-a-number".to_owned()),
    ]
}

fn spec(index: usize) -> impl Strategy<Value = Spec> {
    (
        proptest::option::weighted(0.85, number_text()),
        proptest::option::weighted(0.85, number_text()),
        proptest::collection::vec(number_text(), 0..6),
    )
        .prop_map(move |(a, b, places)| Spec {
            id: format!("p{index}"),
            a,
            b,
            places,
        })
}

fn specs() -> impl Strategy<Value = Vec<Spec>> {
    (0usize..6).prop_flat_map(|n| (0..n).map(spec).collect::<Vec<_>>())
}

/// A permutation of `0..len` derived from `keys` (ties broken by position, so it is total).
fn permutation(keys: &[u32], len: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    order.sort_by_key(|i| (keys.get(*i % keys.len().max(1)).copied().unwrap_or(0), *i));
    order
}

fn permuted<T: Clone>(items: &[T], keys: &[u32]) -> Vec<T> {
    permutation(keys, items.len())
        .into_iter()
        .filter_map(|i| items.get(i).cloned())
        .collect()
}

/// Builds the data inserting participants, fields and elements in the order given by `keys`.
fn build(specs: &[Spec], keys: Option<&[u32]>) -> CompetitionData {
    let ordered = match keys {
        Some(k) => permuted(specs, k),
        None => specs.to_vec(),
    };
    let mut people = BTreeMap::new();
    for s in &ordered {
        let mut fields: Vec<(&str, &String)> = Vec::new();
        if let Some(a) = &s.a {
            fields.push(("a", a));
        }
        if let Some(b) = &s.b {
            fields.push(("b", b));
        }
        let fields = match keys {
            Some(k) => permuted(&fields, k),
            None => fields,
        };
        let fields: BTreeMap<String, DataValue> = fields
            .into_iter()
            .map(|(n, v)| (n.to_owned(), DataValue::Text(v.clone())))
            .collect();
        let places = match keys {
            Some(k) => permuted(&s.places, k),
            None => s.places.clone(),
        };
        let elements: Vec<Element> = places
            .into_iter()
            .map(|p| BTreeMap::from([("place".to_owned(), DataValue::Text(p))]))
            .collect();
        let collections = BTreeMap::from([("results".to_owned(), elements)]);
        people.insert(
            ParticipantId::new(s.id.clone()).unwrap(),
            Participant::new(fields, collections),
        );
    }
    CompetitionData::new(people)
}

fn questions(specs: &[Spec], subset_order: Option<&[u32]>) -> Vec<Question> {
    let mut out = Vec::new();
    for s in specs {
        for target in ["n", "total", "best", "worst", "mix"] {
            out.push(Question::Value {
                target: TargetName::new(target).unwrap(),
                participant: ParticipantId::new(s.id.clone()).unwrap(),
            });
        }
    }
    out.push(Question::Rank {
        target: TargetName::new("standings").unwrap(),
        participants: None,
    });
    let ids: Vec<ParticipantId> = specs
        .iter()
        .step_by(2)
        .map(|s| ParticipantId::new(s.id.clone()).unwrap())
        .collect();
    let ids = match subset_order {
        Some(k) => permuted(&ids, k),
        None => ids,
    };
    out.push(Question::Rank {
        target: TargetName::new("standings").unwrap(),
        participants: Some(ids.into_iter().collect::<BTreeSet<_>>()),
    });
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Feature: ruletrace-compiler, Property 24: Input order independence
    /// **Validates: Requirements 10.4, 10.3, 10.1**
    #[test]
    fn permuting_participants_fields_and_elements_leaves_results_byte_identical(
        specs in specs(),
        keys in proptest::collection::vec(any::<u32>(), 1..16),
    ) {
        let rb = rulebook();
        let plain = build(&specs, None);
        let shuffled = build(&specs, Some(&keys));
        // The data itself is the same value and serializes to the same bytes.
        prop_assert_eq!(serialize_data(&plain), serialize_data(&shuffled));
        prop_assert_eq!(&plain, &shuffled);

        let q_plain = questions(&specs, None);
        let q_shuffled = questions(&specs, Some(&keys));
        prop_assert_eq!(&q_plain, &q_shuffled);

        let first = rt_engine::evaluate(&rb, &plain, &q_plain, None);
        let second = rt_engine::evaluate(&rb, &shuffled, &q_shuffled, None);
        prop_assert_eq!(&first, &second);
        prop_assert_eq!(format!("{first:?}"), format!("{second:?}"));
        for (o, t) in first.outcomes.iter().zip(&first.traces) {
            let replayed = rt_trace::replay(t);
            prop_assert_eq!(replayed.as_ref(), Ok(o));
        }
    }
}

/// A fixed case with a tie and an empty collection (Indeterminate min and max), so the property cannot pass
/// vacuously on a generator that never produces them.
#[test]
fn fixed_case_with_tie_and_missing_value_is_order_independent() {
    let specs = vec![
        Spec {
            id: "p0".into(),
            a: Some("2".into()),
            b: Some("1".into()),
            places: vec!["3".into(), "1".into(), "2".into()],
        },
        Spec {
            id: "p1".into(),
            a: Some("2".into()),
            b: Some("1".into()),
            places: vec![],
        },
        Spec {
            id: "p2".into(),
            a: Some("1".into()),
            b: Some("0".into()),
            places: vec!["2".into(), "2".into()],
        },
    ];
    let rb = rulebook();
    let qs = questions(&specs, None);
    let base = rt_engine::evaluate(&rb, &build(&specs, None), &qs, None);
    for keys in [[3u32, 2, 1, 0], [1, 0, 3, 2], [9, 9, 9, 9]] {
        let other = rt_engine::evaluate(&rb, &build(&specs, Some(&keys)), &qs, None);
        assert_eq!(base, other);
    }
    assert!(base
        .outcomes
        .iter()
        .any(|o| matches!(o, rt_trace::Outcome::Decided(_))));
}
