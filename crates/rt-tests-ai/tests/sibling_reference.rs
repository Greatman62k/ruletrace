//! A rule that reads the target of a sibling rule (prompt-3 evidence).
//!
//! The first live batch ranked by `score` with a `field` node. A `field`
//! node reads a declared input, and `score` is the target of another
//! proposal, so only a `ref` node can resolve it once that rule is in the
//! Rulebook. These tests drive the real review path. `review list` prints
//! the findings that were stored with the proposal, so they do not change
//! when another rule is approved; the decision re-validates against the
//! Rulebook. Before the score rule is adopted both forms are refused with
//! RT-APR-0205. After it is approved the `ref` form is approved and the
//! `field` form is still refused. Expected codes come from the catalogue
//! and the requirements.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod support;

use support::{authored_approvals, fixture, Decision, Work, REVIEWER};

const RB: &str = "rulebook.json";
const LEDGER: &str = "ledger.jsonl";
const REF_KEY: &str = r#""key":{"node":"ref","t":"number","target":"score"}"#;
const FIELD_KEY: &str = r#""key":{"node":"field","t":"number","name":"score"}"#;

fn setup(name: &str, key: &str) -> Work {
    let work = Work::new(name);
    let dir = fixture("");
    work.copy_from(&dir, "source/rules.md", "source/rules.md");
    work.copy_from(&dir, "candidates.json", "candidates.json");
    assert_eq!(work.ingest().code, 0);
    work.copy_from(&dir, "rulebook.declarations.json", RB);
    work.copy_from(&dir, "ledger.empty.jsonl", LEDGER);
    work.write_config(&["openai-responses", "model-1"]);
    let payload = std::fs::read_to_string(fixture("provider/response.json")).unwrap();
    assert!(payload.contains(REF_KEY));
    let payload = payload.replace(REF_KEY, key);
    let id = work.request_id(RB);
    work.write_script(&id, &payload);
    let proposed = work.propose(RB);
    assert_eq!(proposed.code, 0, "{}", proposed.stderr);
    work
}

// The text that `review list` prints for one rule: from its header to the
// next header.
fn section(listing: &str, rule: &str) -> String {
    let header = format!("for rule '{rule}' prospective revision 1");
    let start = listing.find(&header).unwrap();
    let rest = &listing[start + header.len()..];
    let end = rest.find("for rule '").unwrap_or(rest.len());
    rest[..end].to_owned()
}

fn hash_in(section: &str) -> String {
    let marker = "prospective content hash: ";
    let start = section.find(marker).unwrap() + marker.len();
    section[start..start + 64].to_owned()
}

fn approve(work: &Work, rule: &str, hash: &str) -> support::Ran {
    work.review_decide(&Decision {
        rulebook: RB,
        ledger: LEDGER,
        rule,
        content_hash: hash,
        reviewer: REVIEWER,
        reason: None,
        create_ledger: false,
    })
}

fn score_hash() -> String {
    authored_approvals()
        .into_iter()
        .find(|(rule, _)| rule == "score.weighted")
        .unwrap()
        .1
}

fn refused(work: &Work, rule: &str, hash: &str) {
    let ledger_before = work.read(LEDGER);
    let rulebook_before = work.read(RB);
    let ran = approve(work, rule, hash);
    assert_eq!(ran.code, 3, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-APR-0205"], "{}", ran.stderr);
    assert_eq!(work.read(LEDGER), ledger_before, "a refusal writes nothing");
    assert_eq!(work.read(RB), rulebook_before, "a refusal writes nothing");
}

#[test]
fn a_ref_to_a_sibling_score_is_refused_until_the_score_rule_is_approved_and_then_approved() {
    let work = setup("sibling-ref", REF_KEY);
    let listing = work.review_list(RB, LEDGER);
    assert_eq!(listing.code, 0, "{}", listing.stderr);
    let ranking = section(&listing.stdout, "rank.standing");
    assert!(
        ranking.contains("diagnostic: error RT-VAL-0002: undeclared rule 'score' at key"),
        "{ranking}"
    );
    let hash = hash_in(&ranking);
    refused(&work, "rank.standing", &hash);

    let done = approve(&work, "score.weighted", &score_hash());
    assert_eq!(done.code, 0, "{}", done.stderr);
    let approved = approve(&work, "rank.standing", &hash);
    assert_eq!(approved.code, 0, "{}", approved.stderr);
}

#[test]
fn a_field_named_score_stays_refused_after_the_score_rule_is_approved() {
    let work = setup("sibling-field", FIELD_KEY);
    let listing = work.review_list(RB, LEDGER);
    let ranking = section(&listing.stdout, "rank.standing");
    assert!(
        ranking.contains("diagnostic: error RT-VAL-0002: undeclared field 'score' at key"),
        "{ranking}"
    );
    let hash = hash_in(&ranking);
    refused(&work, "rank.standing", &hash);

    let done = approve(&work, "score.weighted", &score_hash());
    assert_eq!(done.code, 0, "{}", done.stderr);
    refused(&work, "rank.standing", &hash);
}
