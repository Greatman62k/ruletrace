//! Property 23: Ranking completeness and positions.
//!
//! **Validates: Requirements 12.1, 12.2, 12.3, 12.8, 12.9, 20.2**
//!
//! Feature: ruletrace-compiler, Property 23: Ranking completeness and
//! positions.
//!
//! The Compiled_Rulebook is built with `CompiledRulebook::from_parts`. The
//! expectation is a simple reference ranking over the generated values: the
//! position of a participant is 1 plus the number of participants strictly
//! ahead on (key, tie-breaker).
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
    CType, CompiledBody, CompiledDocument, CompiledInput, CompiledNode, CompiledParts,
    CompiledRule, CompiledRulebook, CompiledSpan, CompiledTieBreaker, InputIdx, NodeIdx, NodeKind,
    RuleIdx,
};
use rt_data::{CompetitionData, DataValue, Participant, ParticipantId, Question, TargetName};
use rt_engine::{evaluate, Outcome};
use rt_foundation::{Code, DocumentId, RevisionNumber, RuleId};
use rt_hash::sha256;
use rt_ir::{Direction, IrVersion, Name, Ties};
use rt_trace::{Entry, TraceValue};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

fn name(s: &str) -> Name {
    Name::new(s).unwrap()
}

fn field(input: u32) -> CompiledNode {
    CompiledNode {
        ty: CType::Number,
        kind: NodeKind::Field(InputIdx::new(input)),
    }
}

/// `standings` ranks by input `a`, then breaks ties by input `b`, each in its
/// own direction.
fn rulebook(key: Direction, tie: Option<Direction>) -> CompiledRulebook {
    let sha = sha256(b"the source document");
    let span = CompiledSpan {
        document: DocumentId::new("rules.md").unwrap(),
        sha256: sha.clone(),
        start: 20,
        end: 30,
        line: 3,
        column: 1,
    };
    let tie_breakers = tie
        .map(|direction| CompiledTieBreaker {
            expr: NodeIdx::new(1),
            direction,
        })
        .into_iter()
        .collect();
    let rule = CompiledRule {
        id: RuleId::new("rank.main").unwrap(),
        revision: RevisionNumber::new(1).unwrap(),
        target: name("standings"),
        ty: CType::Ranking,
        spans: vec![span],
        nodes: vec![field(0), field(1)],
        body: CompiledBody::Ranking {
            key: NodeIdx::new(0),
            direction: key,
            tie_breakers,
            ties: Ties::Group,
        },
    };
    CompiledRulebook::from_parts(CompiledParts {
        compiler_version: "0.1.0".into(),
        ir_version: IrVersion::V1,
        rulebook_hash: sha256(b"rulebook"),
        documents: vec![CompiledDocument {
            id: DocumentId::new("rules.md").unwrap(),
            sha256: sha,
        }],
        inputs: ["a", "b"]
            .iter()
            .map(|n| CompiledInput {
                name: name(n),
                ty: CType::Number,
            })
            .collect(),
        collections: Vec::new(),
        enums: Vec::new(),
        sets: Vec::new(),
        order: vec![RuleIdx::new(0)],
        rules: vec![rule],
        precedence: Vec::new(),
    })
    .unwrap()
}

type Scores = Vec<(Option<i32>, Option<i32>)>;

fn pid(i: usize) -> String {
    format!("p{i:02}")
}

fn data_of(scores: &Scores) -> CompetitionData {
    let people = scores.iter().enumerate().map(|(i, (a, b))| {
        let mut m = BTreeMap::new();
        for (key, v) in [("a", a), ("b", b)] {
            if let Some(v) = v {
                m.insert(key.to_owned(), DataValue::Text(v.to_string()));
            }
        }
        (
            ParticipantId::new(pid(i)).unwrap(),
            Participant::new(m, BTreeMap::new()),
        )
    });
    CompetitionData::new(people.collect())
}

fn direction() -> impl Strategy<Value = Direction> {
    prop_oneof![Just(Direction::Asc), Just(Direction::Desc)]
}

/// Few distinct values, so that ties and tied tie-breakers are common; `None`
/// is missing.
fn scores() -> impl Strategy<Value = Scores> {
    let value = prop::option::weighted(0.9, 0..4i32);
    prop::collection::vec((value.clone(), value), 0..9)
}

/// `Less` when `x` is ahead of `y` under `direction`.
fn ahead(direction: Direction, x: i32, y: i32) -> Ordering {
    match direction {
        Direction::Asc => x.cmp(&y),
        Direction::Desc => y.cmp(&x),
    }
}

/// The reference ranking: the positions as `(position, members)`, or `None`
/// when a needed value is missing (the key of anyone, or the tie-breaker of a
/// participant that shares its key).
fn reference(
    scores: &Scores,
    key: Direction,
    tie: Option<Direction>,
) -> Option<Vec<(u32, Vec<String>)>> {
    let mut full = Vec::new();
    for (i, (a, b)) in scores.iter().enumerate() {
        let a = (*a)?;
        let shares = scores
            .iter()
            .enumerate()
            .any(|(j, (o, _))| j != i && *o == Some(a));
        let b = match (tie, b) {
            (Some(_), Some(b)) => *b,
            (Some(_), None) if shares => return None,
            _ => 0,
        };
        full.push((a, b));
    }
    let order = |p: &(i32, i32), q: &(i32, i32)| {
        ahead(key, p.0, q.0).then(match tie {
            Some(d) => ahead(d, p.1, q.1),
            None => Ordering::Equal,
        })
    };
    let mut places: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for (i, p) in full.iter().enumerate() {
        let strictly_ahead = full
            .iter()
            .filter(|q| order(q, p) == Ordering::Less)
            .count();
        places
            .entry(u32::try_from(strictly_ahead + 1).unwrap())
            .or_default()
            .push(pid(i));
    }
    Some(places.into_iter().collect())
}

fn check(scores: &Scores, key: Direction, tie: Option<Direction>) -> Result<(), TestCaseError> {
    let rb = rulebook(key, tie);
    let q = Question::Rank {
        target: TargetName::new("standings").unwrap(),
        participants: None,
    };
    let e = evaluate(&rb, &data_of(scores), &[q], None);
    prop_assert_eq!(e.outcomes.len(), 1);
    prop_assert_eq!(e.traces.len(), 1);
    let replayed = rt_trace::replay(&e.traces[0]);
    prop_assert_eq!(replayed.as_ref(), Ok(&e.outcomes[0]));

    match (&e.outcomes[0], reference(scores, key, tie)) {
        (Outcome::Decided(TraceValue::Standings(places)), Some(expected)) => {
            let got: Vec<(u32, Vec<String>)> = places
                .iter()
                .map(|p| (p.position(), p.members().to_vec()))
                .collect();
            // Req 12.9, 20.2: equal to the reference ranking (positions and
            // members).
            prop_assert_eq!(&got, &expected);
            // Req 12.8: every participant holds exactly one position;
            // Tie_Groups are disjoint.
            let all: Vec<&String> = got.iter().flat_map(|(_, m)| m).collect();
            let unique: BTreeSet<&String> = all.iter().copied().collect();
            prop_assert_eq!(all.len(), scores.len());
            prop_assert_eq!(unique.len(), scores.len());
            // Req 12.2: members are listed in ascending identifier order, and a
            // position is 1 plus the number of participants in the places
            // before it.
            let mut before = 0usize;
            for (position, members) in &got {
                prop_assert!(members.windows(2).all(|w| w[0] < w[1]));
                prop_assert_eq!(*position as usize, before + 1);
                before += members.len();
            }
            // Req 12.2: each Tie_Group is recorded as one group place in the
            // trace.
            let groups = e.traces[0]
                .entries()
                .iter()
                .filter(|x| matches!(x, Entry::Place(p) if p.is_tie_group()))
                .count();
            prop_assert_eq!(groups, got.iter().filter(|(_, m)| m.len() > 1).count());
            prop_assert!(e.diagnostics.as_slice().is_empty());
        }
        (Outcome::Indeterminate(_), None) => {
            // Req 12.4: a missing value that a comparison needs is reported,
            // never defaulted.
            let found = e
                .diagnostics
                .as_slice()
                .iter()
                .any(|d| d.code() == Code::ExeRankMissingValue);
            prop_assert!(found);
        }
        (other, expected) => prop_assert!(false, "outcome {other:?} but reference {expected:?}"),
    }
    Ok(())
}

property_tests! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Feature: ruletrace-compiler, Property 23: Ranking completeness and
    /// positions.
    #[test]
    fn ranking_is_complete_and_positions_count_those_strictly_ahead(
        s in scores(),
        key in direction(),
        tie in prop::option::of(direction()),
    ) {
        check(&s, key, tie)?;
    }
}

#[test]
fn a_tie_group_shares_a_position_and_the_next_position_skips_its_size() {
    let s: Scores = vec![
        (Some(10), None),
        (Some(7), None),
        (Some(10), None),
        (Some(3), None),
    ];
    let expected = reference(&s, Direction::Desc, None).unwrap();
    assert_eq!(
        expected,
        vec![
            (1, vec![pid(0), pid(2)]),
            (3, vec![pid(1)]),
            (4, vec![pid(3)])
        ]
    );
    check(&s, Direction::Desc, None).unwrap();
}
