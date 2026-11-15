//! The `slice-ai` fixture end to end (Req 16.18, 18.6, 18.14, 18.16, 23.14,
//! 26.17, 20.20; design 7.3).
//!
//! The Mock_Provider answers with an authored response. A Reviewer adopts
//! and approves each proposal, then the AI-path Rulebook is validated,
//! compiled and evaluated. Its compiled artifact and results must equal the
//! ones of a hand-authored Rulebook with the same Rule_Content.
//!
//! The expected files under `tests/fixtures/slice-ai/expected` are the
//! reviewed goldens of `slice-basic`, whose results were checked by hand:
//! p2 and p4 tie on 0.6 * 7 + 0.4 * 9 = 7.8 and 0.6 * 9 + 0.4 * 6 = 7.8,
//! and p4 wins on the judges score 9 against 7.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod support;

use rt_foundation::InputId;
use rt_ir::{parse_rulebook, ApprovalStatus};
use rt_json::ParseLimits;
use rt_ledger::{derive_statuses, effective_revisions, parse_ledger, unresolved};
use support::{
    authored_approvals, fixture, stored_with_blocking_findings_codes, validation_diagnostics,
    Decision, Ran, Work, REVIEWER,
};

const RB: &str = "rulebook.json";
const LEDGER: &str = "ledger.jsonl";

fn assert_ok(ran: &Ran, what: &str) {
    assert_eq!(ran.code, 0, "{what}: {}", ran.stderr);
}

// Shared inputs of both paths: the source, its model, the data and the
// golden files.
fn load_common(work: &Work) {
    let dir = fixture("");
    work.copy_from(&dir, "source/rules.md", "source/rules.md");
    work.copy_from(&dir, "candidates.json", "candidates.json");
    work.copy_from(&dir, "data.json", "data.json");
    assert_ok(&work.ingest(), "ingest");
    assert_eq!(
        work.read("rules.model.json"),
        std::fs::read(fixture("expected/source-model.json")).unwrap()
    );
}

// The AI path up to a Rulebook in which every proposal is adopted and
// approved. Returns the work directory.
fn ai_path(name: &str) -> Work {
    let work = Work::new(name);
    let dir = fixture("");
    load_common(&work);
    work.copy_from(&dir, "rulebook.declarations.json", RB);
    work.copy_from(&dir, "ledger.empty.jsonl", LEDGER);
    work.write_config(&["openai-responses", "model-1"]);

    let id = work.request_id(RB);
    assert_eq!(id.len(), 64);
    let payload = std::fs::read_to_string(fixture("provider/response.json")).unwrap();
    work.write_script(&id, &payload);

    let before = work.read(RB);
    let proposed = work.propose(RB);
    assert_ok(&proposed, "propose");
    // The third proposal ranks by `score`, which only the second proposal
    // declares, so its stand-alone check finds RT-VAL-0002 and the batch
    // is stored with the blocking-findings Warning.
    assert_eq!(
        proposed.codes(),
        stored_with_blocking_findings_codes(),
        "{}",
        proposed.stderr
    );
    assert!(
        proposed.stdout.contains("run summary: complete"),
        "{}",
        proposed.stdout
    );
    assert!(proposed.stdout.contains("unprocessed:\n  (none)"));
    let batch = format!("store/batches/{id}.json");
    assert!(work.exists(&batch), "{:?}", work.state().keys());
    assert_eq!(work.read(RB), before, "propose never writes the Rulebook");
    let log = String::from_utf8(work.read("work/transmission.log.jsonl")).unwrap();
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(log.contains(&id), "{log}");
    assert!(!log.contains("competitor"), "document text in the log");
    work
}

#[test]
fn proposals_are_listed_with_the_hashes_of_the_authored_ledger() {
    let work = ai_path("slice-ai-list");
    let listed = work.review_list(RB, LEDGER);
    assert_ok(&listed, "review list");
    for (rule, hash) in authored_approvals() {
        let header = format!("for rule '{rule}' prospective revision 1");
        assert!(listed.stdout.contains(&header), "{}", listed.stdout);
        let line = format!("prospective content hash: {hash}");
        assert!(listed.stdout.contains(&line), "{rule}\n{}", listed.stdout);
    }
    assert_eq!(listed.stdout.matches("pending proposal").count(), 3);
    assert!(listed.stdout.contains("diagnostic: error RT-VAL-0002"));
    assert_eq!(
        work.read(LEDGER),
        std::fs::read(fixture("ledger.empty.jsonl")).unwrap(),
        "listing writes nothing"
    );
}

#[test]
fn a_rule_that_reads_an_unadopted_target_is_refused_and_nothing_changes() {
    let work = ai_path("slice-ai-order");
    let (rule, hash) = authored_approvals().pop().unwrap();
    assert_eq!(rule, "rank.standing");
    let before = work.state();
    let refused = work.review_decide(&Decision {
        rulebook: RB,
        ledger: LEDGER,
        rule: &rule,
        content_hash: &hash,
        reviewer: REVIEWER,
        reason: None,
        create_ledger: false,
    });
    assert_eq!(refused.code, 3, "{}", refused.stderr);
    assert_eq!(refused.codes(), ["RT-APR-0205"]);
    assert_eq!(work.state(), before);
    work.assert_lock_files_hold_no_data();
}

#[test]
fn the_ai_path_compiles_and_evaluates_byte_identically_to_the_authored_rulebook() {
    let work = ai_path("slice-ai-identity");
    for (rule, hash) in authored_approvals() {
        let done = work.review_decide(&Decision {
            rulebook: RB,
            ledger: LEDGER,
            rule: &rule,
            content_hash: &hash,
            reviewer: REVIEWER,
            reason: None,
            create_ledger: false,
        });
        assert_ok(&done, &format!("approve {rule}"));
    }
    // The review decisions are the same ones that wrote the authored
    // ledger, so every record, hash and chain link must be equal.
    assert_eq!(
        work.read(LEDGER),
        std::fs::read(fixture("ledger.authored.jsonl")).unwrap()
    );

    let ran = work.pipeline(RB, LEDGER, "ai-");
    assert_eq!(ran.len(), 3);
    for r in &ran {
        assert_ok(r, "ai pipeline");
    }

    let authored = Work::new("slice-ai-authored");
    load_common(&authored);
    let dir = fixture("");
    authored.copy_from(&dir, "rulebook.authored.json", RB);
    authored.copy_from(&dir, "ledger.authored.jsonl", LEDGER);
    let ran = authored.pipeline(RB, LEDGER, "hand-");
    assert_eq!(ran.len(), 3);
    for r in &ran {
        assert_ok(r, "authored pipeline");
    }

    let ai_compiled = work.read("ai-compiled.json");
    let hand_compiled = authored.read("hand-compiled.json");
    assert_eq!(ai_compiled, hand_compiled);
    assert_eq!(
        ai_compiled,
        std::fs::read(fixture("expected/compiled.json")).unwrap()
    );
    let ai_results = work.read("ai-results.json");
    assert_eq!(ai_results, authored.read("hand-results.json"));
    assert_eq!(
        ai_results,
        std::fs::read(fixture("expected/results.json")).unwrap()
    );

    // What differs, and why: the Rulebook and the Validation_Result carry
    // the Origin of each revision. Neither the compiled artifact nor the
    // results name an AI identity.
    assert_ne!(work.read(RB), authored.read(RB));
    // The two validation files differ only in the recorded Origin of each
    // revision. What validation found is the same list of Diagnostics.
    assert_ne!(
        work.read("ai-validation.json"),
        authored.read("hand-validation.json")
    );
    let ai_found = validation_diagnostics(&work.read("ai-validation.json"));
    let hand_found = validation_diagnostics(&authored.read("hand-validation.json"));
    assert_eq!(ai_found, hand_found);
    assert_eq!(ai_found, "[]");
    for text in [&ai_compiled, &ai_results] {
        let text = String::from_utf8(text.clone()).unwrap();
        for name in ["ai_proposal", "proposer-a", "model-1", "mock-replay"] {
            assert!(!text.contains(name), "{name}");
        }
    }
}

#[test]
fn every_adopted_revision_is_approved_by_the_ledger_and_not_before() {
    let work = ai_path("slice-ai-status");
    let input = InputId::new(RB).unwrap();
    let limits = ParseLimits::new(1 << 20, 64);
    let ledger_id = InputId::new(LEDGER).unwrap();

    let empty = parse_ledger(&ledger_id, &work.read(LEDGER), limits).unwrap();
    assert!(empty.records().is_empty());

    for (rule, hash) in authored_approvals() {
        let done = work.review_decide(&Decision {
            rulebook: RB,
            ledger: LEDGER,
            rule: &rule,
            content_hash: &hash,
            reviewer: REVIEWER,
            reason: None,
            create_ledger: false,
        });
        assert_ok(&done, &rule);
    }
    let (rulebook, _) = parse_rulebook(&work.read(RB), &input, 1 << 20, 64)
        .map_err(|r| r.into_diagnostics().as_slice().to_vec())
        .unwrap()
        .into_parts();
    let ledger = parse_ledger(&ledger_id, &work.read(LEDGER), limits).unwrap();
    assert_eq!(rulebook.revisions.len(), 3);
    let approved = derive_statuses(&rulebook, &ledger);
    assert!(approved.values().all(|s| *s == ApprovalStatus::Approved));
    assert_eq!(effective_revisions(&rulebook, &approved).len(), 3);
    assert!(unresolved(&rulebook, &approved).is_empty());

    // The same Rulebook against the ledger that holds no record: every
    // revision is Proposed again, so a status comes from the ledger alone.
    let proposed = derive_statuses(&rulebook, &empty);
    assert_eq!(proposed.len(), 3);
    assert!(proposed.values().all(|s| *s == ApprovalStatus::Proposed));
}
