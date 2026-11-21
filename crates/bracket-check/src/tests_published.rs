//! Published standings (Req 32.6, 31.7): the module maps the file, passes it
//! to the engine and reports each difference the engine returns. It applies
//! no comparison, tolerance, rounding or tie-breaking of its own.
//!
//! Computed standings used below, hand-derived as in `tests_report`:
//! with [`CSV_FULL`] the positions are p4 1, p2 2, p1 3, p3 4. With
//! [`CSV_TIE`] they are p2 and p4 together at 1, p1 3, p3 4.
//!
//! The kinds and codes are those of Req 13.12: `value_differs` is
//! RT-EXE-0011, `tie_broken_without_declared_criterion` is RT-EXE-0012 and
//! `tie_not_in_computed_result` is RT-EXE-0013.

use crate::tests_fixture::{full_request, run_with, sources, target, CSV_FULL, CSV_TIE};
use crate::{BracketRequest, ResultClass};
use rt_explain::report::{PublishedSection, ReportDoc, ReportSection};

type Row = (
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    Vec<String>,
);

fn section(doc: &ReportDoc) -> &PublishedSection {
    doc.sections()
        .iter()
        .find_map(|s| match s {
            ReportSection::Published(p) => Some(p),
            _ => None,
        })
        .unwrap()
}

fn differences(doc: &ReportDoc) -> Vec<Row> {
    let PublishedSection::Compared { differences, .. } = section(doc) else {
        panic!("a published file was supplied");
    };
    differences
        .iter()
        .map(|d| {
            (
                d.participant().to_owned(),
                d.target().to_owned(),
                d.published().to_owned(),
                d.computed().map(str::to_owned),
                d.kind().name().to_owned(),
                d.kind().code().id().to_owned(),
                d.group().to_vec(),
            )
        })
        .collect()
}

fn row(
    who: &str,
    target: &str,
    published: &str,
    computed: &str,
    kind: &str,
    code: &str,
    group: &[&str],
) -> Row {
    (
        who.to_owned(),
        target.to_owned(),
        published.to_owned(),
        Some(computed.to_owned()),
        kind.to_owned(),
        code.to_owned(),
        group.iter().map(|s| (*s).to_owned()).collect(),
    )
}

const EQUAL_BROKEN: &str =
    "participant,target,value\np4,standing,1\np2,standing,2\np1,standing,3\np3,standing,4\n";
const EQUAL_TIED: &str =
    "participant,target,value\np2,standing,1\np4,standing,1\np1,standing,3\np3,standing,4\n";
const SPLIT: &str =
    "participant,target,value\np2,standing,1\np4,standing,2\np1,standing,3\np3,standing,4\n";

#[test]
fn a_published_file_equal_to_the_computed_standings_has_no_difference() {
    let run = run_with(CSV_FULL, Some(EQUAL_BROKEN), &full_request(), &sources()).unwrap();
    let doc = run.report().unwrap();
    assert_eq!(differences(doc), Vec::<Row>::new());
    assert!(matches!(
        section(doc),
        PublishedSection::Compared { unused, .. } if unused.is_empty()
    ));
    assert_eq!(run.class(), ResultClass::Complete);
    let json = String::from_utf8(rt_explain::report::render_json(doc)).unwrap();
    assert!(json.contains("\"published\":{\"state\":\"equal\""));
}

#[test]
fn a_published_tie_equal_to_a_computed_tie_group_has_no_difference() {
    let run = run_with(CSV_TIE, Some(EQUAL_TIED), &full_request(), &sources()).unwrap();
    assert_eq!(differences(run.report().unwrap()), Vec::<Row>::new());
}

#[test]
fn a_published_ordering_that_splits_a_computed_tie_group_is_reported_as_a_difference() {
    let run = run_with(CSV_TIE, Some(SPLIT), &full_request(), &sources()).unwrap();
    let doc = run.report().unwrap();
    // p4 is published at 2 but computed at 1 inside the group {p2, p4}.
    assert_eq!(
        differences(doc),
        vec![row(
            "p4",
            "standing",
            "2",
            "1",
            "tie_broken_without_declared_criterion",
            "RT-EXE-0012",
            &["p2", "p4"],
        )]
    );
    // A Warning decides no exit class.
    assert_eq!(run.class(), ResultClass::Complete);
    let json = String::from_utf8(rt_explain::report::render_json(doc)).unwrap();
    assert!(json.contains("\"state\":\"differs\""));
    assert!(json.contains("\"code\":\"RT-EXE-0012\""));
}

#[test]
fn the_module_breaks_no_tie_the_computed_standings_and_outcomes_ignore_the_published_file() {
    let with = run_with(CSV_TIE, Some(SPLIT), &full_request(), &sources()).unwrap();
    let without = run_with(CSV_TIE, None, &full_request(), &sources()).unwrap();
    assert_eq!(with.evaluation().outcomes, without.evaluation().outcomes);
    let tables = |doc: &ReportDoc| {
        doc.sections()
            .iter()
            .filter_map(|s| match s {
                ReportSection::Standings(t) => Some(t.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        tables(with.report().unwrap()),
        tables(without.report().unwrap())
    );
    // The Questions do not depend on the published file either.
    assert_eq!(
        with.prepared().questions_bytes(),
        without.prepared().questions_bytes()
    );
}

#[test]
fn a_published_tie_that_the_computed_standings_separate_is_reported() {
    let csv =
        "participant,target,value\np4,standing,1\np2,standing,1\np1,standing,3\np3,standing,4\n";
    let run = run_with(CSV_FULL, Some(csv), &full_request(), &sources()).unwrap();
    // p2 is published at 1 but computed at 2, behind p4.
    assert_eq!(
        differences(run.report().unwrap()),
        vec![row(
            "p2",
            "standing",
            "1",
            "2",
            "tie_not_in_computed_result",
            "RT-EXE-0013",
            &["p2", "p4"],
        )]
    );
}

#[test]
fn a_different_position_is_a_value_difference_with_no_group() {
    let csv = "participant,target,value\np1,standing,2\n";
    let run = run_with(CSV_FULL, Some(csv), &full_request(), &sources()).unwrap();
    assert_eq!(
        differences(run.report().unwrap()),
        vec![row(
            "p1",
            "standing",
            "2",
            "3",
            "value_differs",
            "RT-EXE-0011",
            &[],
        )]
    );
}

#[test]
fn a_published_qualification_that_differs_is_reported_with_both_values() {
    let csv = "participant,target,value\np1,qualified,true\np2,qualified,true\n";
    let run = run_with(CSV_FULL, Some(csv), &full_request(), &sources()).unwrap();
    // p1 is computed false (score 7.6 is below 7.7), p2 true.
    assert_eq!(
        differences(run.report().unwrap()),
        vec![row(
            "p1",
            "qualified",
            "true",
            "false",
            "value_differs",
            "RT-EXE-0011",
            &[],
        )]
    );
}

#[test]
fn each_difference_points_at_the_trace_of_the_question_that_computed_it() {
    let run = run_with(CSV_TIE, Some(SPLIT), &full_request(), &sources()).unwrap();
    let PublishedSection::Compared { differences, .. } = section(run.report().unwrap()) else {
        panic!("a published file was supplied");
    };
    let first = differences.first().unwrap();
    assert_eq!(first.trace(), 0);
    assert!(first.entry().is_some());
    assert!(!first.non_terminating());
}

#[test]
fn a_published_row_for_a_declared_target_that_was_not_requested_is_listed_not_dropped() {
    let ranking_only = BracketRequest {
        ranking: [(target("standing"), 2)].into(),
        ..BracketRequest::default()
    };
    let csv = "participant,target,value\np1,qualified,true\np1,standing,3\n";
    let run = run_with(CSV_FULL, Some(csv), &ranking_only, &sources()).unwrap();
    assert_eq!(run.prepared().not_requested().len(), 1);
    let PublishedSection::Compared {
        differences,
        unused,
    } = section(run.report().unwrap())
    else {
        panic!("a published file was supplied");
    };
    assert!(differences.is_empty());
    let rows: Vec<(&str, &str)> = unused
        .iter()
        .map(|u| (u.participant.as_str(), u.target.as_str()))
        .collect();
    assert_eq!(rows, vec![("p1", "qualified")]);
}

#[test]
fn no_published_file_is_reported_as_not_supplied() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    assert_eq!(
        section(run.report().unwrap()),
        &PublishedSection::NotSupplied
    );
}
