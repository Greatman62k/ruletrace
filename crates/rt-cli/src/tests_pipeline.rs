//! Tests of `ruletrace compile` and `ruletrace evaluate`.

use super::*;
use rt_foundation::{DocumentId, InputId};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, rule_content_hash};
use rt_srcmodel::DocumentKind;

const TEXT: &str = "# Spring Open Rules\n\n1. A competitor is eligible if age is at least 16 and holds a licence.\n2. The score is 0.6 times the judges score.\n";
const CLAUSE1: &str = "1. A competitor is eligible if age is at least 16 and holds a licence.";
const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// One derived rule `a` that reads the input `age`.
fn rulebook_text(expr: &str) -> String {
    let hash = sha256(TEXT.as_bytes());
    let start = TEXT.find(CLAUSE1).unwrap();
    let end = start + CLAUSE1.len();
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{hash}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{{"id":"d.a","revision":1,"origin":{{"kind":"manual","author":null}},"kind":"derived","target":"a","type":"number","expr":{expr},"markers":[],"spans":[{{"document":"rules.md","start":{start},"end":{end},"line":3,"column":1}}]}}],"precedence":[]}}"#
    )
}

const AGE: &str = r#"{"node":"field","t":"number","name":"age"}"#;

fn model_text(text: &str) -> String {
    rt_srcmodel::SourceModel::new(
        DocumentId::new("rules.md").unwrap(),
        DocumentKind::Markdown,
        text.to_owned(),
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
        r#"{{"seq":1,"kind":"approval","rule":"d.a","revision":1,"content_hash":"{content}","ir_version":"1","binding":[{{"document":"rules.md","sha256":"{doc}"}}],"reviewer":"A. Reviewer","time":"2026-11-14T09:00:00Z","reason":null,"prev":"{ZERO}""#
    );
    let hash = sha256(format!("{base}}}").as_bytes());
    format!("{}\n{base},\"hash\":\"{hash}\"}}\n", rt_ledger::HEADER_LINE)
}

fn empty_ledger() -> String {
    format!("{}\n", rt_ledger::HEADER_LINE)
}

const DATA: &str = r#"{"format":"ruletrace-data","version":1,"participants":[{"id":"p1","fields":{"age":"17"},"collections":{}},{"id":"p2","fields":{},"collections":{}}]}"#;
const DATA_COMPLETE: &str = r#"{"format":"ruletrace-data","version":1,"participants":[{"id":"p1","fields":{"age":"17"},"collections":{}},{"id":"p2","fields":{"age":"20"},"collections":{}}]}"#;

fn files(ledger: &str) -> MemFs {
    let rulebook = rulebook_text(AGE);
    MemFs::with(&[
        ("rb.json", rulebook.as_bytes()),
        ("ledger.jsonl", ledger.as_bytes()),
        ("model.json", model_text(TEXT).as_bytes()),
        ("data.json", DATA.as_bytes()),
        ("data-complete.json", DATA_COMPLETE.as_bytes()),
    ])
}

const VALIDATE: [&str; 9] = [
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
const COMPILE: [&str; 5] = [
    "compile",
    "--validation",
    "validation.json",
    "--out",
    "compiled.json",
];

fn evaluate_args<'a>(data: &'a str, extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "evaluate",
        "--compiled",
        "compiled.json",
        "--data",
        data,
        "--source-model",
        "model.json",
        "--out",
        "results.json",
    ];
    args.extend_from_slice(extra);
    args
}

/// Validates and compiles the approved fixture; returns the file system with
/// `compiled.json`.
fn compiled_fs() -> MemFs {
    let fs = files(&approved_ledger(&rulebook_text(AGE)));
    assert_eq!(run(&os(&VALIDATE), &fs).code.value(), 0);
    let out = run(&os(&COMPILE), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    fs
}

const PROOF: [&str; 6] = [
    "--rulebook",
    "rb.json",
    "--ledger",
    "ledger.jsonl",
    "--proof-report",
    "proof.txt",
];

#[test]
fn compile_refuses_an_artifact_with_blocking_diagnostics_with_exit_3() {
    let fs = files(&empty_ledger());
    let validated = run(&os(&VALIDATE), &fs);
    assert_eq!(validated.code.value(), 3);
    let out = run(&os(&COMPILE), &fs);
    assert_eq!(out.code.value(), 3, "{}", out.stderr);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("RT-APR-0001"), "{}", out.stderr);
    // The Diagnostics are the ones `validate` reported, unchanged.
    assert_eq!(out.stderr, validated.stderr);
    assert!(fs.get("compiled.json").is_none());
}

#[test]
fn compile_succeeds_and_is_deterministic() {
    let fs = compiled_fs();
    let bytes = fs.get("compiled.json").unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.starts_with(r#"{"format":"#), "{text}");
    assert!(text.ends_with("}\n"));
    let again = compiled_fs();
    assert_eq!(again.get("compiled.json").unwrap(), bytes);
    // The artifact parses as a Compiled_Rulebook.
    let input = InputId::new("compiled.json").unwrap();
    assert!(rt_compiled::parse_compiled(&bytes, &input, 1 << 24, 64).is_ok());
}

#[test]
fn compile_derives_its_verdict_again_and_does_not_trust_missing_diagnostics() {
    let fs = files(&empty_ledger());
    let _ = run(&os(&VALIDATE), &fs);
    let artifact = String::from_utf8(fs.get("validation.json").unwrap()).unwrap();
    let start = artifact.find(r#""diagnostics":["#).unwrap();
    let end = artifact.find(r#","checked_conflict_classes""#).unwrap();
    let tampered = format!(
        "{}{}{}",
        &artifact[..start],
        r#""diagnostics":[]"#,
        &artifact[end..]
    );
    fs.files
        .borrow_mut()
        .insert(PathBuf::from("validation.json"), tampered.into_bytes());
    let out = run(&os(&COMPILE), &fs);
    assert_eq!(out.code.value(), 3, "{}", out.stderr);
    assert!(out.stderr.contains("RT-APR-0005"), "{}", out.stderr);
    assert!(fs.get("compiled.json").is_none());
}

#[test]
fn compile_invalid_artifacts_exit_2_and_write_nothing() {
    let fs = files(&approved_ledger(&rulebook_text(AGE)));
    let _ = run(&os(&VALIDATE), &fs);
    let good = String::from_utf8(fs.get("validation.json").unwrap()).unwrap();
    let cases = [
        ("not json".to_owned(), "RT-IR-0001"),
        (
            good.replace(
                r#""executable":false"#,
                r#""executable":false,"no_errors":true"#,
            ),
            "RT-DGN-0001",
        ),
        (
            good.replace(r#""executable":false"#, r#""executable":true"#),
            "RT-CMP-0003",
        ),
        (
            good.replace(r#""format":"ruletrace-validation""#, r#""format":"x""#),
            "RT-CMP-0003",
        ),
    ];
    for (content, code) in cases {
        let fs = MemFs::with(&[("validation.json", content.as_bytes())]);
        let out = run(&os(&COMPILE), &fs);
        assert_eq!(out.code.value(), 2, "{code}: {}", out.stderr);
        assert!(out.stderr.contains(code), "{code}: {}", out.stderr);
        assert!(fs.get("compiled.json").is_none());
    }
    // Req 9.11: one Diagnostic for each missing component.
    let start = good.find(r#""ledger":"#).unwrap();
    let end = good.find(r#","diagnostics""#).unwrap();
    let without = format!("{}{}", &good[..start - 1], &good[end..]);
    let fs = MemFs::with(&[("validation.json", without.as_bytes())]);
    let out = run(&os(&COMPILE), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("RT-CMP-0002"), "{}", out.stderr);
    // A missing artifact is invalid input too.
    let fs = MemFs::default();
    let out = run(&os(&COMPILE), &fs);
    assert_eq!(out.code.value(), 2);
    assert!(out.stderr.contains("RT-CLI-0003"));
}

#[test]
fn compile_write_failure_exits_70_and_keeps_an_old_output() {
    let fs = files(&approved_ledger(&rulebook_text(AGE)));
    let _ = run(&os(&VALIDATE), &fs);
    fs.files
        .borrow_mut()
        .insert(PathBuf::from("compiled.json"), b"old".to_vec());
    fs.fail_create.set(true);
    let out = run(&os(&COMPILE), &fs);
    assert_eq!(out.code.value(), 70);
    assert_eq!(fs.get("compiled.json").unwrap(), b"old");
}

#[test]
fn compile_arguments_are_checked() {
    assert_cli_failure(&["compile"], "RT-CLI-0002", "at argument 1 (--validation)");
    assert_cli_failure(
        &["compile", "--validation", "v.json", "--out", "v.json"],
        "RT-CLI-0004",
        "at argument 4 (--out)",
    );
}

#[test]
fn evaluate_exits_0_and_writes_outcomes_traces_and_verification() {
    let fs = compiled_fs();
    fs.files.borrow_mut().insert(
        PathBuf::from("data.json"),
        DATA_COMPLETE.as_bytes().to_vec(),
    );
    let out = run(&os(&evaluate_args("data.json", &[])), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.is_empty(), "{}", out.stderr);
    let text = String::from_utf8(fs.get("results.json").unwrap()).unwrap();
    for key in [r#""outcomes":["#, r#""traces":["#, r#""verification":["#] {
        assert!(text.contains(key), "{key}");
    }
    assert!(text.contains(r#""status":"verified""#));
    // Another run gives the same bytes.
    let again = compiled_fs();
    again.files.borrow_mut().insert(
        PathBuf::from("data.json"),
        DATA_COMPLETE.as_bytes().to_vec(),
    );
    let _ = run(&os(&evaluate_args("data.json", &[])), &again);
    assert_eq!(again.get("results.json"), fs.get("results.json"));
}

#[test]
fn proof_report_and_render_never_change_the_results_file() {
    let plain = compiled_fs();
    let first = run(&os(&evaluate_args("data-complete.json", &[])), &plain);
    assert_eq!(first.code.value(), 0, "{}", first.stderr);
    let rich = compiled_fs();
    let mut extra = PROOF.to_vec();
    extra.extend(["--proof-report-json", "proof.json", "--render"]);
    let second = run(&os(&evaluate_args("data-complete.json", &extra)), &rich);
    assert_eq!(second.code.value(), 0, "{}", second.stderr);
    assert_eq!(rich.get("results.json"), plain.get("results.json"));
    let report = String::from_utf8(rich.get("proof.txt").unwrap()).unwrap();
    assert!(report.contains("A. Reviewer"), "{report}");
    assert!(rich.get("proof.json").is_some());
    let stdout = String::from_utf8(second.stdout).unwrap();
    assert!(stdout.contains("question: value of a for p1"), "{stdout}");
    assert!(stdout.contains("provenance: verified"));
    // Both reports are reproducible.
    let again = compiled_fs();
    let _ = run(&os(&evaluate_args("data-complete.json", &extra)), &again);
    assert_eq!(again.get("proof.txt"), rich.get("proof.txt"));
    assert_eq!(again.get("proof.json"), rich.get("proof.json"));
}

#[test]
fn a_missing_value_gives_exit_4_and_complete_results() {
    let fs = compiled_fs();
    let out = run(&os(&evaluate_args("data.json", &[])), &fs);
    assert_eq!(out.code.value(), 4, "{}", out.stderr);
    assert!(out.stderr.contains("RT-EXE-0001"), "{}", out.stderr);
    let text = String::from_utf8(fs.get("results.json").unwrap()).unwrap();
    assert!(text.contains("indeterminate"), "{text}");
    // The same run with a proof report writes an identical results file.
    let with_report = compiled_fs();
    let out = run(&os(&evaluate_args("data.json", &PROOF)), &with_report);
    assert_eq!(out.code.value(), 4, "{}", out.stderr);
    assert_eq!(with_report.get("results.json"), fs.get("results.json"));
    assert!(with_report.get("proof.txt").is_some());
}

#[test]
fn a_provenance_error_gives_exit_2_with_complete_results_marked_not_verified() {
    let reference = compiled_fs();
    let _ = run(&os(&evaluate_args("data-complete.json", &[])), &reference);
    let fs = compiled_fs();
    fs.files.borrow_mut().insert(
        PathBuf::from("model.json"),
        model_text(&TEXT.replace("0.6", "0.7")).into_bytes(),
    );
    let out = run(&os(&evaluate_args("data-complete.json", &[])), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("RT-PRV-0002"), "{}", out.stderr);
    let text = String::from_utf8(fs.get("results.json").unwrap()).unwrap();
    assert!(text.contains(r#""status":"not_verified""#));
    // Outcomes and traces are byte-identical to the verified run (Req 13.19).
    let before = |bytes: Vec<u8>| {
        let text = String::from_utf8(bytes).unwrap();
        text[..text.find(r#""verification":"#).unwrap()].to_owned()
    };
    // Only the diagnostics of the evaluation precede the verification section,
    // and they are equal.
    assert_eq!(
        before(fs.get("results.json").unwrap()),
        before(reference.get("results.json").unwrap())
    );
}

#[test]
fn without_source_models_the_run_succeeds_and_says_not_verified() {
    let fs = compiled_fs();
    let args: Vec<&str> = evaluate_args("data-complete.json", &[])
        .into_iter()
        .filter(|a| *a != "--source-model" && *a != "model.json")
        .collect();
    let out = run(&os(&args), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert!(out.stderr.contains("RT-PRV-0006"), "{}", out.stderr);
    assert_eq!(out.stderr.matches("warning RT-PRV-0006").count(), 1);
}

#[test]
fn invalid_input_exits_2_and_keeps_an_existing_output() {
    for (name, content) in [
        ("compiled.json", b"{".to_vec()),
        ("data.json", b"{".to_vec()),
        ("model.json", b"not json".to_vec()),
    ] {
        let fs = compiled_fs();
        fs.files
            .borrow_mut()
            .insert(PathBuf::from("results.json"), b"old".to_vec());
        fs.files.borrow_mut().insert(PathBuf::from(name), content);
        let out = run(&os(&evaluate_args("data.json", &[])), &fs);
        assert_eq!(out.code.value(), 2, "{name}: {}", out.stderr);
        assert_eq!(fs.get("results.json").unwrap(), b"old", "{name}");
    }
    let fs = compiled_fs();
    let out = run(&os(&evaluate_args("missing.json", &[])), &fs);
    assert_eq!(out.code.value(), 2);
    assert!(out.stderr.contains("RT-CLI-0003"));
    assert!(fs.get("results.json").is_none());
}

#[test]
fn a_question_the_inputs_cannot_answer_is_invalid_input() {
    let fs = compiled_fs();
    fs.files.borrow_mut().insert(
        PathBuf::from("q.json"),
        br#"{"format":"ruletrace-questions","version":1,"questions":[{"kind":"value","target":"zzz","participant":"p1"}]}"#.to_vec(),
    );
    let out = run(
        &os(&evaluate_args(
            "data-complete.json",
            &["--questions", "q.json"],
        )),
        &fs,
    );
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("RT-EXE-0010"), "{}", out.stderr);
    assert!(fs.get("results.json").is_none());
}

#[test]
fn a_differing_published_value_is_a_warning_and_the_proof_report_states_it() {
    let fs = compiled_fs();
    fs.files.borrow_mut().insert(
        PathBuf::from("p.json"),
        br#"{"format":"ruletrace-published","version":1,"entries":[{"participant":"p1","target":"a","value":"18"},{"participant":"p2","target":"a","value":"20"}]}"#.to_vec(),
    );
    let mut extra = PROOF.to_vec();
    extra.extend(["--published", "p.json"]);
    let out = run(&os(&evaluate_args("data-complete.json", &extra)), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert!(out.stderr.contains("RT-EXE-0011"), "{}", out.stderr);
    let report = String::from_utf8(fs.get("proof.txt").unwrap()).unwrap();
    assert!(report.contains("18"), "{report}");
}

#[test]
fn a_proof_report_for_another_rulebook_is_refused_and_nothing_is_written() {
    let fs = compiled_fs();
    // A Rulebook with different content, approved by its own ledger.
    let other = rulebook_text(r#"{"node":"num","t":"number","value":"1"}"#);
    fs.files.borrow_mut().insert(
        PathBuf::from("ledger.jsonl"),
        approved_ledger(&other).into_bytes(),
    );
    fs.files
        .borrow_mut()
        .insert(PathBuf::from("rb.json"), other.into_bytes());
    let before = fs.snapshot();
    let out = run(&os(&evaluate_args("data-complete.json", &PROOF)), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("RT-PRV-0008"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn evaluate_write_failure_exits_70() {
    let fs = compiled_fs();
    fs.fail_create.set(true);
    let out = run(&os(&evaluate_args("data-complete.json", &[])), &fs);
    assert_eq!(out.code.value(), 70);
    assert!(fs.get("results.json").is_none());
}

#[test]
fn evaluate_arguments_are_checked() {
    assert_cli_failure(
        &["evaluate", "--compiled", "c.json", "--out", "r.json"],
        "RT-CLI-0002",
        "at argument 5 (--data)",
    );
    assert_cli_failure(
        &[
            "evaluate",
            "--compiled",
            "c.json",
            "--data",
            "d.json",
            "--out",
            "r.json",
            "--proof-report",
            "p.txt",
        ],
        "RT-CLI-0004",
        "at argument 8 (--proof-report)",
    );
    assert_cli_failure(
        &[
            "evaluate",
            "--compiled",
            "c.json",
            "--data",
            "d.json",
            "--out",
            "r.json",
            "--ledger",
            "l.jsonl",
        ],
        "RT-CLI-0004",
        "at argument 8 (--ledger)",
    );
    assert_cli_failure(
        &[
            "evaluate",
            "--compiled",
            "c.json",
            "--data",
            "d.json",
            "--out",
            "d.json",
        ],
        "RT-CLI-0004",
        "at argument 6 (--out)",
    );
    assert_cli_failure(
        &[
            "evaluate",
            "--compiled",
            "c.json",
            "--data",
            "d.json",
            "--out",
            "r.json",
            "--render",
            "--render",
        ],
        "RT-CLI-0004",
        "at argument 8 (--render)",
    );
}

/// Every command line built from these words exits with 0, 2, 3, 4 or 70, and a
/// nonzero exit code other than 4 changes no file (Req 16.15, 17.13).
#[test]
fn exit_codes_are_closed_for_compile_and_evaluate() {
    let words = [
        "evaluate",
        "compile",
        "--compiled",
        "--data",
        "--validation",
        "--out",
        "--render",
        "compiled.json",
        "data.json",
        "validation.json",
        "results.json",
        "missing.json",
        "--zzz",
    ];
    let base = compiled_fs().snapshot();
    let base: Vec<(&str, &[u8])> = base
        .iter()
        .map(|(p, b)| (p.to_str().unwrap(), b.as_slice()))
        .collect();
    for a in words {
        for b in words {
            for c in words {
                for d in words {
                    let fs = MemFs::with(&base);
                    let before = fs.snapshot();
                    let out = run(&os(&[a, b, c, d]), &fs);
                    let value = out.code.value();
                    assert!([0, 2, 3, 4, 70].contains(&value), "{a} {b} {c} {d}");
                    if value != 0 && value != 4 {
                        assert_eq!(fs.snapshot(), before, "{a} {b} {c} {d}");
                    }
                }
            }
        }
    }
}
