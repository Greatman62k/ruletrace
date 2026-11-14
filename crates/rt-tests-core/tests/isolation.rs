//! Property 40: Isolation of compiled rulebooks.
//!
//! Feature: ruletrace-compiler, Property 40: Isolation of compiled rulebooks
//!
//! Two different Compiled_Rulebooks are evaluated in one process, A then B, B
//! then A, and interleaved per record. Each one's outcomes and traces equal its
//! solitary evaluation.
//!
//! **Validates: Requirements 9.7, 10.8, 20.6**

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
    CompetitionData, DataValue, Element, Participant, ParticipantId, Question, TargetName,
};
use rt_engine::evaluate;
use rt_foundation::{DocumentId, RevisionNumber, RuleId};
use rt_hash::sha256;
use rt_ir::{AggFn, Direction, IrVersion, Name, Ties};
use std::collections::BTreeMap;

fn name(s: &str) -> Name {
    Name::new(s).unwrap()
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
        spans: vec![CompiledSpan {
            document: DocumentId::new("rules.md").unwrap(),
            sha256: sha256(b"the source document"),
            start: 20,
            end: 30,
            line: 3,
            column: 1,
        }],
        nodes,
        body,
    }
}

/// Same names in every rulebook (input `k`, collection `c.v`, Targets `agg` and
/// `rank`), but a different aggregate, direction and tag, so any shared state
/// would show.
fn rulebook(func: AggFn, direction: Direction, tag: &str) -> CompiledRulebook {
    let num = |kind| CompiledNode {
        ty: CType::Number,
        kind,
    };
    let field = (func != AggFn::Count).then(|| FieldIdx::new(0));
    let agg = rule(
        "calc.agg",
        "agg",
        CType::Number,
        vec![num(NodeKind::Agg {
            func,
            collection: CollectionIdx::new(0),
            field,
        })],
        CompiledBody::Derived {
            expr: NodeIdx::new(0),
        },
    );
    let ranking = rule(
        "rank.rank",
        "rank",
        CType::Ranking,
        vec![num(NodeKind::Field(InputIdx::new(0)))],
        CompiledBody::Ranking {
            key: NodeIdx::new(0),
            direction,
            tie_breakers: Vec::<CompiledTieBreaker>::new(),
            ties: Ties::Group,
        },
    );
    CompiledRulebook::from_parts(CompiledParts {
        compiler_version: "0.1.0".into(),
        ir_version: IrVersion::V1,
        rulebook_hash: sha256(tag.as_bytes()),
        documents: vec![CompiledDocument {
            id: DocumentId::new("rules.md").unwrap(),
            sha256: sha256(b"the source document"),
        }],
        inputs: vec![CompiledInput {
            name: name("k"),
            ty: CType::Number,
        }],
        collections: vec![CompiledCollection {
            name: name("c"),
            fields: vec![CompiledField {
                name: name("v"),
                ty: CType::Number,
            }],
        }],
        enums: Vec::new(),
        sets: Vec::new(),
        order: vec![RuleIdx::new(0), RuleIdx::new(1)],
        rules: vec![agg, ranking],
        precedence: Vec::new(),
    })
    .unwrap()
}

type Spec = (Option<u8>, Vec<u8>);

fn specs() -> impl Strategy<Value = Vec<Spec>> {
    proptest::collection::vec(
        (
            proptest::option::weighted(0.8, 0u8..4),
            proptest::collection::vec(0u8..5, 0..4),
        ),
        0..5,
    )
}

fn data(specs: &[Spec]) -> CompetitionData {
    let mut people = BTreeMap::new();
    for (i, (k, vs)) in specs.iter().enumerate() {
        let fields: BTreeMap<String, DataValue> = k
            .iter()
            .map(|k| ("k".to_owned(), DataValue::Text(k.to_string())))
            .collect();
        let elements: Vec<Element> = vs
            .iter()
            .map(|v| BTreeMap::from([("v".to_owned(), DataValue::Text(v.to_string()))]))
            .collect();
        people.insert(
            ParticipantId::new(format!("p{i}")).unwrap(),
            Participant::new(fields, BTreeMap::from([("c".to_owned(), elements)])),
        );
    }
    CompetitionData::new(people)
}

fn questions(n: usize) -> Vec<Question> {
    let mut out: Vec<Question> = (0..n)
        .map(|i| Question::Value {
            target: TargetName::new("agg").unwrap(),
            participant: ParticipantId::new(format!("p{i}")).unwrap(),
        })
        .collect();
    out.push(Question::Rank {
        target: TargetName::new("rank").unwrap(),
        participants: None,
    });
    out
}

/// Outcomes and traces as bytes of their Debug form (the types are not
/// serializable here).
fn view(r: &rt_engine::Evaluation) -> String {
    format!("{:?}|{:?}", r.outcomes, r.traces)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Feature: ruletrace-compiler, Property 40: Isolation of compiled
    /// rulebooks **Validates: Requirements 9.7, 10.8, 20.6**
    #[test]
    fn evaluation_of_two_rulebooks_equals_each_solitary_evaluation(
        sa in specs(),
        sb in specs(),
        fa in prop_oneof![Just(AggFn::Sum), Just(AggFn::Count), Just(AggFn::Min)],
        fb in prop_oneof![Just(AggFn::Max), Just(AggFn::Sum), Just(AggFn::Count)],
    ) {
        let (a, b) = (rulebook(fa, Direction::Desc, "a"), rulebook(fb, Direction::Asc, "b"));
        let (da, db) = (data(&sa), data(&sb));
        let (qa, qb) = (questions(sa.len()), questions(sb.len()));

        let solo_a = view(&evaluate(&a, &da, &qa, None));
        let solo_b = view(&evaluate(&b, &db, &qb, None));

        // A then B, then B then A, with the rulebooks kept alive between runs.
        let a1 = view(&evaluate(&a, &da, &qa, None));
        let b1 = view(&evaluate(&b, &db, &qb, None));
        let b2 = view(&evaluate(&b, &db, &qb, None));
        let a2 = view(&evaluate(&a, &da, &qa, None));
        prop_assert_eq!((&a1, &a2), (&solo_a, &solo_a));
        prop_assert_eq!((&b1, &b2), (&solo_b, &solo_b));

        // Interleaved per record: one Question of A, one of B, and so on.
        let (mut out_a, mut out_b) = (Vec::new(), Vec::new());
        let (mut tr_a, mut tr_b) = (Vec::new(), Vec::new());
        for i in 0..qa.len().max(qb.len()) {
            if let Some(q) = qa.get(i) {
                let r = evaluate(&a, &da, std::slice::from_ref(q), None);
                out_a.extend(r.outcomes);
                tr_a.extend(r.traces);
            }
            if let Some(q) = qb.get(i) {
                let r = evaluate(&b, &db, std::slice::from_ref(q), None);
                out_b.extend(r.outcomes);
                tr_b.extend(r.traces);
            }
        }
        prop_assert_eq!(format!("{out_a:?}|{tr_a:?}"), solo_a);
        prop_assert_eq!(format!("{out_b:?}|{tr_b:?}"), solo_b);
    }
}
