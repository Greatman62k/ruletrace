//! Tests for the gaps the review of 03869c5 found (task 9.4 follow-up):
//! Missing variants, escaping of the standings target and the input role, the
//! "not stated" label, the table of difference kinds, the Bracket_Check
//! ranking statement and the absence of any report parser. Expected texts are
//! written by hand.

use crate::report::{
    difference_kind, render_human, render_json, DifferenceFacts, EligibilityEntry, Missing,
    PublishedDifference, QualificationEntry, ReportApplication, ReportDoc, ReportError,
    ReportSection, Score, ScoreEntry, StandingsResult, StandingsTable, Truth, DIFFERENCE_KINDS,
};
use crate::tests_app_fixtures::*;
use crate::tests_app_report::{
    BRACKET_SCOPE, MEANING_TIE_MERGED, MEANING_TIE_SPLIT, MEANING_VALUE, SCOPE,
};
use rt_foundation::Code;
use rt_trace::{Outcome, QuestionKind, Reason};

const MISSING_JSON: &str = "[{\"kind\":\"empty_aggregate\",\"participant\":null,\"field\":\"pts\",\"code\":null,\"text\":\"min or max of empty pts\"},{\"kind\":\"diagnostic\",\"participant\":null,\"field\":null,\"code\":\"RT-EXE-0011\",\"text\":\"diagnostic RT-EXE-0011\"}]";
const MISSING_TEXT: &str = "min or max of empty pts; diagnostic RT-EXE-0011";

fn reasons() -> Outcome {
    Outcome::indeterminate(vec![
        Reason::Diagnosed(Code::PublishedResultDiffers),
        Reason::EmptyAggregate {
            field: "pts".to_owned(),
        },
    ])
}

fn expected_missing() -> Vec<Missing> {
    vec![
        Missing::EmptyAggregate {
            field: "pts".to_owned(),
        },
        Missing::Diagnosed(Code::PublishedResultDiffers),
    ]
}

fn value_trace(target: &str, rule: &str) -> rt_trace::DecisionTrace {
    trace(
        question(QuestionKind::Value, target, Some("p2")),
        reasons(),
        rule,
        1,
        (0, 20),
        Vec::new(),
        Vec::new(),
    )
}

fn rank_trace(target: &str) -> rt_trace::DecisionTrace {
    trace(
        question(QuestionKind::Rank, target, None),
        reasons(),
        "rank.total",
        1,
        (0, 20),
        Vec::new(),
        Vec::new(),
    )
}

fn doc_of(
    app: ReportApplication,
    sections: Vec<ReportSection>,
    vs: &[crate::Verification],
) -> ReportDoc {
    ReportDoc::new(header(app), vs, sections).unwrap()
}

#[test]
fn empty_aggregate_and_diagnosed_are_indeterminate_with_their_reason_in_every_entry_kind() {
    let t = vec![
        value_trace("elig.age", "elig.age"),
        value_trace("score.total", "score.total"),
        rank_trace("rank.total"),
        value_trace("qual.q", "qual.q"),
    ];
    let vs = verified(&t);
    let eligibility = EligibilityEntry::from_trace(&tref(0, &t, &vs)).unwrap();
    let score = ScoreEntry::from_trace(&tref(1, &t, &vs)).unwrap();
    let table = StandingsTable::from_trace(&tref(2, &t, &vs), true).unwrap();
    let qualification = QualificationEntry::from_trace(&tref(3, &t, &vs)).unwrap();
    assert_eq!(
        eligibility.result(),
        &Truth::Indeterminate(expected_missing())
    );
    assert_eq!(score.result(), &Score::Indeterminate(expected_missing()));
    assert_eq!(
        table.result(),
        &StandingsResult::Indeterminate(expected_missing())
    );
    assert_eq!(
        qualification.result(),
        &Truth::Indeterminate(expected_missing())
    );

    let prize = doc_of(
        ReportApplication::PrizeCheck,
        vec![
            ReportSection::Eligibility(vec![eligibility]),
            ReportSection::Scores(vec![score]),
            ReportSection::Standings(vec![table.clone()]),
        ],
        &vs,
    );
    let human = render_human(&prize);
    assert!(human.contains(&format!("  p2  elig.age  indeterminate   {MISSING_TEXT}\n")));
    assert!(human.contains(&format!(
        "  p2  score.total  indeterminate   {MISSING_TEXT}\n"
    )));
    assert!(human.contains(&format!(
        "STANDINGS rank.total\n  indeterminate   {MISSING_TEXT}\n"
    )));
    let json = String::from_utf8(render_json(&prize)).unwrap();
    assert!(json.contains(&format!(
        "\"result\":\"indeterminate\",\"missing\":{MISSING_JSON},\"trace\":0"
    )));
    assert!(json.contains(&format!(
        "\"result\":\"indeterminate\",\"value\":null,\"missing\":{MISSING_JSON},\"trace\":1"
    )));
    assert!(json.contains(&format!(
        "\"result\":\"indeterminate\",\"places\":[],\"missing\":{MISSING_JSON},\"trace\":2"
    )));
    for forbidden in [
        "\"result\":\"true\"",
        "\"result\":\"false\"",
        "\"result\":\"exact\"",
        "\"result\":\"ordered\"",
    ] {
        assert!(!json.contains(forbidden), "{forbidden}");
    }

    let bracket = doc_of(
        ReportApplication::BracketCheck,
        vec![
            ReportSection::Standings(vec![table]),
            ReportSection::Qualification(vec![qualification]),
        ],
        &vs,
    );
    let human = render_human(&bracket);
    assert!(human.contains(&format!("  p2  qual.q  indeterminate   {MISSING_TEXT}\n")));
    let json = String::from_utf8(render_json(&bracket)).unwrap();
    assert!(json.contains(&format!(
        "\"result\":\"indeterminate\",\"missing\":{MISSING_JSON},\"inputs_read\":[]"
    )));
    assert!(json.contains(&format!(
        "\"result\":\"indeterminate\",\"places\":[],\"missing\":{MISSING_JSON},\"comparisons\":[]"
    )));
}

#[test]
fn each_missing_variant_alone_is_written_with_its_own_kind_and_text() {
    let cases = [
        (
            Reason::MissingInput {
                participant: "p2".to_owned(),
                field: "birth_date".to_owned(),
            },
            "missing input: birth_date (p2)",
            "{\"kind\":\"missing_input\",\"participant\":\"p2\",\"field\":\"birth_date\",\"code\":null,\"text\":\"missing input: birth_date (p2)\"}",
        ),
        (
            Reason::EmptyAggregate {
                field: "pts".to_owned(),
            },
            "min or max of empty pts",
            "{\"kind\":\"empty_aggregate\",\"participant\":null,\"field\":\"pts\",\"code\":null,\"text\":\"min or max of empty pts\"}",
        ),
        (
            Reason::Diagnosed(Code::PublishedResultDiffers),
            "diagnostic RT-EXE-0011",
            "{\"kind\":\"diagnostic\",\"participant\":null,\"field\":null,\"code\":\"RT-EXE-0011\",\"text\":\"diagnostic RT-EXE-0011\"}",
        ),
    ];
    for (reason, text, json_item) in cases {
        let t = vec![trace(
            question(QuestionKind::Value, "elig.age", Some("p2")),
            Outcome::indeterminate(vec![reason]),
            "elig.age",
            1,
            (0, 20),
            Vec::new(),
            Vec::new(),
        )];
        let vs = verified(&t);
        let doc = doc_of(
            ReportApplication::PrizeCheck,
            vec![ReportSection::Eligibility(vec![
                EligibilityEntry::from_trace(&tref(0, &t, &vs)).unwrap(),
            ])],
            &vs,
        );
        assert!(render_human(&doc).contains(&format!("  p2  elig.age  indeterminate   {text}\n")));
        let json = String::from_utf8(render_json(&doc)).unwrap();
        assert!(json.contains(&format!(
            "\"result\":\"indeterminate\",\"missing\":[{json_item}],"
        )));
    }
}

#[test]
fn the_standings_target_and_the_input_role_are_escaped_in_human_output() {
    let t = vec![trace(
        question(QuestionKind::Rank, "ra\u{1b}[0mnk\u{7}", None),
        missing_outcome("p4", "late_score"),
        "rank.late",
        1,
        (0, 20),
        Vec::new(),
        Vec::new(),
    )];
    let vs = verified(&t);
    let mut h = header(ReportApplication::PrizeCheck);
    h.input_hashes = vec![("da\u{1b}[31mta", hash('2'))];
    let doc = ReportDoc::new(
        h,
        &vs,
        vec![ReportSection::Standings(vec![StandingsTable::from_trace(
            &tref(0, &t, &vs),
            false,
        )
        .unwrap()])],
    )
    .unwrap();
    let human = render_human(&doc);
    assert!(human.contains("\nSTANDINGS ra\\u{1b}[0mnk\\u{7}\n"));
    assert!(human.contains(&format!(
        "\ninput da\\u{{1b}}[31mta sha256:{}\n",
        "2".repeat(64)
    )));
    for c in human.chars() {
        assert!(c == '\n' || !c.is_control(), "raw control {c:?}");
    }
    let json = String::from_utf8(render_json(&doc)).unwrap();
    assert!(!json.chars().any(|c| c.is_control() && c != '\n'));
}

#[test]
fn an_unstated_synthetic_label_is_not_written_as_synthetic() {
    let t = prize_traces();
    let vs = verified(&t);
    for (stated, human_line, json_member) in [
        (None, "data: not stated\n", "\"synthetic_data\":null"),
        (Some(true), "data: synthetic\n", "\"synthetic_data\":true"),
        (
            Some(false),
            "data: not synthetic\n",
            "\"synthetic_data\":false",
        ),
    ] {
        let mut h = header(ReportApplication::PrizeCheck);
        h.synthetic_data = stated;
        let doc = ReportDoc::new(h, &vs, Vec::new()).unwrap();
        let human = render_human(&doc);
        assert!(human.contains(human_line), "{human}");
        assert_eq!(human.matches("\ndata: ").count(), 1);
        let json = String::from_utf8(render_json(&doc)).unwrap();
        assert!(json.contains(json_member), "{json}");
    }
}

#[test]
fn the_bracket_header_states_that_eligibility_does_not_filter_the_standings() {
    let t = bracket_traces();
    let doc = bracket_doc(&t, &verified(&t));
    let human = render_human(&doc);
    assert!(human.contains(&format!("\nranking: {BRACKET_SCOPE}\n")));
    let json = String::from_utf8(render_json(&doc)).unwrap();
    assert!(json.contains(&format!("\"ranking_scope\":\"{BRACKET_SCOPE}\"")));
    assert!(crate::report::FIELD_SET_BRACKET.contains(&"header.ranking_scope"));
    let tail = "eligibility is evaluated independently and is not applied as a ranking filter.";
    assert!(BRACKET_SCOPE.ends_with(tail));
    assert!(SCOPE.ends_with(tail));
    assert!(BRACKET_SCOPE.starts_with("Standings are calculated from the declared ranking rules;"));
    let pt = prize_traces();
    let prize = render_human(&prize_doc(&pt, &verified(&pt)));
    assert!(prize.contains(&format!("\nranking: {SCOPE}\n")));
    assert!(!prize.contains(BRACKET_SCOPE));
    assert!(!human.contains(SCOPE));
}

#[test]
fn every_supported_kind_maps_to_its_catalogue_code_and_description() {
    let expected = [
        (
            "value_differs",
            Code::PublishedResultDiffers,
            "RT-EXE-0011",
            MEANING_VALUE,
        ),
        (
            "tie_broken_without_declared_criterion",
            Code::PublishedTieGroupSplit,
            "RT-EXE-0012",
            MEANING_TIE_SPLIT,
        ),
        (
            "tie_not_in_computed_result",
            Code::PublishedTieNotComputed,
            "RT-EXE-0013",
            MEANING_TIE_MERGED,
        ),
    ];
    assert_eq!(DIFFERENCE_KINDS.len(), expected.len());
    for (name, code, id, description) in expected {
        let row = difference_kind(name).unwrap();
        assert_eq!(row.name(), name);
        assert_eq!(row.code(), code);
        assert_eq!(row.code().id(), id);
        assert_eq!(row.description(), description);
        let d = PublishedDifference::new(name, facts(&[])).unwrap();
        assert_eq!(d.kind(), row);
    }
    let names: std::collections::BTreeSet<&str> =
        DIFFERENCE_KINDS.iter().map(|k| k.name()).collect();
    assert_eq!(names.len(), DIFFERENCE_KINDS.len());
}

#[test]
fn an_unknown_difference_kind_is_refused() {
    for bad in [
        "",
        "value_differ",
        "VALUE_DIFFERS",
        " value_differs",
        "value_differs ",
        "tie_broken",
        "tie_not_in_computed",
        "value_differs\u{1b}",
    ] {
        assert!(difference_kind(bad).is_none(), "{bad:?}");
        assert_eq!(
            PublishedDifference::new(bad, facts(&[])),
            Err(ReportError::UnknownDifferenceKind {
                kind: bad.to_owned()
            })
        );
    }
    let shown = ReportError::UnknownDifferenceKind {
        kind: "x\u{1b}[0m".to_owned(),
    }
    .to_string();
    assert_eq!(shown, "difference kind x\\u{1b}[0m is not supported");
}

#[test]
fn a_difference_cannot_carry_a_code_other_than_its_kinds() {
    let t = prize_traces();
    let vs = verified(&t);
    for (kind, id) in [
        ("value_differs", "RT-EXE-0011"),
        ("tie_broken_without_declared_criterion", "RT-EXE-0012"),
        ("tie_not_in_computed_result", "RT-EXE-0013"),
    ] {
        let doc = doc_of(
            ReportApplication::BracketCheck,
            vec![ReportSection::Published(
                crate::report::PublishedSection::Compared {
                    differences: vec![PublishedDifference::new(
                        kind,
                        DifferenceFacts {
                            group: vec!["p1".to_owned()],
                            ..facts(&[])
                        },
                    )
                    .unwrap()],
                    unused: Vec::new(),
                },
            )],
            &vs,
        );
        let json = String::from_utf8(render_json(&doc)).unwrap();
        assert_eq!(json.matches("RT-EXE-").count(), 1);
        assert!(json.contains(&format!("\"kind\":\"{kind}\",\"code\":\"{id}\"")));
    }
}

/// Source lines that are not comments.
fn code_lines(source: &str) -> Vec<&str> {
    source
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect()
}

#[test]
fn no_source_of_the_report_model_parses_a_report() {
    let sources = [
        ("model.rs", include_str!("report/model.rs")),
        ("common.rs", include_str!("report/common.rs")),
        ("human.rs", include_str!("report/human.rs")),
        ("machine.rs", include_str!("report/machine.rs")),
        ("difference.rs", include_str!("report/difference.rs")),
        ("mod.rs", include_str!("report/mod.rs")),
        ("verify.rs", include_str!("verify.rs")),
    ];
    for (name, source) in sources {
        for line in code_lines(source) {
            for forbidden in [
                "from_str",
                "FromStr",
                "Deserialize",
                "fn parse",
                "rt_json::parse",
                "impl Default for",
                "impl TryFrom",
                "impl From<",
            ] {
                assert!(!line.contains(forbidden), "{name}: {line}");
            }
        }
    }
}

/// The lines of the block that starts at the line `header` and ends at the
/// next line that is exactly `}`.
fn block<'a>(source: &'a str, header: &str) -> Vec<&'a str> {
    let mut lines = source.lines().skip_while(|l| *l != header);
    assert!(lines.next().is_some(), "no block {header}");
    lines.take_while(|l| *l != "}").collect()
}

fn public_fns(source: &str, impl_header: &str) -> Vec<String> {
    block(source, impl_header)
        .iter()
        .filter_map(|l| l.trim_start().strip_prefix("pub fn "))
        .map(|rest| rest.split(['(', '<']).next().unwrap_or("").to_owned())
        .collect()
}

const MODEL: &str = include_str!("report/model.rs");
const VERIFY: &str = include_str!("verify.rs");

#[test]
fn the_trust_bearing_types_expose_only_read_accessors_and_their_builders() {
    let cases: [(&str, &str, &[&str]); 11] = [
        (
            MODEL,
            "impl Evidence {",
            &["new", "trace", "verified", "rules", "sources"],
        ),
        (MODEL, "impl<'a> TraceRef<'a> {", &["new", "index"]),
        (
            MODEL,
            "impl EligibilityEntry {",
            &["participant", "target", "result", "evidence", "from_trace"],
        ),
        (
            MODEL,
            "impl ScoreEntry {",
            &["participant", "target", "result", "evidence", "from_trace"],
        ),
        (
            MODEL,
            "impl StandingsTable {",
            &["target", "result", "comparisons", "evidence", "from_trace"],
        ),
        (
            MODEL,
            "impl QualificationEntry {",
            &[
                "participant",
                "target",
                "result",
                "inputs_read",
                "evidence",
                "from_trace",
            ],
        ),
        (MODEL, "impl ProvenanceSummary {", &["from_verifications"]),
        (
            MODEL,
            "impl ReportDoc {",
            &["new", "header", "provenance", "sections"],
        ),
        (
            MODEL,
            "impl PublishedDifference {",
            &[
                "new",
                "participant",
                "target",
                "published",
                "computed",
                "trace",
                "entry",
                "kind",
                "non_terminating",
                "group",
            ],
        ),
        (
            VERIFY,
            "impl SpanVerdict {",
            &["rule", "revision", "span", "resolution", "text"],
        ),
        (
            VERIFY,
            "impl Verification {",
            &["spans", "diagnostics", "status"],
        ),
    ];
    for (source, header, allowed) in cases {
        assert_eq!(public_fns(source, header), allowed, "{header}");
    }
}

#[test]
fn no_public_function_takes_a_mutable_receiver_and_no_trust_field_is_public() {
    for (name, source) in [("model.rs", MODEL), ("verify.rs", VERIFY)] {
        for line in code_lines(source) {
            assert!(!line.contains("&mut self"), "{name}: {line}");
        }
    }
    for (source, header) in [
        (MODEL, "pub struct Evidence {"),
        (MODEL, "pub struct TraceRef<'a> {"),
        (MODEL, "pub struct EligibilityEntry {"),
        (MODEL, "pub struct ScoreEntry {"),
        (MODEL, "pub struct StandingsTable {"),
        (MODEL, "pub struct QualificationEntry {"),
        (MODEL, "pub struct PublishedDifference {"),
        (MODEL, "pub struct ReportDoc {"),
        (VERIFY, "pub struct Verification {"),
    ] {
        for field in block(source, header) {
            let f = field.trim_start();
            assert!(
                f.starts_with("//") || !f.starts_with("pub ") || f.starts_with("pub("),
                "{header} has a public field: {field}"
            );
        }
    }
    assert!(MODEL.contains("#[non_exhaustive]\n    Verified {"));
    assert!(VERIFY.contains("#[non_exhaustive]\npub struct SpanVerdict {"));
}
