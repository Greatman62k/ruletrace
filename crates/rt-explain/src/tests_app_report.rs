//! Tests for the application report model and writers (task 9.4; Req 31.6,
//! 31.13, 31.14, 32.1, 32.2, 32.5, 22.22). Expected texts are written by hand
//! from the design layout, not produced by the writers.

use crate::report::{
    render_human, render_json, DifferenceFacts, EligibilityEntry, Evidence, Missing,
    ProvenanceSummary, PublishedDifference, PublishedSection, ReportApplication, ReportDoc,
    ReportError, ReportSection, Score, ScoreEntry, StandingsPlace, StandingsResult, StandingsTable,
    FIELD_SET_BRACKET, FIELD_SET_PRIZE,
};
use crate::tests_app_fixtures::*;
use rt_foundation::{Code, InputId};
use rt_json::{Kind, ParseLimits, Value};
use rt_trace::{Outcome, QuestionKind, TraceValue};
use std::collections::BTreeSet;

pub(crate) const SCOPE: &str = "Score ranking \u{2014} all submitted participants; eligibility is evaluated independently and is not applied as a ranking filter.";
pub(crate) const BRACKET_SCOPE: &str = "Standings are calculated from the declared ranking rules; eligibility is evaluated independently and is not applied as a ranking filter.";
pub(crate) const MEANING_VALUE: &str = "the published value is not the computed value";
pub(crate) const MEANING_TIE_SPLIT: &str =
    "the published result gives members of one computed tie group different positions";
pub(crate) const MEANING_TIE_MERGED: &str =
    "the published result gives one position to participants that the computed result separates";
const TIE_NOTE: &str =
    "members are listed in ascending identifier order, which has no ranking meaning";

/// Key paths of a JSON document: object keys joined by `.`, `[]` after an array
/// key, empty arrays contribute nothing. Written here independently of the
/// writer.
pub(crate) fn key_paths(bytes: &[u8]) -> BTreeSet<String> {
    let limits = ParseLimits::new(1 << 24, 64);
    let v = rt_json::parse(&InputId::new("report").unwrap(), bytes, limits).unwrap();
    let mut out = BTreeSet::new();
    walk(&v, "", &mut out);
    out
}

fn walk<P>(v: &Value<P>, prefix: &str, out: &mut BTreeSet<String>) {
    match v.kind() {
        Kind::Object(members) => {
            for m in members {
                let key = if prefix.is_empty() {
                    m.key.clone()
                } else {
                    format!("{prefix}.{}", m.key)
                };
                walk(&m.value, &key, out);
            }
        }
        Kind::Array(items) => {
            for i in items {
                walk(i, &format!("{prefix}[]"), out);
            }
        }
        _ => {
            out.insert(prefix.to_owned());
        }
    }
}

/// Documents whose traces all fail with a hash mismatch, so that the
/// provenance summary carries a code.
fn prize_with_codes() -> ReportDoc {
    let t = prize_traces();
    prize_doc(&t, &mismatched(&t))
}

fn bracket_with_codes() -> ReportDoc {
    let t = bracket_traces();
    bracket_doc(&t, &mismatched(&t))
}

fn h(d: char) -> String {
    d.to_string().repeat(64)
}

fn src(rule: &str, rev: u32) -> String {
    format!("      source {rule}@{rev} rules.md 0..20 line 1:1\n        \"Entrants must be 16.\"\n")
}

fn prize() -> (ReportDoc, Vec<rt_trace::DecisionTrace>) {
    let t = prize_traces();
    let vs = verified(&t);
    (prize_doc(&t, &vs), t)
}

fn bracket() -> ReportDoc {
    let t = bracket_traces();
    let vs = verified(&t);
    bracket_doc(&t, &vs)
}

#[test]
fn prize_human_matches_the_hand_written_layout() {
    let (doc, _) = prize();
    let mut e = String::new();
    e.push_str("RuleTrace prize-check report   format ruletrace-app-report v1\n");
    e.push_str(&format!("rulebook sha256:{}\n", h('1')));
    e.push_str(&format!("input data sha256:{}\n", h('2')));
    e.push_str(&format!("input published sha256:{}\n", h('3')));
    e.push_str("provenance: verified (6 of 6 traces)\n");
    e.push_str("statements: ST-BOUNDARY ST-VERIFIED-MEANING\n");
    e.push_str("data: synthetic\n");
    e.push_str(&format!("ranking: {SCOPE}\n"));
    e.push_str("\nELIGIBILITY\n");
    e.push_str("  p1  elig.age  true\n");
    e.push_str("      trace #0  verified  rules elig.age@1\n");
    e.push_str(&src("elig.age", 1));
    e.push_str("  p2  elig.age  indeterminate   missing input: birth_date (p2)\n");
    e.push_str("      trace #1  verified  rules elig.age@1\n");
    e.push_str(&src("elig.age", 1));
    e.push_str("\nSCORES\n");
    e.push_str("  p1  score.total  7.5\n");
    e.push_str("      trace #2  verified  rules score.total@2\n");
    e.push_str(&src("score.total", 2));
    e.push_str("  p2  score.total  indeterminate   missing input: points (p2)\n");
    e.push_str("      trace #3  verified  rules score.total@2\n");
    e.push_str(&src("score.total", 2));
    e.push_str("\nSTANDINGS rank.total\n");
    e.push_str("  1  p3\n");
    e.push_str(&format!(
        "  2  {{p1, p2}}   tie group: members share position 2; {TIE_NOTE}\n"
    ));
    e.push_str("      trace #4  verified  rules rank.total@1\n");
    e.push_str(&src("rank.total", 1));
    e.push_str("\nSTANDINGS rank.late\n");
    e.push_str("  indeterminate   missing input: late_score (p4)\n");
    e.push_str("      trace #5  verified  rules rank.late@1\n");
    e.push_str(&src("rank.late", 1));
    e.push_str("\nPUBLISHED CONSISTENCY\n");
    e.push_str("  differs: p1 rank.total published 3, computed 2   kind value_differs (RT-EXE-0011)   trace #3\n");
    e.push_str(&format!("      meaning: {MEANING_VALUE}\n"));
    e.push_str("      trace entry 5\n");
    e.push_str("  differs: p2 rank.total published 3, computed 2/3   kind tie_broken_without_declared_criterion (RT-EXE-0012)   trace #3\n");
    e.push_str(&format!("      meaning: {MEANING_TIE_SPLIT}\n"));
    e.push_str("      trace entry 5\n");
    e.push_str("      computed value has a non-terminating expansion\n");
    e.push_str("      group {p1, p2}\n");
    e.push_str("  published_unused: p9 rank.total\n");
    e.push_str("\nDIAGNOSTICS\n");
    e.push_str("  RT-EXE-0011 warning: published 3 differs from computed 2   at no location (RT-EXE-0011)\n");
    assert_eq!(render_human(&doc), e);
}

fn ev(trace: u32, rule: &str, rev: u32) -> String {
    format!(
        "\"trace\":{trace},\"verified\":true,\"rules\":[{{\"rule\":\"{rule}\",\"revision\":{rev}}}],\"sources\":[{{\"rule\":\"{rule}\",\"revision\":{rev},\"document\":\"rules.md\",\"start\":0,\"end\":20,\"line\":1,\"column\":1,\"text\":\"Entrants must be 16.\",\"code\":null}}]"
    )
}

fn miss(who: &str, field: &str) -> String {
    format!(
        "{{\"kind\":\"missing_input\",\"participant\":\"{who}\",\"field\":\"{field}\",\"code\":null,\"text\":\"missing input: {field} ({who})\"}}"
    )
}

const PUBLISHED_JSON: &str = "\"published\":{\"state\":\"differs\",\"differences\":[{\"participant\":\"p1\",\"target\":\"rank.total\",\"published\":\"3\",\"computed\":\"2\",\"trace\":3,\"entry\":5,\"kind\":\"value_differs\",\"code\":\"RT-EXE-0011\",\"non_terminating\":false,\"group\":[]},{\"participant\":\"p2\",\"target\":\"rank.total\",\"published\":\"3\",\"computed\":\"2/3\",\"trace\":3,\"entry\":5,\"kind\":\"tie_broken_without_declared_criterion\",\"code\":\"RT-EXE-0012\",\"non_terminating\":true,\"group\":[\"p1\",\"p2\"]}],\"unused\":[{\"participant\":\"p9\",\"target\":\"rank.total\"}]}";
const DIAG_JSON: &str = "\"diagnostics\":[{\"code\":\"RT-EXE-0011\",\"severity\":\"warning\",\"message\":\"published 3 differs from computed 2\",\"location\":\"no location (RT-EXE-0011)\"}]";

fn places_json() -> String {
    format!(
        "[{{\"position\":1,\"members\":[\"p3\"],\"tie_group\":false,\"order_note\":null}},{{\"position\":2,\"members\":[\"p1\",\"p2\"],\"tie_group\":true,\"order_note\":\"{TIE_NOTE}\"}}]"
    )
}

#[test]
fn prize_json_matches_the_hand_written_document() {
    let (doc, _) = prize();
    let header = format!(
        "\"header\":{{\"format\":\"ruletrace-app-report\",\"version\":1,\"application\":\"prize-check\",\"statements\":[\"ST-BOUNDARY\",\"ST-VERIFIED-MEANING\"],\"rulebook_hash\":\"{}\",\"inputs\":[{{\"role\":\"data\",\"sha256\":\"{}\"}},{{\"role\":\"published\",\"sha256\":\"{}\"}}],\"provenance\":{{\"status\":\"verified\",\"text\":\"verified\",\"verified_traces\":6,\"total_traces\":6,\"codes\":[]}},\"synthetic_data\":true,\"ranking_scope\":\"{SCOPE}\"}}",
        h('1'),
        h('2'),
        h('3')
    );
    let elig = format!(
        "\"eligibility\":[{{\"participant\":\"p1\",\"target\":\"elig.age\",\"result\":\"true\",\"missing\":[],{}}},{{\"participant\":\"p2\",\"target\":\"elig.age\",\"result\":\"indeterminate\",\"missing\":[{}],{}}}]",
        ev(0, "elig.age", 1),
        miss("p2", "birth_date"),
        ev(1, "elig.age", 1)
    );
    let scores = format!(
        "\"scores\":[{{\"participant\":\"p1\",\"target\":\"score.total\",\"result\":\"exact\",\"value\":\"7.5\",\"missing\":[],{}}},{{\"participant\":\"p2\",\"target\":\"score.total\",\"result\":\"indeterminate\",\"value\":null,\"missing\":[{}],{}}}]",
        ev(2, "score.total", 2),
        miss("p2", "points"),
        ev(3, "score.total", 2)
    );
    let standings = format!(
        "\"standings\":[{{\"target\":\"rank.total\",\"result\":\"ordered\",\"places\":{},\"missing\":[],{}}},{{\"target\":\"rank.late\",\"result\":\"indeterminate\",\"places\":[],\"missing\":[{}],{}}}]",
        places_json(),
        ev(4, "rank.total", 1),
        miss("p4", "late_score"),
        ev(5, "rank.late", 1)
    );
    let expected =
        format!("{{{header},{elig},{scores},{standings},{PUBLISHED_JSON},{DIAG_JSON}}}\n");
    assert_eq!(String::from_utf8(render_json(&doc)).unwrap(), expected);
}

#[test]
fn bracket_human_matches_the_hand_written_layout() {
    let doc = bracket();
    let mut e = String::new();
    e.push_str("RuleTrace bracket-check report   format ruletrace-app-report v1\n");
    e.push_str(&format!("rulebook sha256:{}\n", h('1')));
    e.push_str(&format!("input data sha256:{}\n", h('2')));
    e.push_str(&format!("input published sha256:{}\n", h('3')));
    e.push_str("provenance: verified (4 of 4 traces)\n");
    e.push_str("statements: ST-BOUNDARY ST-VERIFIED-MEANING\n");
    e.push_str("data: synthetic\n");
    e.push_str(&format!("ranking: {BRACKET_SCOPE}\n"));
    e.push_str("\nSTANDINGS rank.total\n");
    e.push_str("  1  p3\n");
    e.push_str(&format!(
        "  2  {{p1, p2}}   tie group: members share position 2; {TIE_NOTE}\n"
    ));
    e.push_str("    compare p3 ahead of p1\n");
    e.push_str("      criterion 0 (desc, rule rank.total): 9 vs 8 -> p3 ahead\n");
    e.push_str("    compare p1 tied with p2\n");
    e.push_str("      criterion 0 (desc, rule rank.total): 9 vs 9 -> equal\n");
    e.push_str("      trace #0  verified  rules rank.total@1\n");
    e.push_str(&src("rank.total", 1));
    e.push_str("\nSTANDINGS rank.late\n");
    e.push_str("  indeterminate   missing input: late_score (p4)\n");
    e.push_str("      trace #1  verified  rules rank.late@1\n");
    e.push_str(&src("rank.late", 1));
    e.push_str("\nQUALIFICATION\n");
    e.push_str("  p1  qual.q  true\n");
    e.push_str("      input read: points (p1) = 70.5 [number]\n");
    e.push_str("      trace #2  verified  rules qual.q@3\n");
    e.push_str(&src("qual.q", 3));
    e.push_str("  p2  qual.q  indeterminate   missing input: points (p2)\n");
    e.push_str("      input read: points (p2) = 70.5 [number]\n");
    e.push_str("      trace #3  verified  rules qual.q@3\n");
    e.push_str(&src("qual.q", 3));
    e.push_str("\nPUBLISHED CONSISTENCY\n");
    e.push_str("  differs: p1 rank.total published 3, computed 2   kind value_differs (RT-EXE-0011)   trace #3\n");
    e.push_str(&format!("      meaning: {MEANING_VALUE}\n"));
    e.push_str("      trace entry 5\n");
    e.push_str("  differs: p2 rank.total published 3, computed 2/3   kind tie_broken_without_declared_criterion (RT-EXE-0012)   trace #3\n");
    e.push_str(&format!("      meaning: {MEANING_TIE_SPLIT}\n"));
    e.push_str("      trace entry 5\n");
    e.push_str("      computed value has a non-terminating expansion\n");
    e.push_str("      group {p1, p2}\n");
    e.push_str("  published_unused: p9 rank.total\n");
    e.push_str("\nDIAGNOSTICS\n");
    e.push_str("  RT-EXE-0011 warning: published 3 differs from computed 2   at no location (RT-EXE-0011)\n");
    assert_eq!(render_human(&doc), e);
}

#[test]
fn bracket_json_matches_the_hand_written_document() {
    let doc = bracket();
    let header = format!(
        "\"header\":{{\"format\":\"ruletrace-app-report\",\"version\":1,\"application\":\"bracket-check\",\"statements\":[\"ST-BOUNDARY\",\"ST-VERIFIED-MEANING\"],\"rulebook_hash\":\"{}\",\"inputs\":[{{\"role\":\"data\",\"sha256\":\"{}\"}},{{\"role\":\"published\",\"sha256\":\"{}\"}}],\"provenance\":{{\"status\":\"verified\",\"text\":\"verified\",\"verified_traces\":4,\"total_traces\":4,\"codes\":[]}},\"synthetic_data\":true,\"ranking_scope\":\"{BRACKET_SCOPE}\"}}",
        h('1'),
        h('2'),
        h('3')
    );
    let criterion = |right: &str, result: &str| {
        format!(
            "{{\"index\":0,\"rule\":\"rank.total\",\"direction\":\"desc\",\"left_type\":\"number\",\"left\":\"9\",\"right_type\":\"number\",\"right\":\"{right}\",\"result\":\"{result}\"}}"
        )
    };
    let standings = format!(
        "\"standings\":[{{\"target\":\"rank.total\",\"result\":\"ordered\",\"places\":{},\"missing\":[],\"comparisons\":[{{\"between\":[\"p3\",\"p1\"],\"first_ahead\":true,\"criteria\":[{}]}},{{\"between\":[\"p1\",\"p2\"],\"first_ahead\":false,\"criteria\":[{}]}}],{}}},{{\"target\":\"rank.late\",\"result\":\"indeterminate\",\"places\":[],\"missing\":[{}],\"comparisons\":[],{}}}]",
        places_json(),
        criterion("8", "first_ahead"),
        criterion("9", "equal"),
        ev(0, "rank.total", 1),
        miss("p4", "late_score"),
        ev(1, "rank.late", 1)
    );
    let read = |who: &str| {
        format!(
            "\"inputs_read\":[{{\"name\":\"points\",\"participant\":\"{who}\",\"type\":\"number\",\"value\":\"70.5\"}}]"
        )
    };
    let qualification = format!(
        "\"qualification\":[{{\"participant\":\"p1\",\"target\":\"qual.q\",\"result\":\"true\",\"missing\":[],{},{}}},{{\"participant\":\"p2\",\"target\":\"qual.q\",\"result\":\"indeterminate\",\"missing\":[{}],{},{}}}]",
        read("p1"),
        ev(2, "qual.q", 3),
        miss("p2", "points"),
        read("p2"),
        ev(3, "qual.q", 3)
    );
    let expected =
        format!("{{{header},{standings},{qualification},{PUBLISHED_JSON},{DIAG_JSON}}}\n");
    assert_eq!(String::from_utf8(render_json(&doc)).unwrap(), expected);
}

#[test]
fn key_paths_equal_the_exported_field_sets_exactly() {
    let got = key_paths(&render_json(&prize_with_codes()));
    let want: BTreeSet<String> = FIELD_SET_PRIZE.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(
        got.difference(&want).collect::<Vec<_>>(),
        Vec::<&String>::new()
    );
    assert_eq!(
        want.difference(&got).collect::<Vec<_>>(),
        Vec::<&String>::new()
    );
    let got = key_paths(&render_json(&bracket_with_codes()));
    let want: BTreeSet<String> = FIELD_SET_BRACKET.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(
        got.difference(&want).collect::<Vec<_>>(),
        Vec::<&String>::new()
    );
    assert_eq!(
        want.difference(&got).collect::<Vec<_>>(),
        Vec::<&String>::new()
    );
}

#[test]
fn field_sets_are_sorted_unique_and_free_of_forbidden_names() {
    for list in [FIELD_SET_PRIZE, FIELD_SET_BRACKET] {
        assert!(list.windows(2).all(|w| w[0] < w[1]), "sorted and unique");
        for path in list {
            for segment in path.split('.') {
                let name = segment.trim_end_matches("[]");
                for bad in [
                    "valid",
                    "ok",
                    "passed",
                    "has_errors",
                    "no_errors",
                    "pairing",
                    "pairings",
                    "round",
                    "bracket",
                    "bracket_position",
                    "seed",
                    "slot",
                    "advancement",
                    "elimination",
                    "prize",
                    "award",
                    "amount",
                    "allocation",
                    "timestamp",
                    "time",
                    "path",
                ] {
                    assert_ne!(name, bad, "forbidden field {bad} in {path}");
                }
            }
        }
    }
    assert!(FIELD_SET_PRIZE
        .iter()
        .any(|p| p.starts_with("eligibility[]")));
    assert!(!FIELD_SET_BRACKET
        .iter()
        .any(|p| p.starts_with("eligibility") || p.starts_with("scores")));
    assert!(!FIELD_SET_PRIZE
        .iter()
        .any(|p| p.contains("comparisons") || p.starts_with("qualification")));
}

#[test]
fn rendered_output_has_no_forbidden_key_and_no_time_or_path() {
    for bytes in [render_json(&prize().0), render_json(&bracket())] {
        for p in key_paths(&bytes) {
            for segment in p.split('.') {
                let name = segment.trim_end_matches("[]");
                assert!(!["valid", "ok", "passed", "has_errors", "no_errors"].contains(&name));
                assert!(!["prize", "award", "amount", "slot", "seed", "round"].contains(&name));
            }
        }
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("/home"));
        assert!(!text.contains("T09:"));
    }
}

#[test]
fn repeated_runs_are_byte_identical() {
    let (doc, t) = prize();
    assert_eq!(render_human(&doc), render_human(&doc));
    assert_eq!(render_json(&doc), render_json(&doc));
    let vs = verified(&t);
    let again = prize_doc(&t, &vs);
    assert_eq!(render_json(&doc), render_json(&again));
    assert_eq!(render_human(&doc), render_human(&again));
}

#[test]
fn sections_are_written_in_the_fixed_order_whatever_the_construction_order() {
    let t = prize_traces();
    let vs = verified(&t);
    let doc = prize_doc(&t, &vs);
    let mut sections = doc.sections().to_vec();
    sections.reverse();
    let reversed = ReportDoc::new(doc.header().clone(), &vs, sections).unwrap();
    assert_eq!(render_json(&doc), render_json(&reversed));
    assert_eq!(render_human(&doc), render_human(&reversed));
    let names: Vec<&str> = reversed
        .sections()
        .iter()
        .map(ReportSection::name)
        .collect();
    assert_eq!(
        names,
        [
            "eligibility",
            "scores",
            "standings",
            "published",
            "diagnostics"
        ]
    );
}

#[test]
fn entries_keep_the_order_of_the_outcomes() {
    let t = vec![
        elig_trace("p9", Outcome::Decided(TraceValue::Bool(true))),
        elig_trace("p1", Outcome::Decided(TraceValue::Bool(false))),
    ];
    let vs = verified(&t);
    let entries = vec![
        EligibilityEntry::from_trace(&tref(0, &t, &vs)).unwrap(),
        EligibilityEntry::from_trace(&tref(1, &t, &vs)).unwrap(),
    ];
    let doc = ReportDoc::new(
        header(ReportApplication::PrizeCheck),
        &vs,
        vec![ReportSection::Eligibility(entries)],
    )
    .unwrap();
    let human = render_human(&doc);
    let p9 = human.find("  p9  elig.age  true").unwrap();
    let p1 = human.find("  p1  elig.age  false").unwrap();
    assert!(p9 < p1);
}

#[test]
fn tie_group_members_are_written_ascending_whatever_the_model_order() {
    let (doc, t) = prize();
    let vs = verified(&t);
    let mut sections = doc.sections().to_vec();
    for s in &mut sections {
        if let ReportSection::Standings(tables) = s {
            if let StandingsResult::Places(places) = &mut tables[0].result {
                places[1].members = vec!["p2".to_owned(), "p1".to_owned()];
            }
        }
    }
    let shuffled = ReportDoc::new(doc.header().clone(), &vs, sections).unwrap();
    assert_eq!(render_json(&doc), render_json(&shuffled));
    let json = String::from_utf8(render_json(&shuffled)).unwrap();
    assert!(json.contains("\"members\":[\"p1\",\"p2\"]"));
    assert!(!json.contains("\"members\":[\"p2\",\"p1\"]"));
    assert!(render_human(&shuffled).contains("{p1, p2}   tie group"));
}

#[test]
fn a_tie_group_entry_states_that_its_order_has_no_ranking_meaning() {
    let (doc, _) = prize();
    let human = render_human(&doc);
    assert_eq!(human.matches(TIE_NOTE).count(), 1);
    let json = String::from_utf8(render_json(&doc)).unwrap();
    assert_eq!(json.matches(TIE_NOTE).count(), 1);
    assert!(json.contains("\"order_note\":null"));
}

#[test]
fn an_indeterminate_entry_names_its_missing_inputs() {
    let t = vec![elig_trace(
        "p2",
        Outcome::indeterminate(vec![
            rt_trace::Reason::MissingInput {
                participant: "p2".to_owned(),
                field: "birth_date".to_owned(),
            },
            rt_trace::Reason::MissingInput {
                participant: "p2".to_owned(),
                field: "country".to_owned(),
            },
        ]),
    )];
    let vs = verified(&t);
    let e = EligibilityEntry::from_trace(&tref(0, &t, &vs)).unwrap();
    let doc = ReportDoc::new(
        header(ReportApplication::PrizeCheck),
        &vs,
        vec![ReportSection::Eligibility(vec![e])],
    )
    .unwrap();
    assert!(render_human(&doc).contains(
        "  p2  elig.age  indeterminate   missing input: birth_date (p2); missing input: country (p2)\n"
    ));
    let json = String::from_utf8(render_json(&doc)).unwrap();
    assert!(json.contains("\"result\":\"indeterminate\""));
    assert!(json.contains("\"field\":\"birth_date\""));
    assert!(json.contains("\"field\":\"country\""));
}

#[test]
fn without_source_models_the_header_says_so_and_carries_rt_prv_0006() {
    let t = prize_traces();
    let vs = unverified(&t);
    assert_eq!(
        ProvenanceSummary::from_verifications(&vs),
        ProvenanceSummary::NotVerifiedNoSources {
            codes: vec![Code::ProvNotVerifiedNoSources],
            without_sources: 6,
            verified: 0,
            total: 6
        }
    );
    let doc = prize_doc(&t, &vs);
    let human = render_human(&doc);
    assert!(human.contains(
        "provenance: not verified: no source models were supplied (warning RT-PRV-0006)\n"
    ));
    assert!(!human.contains("provenance: verified"));
    assert!(human.contains("      trace #0  not verified  rules elig.age@1\n"));
    assert!(human.contains("        unresolved: RT-PRV-0006\n"));
    assert!(!human.contains("\"Entrants must be 16.\""));
    let json = String::from_utf8(render_json(&doc)).unwrap();
    assert!(json.contains(
        "\"provenance\":{\"status\":\"not_verified_no_sources\",\"text\":\"not verified: no source models were supplied\",\"verified_traces\":null,\"total_traces\":null,\"codes\":[\"RT-PRV-0006\"]}"
    ));
    assert!(json.contains("\"verified\":false"));
    assert!(!json.contains("\"verified\":true"));
    assert_eq!(Code::ProvNotVerifiedNoSources.id(), "RT-PRV-0006");
}

#[test]
fn a_run_where_some_traces_lack_sources_states_the_counts() {
    let t = prize_traces();
    let mut vs = verified(&t);
    vs[1] = unverified(&t[1..2]).remove(0);
    let expected = ProvenanceSummary::NotVerifiedNoSources {
        codes: vec![Code::ProvNotVerifiedNoSources],
        without_sources: 1,
        verified: 5,
        total: 6,
    };
    assert_eq!(ProvenanceSummary::from_verifications(&vs), expected);
    let doc = prize_doc(&t, &vs);
    assert_eq!(doc.provenance(), &expected);
    let human = render_human(&doc);
    assert!(human.contains(
        "provenance: not verified: 1 of 6 traces had no source models (5 verified; warning RT-PRV-0006)\n"
    ));
    assert!(!human.contains("no source models were supplied"));
    assert!(!human.contains("provenance: verified"));
    let json = String::from_utf8(render_json(&doc)).unwrap();
    assert!(json.contains(
        "\"provenance\":{\"status\":\"not_verified_no_sources\",\"text\":\"not verified: 1 of 6 traces had no source models\",\"verified_traces\":5,\"total_traces\":6,\"codes\":[\"RT-PRV-0006\"]}"
    ));
    assert_eq!(
        ProvenanceSummary::from_verifications(&[]),
        ProvenanceSummary::NotVerified {
            codes: Vec::new(),
            verified: 0,
            total: 0
        }
    );
}

#[test]
fn one_verified_and_one_hash_mismatch_trace_are_never_summarised_as_verified() {
    let t = vec![
        elig_trace("p1", Outcome::Decided(TraceValue::Bool(true))),
        elig_trace("p2", Outcome::Decided(TraceValue::Bool(true))),
    ];
    let vs = vec![
        crate::verify_trace(&t[0], &models()),
        crate::verify_trace(&t[1], &other_models()),
    ];
    let expected = ProvenanceSummary::NotVerified {
        codes: vec![Code::ProvHashMismatch],
        verified: 1,
        total: 2,
    };
    assert_eq!(ProvenanceSummary::from_verifications(&vs), expected);
    let doc = ReportDoc::new(
        header(ReportApplication::PrizeCheck),
        &vs,
        vec![ReportSection::Eligibility(vec![
            EligibilityEntry::from_trace(&tref(0, &t, &vs)).unwrap(),
            EligibilityEntry::from_trace(&tref(1, &t, &vs)).unwrap(),
        ])],
    )
    .unwrap();
    assert_eq!(doc.provenance(), &expected);
    let human = render_human(&doc);
    assert!(human.contains("provenance: not verified (1 of 2 traces verified; RT-PRV-0002)\n"));
    assert!(human.contains("      trace #0  verified  rules elig.age@1\n"));
    assert!(human.contains("      trace #1  not verified  rules elig.age@1\n"));
    let json = String::from_utf8(render_json(&doc)).unwrap();
    assert!(json.contains("\"status\":\"not_verified\""));
    assert!(!json.contains("\"status\":\"verified\""));
}

#[test]
fn a_hash_mismatch_gives_not_verified_with_its_code() {
    let t = prize_traces();
    let mut other = rt_srcmodel::SourceModels::new();
    let m = rt_srcmodel::SourceModel::new(
        rt_foundation::DocumentId::new("rules.md").unwrap(),
        rt_srcmodel::DocumentKind::Markdown,
        "Different text entirely.".to_owned(),
        vec![rt_srcmodel::Element {
            id: 0,
            kind: rt_srcmodel::ElementKind::Paragraph,
            level: None,
            label: None,
            start: 0,
            end: 24,
            line: 1,
            column: 1,
            parent: None,
        }],
    )
    .unwrap();
    other.insert(m).unwrap();
    let vs: Vec<_> = t.iter().map(|x| crate::verify_trace(x, &other)).collect();
    let s = ProvenanceSummary::from_verifications(&vs);
    assert_eq!(
        s,
        ProvenanceSummary::NotVerified {
            codes: vec![Code::ProvHashMismatch],
            verified: 0,
            total: 6
        }
    );
    let human = render_human(&prize_doc(&t, &vs));
    assert!(human.contains("provenance: not verified (0 of 6 traces verified; RT-PRV-0002)\n"));
}

#[test]
fn control_characters_in_every_dynamic_field_are_escaped_in_human_output() {
    let t = vec![
        elig_trace("p\u{1b}[31m1", Outcome::Decided(TraceValue::Bool(true))),
        elig_trace("p2", missing_outcome("p\u{7}2", "bi\u{0}rth")),
        score_trace("p3", "1"),
    ];
    let mut vs = verified(&t);
    let mut t2 = t.clone();
    // A span that covers the ESC byte and the final line feed of the source.
    t2[2] = trace(
        question(QuestionKind::Value, "sc\u{8}ore", Some("p3")),
        Outcome::Decided(num("1")),
        "score.total",
        2,
        (0, 25),
        Vec::new(),
        Vec::new(),
    );
    vs[2] = crate::verify_trace(&t2[2], &models());
    let diag = rt_foundation::Diagnostic::new(
        Code::PublishedResultDiffers,
        "line\u{1b}[0m\n".to_owned(),
        rt_foundation::Location::None(Code::PublishedResultDiffers),
    );
    let doc = ReportDoc::new(
        header(ReportApplication::PrizeCheck),
        &vs,
        vec![
            ReportSection::Eligibility(vec![
                EligibilityEntry::from_trace(&tref(0, &t2, &vs)).unwrap(),
                EligibilityEntry::from_trace(&tref(1, &t2, &vs)).unwrap(),
            ]),
            ReportSection::Scores(vec![ScoreEntry::from_trace(&tref(2, &t2, &vs)).unwrap()]),
            ReportSection::Diagnostics(vec![diag]),
        ],
    )
    .unwrap();
    let human = render_human(&doc);
    for c in human.chars() {
        assert!(c == '\n' || !c.is_control(), "raw control {:?}", c);
    }
    assert!(human.contains("p\\u{1b}[31m1"));
    assert!(human.contains("missing input: bi\\u{0}rth (p\\u{7}2)"));
    assert!(human.contains("sc\\u{8}ore"));
    assert!(human.contains("\"Entrants must be 16.\\u{1b}[0m\\u{a}\""));
    assert!(human.contains("line\\u{1b}[0m\\u{a}   at"));
    let json = String::from_utf8(render_json(&doc)).unwrap();
    assert!(!json.contains('\u{1b}'));
}

#[test]
fn published_section_states_not_supplied_equal_or_differs() {
    let t = prize_traces();
    let vs = verified(&t);
    let render = |p: PublishedSection| {
        let doc = ReportDoc::new(
            header(ReportApplication::PrizeCheck),
            &vs,
            vec![ReportSection::Published(p)],
        )
        .unwrap();
        (
            render_human(&doc),
            String::from_utf8(render_json(&doc)).unwrap(),
        )
    };
    let (human, json) = render(PublishedSection::NotSupplied);
    assert!(human.ends_with("PUBLISHED CONSISTENCY\n  not supplied\n"));
    assert!(json.contains("\"published\":{\"state\":\"not_supplied\""));
    let equal = PublishedSection::Compared {
        differences: Vec::new(),
        unused: Vec::new(),
    };
    let (human, json) = render(equal);
    assert!(human.ends_with("PUBLISHED CONSISTENCY\n  equal\n"));
    assert!(json.contains("\"published\":{\"state\":\"equal\",\"differences\":[],\"unused\":[]}"));
    let indeterminate_facts = DifferenceFacts {
        computed: None,
        ..facts(&[])
    };
    let indeterminate = PublishedDifference::new("value_differs", indeterminate_facts).unwrap();
    let (human, json) = render(PublishedSection::Compared {
        differences: vec![indeterminate],
        unused: Vec::new(),
    });
    assert!(human.contains("published 3, computed indeterminate   kind value_differs"));
    assert!(json.contains("\"computed\":null"));
}

#[test]
fn each_difference_kind_is_written_with_its_catalogue_code() {
    for (kind, id, meaning) in [
        ("value_differs", "RT-EXE-0011", MEANING_VALUE),
        (
            "tie_broken_without_declared_criterion",
            "RT-EXE-0012",
            MEANING_TIE_SPLIT,
        ),
        (
            "tie_not_in_computed_result",
            "RT-EXE-0013",
            MEANING_TIE_MERGED,
        ),
    ] {
        let t = prize_traces();
        let vs = verified(&t);
        let doc = ReportDoc::new(
            header(ReportApplication::BracketCheck),
            &vs,
            vec![ReportSection::Published(PublishedSection::Compared {
                differences: vec![difference(kind, &["p4", "p3"])],
                unused: Vec::new(),
            })],
        )
        .unwrap();
        let json = String::from_utf8(render_json(&doc)).unwrap();
        assert!(json.contains(&format!("\"kind\":\"{kind}\",\"code\":\"{id}\"")));
        assert!(json.contains("\"group\":[\"p3\",\"p4\"]"));
        let human = render_human(&doc);
        assert!(human.contains(&format!("kind {kind} ({id})")));
        assert!(human.contains(&format!("      meaning: {meaning}\n")));
    }
}

#[test]
fn assembly_refuses_a_foreign_duplicate_or_mismatched_part() {
    let t = prize_traces();
    let vs = verified(&t);
    let prize_header = || header(ReportApplication::PrizeCheck);
    assert_eq!(
        ReportDoc::new(
            prize_header(),
            &vs,
            vec![ReportSection::Qualification(vec![])]
        ),
        Err(ReportError::SectionNotAllowed {
            section: "qualification"
        })
    );
    assert_eq!(
        ReportDoc::new(
            header(ReportApplication::BracketCheck),
            &vs,
            vec![ReportSection::Scores(vec![])]
        ),
        Err(ReportError::SectionNotAllowed { section: "scores" })
    );
    assert_eq!(
        ReportDoc::new(
            prize_header(),
            &vs,
            vec![ReportSection::Scores(vec![]), ReportSection::Scores(vec![])]
        ),
        Err(ReportError::DuplicateSection { section: "scores" })
    );
    let with_comparisons = StandingsTable::from_trace(&tref(4, &t, &vs), true).unwrap();
    assert_eq!(
        ReportDoc::new(
            prize_header(),
            &vs,
            vec![ReportSection::Standings(vec![with_comparisons])]
        ),
        Err(ReportError::ComparisonsNotAllowed)
    );
}

#[test]
fn builders_refuse_a_trace_of_the_wrong_kind() {
    let t = prize_traces();
    let vs = verified(&t);
    assert_eq!(
        EligibilityEntry::from_trace(&tref(2, &t, &vs)),
        Err(ReportError::OutcomeKind {
            trace: 2,
            expected: "a Boolean"
        })
    );
    assert_eq!(
        ScoreEntry::from_trace(&tref(0, &t, &vs)),
        Err(ReportError::OutcomeKind {
            trace: 0,
            expected: "a number"
        })
    );
    assert_eq!(
        StandingsTable::from_trace(&tref(0, &t, &vs), false),
        Err(ReportError::OutcomeKind {
            trace: 0,
            expected: "a rank question"
        })
    );
    assert_eq!(
        EligibilityEntry::from_trace(&tref(4, &t, &vs)),
        Err(ReportError::OutcomeKind {
            trace: 4,
            expected: "a value question"
        })
    );
}

#[test]
fn entries_copy_values_from_the_outcome_and_trace() {
    let t = prize_traces();
    let vs = verified(&t);
    let s = ScoreEntry::from_trace(&tref(2, &t, &vs)).unwrap();
    assert_eq!(
        s.result,
        Score::Exact(rt_numeric::parse_literal("7.5").unwrap())
    );
    assert_eq!(s.evidence.trace, 2);
    assert!(s.evidence.verified);
    assert_eq!(s.evidence.rules.len(), 1);
    assert_eq!(s.evidence.sources[0].text(), Some("Entrants must be 16."));
    let st = StandingsTable::from_trace(&tref(4, &t, &vs), true).unwrap();
    assert_eq!(
        st.result,
        StandingsResult::Places(vec![
            StandingsPlace {
                position: 1,
                members: vec!["p3".to_owned()]
            },
            StandingsPlace {
                position: 2,
                members: vec!["p1".to_owned(), "p2".to_owned()]
            },
        ])
    );
    assert_eq!(st.comparisons.len(), 2);
    let ind = StandingsTable::from_trace(&tref(5, &t, &vs), true).unwrap();
    assert_eq!(
        ind.result,
        StandingsResult::Indeterminate(vec![Missing::Input {
            participant: "p4".to_owned(),
            field: "late_score".to_owned()
        }])
    );
    let _: &Evidence = &ind.evidence;
}
