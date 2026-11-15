//! Tests of `ruletrace review list`, `approve` and `reject`.

use super::validate_tests::{empty_ledger, model_text, rulebook_text, TEXT};
use super::*;
use rt_foundation::InputId;
use rt_ir::{parse_rulebook, rule_content_hash};

const TIME: &str = "2026-11-14T09:00:00Z";

fn content_hash() -> String {
    let rb = parse_rulebook(
        rulebook_text().as_bytes(),
        &InputId::new("rb.json").unwrap(),
        1 << 22,
        64,
    )
    .unwrap()
    .into_parts()
    .0;
    rule_content_hash(&rb.ir_version, &rb.revisions[0], &rb.documents)
        .unwrap()
        .to_string()
}

fn files() -> MemFs {
    MemFs::with(&[
        ("rb.json", rulebook_text().as_bytes()),
        ("ledger.jsonl", empty_ledger().as_bytes()),
        ("model.json", model_text(TEXT).as_bytes()),
    ])
}

fn decide<'a>(verb: &'a str, hash: &'a str, reviewer: &'a str) -> Vec<&'a str> {
    let mut a = vec![
        "review",
        verb,
        "--rulebook",
        "rb.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "model.json",
        "--rule",
        "d.a",
        "--revision",
        "1",
        "--content-hash",
        hash,
        "--reviewer",
        reviewer,
        "--time",
        TIME,
    ];
    if verb == "reject" {
        a.extend(["--reason", "wrong threshold"]);
    }
    a
}

fn validate(fs: &MemFs) -> Outcome {
    run(
        &os(&[
            "validate",
            "--rulebook",
            "rb.json",
            "--ledger",
            "ledger.jsonl",
            "--source-model",
            "model.json",
            "--out",
            "validation.json",
        ]),
        fs,
    )
}

#[test]
fn approve_appends_one_record_and_validate_then_passes() {
    let fs = files();
    assert_eq!(validate(&fs).code.value(), 3);
    let before = fs.snapshot();
    let hash = content_hash();
    let out = run(&os(&decide("approve", &hash, "A. Reviewer")), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert!(out.stderr.is_empty());
    assert!(String::from_utf8(out.stdout)
        .unwrap()
        .starts_with("approved rule 'd.a' revision 1"));
    let old = String::from_utf8(before[&PathBuf::from("ledger.jsonl")].clone()).unwrap();
    let new = String::from_utf8(fs.get("ledger.jsonl").unwrap()).unwrap();
    assert!(new.starts_with(&old));
    assert_eq!(new[old.len()..].matches('\n').count(), 1);
    assert!(new.contains(&format!(r#""time":"{TIME}""#)));
    assert_eq!(
        fs.get("rb.json"),
        before.get(&PathBuf::from("rb.json")).cloned()
    );
    // No lock or temporary file remains, and validation now passes.
    assert_eq!(fs.snapshot().len(), before.len());
    assert_eq!(validate(&fs).code.value(), 0);
}

#[test]
fn the_same_request_gives_the_same_bytes() {
    let (a, b) = (files(), files());
    let hash = content_hash();
    let args = os(&decide("approve", &hash, "A. Reviewer"));
    assert_eq!(run(&args, &a), run(&args, &b));
    assert_eq!(a.snapshot(), b.snapshot());
}

#[test]
fn wrong_hash_exits_3_and_changes_nothing() {
    let fs = files();
    let before = fs.snapshot();
    let wrong = "0".repeat(64);
    let out = run(&os(&decide("approve", &wrong, "A. Reviewer")), &fs);
    assert_eq!(out.code.value(), 3);
    assert_eq!(out.stderr.matches("RT-APR-").count(), 1, "{}", out.stderr);
    assert!(out.stderr.contains("RT-APR-0203"), "{}", out.stderr);
    assert!(out.stdout.is_empty());
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn reviewer_refusals_exit_3_and_change_nothing() {
    let fs = files();
    let before = fs.snapshot();
    let hash = content_hash();
    let out = run(&os(&decide("approve", &hash, "")), &fs);
    assert_eq!(out.code.value(), 3);
    assert!(out.stderr.contains("RT-APR-0202"), "{}", out.stderr);
    let long = "x".repeat(201);
    let out = run(&os(&decide("approve", &hash, &long)), &fs);
    assert!(out.stderr.contains("RT-APR-0202"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn missing_reviewer_or_time_is_exit_2_and_touches_nothing() {
    let fs = files();
    let before = fs.snapshot();
    let hash = content_hash();
    for name in ["--reviewer", "--time"] {
        let full = decide("approve", &hash, "A. Reviewer");
        let at = full.iter().position(|a| *a == name).unwrap();
        let mut args = full.clone();
        args.drain(at..=at + 1);
        let out = run(&os(&args), &fs);
        assert_eq!(out.code.value(), 2, "{name}");
        assert!(out.stderr.contains("RT-CLI-0002"), "{}", out.stderr);
    }
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn reject_records_the_reason_and_a_second_decision_is_refused() {
    let fs = files();
    let hash = content_hash();
    let out = run(&os(&decide("reject", &hash, "A. Reviewer")), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    let ledger = String::from_utf8(fs.get("ledger.jsonl").unwrap()).unwrap();
    assert!(ledger.contains(r#""kind":"rejection""#) && ledger.contains("wrong threshold"));
    let before = fs.snapshot();
    let out = run(&os(&decide("approve", &hash, "A. Reviewer")), &fs);
    assert_eq!(out.code.value(), 3);
    assert!(out.stderr.contains("RT-APR-0204"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn reject_needs_a_valid_reason() {
    let fs = files();
    let before = fs.snapshot();
    let hash = content_hash();
    let mut args = decide("reject", &hash, "A. Reviewer");
    let at = args.iter().position(|a| *a == "wrong threshold").unwrap();
    args[at] = "";
    let out = run(&os(&args), &fs);
    assert_eq!(out.code.value(), 2);
    assert!(out.stderr.contains("RT-APR-0207"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn unknown_revision_and_missing_source_model_are_exit_2() {
    let fs = files();
    let before = fs.snapshot();
    let hash = content_hash();
    let mut args = decide("approve", &hash, "A. Reviewer");
    let at = args.iter().position(|a| *a == "--revision").unwrap();
    args[at + 1] = "7";
    let out = run(&os(&args), &fs);
    assert_eq!(out.code.value(), 2);
    assert!(out.stderr.contains("RT-APR-0206"), "{}", out.stderr);
    let mut args = decide("approve", &hash, "A. Reviewer");
    args.drain(6..8);
    let out = run(&os(&args), &fs);
    assert_eq!(out.code.value(), 2);
    assert!(out.stderr.contains("RT-APR-0209"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn a_missing_ledger_needs_create_ledger() {
    let fs = MemFs::with(&[
        ("rb.json", rulebook_text().as_bytes()),
        ("model.json", model_text(TEXT).as_bytes()),
    ]);
    let hash = content_hash();
    let out = run(&os(&decide("approve", &hash, "A. Reviewer")), &fs);
    assert_eq!(out.code.value(), 2);
    assert!(fs.get("ledger.jsonl").is_none());
    let mut args = decide("approve", &hash, "A. Reviewer");
    args.push("--create-ledger");
    let out = run(&os(&args), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert_eq!(validate(&fs).code.value(), 0);
}

#[test]
fn a_held_lock_is_exit_70_and_changes_nothing() {
    let fs = files();
    fs.lock_held.set(true);
    let before = fs.snapshot();
    let hash = content_hash();
    let out = run(&os(&decide("approve", &hash, "A. Reviewer")), &fs);
    assert_eq!(out.code.value(), 70);
    assert!(out.stderr.contains("RT-APR-0212"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn a_failed_rename_is_exit_70_and_the_ledger_is_unchanged() {
    let fs = files();
    let before = fs.snapshot();
    fs.fail_rename.set(Some(1));
    let hash = content_hash();
    let out = run(&os(&decide("approve", &hash, "A. Reviewer")), &fs);
    assert_eq!(out.code.value(), 70);
    assert!(out.stderr.contains("RT-APR-0208"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn list_shows_hash_and_source_text_and_writes_nothing() {
    let fs = files();
    let before = fs.snapshot();
    let args = [
        "review",
        "list",
        "--rulebook",
        "rb.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "model.json",
    ];
    let out = run(&os(&args), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("rule 'd.a' revision 1 status Proposed"),
        "{text}"
    );
    assert!(
        text.contains(&format!("content hash: {}", content_hash())),
        "{text}"
    );
    assert!(
        text.contains("A competitor is eligible if age is at least 16"),
        "{text}"
    );
    assert_eq!(fs.snapshot(), before);
    // Without a Source_Model the text is not invented.
    let out = run(&os(&args[..6]), &fs);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("not verified RT-PRV-"), "{text}");
    assert!(!text.contains("A competitor"), "{text}");
    // After the approval nothing awaits a decision.
    let hash = content_hash();
    let _ = run(&os(&decide("approve", &hash, "A. Reviewer")), &fs);
    let out = run(&os(&args), &fs);
    assert!(String::from_utf8(out.stdout)
        .unwrap()
        .contains("no revision awaits"));
}

#[test]
fn list_of_an_unknown_rule_is_exit_2() {
    let fs = files();
    let out = run(
        &os(&[
            "review",
            "list",
            "--rulebook",
            "rb.json",
            "--ledger",
            "ledger.jsonl",
            "--rule",
            "nope",
        ]),
        &fs,
    );
    assert_eq!(out.code.value(), 2);
    assert!(out.stderr.contains("RT-APR-0206"), "{}", out.stderr);
}

/// The first rendered `at ...` location line of a refusal.
fn location_line(stderr: &str) -> &str {
    stderr
        .lines()
        .find(|l| l.trim_start().starts_with("at "))
        .unwrap_or("")
        .trim()
}

fn index_of(args: &[&str], option: &str) -> usize {
    args.iter().position(|a| *a == option).unwrap()
}

fn assert_refused_at(fs: &MemFs, args: &[&str], option: &str) {
    let out = run(&os(args), fs);
    assert_ne!(out.code.value(), 0, "{option}: {}", out.stderr);
    let line = location_line(&out.stderr);
    assert!(
        !line.contains(&format!("at argument 0 ({option})")),
        "{option}: fabricated position in {line:?}"
    );
    // Value diagnostics point at the value, the argument after the option.
    let real = index_of(args, option) + 1;
    assert_eq!(line, format!("at argument {real} ({option})"));
}

#[test]
fn a_refusal_that_names_an_option_carries_its_real_argv_index() {
    let hash = content_hash();
    let wrong = "0".repeat(64);

    let args = decide("approve", &hash, "");
    assert_refused_at(&files(), &args, "--reviewer");

    let mut args = decide("approve", &hash, "A. Reviewer");
    let at = index_of(&args, "--time");
    args[at + 1] = "yesterday";
    assert_refused_at(&files(), &args, "--time");

    let args = decide("approve", &wrong, "A. Reviewer");
    assert_refused_at(&files(), &args, "--content-hash");

    let mut args = decide("reject", &hash, "A. Reviewer");
    let at = index_of(&args, "--reason");
    args[at + 1] = "";
    assert_refused_at(&files(), &args, "--reason");

    let mut args = decide("approve", &hash, "A. Reviewer");
    let at = index_of(&args, "--rule");
    args[at + 1] = "nope.x";
    assert_refused_at(&files(), &args, "--rule");
}

#[test]
fn a_second_decision_names_the_real_index_of_revision() {
    let fs = files();
    let hash = content_hash();
    let args = decide("approve", &hash, "A. Reviewer");
    assert_eq!(run(&os(&args), &fs).code.value(), 0);
    assert_refused_at(&fs, &args, "--revision");
}

#[test]
fn a_refused_list_rule_names_its_real_argv_index() {
    let list = [
        "review",
        "list",
        "--rulebook",
        "rb.json",
        "--ledger",
        "ledger.jsonl",
        "--rule",
        "nope",
    ];
    assert_refused_at(&files(), &list, "--rule");
}
