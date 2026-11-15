//! A revision proposal from `ruletrace-ai propose --revise-*` is adopted by
//! `ruletrace review` as the next revision of the same rule (Req 23.16,
//! 23.9, 25.4, 27.4).
//!
//! The Rulebook holds `elig.basic` revision 1 with Origin Manual. The
//! provider's answer names the same rule with another threshold. Adoption
//! must give it revision 2 with Origin AI_Proposal, and the Approval_Ledger
//! must derive Approved for it only after the human approval.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod support;

use rt_foundation::InputId;
use rt_ir::{parse_rulebook, ApprovalStatus, Origin};
use rt_json::ParseLimits;
use rt_ledger::{derive_statuses, parse_ledger};
use support::{fixture, Work, CREATED_AT, REVIEWER, REVIEW_TIME};

const RB: &str = "rulebook.json";
const LEDGER: &str = "ledger.jsonl";
const DOC_HASH: &str = "eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a";
const FEEDBACK: &str = "Raise the age limit to 18.";

fn payload() -> String {
    format!(
        r#"{{"schema_version":"1","proposals":[{{"kind":"rule_suggestion","proposer":"proposer-a","rationale":"Reviewer asked for 18.","evidence":[{{"document":"rules.md","sha256":"{DOC_HASH}","start":21,"end":91,"line":3,"column":1,"quote":"1. A competitor is eligible if age is at least 16 and holds a licence."}}],"rule":{{"id":"elig.basic","kind":"eligibility","target":"eligible","type":"boolean","expr":{{"node":"and","t":"boolean","args":[{{"node":"cmp","t":"boolean","op":"ge","left":{{"node":"field","t":"number","name":"age"}},"right":{{"node":"num","t":"number","value":"18"}}}},{{"node":"field","t":"boolean","name":"licence"}}]}},"markers":[],"spans":[{{"document":"rules.md","start":21,"end":91,"line":3,"column":1}}]}}}}]}}"#
    )
}

fn propose_args(work: &Work, extra: &[&str]) -> Vec<String> {
    let mut args = vec!["propose".to_owned()];
    for (name, rel) in [
        ("--config", "ai.toml"),
        ("--rulebook", RB),
        ("--source-model", "rules.model.json"),
        ("--candidates", "candidates.json"),
        ("--store", "store"),
    ] {
        args.push(name.to_owned());
        args.push(work.arg(rel));
    }
    args.extend(["--created-at".to_owned(), CREATED_AT.to_owned()]);
    args.extend([
        "--revise-rule".to_owned(),
        "elig.basic".to_owned(),
        "--revise-revision".to_owned(),
        "1".to_owned(),
        "--feedback-file".to_owned(),
        work.arg("feedback.txt"),
    ]);
    args.extend(extra.iter().map(|s| (*s).to_owned()));
    args
}

fn ai(work: &Work, extra: &[&str]) -> support::Ran {
    let args = propose_args(work, extra);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    work.ruletrace_ai(&refs)
}

fn setup(name: &str) -> Work {
    let work = Work::new(name);
    let dir = fixture("");
    work.copy_from(&dir, "source/rules.md", "source/rules.md");
    work.copy_from(&dir, "candidates.json", "candidates.json");
    work.copy_from(&dir, "rulebook.authored.json", RB);
    work.copy_from(&dir, "ledger.authored.jsonl", LEDGER);
    work.write("feedback.txt", FEEDBACK.as_bytes());
    assert_eq!(work.ingest().code, 0);
    work.write_config(&["openai-responses", "model-1"]);
    work
}

#[test]
fn a_revision_proposal_is_adopted_as_the_next_revision_with_origin_ai_proposal() {
    let work = setup("revise-adopt");
    let dry = ai(&work, &["--dry-run"]);
    assert_eq!(dry.code, 0, "{}", dry.stderr);
    let id = dry.stdout.lines().next().unwrap().to_owned();
    work.write_script(&id, &payload());
    let rulebook_before = work.read(RB);
    let ledger_before = work.read(LEDGER);

    let ran = ai(&work, &["--consent", "rules.md"]);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert!(
        ran.stdout.contains("run summary: complete"),
        "{}",
        ran.stdout
    );
    assert_eq!(work.read(RB), rulebook_before);
    assert_eq!(work.read(LEDGER), ledger_before);
    let log = String::from_utf8(work.read("work/transmission.log.jsonl")).unwrap();
    assert!(log.contains(&id), "{log}");
    assert!(!log.contains(FEEDBACK), "{log}");

    let listed = work.review_list(RB, LEDGER);
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    assert!(
        listed
            .stdout
            .contains("for rule 'elig.basic' prospective revision 2"),
        "{}",
        listed.stdout
    );
    let hash = listed
        .stdout
        .lines()
        .find_map(|l| l.trim_start().strip_prefix("prospective content hash: "))
        .unwrap()
        .to_owned();

    let approve = work.ruletrace(&[
        "review",
        "approve",
        "--rulebook",
        &work.arg(RB),
        "--ledger",
        &work.arg(LEDGER),
        "--source-model",
        &work.arg("rules.model.json"),
        "--proposals",
        &work.arg("store"),
        "--rule",
        "elig.basic",
        "--revision",
        "2",
        "--content-hash",
        &hash,
        "--reviewer",
        REVIEWER,
        "--time",
        REVIEW_TIME,
    ]);
    assert_eq!(approve.code, 0, "{}", approve.stderr);

    let (rulebook, _) = parse_rulebook(&work.read(RB), &InputId::new(RB).unwrap(), 1 << 20, 64)
        .map_err(|r| r.into_diagnostics().as_slice().to_vec())
        .unwrap()
        .into_parts();
    let of_rule: Vec<_> = rulebook
        .revisions
        .iter()
        .filter(|r| r.key.id.as_str() == "elig.basic")
        .collect();
    assert_eq!(of_rule.len(), 2);
    assert_eq!(of_rule[0].key.revision.get(), 1);
    assert!(matches!(of_rule[0].origin, Origin::Manual { .. }));
    assert_eq!(of_rule[1].key.revision.get(), 2);
    match &of_rule[1].origin {
        Origin::AiProposal(origin) => {
            assert_eq!(origin.request_hash.as_str(), id);
            assert_eq!(origin.proposer.as_str(), "proposer-a");
        }
        other => panic!("revision 2 has origin {other:?}"),
    }
    let ledger = parse_ledger(
        &InputId::new(LEDGER).unwrap(),
        &work.read(LEDGER),
        ParseLimits::new(1 << 20, 64),
    )
    .unwrap();
    let statuses = derive_statuses(&rulebook, &ledger);
    assert!(statuses.values().all(|s| *s == ApprovalStatus::Approved));
    assert_eq!(statuses.len(), 4);
}

#[test]
fn the_revision_request_differs_from_the_plain_request_of_the_same_clauses() {
    let work = setup("revise-differs");
    let with = ai(&work, &["--dry-run"]);
    let plain_args = [
        "propose",
        "--config",
        &work.arg("ai.toml"),
        "--rulebook",
        &work.arg(RB),
        "--source-model",
        &work.arg("rules.model.json"),
        "--candidates",
        &work.arg("candidates.json"),
        "--store",
        &work.arg("store"),
        "--created-at",
        CREATED_AT,
        "--dry-run",
    ];
    let plain = work.ruletrace_ai(&plain_args);
    assert_eq!(with.code, 0, "{}", with.stderr);
    assert_eq!(plain.code, 0, "{}", plain.stderr);
    assert_ne!(
        with.stdout.lines().next().unwrap(),
        plain.stdout.lines().next().unwrap()
    );
}
