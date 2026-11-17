//! The Diagnostic code of each published-result difference kind through
//! `ruletrace evaluate` (RN-26, decision 2 of 2026-11-17: Req 13.12).
//!
//! Every expectation is written by hand from the ranking
//! `p1 = p2 = 30 > p3 = 20 > p4 = 10`, whose computed Rank_Positions are
//! 1, 1, 3 and 4 (position = 1 + participants strictly ahead).

use super::*;
use rt_foundation::{DocumentId, InputId};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, rule_content_hash};
use rt_srcmodel::DocumentKind;

const TEXT: &str = "# Spring Open Rules\n\n1. Competitors are ranked by age, oldest first.\n";
const CLAUSE: &str = "1. Competitors are ranked by age, oldest first.";
const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn rulebook_text() -> String {
    let hash = sha256(TEXT.as_bytes());
    let start = TEXT.find(CLAUSE).unwrap();
    let end = start + CLAUSE.len();
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{hash}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{{"id":"r.s","revision":1,"origin":{{"kind":"manual","author":null}},"kind":"ranking","target":"standing","type":"ranking","key":{{"node":"field","t":"number","name":"age"}},"direction":"desc","tie_breakers":[],"ties":"group","markers":[],"spans":[{{"document":"rules.md","start":{start},"end":{end},"line":3,"column":1}}]}}],"precedence":[]}}"#
    )
}

fn model_text() -> String {
    rt_srcmodel::SourceModel::new(
        DocumentId::new("rules.md").unwrap(),
        DocumentKind::Markdown,
        TEXT.to_owned(),
        Vec::new(),
    )
    .unwrap()
    .print()
}

fn approved_ledger(rulebook: &str) -> String {
    let rb = parse_rulebook(
        rulebook.as_bytes(),
        &InputId::new("rb.json").unwrap(),
        1 << 22,
        64,
    )
    .unwrap()
    .into_parts()
    .0;
    let content = rule_content_hash(&rb.ir_version, &rb.revisions[0], &rb.documents).unwrap();
    let doc = sha256(TEXT.as_bytes());
    let base = format!(
        r#"{{"seq":1,"kind":"approval","rule":"r.s","revision":1,"content_hash":"{content}","ir_version":"1","binding":[{{"document":"rules.md","sha256":"{doc}"}}],"reviewer":"A. Reviewer","time":"2026-11-14T09:00:00Z","reason":null,"prev":"{ZERO}""#
    );
    let hash = sha256(format!("{base}}}").as_bytes());
    format!("{}\n{base},\"hash\":\"{hash}\"}}\n", rt_ledger::HEADER_LINE)
}

const DATA: &str = r#"{"format":"ruletrace-data","version":1,"participants":[{"id":"p1","fields":{"age":"30"},"collections":{}},{"id":"p2","fields":{"age":"30"},"collections":{}},{"id":"p3","fields":{"age":"20"},"collections":{}},{"id":"p4","fields":{"age":"10"},"collections":{}}]}"#;
const DATA_MISSING: &str = r#"{"format":"ruletrace-data","version":1,"participants":[{"id":"p1","fields":{"age":"30"},"collections":{}},{"id":"p2","fields":{"age":"30"},"collections":{}},{"id":"p3","fields":{"age":"20"},"collections":{}},{"id":"p4","fields":{},"collections":{}}]}"#;
const QUESTIONS: &str = r#"{"format":"ruletrace-questions","version":1,"questions":[{"kind":"rank","target":"standing","participants":null}]}"#;

fn published(entries: &[(&str, &str)]) -> String {
    let list: Vec<String> = entries
        .iter()
        .map(|(p, v)| format!(r#"{{"participant":"{p}","target":"standing","value":"{v}"}}"#))
        .collect();
    format!(
        r#"{{"format":"ruletrace-published","version":1,"entries":[{}]}}"#,
        list.join(",")
    )
}

/// Evaluates the ranking over `data` against the published entries.
fn evaluate_published(data: &str, entries: &[(&str, &str)]) -> Outcome {
    let rulebook = rulebook_text();
    let fs = MemFs::with(&[
        ("rb.json", rulebook.as_bytes()),
        ("ledger.jsonl", approved_ledger(&rulebook).as_bytes()),
        ("model.json", model_text().as_bytes()),
        ("data.json", data.as_bytes()),
        ("q.json", QUESTIONS.as_bytes()),
        ("p.json", published(entries).as_bytes()),
    ]);
    let validate = [
        "validate",
        "--rulebook",
        "rb.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "model.json",
        "--out",
        "validation.json",
    ];
    assert_eq!(run(&os(&validate), &fs).code.value(), 0);
    let compile = [
        "compile",
        "--validation",
        "validation.json",
        "--out",
        "compiled.json",
    ];
    assert_eq!(run(&os(&compile), &fs).code.value(), 0);
    let evaluate = [
        "evaluate",
        "--compiled",
        "compiled.json",
        "--data",
        "data.json",
        "--questions",
        "q.json",
        "--published",
        "p.json",
        "--source-model",
        "model.json",
        "--out",
        "results.json",
    ];
    run(&os(&evaluate), &fs)
}

#[test]
fn a_split_computed_tie_group_is_reported_under_rt_exe_0012() {
    // p1 and p2 share computed position 1; the published file separates them.
    let out = evaluate_published(DATA, &[("p1", "1"), ("p2", "2")]);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert_eq!(
        out.stderr,
        "warning RT-EXE-0012: participant 'p2', target 'standing': published 2 differs from computed 1; the published file gives members of the computed Tie_Group [p1, p2] different positions\n  at input 'p.json' field entries[p2/standing]\n"
    );
}

#[test]
fn a_published_tie_the_computed_result_separates_is_reported_under_rt_exe_0013() {
    // p3 and p4 are published as tied at 3; the computed positions are 3, 4.
    let out = evaluate_published(DATA, &[("p3", "3"), ("p4", "3")]);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert_eq!(
        out.stderr,
        "warning RT-EXE-0013: participant 'p4', target 'standing': published 3 differs from computed 4; the published file ties [p3, p4] and the computed result separates them\n  at input 'p.json' field entries[p4/standing]\n"
    );
}

#[test]
fn a_plain_value_difference_stays_under_rt_exe_0011() {
    let out = evaluate_published(DATA, &[("p4", "5")]);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert_eq!(
        out.stderr,
        "warning RT-EXE-0011: participant 'p4', target 'standing': published 5 differs from computed 4\n  at input 'p.json' field entries[p4/standing]\n"
    );
}

#[test]
fn a_difference_with_an_indeterminate_ranking_is_rt_exe_0011_and_exit_4() {
    // p4 has no age, so the whole ranking is Indeterminate (RT-EXE-0006)
    // and a published tie cannot be classified.
    let out = evaluate_published(DATA_MISSING, &[("p3", "3"), ("p4", "3")]);
    assert_eq!(out.code.value(), 4, "{}", out.stderr);
    assert!(out.stderr.contains("RT-EXE-0006"), "{}", out.stderr);
    assert!(
        out.stderr.contains("warning RT-EXE-0011:"),
        "{}",
        out.stderr
    );
    assert!(!out.stderr.contains("RT-EXE-0012"), "{}", out.stderr);
    assert!(!out.stderr.contains("RT-EXE-0013"), "{}", out.stderr);
}
