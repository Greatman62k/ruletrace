//! Tests of `ruletrace validate` (task 5.4).

use super::*;
use rt_foundation::{DocumentId, InputId};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, rule_content_hash};
use rt_srcmodel::DocumentKind;

pub(super) const TEXT: &str = "# Spring Open Rules\n\n1. A competitor is eligible if age is at least 16 and holds a licence.\n2. The score is 0.6 times the judges score.\n";
const CLAUSE1: &str = "1. A competitor is eligible if age is at least 16 and holds a licence.";
pub(super) const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";

pub(super) fn rulebook_text() -> String {
    let hash = sha256(TEXT.as_bytes());
    let start = TEXT.find(CLAUSE1).unwrap();
    let end = start + CLAUSE1.len();
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{hash}"}}],"types":{{"inputs":[],"enums":[],"sets":[],"collections":[]}},"rules":[{{"id":"d.a","revision":1,"origin":{{"kind":"manual","author":null}},"kind":"derived","target":"a","type":"number","expr":{{"node":"num","t":"number","value":"1"}},"markers":[],"spans":[{{"document":"rules.md","start":{start},"end":{end},"line":3,"column":1}}]}}],"precedence":[]}}"#
    )
}

pub(super) fn model_text(text: &str) -> String {
    rt_srcmodel::SourceModel::new(
        DocumentId::new("rules.md").unwrap(),
        DocumentKind::Markdown,
        text.to_owned(),
        Vec::new(),
    )
    .unwrap()
    .print()
}

/// A ledger with one approval of `d.a` revision 1, written by hand with its hash chain.
fn approved_ledger() -> String {
    let rb = parse_rulebook(
        rulebook_text().as_bytes(),
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
        r#"{{"seq":1,"kind":"approval","rule":"d.a","revision":1,"content_hash":"{content}","ir_version":"1","binding":[{{"document":"rules.md","sha256":"{doc}"}}],"reviewer":"A. Reviewer","time":"2026-11-14T09:00:00Z","reason":null,"prev":"{ZERO}""#
    );
    let hash = sha256(format!("{base}}}").as_bytes());
    format!("{}\n{base},\"hash\":\"{hash}\"}}\n", rt_ledger::HEADER_LINE)
}

pub(super) fn empty_ledger() -> String {
    format!("{}\n", rt_ledger::HEADER_LINE)
}

fn files(ledger: &str) -> MemFs {
    MemFs::with(&[
        ("rb.json", rulebook_text().as_bytes()),
        ("ledger.jsonl", ledger.as_bytes()),
        ("model.json", model_text(TEXT).as_bytes()),
    ])
}

const ARGS: [&str; 9] = [
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

fn artifact_has_no_state_flags(bytes: &[u8]) {
    let input = InputId::new("validation.json").unwrap();
    let root = rt_json::parse(&input, bytes, rt_json::ParseLimits::new(1 << 24, 64)).unwrap();
    assert!(rt_json::check_flag_keys(&input, &root).is_ok());
    assert_eq!(
        root.get("format").and_then(|m| m.value.as_str()),
        Some("ruletrace-validation")
    );
}

#[test]
fn approved_rulebook_exits_0_and_writes_the_artifact() {
    let fs = files(&approved_ledger());
    let out = run(&os(&ARGS), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.is_empty(), "{}", out.stderr);
    let bytes = fs.get("validation.json").unwrap();
    artifact_has_no_state_flags(&bytes);
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains(r#""executable":false"#));
    assert!(text.contains(r#""diagnostics":[]"#));
    assert!(text.contains(r#""checked_conflict_classes":["ContradictoryDefault","#));
    // A second run gives the same bytes.
    let again = files(&approved_ledger());
    let _ = run(&os(&ARGS), &again);
    assert_eq!(again.get("validation.json").unwrap(), bytes);
}

#[test]
fn unresolved_revision_exits_3_and_writes_the_artifact_with_the_diagnostic() {
    let fs = files(&empty_ledger());
    let out = run(&os(&ARGS), &fs);
    assert_eq!(out.code.value(), 3);
    assert!(out.stderr.contains("RT-APR-0001"), "{}", out.stderr);
    let bytes = fs.get("validation.json").unwrap();
    artifact_has_no_state_flags(&bytes);
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("RT-APR-0001"));
    assert!(text.contains(r#""executable":false"#));
}

#[test]
fn json_output_format_renders_one_array_line() {
    let fs = files(&empty_ledger());
    let mut args = ARGS.to_vec();
    args.extend(["--output-format", "json"]);
    let out = run(&os(&args), &fs);
    assert_eq!(out.code.value(), 3);
    assert!(out.stderr.starts_with('['));
    assert!(out.stderr.ends_with("]\n"));
    assert_eq!(out.stderr.matches('\n').count(), 1);
    assert!(out.stderr.contains(r#""code":"RT-APR-0001""#));
}

#[test]
fn source_model_hash_mismatch_exits_2_and_writes_nothing() {
    let other = TEXT.replace("0.6", "0.7");
    let fs = files(&approved_ledger());
    fs.files
        .borrow_mut()
        .insert(PathBuf::from("model.json"), model_text(&other).into_bytes());
    let before = fs.snapshot();
    let out = run(&os(&ARGS), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("RT-VAL-0018"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn invalid_input_exits_2_and_keeps_an_existing_output() {
    let mut tampered = approved_ledger();
    tampered = tampered.replace("A. Reviewer", "B. Reviewer");
    for (name, content) in [
        ("rb.json", b"{".to_vec()),
        ("ledger.jsonl", tampered.into_bytes()),
        ("model.json", b"not json".to_vec()),
    ] {
        let fs = files(&approved_ledger());
        fs.files
            .borrow_mut()
            .insert(PathBuf::from("validation.json"), b"old".to_vec());
        fs.files.borrow_mut().insert(PathBuf::from(name), content);
        let out = run(&os(&ARGS), &fs);
        assert_eq!(out.code.value(), 2, "{name}: {}", out.stderr);
        assert_eq!(fs.get("validation.json").unwrap(), b"old", "{name}");
    }
    let fs = files(&approved_ledger());
    fs.files.borrow_mut().remove(Path::new("rb.json"));
    let out = run(&os(&ARGS), &fs);
    assert_eq!(out.code.value(), 2);
    assert!(out.stderr.contains("RT-CLI-0003"));
    assert!(fs.get("validation.json").is_none());
}

#[test]
fn max_bytes_limits_the_rulebook() {
    let fs = files(&approved_ledger());
    let mut args = ARGS.to_vec();
    args.extend(["--max-bytes", "50"]);
    let out = run(&os(&args), &fs);
    assert_eq!(out.code.value(), 2);
    assert!(out.stderr.contains("RT-LIM-0001"), "{}", out.stderr);
    assert!(fs.get("validation.json").is_none());
}

#[test]
fn write_failure_exits_70_and_creates_no_file() {
    let fs = files(&approved_ledger());
    fs.fail_create.set(true);
    let out = run(&os(&ARGS), &fs);
    assert_eq!(out.code.value(), 70);
    assert!(out.stderr.contains("RT-CLI-0005"));
    assert!(fs.get("validation.json").is_none());
}

#[test]
fn candidates_for_an_unknown_document_exit_2() {
    let fs = files(&approved_ledger());
    fs.files.borrow_mut().insert(
        PathBuf::from("c.json"),
        br#"{"format":"ruletrace-candidates","version":1,"document":"other.md","clauses":[]}"#
            .to_vec(),
    );
    let mut args = ARGS.to_vec();
    args.extend(["--candidates", "c.json"]);
    let out = run(&os(&args), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("RT-CLI-0004"));
    assert!(fs.get("validation.json").is_none());
}

#[test]
fn argument_errors_touch_nothing() {
    let fs = files(&approved_ledger());
    let before = fs.snapshot();
    let cases: [&[&str]; 6] = [
        &["validate"],
        &[
            "validate",
            "--rulebook",
            "rb.json",
            "--ledger",
            "ledger.jsonl",
            "--out",
            "v.json",
        ],
        &[
            "validate",
            "--rulebook",
            "rb.json",
            "--rulebook",
            "rb.json",
            "--ledger",
            "l",
            "--source-model",
            "m",
            "--out",
            "v",
        ],
        &[
            "validate",
            "--rulebook",
            "rb.json",
            "--ledger",
            "l",
            "--source-model",
            "m",
            "--out",
            "rb.json",
        ],
        &[
            "validate",
            "--rulebook",
            "a",
            "--ledger",
            "l",
            "--source-model",
            "m",
            "--out",
            "v",
            "--output-format",
            "xml",
        ],
        &[
            "validate",
            "--rulebook",
            "a",
            "--ledger",
            "l",
            "--source-model",
            "m",
            "--out",
            "v",
            "--max-depth",
            "0",
        ],
    ];
    for case in cases {
        let out = run(&os(case), &fs);
        assert_eq!(out.code.value(), 2, "{case:?}: {}", out.stderr);
        assert!(out.stderr.contains("RT-CLI-000"), "{case:?}");
        assert_eq!(fs.snapshot(), before, "{case:?}");
    }
}

#[test]
fn source_model_may_repeat_and_duplicates_of_one_document_are_invalid() {
    let fs = files(&approved_ledger());
    fs.files
        .borrow_mut()
        .insert(PathBuf::from("again.json"), model_text(TEXT).into_bytes());
    let mut args = ARGS.to_vec();
    args.extend(["--source-model", "again.json"]);
    let out = run(&os(&args), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("more than one Source_Model"));
    assert!(fs.get("validation.json").is_none());
}
