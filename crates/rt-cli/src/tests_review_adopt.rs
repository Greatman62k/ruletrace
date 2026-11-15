//! Tests of `review list` over a Proposal_Store and of the adoption through
//! `review approve` and `review reject` (design §3.9.1; Req 5.8 to 5.12, 17.11,
//! 23.12, 23.16, 26.28).
//!
//! The fixture is that of `tests_commit`: the Rulebook holds `d.a` revision 1
//! (Proposed) and the store holds one pending suggestion of rule `d.b`, whose
//! cited clause is the first sentence of the source text.

use super::commit_tests::{adoption_fs, approve, decide_args, fixture, Fixture};
use super::fault_tests::FaultFs;
use super::validate_tests::{empty_ledger, model_text, rulebook_text, TEXT, ZERO};
use super::*;
use rt_foundation::{Code, DocumentId, InputId, RevisionNumber, RuleId};
use rt_hash::sha256;
use rt_ir::{draft_hash, parse_draft, parse_rulebook, Identity};
use rt_proposals::{parse_batch, print_batch, StoredDiagnostic, StoredLocation};
use rt_proposals::{
    proposal_id, Batch, Completion, Evidence, Proposal, ProposalKind, RuleDraft, StoredResponse,
};

const CLAUSE1: &str = "1. A competitor is eligible if age is at least 16 and holds a licence.";

fn list_args(extra: &[&str]) -> Vec<OsString> {
    let mut a = vec![
        "review",
        "list",
        "--rulebook",
        "rb.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "model.json",
    ];
    a.extend_from_slice(extra);
    os(&a)
}

fn list_text(fs: &FaultFs, extra: &[&str]) -> String {
    let out = run(&list_args(extra), fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert_eq!(out.stderr, "");
    String::from_utf8(out.stdout).unwrap()
}

fn pending_block(text: &str) -> &str {
    let at = text.find("pending proposal").expect("a pending proposal");
    &text[at..]
}

#[test]
fn list_shows_the_pending_proposal_with_revision_hash_origin_and_source_text() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let before = fs.snapshot();
    let text = list_text(&fs, &["--proposals", "store"]);
    let block = pending_block(&text);
    assert!(
        block.starts_with(&format!(
            "pending proposal '{}' for rule 'd.b' prospective revision 1\n",
            fx.proposal_id
        )),
        "{text}"
    );
    assert!(
        block.contains(&format!(
            "  origin: AI proposal '{}' (proposer 'ai-1', provider 'mock', model 'm1')\n",
            fx.proposal_id
        )),
        "{text}"
    );
    assert!(
        block.contains(&format!("  prospective content hash: {}\n", fx.hash)),
        "{text}"
    );
    let start = super::validate_tests::TEXT.find(CLAUSE1).unwrap();
    assert!(
        block.contains(&format!(
            "bytes {start}..{} (line 3, column 1): text \"{CLAUSE1}\"",
            start + CLAUSE1.len()
        )),
        "{text}"
    );
    // The Proposed revision of the Rulebook is still listed, before it.
    assert!(
        text.starts_with("rule 'd.a' revision 1 status Proposed"),
        "{text}"
    );
    assert_eq!(fs.snapshot(), before, "list writes nothing");
}

#[test]
fn without_the_proposals_option_no_pending_proposal_is_shown() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let text = list_text(&fs, &[]);
    assert!(!text.contains("pending proposal"), "{text}");
}

#[test]
fn without_a_source_model_the_pending_text_is_not_invented() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let out = run(
        &os(&[
            "review",
            "list",
            "--rulebook",
            "rb.json",
            "--ledger",
            "ledger.jsonl",
            "--proposals",
            "store",
        ]),
        &fs,
    );
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        pending_block(&text).contains("not verified RT-PRV-"),
        "{text}"
    );
    assert!(!text.contains("A competitor"), "{text}");
}

#[test]
fn list_shows_the_stored_diagnostics_of_the_proposal() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let mut batch = parse_batch(&InputId::new("b").unwrap(), &fx.batch).unwrap();
    batch.proposals[0].outcome.push(StoredDiagnostic {
        code: Code::AiStoreInvalid,
        message: "example finding".to_owned(),
        location: StoredLocation::Field("rule.spans[0]".to_owned()),
    });
    let mut files = fs.snapshot();
    files.insert(
        PathBuf::from("store/batches/req.json"),
        print_batch(&batch).into_bytes(),
    );
    fs.replace_all(files);
    let text = list_text(&fs, &["--proposals", "store"]);
    assert!(
        pending_block(&text).contains("  diagnostic: error RT-AI-0302: example finding\n"),
        "{text}"
    );
}

#[test]
fn rule_and_revision_selections_filter_the_pending_proposals() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let text = list_text(&fs, &["--proposals", "store", "--rule", "d.b"]);
    assert!(text.starts_with("pending proposal"), "{text}");
    assert!(!text.contains("rule 'd.a'"), "{text}");
    let text = list_text(
        &fs,
        &["--proposals", "store", "--rule", "d.b", "--revision", "1"],
    );
    assert!(text.contains("prospective revision 1"), "{text}");
    // The next revision of d.b is 1, so a selection of 2 finds nothing.
    let out = run(
        &list_args(&["--proposals", "store", "--rule", "d.b", "--revision", "2"]),
        &fs,
    );
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("RT-APR-0206"), "{}", out.stderr);
    // A rule that is neither in the Rulebook nor pending is refused as before.
    let out = run(&list_args(&["--proposals", "store", "--rule", "x.y"]), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("RT-APR-0206"), "{}", out.stderr);
    // The pending rule is unknown without the store.
    let out = run(&list_args(&["--rule", "d.b"]), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
}

#[test]
fn a_malformed_store_makes_list_exit_2_with_rt_apr_0213() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let mut files = fs.snapshot();
    files.insert(PathBuf::from("store/batches/req.json"), b"{".to_vec());
    fs.replace_all(files);
    let before = fs.snapshot();
    let out = run(&list_args(&["--proposals", "store"]), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("RT-APR-0213"), "{}", out.stderr);
    assert!(out.stdout.is_empty());
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn approving_the_listed_hash_adopts_and_the_rule_is_then_approved() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let text = list_text(&fs, &["--proposals", "store"]);
    // The Reviewer names the hash exactly as printed.
    let printed = pending_block(&text)
        .lines()
        .find_map(|l| l.strip_prefix("  prospective content hash: "))
        .unwrap()
        .to_owned();
    let out = run(&decide_args("approve", "d.b", &printed, &[]), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("approved rule 'd.b' revision 1"),
        "approval line"
    );
    // The proposal is no longer pending, and d.b is listed as Approved.
    let after = list_text(&fs, &["--proposals", "store", "--rule", "d.b"]);
    assert!(!after.contains("pending proposal"), "{after}");
    assert!(
        after.starts_with("rule 'd.b' revision 1 status Approved"),
        "{after}"
    );
    assert!(
        after.contains(&format!("content hash: {printed}\n")),
        "{after}"
    );
}

#[test]
fn rejecting_the_listed_proposal_lists_it_as_rejected_and_not_pending() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let out = run(&decide_args("reject", "d.b", &fx.hash, &[]), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    let after = list_text(&fs, &["--proposals", "store", "--rule", "d.b"]);
    assert!(!after.contains("pending proposal"), "{after}");
    assert!(
        after.starts_with("rule 'd.b' revision 1 status Rejected"),
        "{after}"
    );
}

#[test]
fn self_approval_by_an_ai_identity_is_refused_with_exit_3_and_changes_nothing() {
    let fx = fixture();
    for reviewer in ["ai-1", "mock", "m1"] {
        let fs = adoption_fs(&fx, true);
        let before = fs.snapshot();
        let mut args = approve(&fx);
        let at = args.iter().position(|a| a == "A. Reviewer").unwrap();
        args[at] = reviewer.into();
        let out = run(&args, &fs);
        assert_eq!(out.code.value(), 3, "{reviewer}: {}", out.stderr);
        assert!(out.stderr.contains("RT-APR-0201"), "{reviewer}");
        assert_eq!(out.stderr.matches("RT-APR-").count(), 1, "{}", out.stderr);
        assert_eq!(fs.snapshot(), before, "{reviewer}");
    }
}

#[test]
fn a_pending_rule_that_does_not_exist_is_refused_with_rt_apr_0206() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let before = fs.snapshot();
    let out = run(&decide_args("approve", "x.y", ZERO, &[]), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("RT-APR-0206"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn adopting_everything_makes_validate_pass() {
    let fx: Fixture = fixture();
    let fs = adoption_fs(&fx, true);
    let out = run(&approve(&fx), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    // d.a is approved the usual way, with the hash that `list` prints.
    let text = list_text(&fs, &["--proposals", "store"]);
    let hash = text
        .lines()
        .find_map(|l| l.strip_prefix("  content hash: "))
        .unwrap()
        .to_owned();
    let args = decide_args("approve", "d.a", &hash, &[]);
    let out = run(&args, &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    let out = run(
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
        &fs,
    );
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
}

fn draft_json(rule: &str, target: &str) -> String {
    let start = TEXT.find(CLAUSE1).unwrap();
    let end = start + CLAUSE1.len();
    format!(
        r#"{{"id":"{rule}","kind":"derived","target":"{target}","type":"number","expr":{{"node":"num","t":"number","value":"2"}},"markers":[],"spans":[{{"document":"rules.md","start":{start},"end":{end},"line":3,"column":1}}]}}"#
    )
}

/// A store batch with one suggestion of `rule`, whose stored Draft hash is the
/// one `draft_hash` computes for the draft.
fn batch_of(rule: &str, target: &str) -> Batch {
    let rb = parse_rulebook(
        rulebook_text().as_bytes(),
        &InputId::new("rb.json").unwrap(),
        1 << 22,
        64,
    )
    .unwrap()
    .into_parts()
    .0;
    let json = draft_json(rule, target);
    let content = parse_draft(
        json.as_bytes(),
        &InputId::new("d").unwrap(),
        RevisionNumber::new(1).unwrap(),
        rb.revisions[0].origin.clone(),
        1 << 20,
        64,
    )
    .unwrap()
    .into_parts()
    .0
    .content;
    let id = RuleId::new(rule).unwrap();
    let hash = draft_hash(&rb.ir_version, &id, &content, &rb.documents).unwrap();
    let start = TEXT.find(CLAUSE1).unwrap();
    let end = start + CLAUSE1.len();
    let request_id = sha256(b"request-2");
    Batch {
        request_id: request_id.clone(),
        response_sha256: sha256(b"payload"),
        response: StoredResponse {
            completion: Completion::Complete,
            payload: "payload".to_owned(),
        },
        provider: "mock".to_owned(),
        model: "m1".to_owned(),
        adapter_version: "0.1".to_owned(),
        ai_identities: vec![Identity::new("ai-1").unwrap()],
        created_at: "2026-11-14T08:00:00Z".to_owned(),
        proposals: vec![Proposal {
            proposal_id: proposal_id(&request_id, 0),
            kind: ProposalKind::RuleSuggestion,
            proposer: Identity::new("ai-1").unwrap(),
            rationale: "r".to_owned(),
            evidence: vec![Evidence {
                document: DocumentId::new("rules.md").unwrap(),
                sha256: sha256(TEXT.as_bytes()),
                start: u32::try_from(start).unwrap(),
                end: u32::try_from(end).unwrap(),
                line: 3,
                column: 1,
                quote: CLAUSE1.to_owned(),
            }],
            rule: Some((RuleDraft { id, json }, hash)),
            outcome: Vec::new(),
        }],
    }
}

fn fs_with(rulebook: &str, batch: &Batch) -> FaultFs {
    let model = model_text(TEXT);
    let ledger = empty_ledger();
    FaultFs::with(&[
        ("rb.json", rulebook.as_bytes()),
        ("model.json", model.as_bytes()),
        ("ledger.jsonl", ledger.as_bytes()),
        ("store/batches/req.json", print_batch(batch).as_bytes()),
    ])
}

fn unadoptable_block(batch: &Batch, rulebook: &str, code: &str) {
    let fs = fs_with(rulebook, batch);
    let id = batch.proposals[0].proposal_id.as_str();
    let rule = &batch.proposals[0].rule.as_ref().unwrap().0.id;
    let text = list_text(&fs, &["--proposals", "store", "--rule", rule.as_str()]);
    let block = pending_block(&text);
    assert!(
        block.starts_with(&format!(
            "pending proposal '{id}' for rule '{rule}' cannot be adopted\n  diagnostic: "
        )),
        "{text}"
    );
    let first = block.lines().nth(1).unwrap();
    assert!(first.contains(code), "{text}");
    assert!(!block.contains("prospective revision"), "{text}");
    // A revision selection can never match a proposal that has none.
    for n in ["1", "2", "4294967295"] {
        let out = run(
            &list_args(&[
                "--proposals",
                "store",
                "--rule",
                rule.as_str(),
                "--revision",
                n,
            ]),
            &fs,
        );
        assert!(
            !out.stdout.windows(14).any(|w| w == b"cannot be adop"),
            "revision {n}: {}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}

#[test]
fn a_stored_draft_hash_mismatch_is_listed_as_unadoptable_and_not_selected_by_revision() {
    let mut batch = batch_of("d.b", "b");
    batch.proposals[0].rule.as_mut().unwrap().1 = sha256(b"some other draft");
    unadoptable_block(&batch, &rulebook_text(), "RT-APR-0213");
}

#[test]
fn a_proposal_for_an_existing_rule_lists_and_adopts_revision_two() {
    let batch = batch_of("d.a", "a");
    let fs = fs_with(&rulebook_text(), &batch);
    let text = list_text(&fs, &["--proposals", "store", "--rule", "d.a"]);
    let block = pending_block(&text);
    assert!(
        block.starts_with(&format!(
            "pending proposal '{}' for rule 'd.a' prospective revision 2\n",
            batch.proposals[0].proposal_id
        )),
        "{text}"
    );
    assert!(text.starts_with("rule 'd.a' revision 1 status Proposed"));
    let printed = block
        .lines()
        .find_map(|l| l.strip_prefix("  prospective content hash: "))
        .unwrap()
        .to_owned();
    // Revision 1 is selected by its own number; 2 selects the proposal.
    let one = list_text(
        &fs,
        &["--proposals", "store", "--rule", "d.a", "--revision", "1"],
    );
    assert!(!one.contains("pending proposal"), "{one}");
    let two = list_text(
        &fs,
        &["--proposals", "store", "--rule", "d.a", "--revision", "2"],
    );
    assert!(two.starts_with("pending proposal"), "{two}");

    let mut args = decide_args("approve", "d.a", &printed, &[]);
    let at = args.iter().position(|a| a == "--revision").unwrap();
    args[at + 1] = "2".into();
    let out = run(&args, &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("approved rule 'd.a' revision 2"),
        "approval line"
    );
    let after = list_text(&fs, &["--proposals", "store", "--rule", "d.a"]);
    assert!(!after.contains("pending proposal"), "{after}");
    assert!(
        after.contains("rule 'd.a' revision 1 status Proposed"),
        "{after}"
    );
    let rev2 = &after[after.find("rule 'd.a' revision 2").unwrap()..];
    assert!(
        rev2.starts_with("rule 'd.a' revision 2 status Approved"),
        "{after}"
    );
    assert!(
        rev2.contains(&format!("content hash: {printed}\n")),
        "{after}"
    );
}

#[test]
fn list_without_rule_places_an_unadoptable_draft_at_the_store_directory() {
    let mut batch = batch_of("d.b", "b");
    batch.proposals[0].rule.as_mut().unwrap().1 = sha256(b"some other draft");
    let fs = fs_with(&rulebook_text(), &batch);
    let out = run(&list_args(&["--proposals", "store"]), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = pending_block(&text).lines().collect();
    assert!(
        lines[1].starts_with("  diagnostic: error RT-APR-0213: "),
        "{text}"
    );
    assert_eq!(lines[2], "  diagnostic:   at input 'store'", "{text}");
}
