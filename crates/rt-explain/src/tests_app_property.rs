//! Property tests of the application report (task 9.4): determinism,
//! construction-order independence and the closed field set, each over at
//! least 256 generated documents.

use crate::report::{
    render_human, render_json, PublishedSection, ReportDoc, ReportSection, StandingsResult,
    FIELD_SET_BRACKET, FIELD_SET_PRIZE,
};
use crate::tests_app_fixtures::*;
use crate::tests_app_report::key_paths;
use crate::Verification;
use proptest::prelude::*;
use std::collections::BTreeSet;

const CASES: u32 = 256;

/// Fixture sections with the participants, targets and group members replaced
/// by the generated names, which may hold any character.
fn renamed(doc: &ReportDoc, vs: &[Verification], names: &[String]) -> ReportDoc {
    let pick = |i: usize| names[i % names.len()].clone();
    let mut sections = doc.sections().to_vec();
    for s in &mut sections {
        match s {
            ReportSection::Eligibility(es) => {
                for (i, e) in es.iter_mut().enumerate() {
                    e.participant = pick(i);
                    e.target = pick(i + 1);
                }
            }
            ReportSection::Scores(es) => {
                for (i, e) in es.iter_mut().enumerate() {
                    e.participant = pick(i + 2);
                }
            }
            ReportSection::Qualification(es) => {
                for (i, e) in es.iter_mut().enumerate() {
                    e.participant = pick(i + 1);
                }
            }
            ReportSection::Standings(ts) => {
                for t in ts.iter_mut() {
                    t.target = pick(0);
                    if let StandingsResult::Places(places) = &mut t.result {
                        for (i, p) in places.iter_mut().enumerate() {
                            p.members = names.iter().skip(i).cloned().collect();
                            if p.members.is_empty() {
                                p.members = vec![pick(i)];
                            }
                        }
                    }
                }
            }
            ReportSection::Published(PublishedSection::Compared {
                differences,
                unused,
            }) => {
                for (i, d) in differences.iter_mut().enumerate() {
                    d.participant = pick(i);
                    d.published = pick(i + 1);
                    d.group = names.to_vec();
                }
                for u in unused.iter_mut() {
                    u.participant = pick(1);
                }
            }
            _ => {}
        }
    }
    ReportDoc::new(doc.header().clone(), vs, sections).unwrap()
}

/// A document whose traces all fail with a hash mismatch, so that the
/// provenance summary carries a code and the report holds every field.
fn base(bracket: bool) -> (ReportDoc, Vec<Verification>) {
    if bracket {
        let t = bracket_traces();
        let vs = mismatched(&t);
        (bracket_doc(&t, &vs), vs)
    } else {
        let t = prize_traces();
        let vs = mismatched(&t);
        (prize_doc(&t, &vs), vs)
    }
}

fn renamed_base(bracket: bool, names: &[String]) -> ReportDoc {
    let (doc, vs) = base(bracket);
    renamed(&doc, &vs, names)
}

fn names() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(any::<String>(), 1..5)
}

fn expected(bracket: bool) -> BTreeSet<String> {
    let list = if bracket {
        FIELD_SET_BRACKET
    } else {
        FIELD_SET_PRIZE
    };
    list.iter().map(|s| (*s).to_owned()).collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn key_paths_equal_the_field_set_for_any_names(names in names(), bracket in any::<bool>()) {
        let doc = renamed_base(bracket, &names);
        let paths = key_paths(&render_json(&doc));
        prop_assert_eq!(paths, expected(bracket));
    }

    #[test]
    fn any_subset_of_sections_stays_inside_the_field_set(
        names in names(),
        bracket in any::<bool>(),
        keep in prop::collection::vec(any::<bool>(), 6),
    ) {
        let (b, vs) = base(bracket);
        let full = renamed(&b, &vs, &names);
        let sections: Vec<ReportSection> = full
            .sections()
            .iter()
            .enumerate()
            .filter(|(i, _)| keep[i % keep.len()])
            .map(|(_, s)| s.clone())
            .collect();
        let doc = ReportDoc::new(full.header().clone(), &vs, sections).unwrap();
        let paths = key_paths(&render_json(&doc));
        let allowed = expected(bracket);
        prop_assert!(paths.is_subset(&allowed), "extra: {:?}", paths.difference(&allowed).collect::<Vec<_>>());
    }

    #[test]
    fn output_is_deterministic_and_independent_of_construction_order(
        names in names(),
        bracket in any::<bool>(),
    ) {
        let doc = renamed_base(bracket, &names);
        let again = renamed_base(bracket, &names);
        prop_assert_eq!(render_json(&doc), render_json(&again));
        prop_assert_eq!(render_human(&doc), render_human(&again));
        let mut sections = doc.sections().to_vec();
        sections.reverse();
        for s in &mut sections {
            match s {
                ReportSection::Standings(ts) => {
                    for t in ts.iter_mut() {
                        if let StandingsResult::Places(ps) = &mut t.result {
                            for p in ps.iter_mut() {
                                p.members.reverse();
                            }
                        }
                    }
                }
                ReportSection::Published(PublishedSection::Compared { differences, .. }) => {
                    for d in differences.iter_mut() {
                        d.group.reverse();
                    }
                }
                _ => {}
            }
        }
        let shuffled = ReportDoc::new(doc.header().clone(), &base(bracket).1, sections).unwrap();
        prop_assert_eq!(render_json(&doc), render_json(&shuffled));
        prop_assert_eq!(render_human(&doc), render_human(&shuffled));
    }

    #[test]
    fn human_output_never_holds_a_raw_control_character(names in names(), bracket in any::<bool>()) {
        let doc = renamed_base(bracket, &names);
        for c in render_human(&doc).chars() {
            prop_assert!(c == '\n' || !c.is_control(), "raw control {:?}", c);
        }
    }
}
