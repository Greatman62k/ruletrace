//! Published ranking positions and ties (task 9.2, Req 13.12, 13.20, 31.7,
//! 32.6, design §3.6.3). Expected values are computed by hand from the
//! scores, never by the engine.

use crate::tests::*;
use crate::tests_rank::{pnum, published, rank_all, ranking, scored, INPUTS};
use crate::*;
use proptest::prelude::*;
use rt_compiled::Direction;
use rt_data::PublishedValue;
use rt_trace::{Entry, TraceValue};
use std::collections::BTreeMap;

fn run(
    scores: &[(&str, Option<&str>)],
    entries: &[(&str, PublishedValue)],
) -> (Evaluation, Comparison) {
    let rb = build(&INPUTS, vec![ranking("rank.main", Direction::Desc, &[])]);
    let people: Vec<(&str, Option<&str>, Option<&str>)> =
        scores.iter().map(|(p, a)| (*p, *a, None)).collect();
    let rows: Vec<(&str, &str, PublishedValue)> = entries
        .iter()
        .map(|(p, v)| (*p, "standings", v.clone()))
        .collect();
    let e = evaluate(
        &rb,
        &scored(&people),
        &[rank_all()],
        Some(&published(&rows)),
    );
    let c = e.comparison.clone().expect("a comparison");
    (e, c)
}

fn summary(c: &Comparison) -> Vec<(&str, DifferenceKind, Vec<&str>)> {
    c.differences
        .iter()
        .map(|d| {
            (
                d.participant.as_str(),
                d.kind,
                d.group.iter().map(|g| g.as_str()).collect(),
            )
        })
        .collect()
}

use DifferenceKind::ValueDiffers as Plain;
use DifferenceKind::{
    TieBrokenWithoutDeclaredCriterion as Split, TieNotInComputedResult as Joined,
};

// Scores 10, 10, 2 give computed positions 1, 1, 3.
const TIED: [(&str, Option<&str>); 3] = [("p1", Some("10")), ("p2", Some("10")), ("p3", Some("2"))];
// Scores 9, 5, 1 give computed positions 1, 2, 3.
const APART: [(&str, Option<&str>); 3] = [("p1", Some("9")), ("p2", Some("5")), ("p3", Some("1"))];

#[test]
fn the_names_of_the_kinds_are_the_design_names() {
    assert_eq!(Plain.as_str(), "value_differs");
    assert_eq!(Split.as_str(), "tie_broken_without_declared_criterion");
    assert_eq!(Joined.as_str(), "tie_not_in_computed_result");
}

#[test]
fn equal_positions_including_a_computed_tie_give_no_difference() {
    let (_, c) = run(
        &TIED,
        &[("p1", pnum("1")), ("p2", pnum("1")), ("p3", pnum("3"))],
    );
    assert!(c.is_equal());
    assert_eq!(c.compared.len(), 3);
    assert!(c.differences.is_empty());
}

#[test]
fn a_published_split_of_a_computed_tie_group_is_a_difference_for_the_member_that_differs() {
    let (e, c) = run(
        &TIED,
        &[("p1", pnum("1")), ("p2", pnum("2")), ("p3", pnum("3"))],
    );
    assert_eq!(summary(&c), vec![("p2", Split, vec!["p1", "p2"])]);
    let d = &c.differences[0];
    assert_eq!(d.published, pnum("2"));
    assert_eq!(d.computed, Computed::Value(TraceValue::Number(number("1"))));
    // The reference is the Place entry of the group in the trace.
    let entry = usize::try_from(d.entry.unwrap()).unwrap();
    assert!(matches!(
        &e.traces[0].entries()[entry - 1],
        Entry::Place(p) if p.members() == ["p1".to_owned(), "p2".to_owned()]
    ));
    assert!(d.message.contains("published 2 differs from computed 1"));
}

#[test]
fn every_member_of_a_split_group_whose_position_differs_is_reported() {
    let (_, c) = run(
        &TIED,
        &[("p1", pnum("2")), ("p2", pnum("3")), ("p3", pnum("4"))],
    );
    assert_eq!(
        summary(&c),
        vec![
            ("p1", Split, vec!["p1", "p2"]),
            ("p2", Split, vec!["p1", "p2"]),
            ("p3", Plain, vec![]),
        ]
    );
}

#[test]
fn a_published_tie_that_the_computed_result_separates_is_a_difference() {
    let (e, c) = run(
        &APART,
        &[("p1", pnum("1")), ("p2", pnum("1")), ("p3", pnum("3"))],
    );
    assert_eq!(summary(&c), vec![("p2", Joined, vec!["p1", "p2"])]);
    let d = &c.differences[0];
    assert_eq!(d.published, pnum("1"));
    assert_eq!(d.computed, Computed::Value(TraceValue::Number(number("2"))));
    let entry = usize::try_from(d.entry.unwrap()).unwrap();
    assert!(matches!(
        &e.traces[0].entries()[entry - 1],
        Entry::Place(p) if p.members() == ["p2".to_owned()]
    ));
}

#[test]
fn a_published_tie_at_the_position_of_one_member_reports_the_other() {
    let (_, c) = run(
        &APART,
        &[("p1", pnum("2")), ("p2", pnum("2")), ("p3", pnum("3"))],
    );
    assert_eq!(summary(&c), vec![("p1", Joined, vec!["p1", "p2"])]);
}

#[test]
fn a_published_tie_of_a_computed_group_at_another_position_is_a_plain_difference() {
    let (_, c) = run(
        &TIED,
        &[("p1", pnum("2")), ("p2", pnum("2")), ("p3", pnum("3"))],
    );
    assert_eq!(
        summary(&c),
        vec![("p1", Plain, vec![]), ("p2", Plain, vec![])]
    );
}

#[test]
fn a_published_value_that_is_not_a_position_tie_is_a_plain_difference() {
    let (_, c) = run(
        &APART,
        &[("p1", pnum("1")), ("p2", pnum("3")), ("p3", pnum("2"))],
    );
    assert_eq!(
        summary(&c),
        vec![("p2", Plain, vec![]), ("p3", Plain, vec![])]
    );
}

#[test]
fn a_published_boolean_for_a_ranking_target_is_a_plain_difference() {
    let (_, c) = run(&APART, &[("p1", PublishedValue::Bool(true))]);
    assert_eq!(summary(&c), vec![("p1", Plain, vec![])]);
}

#[test]
fn only_listed_entries_are_compared_and_unknown_participants_are_unused() {
    let (_, c) = run(&TIED, &[("p1", pnum("1")), ("p9", pnum("1"))]);
    assert!(c.is_equal());
    assert_eq!(c.compared.len(), 1);
    assert_eq!(c.compared[0].0.as_str(), "p1");
    assert_eq!(c.unused.len(), 1);
    assert_eq!(c.unused[0].0.as_str(), "p9");
}

#[test]
fn an_unlisted_member_of_a_computed_group_is_not_compared_and_does_not_split_it() {
    let (_, c) = run(&TIED, &[("p1", pnum("1")), ("p3", pnum("4"))]);
    assert_eq!(summary(&c), vec![("p3", Plain, vec![])]);
}

#[test]
fn an_indeterminate_ranking_is_never_equal_and_never_a_tie_kind() {
    let (_, c) = run(
        &[("p1", Some("5")), ("p2", None), ("p3", Some("5"))],
        &[("p1", pnum("1")), ("p2", pnum("1")), ("p3", pnum("1"))],
    );
    assert_eq!(c.differences.len(), 3);
    for d in &c.differences {
        assert_eq!(d.kind, Plain);
        assert!(d.group.is_empty());
        assert_eq!(d.computed, Computed::Indeterminate);
    }
}

#[test]
fn the_comparison_does_not_depend_on_the_order_of_the_input() {
    let forward = run(
        &TIED,
        &[("p1", pnum("2")), ("p2", pnum("3")), ("p3", pnum("4"))],
    );
    let mut reversed_scores = TIED;
    reversed_scores.reverse();
    let backward = run(
        &reversed_scores,
        &[("p3", pnum("4")), ("p2", pnum("3")), ("p1", pnum("2"))],
    );
    assert_eq!(forward.1, backward.1);
}

// Independent reference model: for each participant the position is one plus
// the number of strictly higher scores, derived here from the scores alone.
fn model(
    scores: &[u8],
    published: &BTreeMap<usize, u8>,
) -> Vec<(usize, DifferenceKind, Vec<usize>)> {
    let pos = |i: usize| 1 + scores.iter().filter(|s| **s > scores[i]).count();
    let mut out = Vec::new();
    for (&i, &pp) in published {
        if usize::from(pp) == pos(i) {
            continue;
        }
        let group: Vec<usize> = (0..scores.len())
            .filter(|&j| scores[j] == scores[i])
            .collect();
        let group_positions: std::collections::BTreeSet<u8> = group
            .iter()
            .filter_map(|j| published.get(j).copied())
            .collect();
        let tie: Vec<usize> = published
            .iter()
            .filter(|(_, v)| **v == pp)
            .map(|(j, _)| *j)
            .collect();
        let separated = tie.iter().any(|&j| pos(j) != pos(i));
        if group.len() > 1 && group_positions.len() > 1 {
            out.push((i, Split, group));
        } else if separated {
            out.push((i, Joined, tie));
        } else {
            out.push((i, Plain, Vec::new()));
        }
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn differences_equal_the_reference_model_and_are_deterministic(
        scores in proptest::collection::vec(0u8..4, 2..8),
        picks in proptest::collection::vec(proptest::option::of(1u8..9), 8),
    ) {
        let names: Vec<String> = (0..scores.len()).map(|i| format!("p{i}")).collect();
        let people: Vec<(&str, Option<String>)> = names
            .iter()
            .zip(&scores)
            .map(|(n, s)| (n.as_str(), Some(s.to_string())))
            .collect();
        let people_ref: Vec<(&str, Option<&str>)> =
            people.iter().map(|(n, s)| (*n, s.as_deref())).collect();
        let listed: BTreeMap<usize, u8> = picks
            .iter()
            .take(scores.len())
            .enumerate()
            .filter_map(|(i, p)| p.map(|v| (i, v)))
            .collect();
        let entries: Vec<(&str, PublishedValue)> = listed
            .iter()
            .map(|(i, v)| (names[*i].as_str(), pnum(&v.to_string())))
            .collect();
        let (_, c) = run(&people_ref, &entries);
        let (_, again) = run(&people_ref, &entries);
        prop_assert_eq!(&c, &again);
        let got: Vec<(usize, DifferenceKind, Vec<usize>)> = c
            .differences
            .iter()
            .map(|d| {
                let idx = |p: &str| names.iter().position(|n| n == p).unwrap();
                (
                    idx(d.participant.as_str()),
                    d.kind,
                    d.group.iter().map(|g| idx(g.as_str())).collect(),
                )
            })
            .collect();
        let mut want = model(&scores, &listed);
        want.sort_by_key(|(i, _, _)| names[*i].clone());
        prop_assert_eq!(got, want);
        prop_assert_eq!(c.compared.len(), listed.len());
    }

    #[test]
    fn a_published_result_equal_to_the_computed_one_has_no_difference(
        scores in proptest::collection::vec(0u8..4, 1..9),
    ) {
        let names: Vec<String> = (0..scores.len()).map(|i| format!("p{i}")).collect();
        let people: Vec<(&str, Option<String>)> = names
            .iter()
            .zip(&scores)
            .map(|(n, s)| (n.as_str(), Some(s.to_string())))
            .collect();
        let people_ref: Vec<(&str, Option<&str>)> =
            people.iter().map(|(n, s)| (*n, s.as_deref())).collect();
        let entries: Vec<(&str, PublishedValue)> = names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let pos = 1 + scores.iter().filter(|s| **s > scores[i]).count();
                (n.as_str(), pnum(&pos.to_string()))
            })
            .collect();
        let (_, c) = run(&people_ref, &entries);
        prop_assert!(c.is_equal());
        prop_assert_eq!(c.compared.len(), scores.len());
    }
}
