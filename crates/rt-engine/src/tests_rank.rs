//! Ranking, precedence, execution conflicts and published comparison (task 5.10).

use crate::tests::*;
use crate::*;
use proptest::prelude::*;
use rt_compiled::{
    CType, CompiledBody, CompiledRule, CompiledTieBreaker, NodeIdx, NodeKind, TargetIdx,
};
use rt_data::{
    DataValue, ParticipantId, PublishedNumber, PublishedResult, PublishedValue, Question,
    TargetName,
};
use rt_ir::BinaryOp;
use rt_ir::{Direction, Ties};
use rt_numeric::parse_literal;
use rt_trace::{CriterionResult, Entry, NotAppliedReason, Place, TraceValue};
use std::collections::BTreeMap;

fn ranking(id: &str, key: Direction, tie_breakers: &[Direction]) -> CompiledRule {
    let mut n = Nodes::default();
    let key_node = n.field(NUM, 0);
    let tbs: Vec<CompiledTieBreaker> = tie_breakers
        .iter()
        .enumerate()
        .map(|(i, d)| CompiledTieBreaker {
            expr: n.field(NUM, u32::try_from(i + 1).unwrap()),
            direction: *d,
        })
        .collect();
    rule(
        id,
        "standings",
        CType::Ranking,
        n,
        CompiledBody::Ranking {
            key: key_node,
            direction: key,
            tie_breakers: tbs,
            ties: Ties::Group,
        },
    )
}

fn rank_all() -> Question {
    Question::Rank {
        target: TargetName::new("standings").unwrap(),
        participants: None,
    }
}

fn scored(people: &[(&str, Option<&str>, Option<&str>)]) -> CompetitionData {
    data(
        people
            .iter()
            .map(|(id, a, b)| {
                let mut fields = Vec::new();
                if let Some(a) = a {
                    fields.push(("a", text(a)));
                }
                if let Some(b) = b {
                    fields.push(("b", text(b)));
                }
                (*id, person(&fields, None))
            })
            .collect(),
    )
}

fn places(outcome: &Outcome) -> Vec<(u32, Vec<String>)> {
    match outcome {
        Outcome::Decided(TraceValue::Standings(p)) => p
            .iter()
            .map(|x| (x.position(), x.members().to_vec()))
            .collect(),
        other => panic!("not decided standings: {other:?}"),
    }
}

fn ids(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

const INPUTS: [(&str, CType); 3] = [("a", NUM), ("b", NUM), ("c", NUM)];

#[test]
fn a_tie_is_one_tie_group_and_the_next_position_skips_its_size() {
    let rb = build(&INPUTS, vec![ranking("rank.main", Direction::Desc, &[])]);
    let d = scored(&[
        ("p1", Some("10"), None),
        ("p2", Some("7"), None),
        ("p3", Some("10"), None),
        ("p4", Some("3"), None),
    ]);
    let (outcome, trace, _) = eval_one(&rb, &d, rank_all());
    assert_eq!(
        places(&outcome),
        vec![
            (1, ids(&["p1", "p3"])),
            (3, ids(&["p2"])),
            (4, ids(&["p4"]))
        ]
    );
    let groups: Vec<&Place> = trace
        .entries()
        .iter()
        .filter_map(|e| match e {
            Entry::Place(p) if p.is_tie_group() => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].members(), ids(&["p1", "p3"]));
    // The compared values of the tied pair are in the trace.
    let tied = trace.entries().iter().find_map(|e| match e {
        Entry::Compare(c) if !c.first_ahead => Some(c),
        _ => None,
    });
    let tied = tied.expect("a tied comparison");
    assert_eq!(tied.between, ("p1".to_owned(), "p3".to_owned()));
    assert_eq!(tied.criteria.len(), 1);
    assert_eq!(tied.criteria[0].left, TraceValue::Number(number("10")));
    assert_eq!(tied.criteria[0].right, TraceValue::Number(number("10")));
    assert_eq!(tied.criteria[0].result, CriterionResult::Equal);
}

#[test]
fn ascending_direction_puts_smaller_values_ahead() {
    let rb = build(&INPUTS, vec![ranking("rank.main", Direction::Asc, &[])]);
    let d = scored(&[("p1", Some("10"), None), ("p2", Some("7"), None)]);
    let (outcome, _, _) = eval_one(&rb, &d, rank_all());
    assert_eq!(places(&outcome), vec![(1, ids(&["p2"])), (2, ids(&["p1"]))]);
}

#[test]
fn tie_breakers_apply_in_declared_order_and_only_to_participants_still_tied() {
    let rb = build(
        &INPUTS,
        vec![ranking(
            "rank.main",
            Direction::Desc,
            &[Direction::Asc, Direction::Desc],
        )],
    );
    // p1 and p2 tie on a, and b separates them (smaller b first). p3 and p4 tie on a and b, and
    // c is their only separator (larger c first). p5 is alone on a, so no tie-breaker of p5 is
    // needed: it has none.
    let d = data(vec![
        (
            "p1",
            person(
                &[("a", text("9")), ("b", text("2")), ("c", text("1"))],
                None,
            ),
        ),
        (
            "p2",
            person(
                &[("a", text("9")), ("b", text("1")), ("c", text("1"))],
                None,
            ),
        ),
        (
            "p3",
            person(
                &[("a", text("5")), ("b", text("1")), ("c", text("4"))],
                None,
            ),
        ),
        (
            "p4",
            person(
                &[("a", text("5")), ("b", text("1")), ("c", text("8"))],
                None,
            ),
        ),
        ("p5", person(&[("a", text("1"))], None)),
    ]);
    let (outcome, trace, _) = eval_one(&rb, &d, rank_all());
    assert_eq!(
        places(&outcome),
        vec![
            (1, ids(&["p2"])),
            (2, ids(&["p1"])),
            (3, ids(&["p4"])),
            (4, ids(&["p3"])),
            (5, ids(&["p5"]))
        ]
    );
    // p5 has no tie-breaker value and is not tied, so nothing is missing.
    assert!(trace.missing().is_empty());
    // The comparison of p2 and p1 lists the key, which was equal, then the tie-breaker.
    let c = trace
        .entries()
        .iter()
        .find_map(|e| match e {
            Entry::Compare(c) if c.between.0 == "p2" && c.between.1 == "p1" => Some(c),
            _ => None,
        })
        .unwrap();
    let results: Vec<_> = c.criteria.iter().map(|k| k.result).collect();
    assert_eq!(
        results,
        vec![CriterionResult::Equal, CriterionResult::First]
    );
}

#[test]
fn equal_on_every_criterion_is_a_tie_group_never_ordered_by_identifier() {
    let rb = build(
        &INPUTS,
        vec![ranking("rank.main", Direction::Desc, &[Direction::Asc])],
    );
    let d = scored(&[
        ("p1", Some("4"), Some("1")),
        ("p2", Some("4"), Some("1")),
        ("p3", Some("4"), Some("1")),
    ]);
    let (outcome, _, _) = eval_one(&rb, &d, rank_all());
    assert_eq!(places(&outcome), vec![(1, ids(&["p1", "p2", "p3"]))]);
}

#[test]
fn a_missing_ranking_key_makes_the_whole_set_indeterminate_naming_the_participant() {
    let rb = build(&INPUTS, vec![ranking("rank.main", Direction::Desc, &[])]);
    let d = scored(&[
        ("p1", Some("4"), None),
        ("p2", None, None),
        ("p3", Some("9"), None),
    ]);
    let (outcome, trace, e) = eval_one(&rb, &d, rank_all());
    match &outcome {
        Outcome::Indeterminate(reasons) => {
            assert!(reasons.contains(&Reason::Diagnosed(Code::ExeRankMissingValue)));
            assert!(reasons.contains(&Reason::MissingInput {
                participant: "p2".to_owned(),
                field: "a".to_owned()
            }));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(codes(&e), vec![Code::ExeRankMissingValue]);
    let d0 = &e.diagnostics.as_slice()[0];
    assert!(d0.message().contains("'p2'"));
    // No participant gets a position.
    assert!(trace
        .entries()
        .iter()
        .all(|x| !matches!(x, Entry::Place(_))));
    assert_eq!(trace.missing().len(), 1);
}

#[test]
fn a_missing_tie_breaker_makes_only_the_tied_set_indeterminate() {
    let rb = build(
        &INPUTS,
        vec![ranking("rank.main", Direction::Desc, &[Direction::Asc])],
    );
    // p1 and p2 tie on a and p2 lacks b. p3 is behind them and p0 ahead of them.
    let d = scored(&[
        ("p0", Some("20"), None),
        ("p1", Some("10"), Some("1")),
        ("p2", Some("10"), None),
        ("p3", Some("5"), None),
    ]);
    let (outcome, trace, e) = eval_one(&rb, &d, rank_all());
    assert!(matches!(outcome, Outcome::Indeterminate(_)));
    let placed: Vec<(u32, Vec<String>)> = trace
        .entries()
        .iter()
        .filter_map(|x| match x {
            Entry::Place(p) => Some((p.position(), p.members().to_vec())),
            _ => None,
        })
        .collect();
    // p0 is first and p3 is fourth, because its position counts both participants of the set.
    assert_eq!(placed, vec![(1, ids(&["p0"])), (4, ids(&["p3"]))]);
    assert_eq!(
        trace.missing(),
        &[rt_trace::MissingInput {
            participant: "p2".to_owned(),
            field: "b".to_owned()
        }]
    );
    assert_eq!(codes(&e), vec![Code::ExeRankMissingValue]);
}

#[test]
fn a_ranking_question_with_a_subset_ranks_only_that_subset() {
    let rb = build(&INPUTS, vec![ranking("rank.main", Direction::Desc, &[])]);
    let d = scored(&[
        ("p1", Some("1"), None),
        ("p2", Some("9"), None),
        ("p3", Some("5"), None),
    ]);
    let subset = std::collections::BTreeSet::from([
        ParticipantId::new("p1").unwrap(),
        ParticipantId::new("p3").unwrap(),
    ]);
    let q = Question::Rank {
        target: TargetName::new("standings").unwrap(),
        participants: Some(subset),
    };
    let (outcome, _, _) = eval_one(&rb, &d, q);
    assert_eq!(places(&outcome), vec![(1, ids(&["p3"])), (2, ids(&["p1"]))]);
}

fn same_target(id: &str, value: &str) -> CompiledRule {
    let mut n = Nodes::default();
    let r = n.num(value);
    derived(id, "same", n, r)
}

#[test]
fn precedence_uses_only_the_first_listed_rule_and_records_the_others() {
    let rb = build_with(
        &[],
        vec![same_target("calc.a", "1"), same_target("calc.b", "2")],
        &[("prec.same", "same", &["calc.b", "calc.a"])],
    );
    let d = data(vec![("p1", person(&[], None))]);
    let (outcome, trace, e) = eval_one(&rb, &d, ask("same", "p1"));
    assert_eq!(outcome, decided_number("2"));
    assert!(e.diagnostics.as_slice().is_empty());
    assert_eq!(trace.not_applied().len(), 1);
    assert_eq!(trace.not_applied()[0].rule.as_str(), "calc.a");
    assert_eq!(
        trace.not_applied()[0].reason,
        NotAppliedReason::Precedence {
            id: "prec.same".to_owned()
        }
    );
    // The overridden rule was not evaluated and is not an applied rule.
    assert_eq!(trace.rules().len(), 1);
    assert_eq!(trace.rules()[0].rule.as_str(), "calc.b");
}

#[test]
fn an_indeterminate_first_listed_rule_makes_the_outcome_indeterminate() {
    let mk = |id: &str, with_input: bool| {
        let mut n = Nodes::default();
        let r = if with_input {
            n.field(NUM, 0)
        } else {
            n.num("5")
        };
        derived(id, "same", n, r)
    };
    let rb = build_with(
        &[("a", NUM)],
        vec![mk("calc.a", true), mk("calc.b", false)],
        &[("prec.same", "same", &["calc.a", "calc.b"])],
    );
    let d = data(vec![("p1", person(&[], None))]);
    let (outcome, trace, _) = eval_one(&rb, &d, ask("same", "p1"));
    assert!(matches!(outcome, Outcome::Indeterminate(_)));
    assert_eq!(trace.not_applied().len(), 1);
    assert_eq!(trace.not_applied()[0].rule.as_str(), "calc.b");
}

#[test]
fn precedence_applies_when_another_rule_reads_the_target() {
    let reader = {
        let mut n = Nodes::default();
        let t = n.add(NUM, NodeKind::Ref(TargetIdx::new(1)));
        let one = n.num("1");
        let sum = n.bin(BinaryOp::Add, t, one);
        derived("calc.reader", "reader", n, sum)
    };
    let rb = build_with(
        &[],
        vec![
            same_target("calc.a", "1"),
            same_target("calc.b", "2"),
            reader,
        ],
        &[("prec.same", "same", &["calc.b", "calc.a"])],
    );
    let d = data(vec![("p1", person(&[], None))]);
    let (outcome, trace, _) = eval_one(&rb, &d, ask("reader", "p1"));
    assert_eq!(outcome, decided_number("3"));
    assert_eq!(trace.not_applied().len(), 1);
}

#[test]
fn equal_known_results_without_precedence_are_compatible() {
    let rb = build(
        &[],
        vec![same_target("calc.a", "2"), same_target("calc.b", "2")],
    );
    let d = data(vec![("p1", person(&[], None))]);
    let (outcome, _, e) = eval_one(&rb, &d, ask("same", "p1"));
    assert_eq!(outcome, decided_number("2"));
    assert!(e.diagnostics.as_slice().is_empty());
}

#[test]
fn unequal_results_without_precedence_give_one_conflict_and_record_each_result() {
    let rb = build(
        &[],
        vec![same_target("calc.b", "2"), same_target("calc.a", "1")],
    );
    let d = data(vec![("p1", person(&[], None))]);
    let (outcome, trace, e) = eval_one(&rb, &d, ask("same", "p1"));
    assert_eq!(
        outcome,
        Outcome::indeterminate(vec![Reason::Diagnosed(Code::ExecutionConflictDetected)])
    );
    assert_eq!(codes(&e), vec![Code::ExecutionConflictDetected]);
    let recorded: Vec<(&str, &rt_trace::DecisionValue)> = trace
        .entries()
        .iter()
        .filter_map(|x| match x {
            Entry::Decision(d) => Some((d.rule.as_str(), &d.value)),
            _ => None,
        })
        .collect();
    assert!(recorded.iter().any(|(r, v)| *r == "calc.a"
        && **v == rt_trace::DecisionValue::Known(TraceValue::Number(number("1")))));
    assert!(recorded.iter().any(|(r, v)| *r == "calc.b"
        && **v == rt_trace::DecisionValue::Known(TraceValue::Number(number("2")))));
    assert!(trace.not_applied().is_empty());
}

// ----- published comparison -----

fn published(entries: &[(&str, &str, PublishedValue)]) -> PublishedResult {
    PublishedResult::new(
        entries
            .iter()
            .map(|(p, t, v)| {
                (
                    (
                        ParticipantId::new(*p).unwrap(),
                        TargetName::new(*t).unwrap(),
                    ),
                    v.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>(),
    )
}

fn pnum(text: &str) -> PublishedValue {
    PublishedValue::Number(PublishedNumber::parse(text).unwrap())
}

#[test]
fn a_published_value_that_differs_by_one_unit_is_reported_with_its_trace_entry() {
    let mut n = Nodes::default();
    let a = n.field(NUM, 0);
    let one = n.num("1");
    let sum = n.bin(BinaryOp::Add, a, one);
    let rb = build(&[("a", NUM)], vec![derived("calc.total", "total", n, sum)]);
    let d = data(vec![
        ("p1", person(&[("a", text("4"))], None)),
        ("p2", person(&[("a", text("9"))], None)),
    ]);
    let qs = [ask("total", "p1"), ask("total", "p2")];
    // p1 computes 5 and p2 computes 10; the published 5.000... equals, and 11 differs by one unit.
    let p = published(&[("p1", "total", pnum("5.0")), ("p2", "total", pnum("11"))]);
    let e = evaluate(&rb, &d, &qs, Some(&p));
    let c = e.comparison.unwrap();
    assert!(!c.is_equal());
    assert_eq!(c.compared.len(), 2);
    assert_eq!(c.differences.len(), 1);
    let diff = &c.differences[0];
    assert_eq!(diff.participant.as_str(), "p2");
    assert_eq!(diff.question, 1);
    assert_eq!(
        diff.computed,
        Computed::Value(TraceValue::Number(number("10")))
    );
    assert_eq!(diff.published, pnum("11"));
    assert!(!diff.non_terminating);
    // The reference is the entry of the trace that holds the computed value.
    let entry = diff.entry.unwrap();
    let trace = &e.traces[1];
    assert_eq!(usize::try_from(entry).unwrap(), trace.entries().len());
    assert!(matches!(
        trace.entries()[usize::try_from(entry).unwrap() - 1],
        Entry::Decision(_)
    ));
}

#[test]
fn an_equal_published_result_lists_no_difference_and_unused_entries_are_not_compared() {
    let rb = licence_rule(true);
    let d = data(vec![(
        "p1",
        person(
            &[("age", text("30")), ("licence", DataValue::Bool(true))],
            None,
        ),
    )]);
    let p = published(&[
        ("p1", "eligible", PublishedValue::Bool(true)),
        ("p9", "eligible", PublishedValue::Bool(false)),
    ]);
    let e = evaluate(&rb, &d, &[ask("eligible", "p1")], Some(&p));
    let c = e.comparison.unwrap();
    assert!(c.is_equal());
    assert_eq!(c.compared.len(), 1);
    assert_eq!(c.unused.len(), 1);
    assert_eq!(c.unused[0].0.as_str(), "p9");
}

#[test]
fn a_non_terminating_computed_value_is_stated_in_the_difference() {
    let mut n = Nodes::default();
    let (one, three) = (n.num("1"), n.num("3"));
    let q = n.bin(BinaryOp::Div, one, three);
    let rb = build(&[], vec![derived("calc.third", "third", n, q)]);
    let d = data(vec![("p1", person(&[], None))]);
    let p = published(&[("p1", "third", pnum("0.33"))]);
    let e = evaluate(&rb, &d, &[ask("third", "p1")], Some(&p));
    let c = e.comparison.unwrap();
    assert_eq!(c.differences.len(), 1);
    assert!(c.differences[0].non_terminating);
    assert!(c.differences[0].message.contains("non-terminating"));
}

#[test]
fn an_indeterminate_outcome_never_equals_a_published_value() {
    let rb = licence_rule(true);
    let d = data(vec![("p1", person(&[], None))]);
    let p = published(&[("p1", "eligible", PublishedValue::Bool(false))]);
    let e = evaluate(&rb, &d, &[ask("eligible", "p1")], Some(&p));
    let c = e.comparison.unwrap();
    assert_eq!(c.differences.len(), 1);
    assert_eq!(c.differences[0].computed, Computed::Indeterminate);
}

#[test]
fn published_rank_positions_are_compared_exactly_and_in_participant_order() {
    let rb = build(&INPUTS, vec![ranking("rank.main", Direction::Desc, &[])]);
    let d = scored(&[
        ("p1", Some("10"), None),
        ("p2", Some("10"), None),
        ("p3", Some("2"), None),
    ]);
    // Computed positions: p1 1, p2 1, p3 3.
    let p = published(&[
        ("p3", "standings", pnum("2")),
        ("p1", "standings", pnum("1")),
        ("p2", "standings", pnum("2")),
    ]);
    let e = evaluate(&rb, &d, &[rank_all()], Some(&p));
    let c = e.comparison.unwrap();
    let who: Vec<&str> = c
        .differences
        .iter()
        .map(|x| x.participant.as_str())
        .collect();
    assert_eq!(who, vec!["p2", "p3"]);
    assert_eq!(
        c.differences[0].computed,
        Computed::Value(TraceValue::Number(number("1")))
    );
    assert_eq!(
        c.differences[1].computed,
        Computed::Value(TraceValue::Number(number("3")))
    );
    let entry = c.differences[1].entry.unwrap();
    assert!(matches!(
        &e.traces[0].entries()[usize::try_from(entry).unwrap() - 1],
        Entry::Place(p) if p.members() == ids(&["p3"])
    ));
}

#[test]
fn a_ranking_with_several_rules_and_no_precedence_is_a_conflict() {
    let rb = build(
        &INPUTS,
        vec![
            ranking("rank.a", Direction::Desc, &[]),
            ranking("rank.b", Direction::Asc, &[]),
        ],
    );
    let d = scored(&[("p1", Some("1"), None)]);
    let (outcome, _, e) = eval_one(&rb, &d, rank_all());
    assert_eq!(
        outcome,
        Outcome::indeterminate(vec![Reason::Diagnosed(Code::ExecutionConflictDetected)])
    );
    assert_eq!(codes(&e), vec![Code::ExecutionConflictDetected]);
}

#[test]
fn ranking_is_the_same_for_any_participant_insertion_order() {
    let rb = build(&INPUTS, vec![ranking("rank.main", Direction::Desc, &[])]);
    let forward = scored(&[("p1", Some("1"), None), ("p2", Some("2"), None)]);
    let backward = scored(&[("p2", Some("2"), None), ("p1", Some("1"), None)]);
    assert_eq!(
        evaluate(&rb, &forward, &[rank_all()], None),
        evaluate(&rb, &backward, &[rank_all()], None)
    );
}

// Property 23: the engine equals a simple reference ranking.
proptest! {
    #[test]
    fn ranking_equals_a_reference_model(
        rows in proptest::collection::vec((0u8..5, 0u8..3), 1..9),
        key_desc in any::<bool>(),
        tb_desc in any::<bool>(),
    ) {
        let key_dir = if key_desc { Direction::Desc } else { Direction::Asc };
        let tb_dir = if tb_desc { Direction::Desc } else { Direction::Asc };
        let rb = build(&INPUTS, vec![ranking("rank.main", key_dir, &[tb_dir])]);
        let people: Vec<(String, u8, u8)> = rows
            .iter()
            .enumerate()
            .map(|(i, (a, b))| (format!("p{i:02}"), *a, *b))
            .collect();
        let d = data(
            people
                .iter()
                .map(|(id, a, b)| {
                    (
                        id.as_str(),
                        person(&[("a", text(&a.to_string())), ("b", text(&b.to_string()))], None),
                    )
                })
                .collect(),
        );
        let e = evaluate(&rb, &d, &[rank_all()], None);
        let replayed = rt_trace::replay(&e.traces[0]);
        prop_assert_eq!(replayed.as_ref(), Ok(&e.outcomes[0]));
        let got = places(&e.outcomes[0]);
        let is_ahead = |x: &(String, u8, u8), y: &(String, u8, u8)| {
            let by_key = if key_desc { x.1 > y.1 } else { x.1 < y.1 };
            let tied_key = x.1 == y.1;
            let by_tb = if tb_desc { x.2 > y.2 } else { x.2 < y.2 };
            by_key || (tied_key && by_tb)
        };
        let mut seen = 0usize;
        for (position, members) in &got {
            for m in members {
                seen += 1;
                let me = people.iter().find(|p| &p.0 == m).unwrap();
                let strictly_ahead = people.iter().filter(|o| is_ahead(o, me)).count();
                prop_assert_eq!(usize::try_from(*position).unwrap(), strictly_ahead + 1);
                for other in members {
                    let o = people.iter().find(|p| &p.0 == other).unwrap();
                    prop_assert!(o.1 == me.1 && o.2 == me.2);
                }
            }
            // Groups are complete: nobody outside the group is equal to a member.
            let me = people.iter().find(|p| p.0 == members[0]).unwrap();
            let equal = people.iter().filter(|o| o.1 == me.1 && o.2 == me.2).count();
            prop_assert_eq!(equal, members.len());
        }
        prop_assert_eq!(seen, people.len());
    }
}

#[test]
fn unused_helpers_compile() {
    let _ = (
        NodeIdx::new(0),
        parse_literal("1").unwrap(),
        PublishedValue::Bool(true),
    );
}
