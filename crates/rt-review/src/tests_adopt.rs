//! Tests of adoption planning (design §3.8.2; Req 23.12, 23.13, 25.1).

use super::*;
use rt_ir::{draft_hash, print_pretty, Identity};
use rt_proposals::{
    proposal_id, Batch, Completion, Evidence, Format, Proposal, ProposalKind, RuleDraft,
    StoredResponse,
};

fn draft_json(id: &str, target: &str) -> String {
    format!(
        r#"{{"id":"{id}","kind":"eligibility","target":"{target}","type":"boolean","expr":{},"markers":[],"spans":[{}]}}"#,
        good_expr(),
        span()
    )
}

fn batch_with(rb: &Rulebook, seed: &str, drafts: &[(&str, &str)]) -> Batch {
    let request_id = sha256(seed.as_bytes());
    let proposals = drafts
        .iter()
        .enumerate()
        .map(|(index, (id, target))| {
            let json = draft_json(id, target);
            let probe = rulebook_with_draft(rb, &json);
            let hash = draft_hash(
                &rb.ir_version,
                &RuleId::new(*id).unwrap(),
                &probe,
                &rb.documents,
            )
            .unwrap();
            Proposal {
                proposal_id: proposal_id(&request_id, index),
                kind: ProposalKind::RuleSuggestion,
                proposer: Identity::new("ai-1").unwrap(),
                rationale: "age clause".to_owned(),
                evidence: Vec::<Evidence>::new(),
                rule: Some((
                    RuleDraft {
                        id: RuleId::new(*id).unwrap(),
                        json,
                    },
                    hash,
                )),
                outcome: Vec::new(),
            }
        })
        .collect();
    Batch {
        format: Format::V1,
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
        proposals,
    }
}

fn rulebook_with_draft(rb: &Rulebook, json: &str) -> rt_ir::RuleContent {
    let origin = rb
        .revisions
        .first()
        .map_or_else(|| panic!("fixture has a rule"), |r| r.origin.clone());
    rt_ir::parse_draft(
        json.as_bytes(),
        &InputId::new("d").unwrap(),
        RevisionNumber::new(1).unwrap(),
        origin,
        1 << 20,
        64,
    )
    .unwrap()
    .into_parts()
    .0
    .content
}

fn adoption(rule: &str, revision: u32, hash: Sha256Hex, action: ReviewAction) -> ReviewDecision {
    ReviewDecision {
        rule: RuleId::new(rule).unwrap(),
        revision: RevisionNumber::new(revision).unwrap(),
        content_hash: hash,
        reviewer: "A. Reviewer".to_owned(),
        time: "2026-11-14T09:00:00Z".to_owned(),
        action,
        batch_identities: AiIdentitySet::default(),
        sites: sites(),
    }
}

fn base() -> Rulebook {
    rulebook(&[rule("elig.basic", MANUAL, "eligible", good_expr())])
}

fn prospective_of(rb: &Rulebook, b: &Batch, index: usize) -> Prospective {
    let pending = rt_proposals::pending_proposals(std::slice::from_ref(b), rb);
    prospective_adoption(rb, &pending[index], &sites().rule).unwrap()
}

#[test]
fn adopting_a_pending_proposal_gives_revision_one_and_bytes_for_both_files() {
    let rb = base();
    let b = batch_with(&rb, "req-1", &[("elig.new", "eligible2")]);
    let p = prospective_of(&rb, &b, 0);
    assert_eq!(p.revision.get(), 1);
    let plan = plan_review_with_store(
        &rb,
        &ApprovalLedger::empty(),
        &adoption("elig.new", 1, p.content_hash.clone(), ReviewAction::Approve),
        &sources(),
        std::slice::from_ref(&b),
    )
    .unwrap();
    let bytes = plan
        .rulebook_bytes()
        .expect("adoption holds rulebook bytes");
    let new_rb = parse_rulebook(bytes, &InputId::new("rb").unwrap(), 1 << 22, 64)
        .unwrap()
        .into_parts()
        .0;
    assert_eq!(new_rb.revisions.len(), 2);
    let adopted = new_rb
        .revision(&rt_ir::RuleKey {
            id: RuleId::new("elig.new").unwrap(),
            revision: RevisionNumber::new(1).unwrap(),
        })
        .unwrap();
    match &adopted.origin {
        rt_ir::Origin::AiProposal(o) => {
            assert_eq!(o.proposal_id, b.proposals[0].proposal_id.as_str());
            assert_eq!(o.request_hash, b.request_id);
        }
        other => panic!("origin must be an AI proposal: {other:?}"),
    }
    assert_eq!(hash_of(&new_rb, "elig.new"), p.content_hash);
    let ledger = reparse(plan.ledger_bytes());
    assert_eq!(ledger.len(), 1);
    assert_eq!(ledger.records()[0].kind(), RecordKind::Approval);
    // The pre-existing revision is untouched: the new Rulebook minus the
    // adopted revision prints byte for byte like the old one.
    assert_eq!(new_rb.revisions[0], rb.revisions[0]);
    let mut without = new_rb.clone();
    without
        .revisions
        .retain(|r| r.key.id.as_str() != "elig.new");
    assert_eq!(print_pretty(&without).unwrap(), print_pretty(&rb).unwrap());
}

#[test]
fn an_adopted_rule_validates_with_no_blocking_diagnostic_anywhere() {
    let rb = base();
    let first = plan_review(
        &rb,
        &ApprovalLedger::empty(),
        &request(&rb, "elig.basic", ReviewAction::Approve),
        &sources(),
    )
    .unwrap();
    let b = batch_with(&rb, "req-1b", &[("elig.new", "eligible2")]);
    let p = prospective_of(&rb, &b, 0);
    let plan = plan_review_with_store(
        &rb,
        first.ledger(),
        &adoption("elig.new", 1, p.content_hash, ReviewAction::Approve),
        &sources(),
        std::slice::from_ref(&b),
    )
    .unwrap();
    let new_rb = parse_rulebook(
        plan.rulebook_bytes().unwrap(),
        &InputId::new("rb").unwrap(),
        1 << 22,
        64,
    )
    .unwrap()
    .into_parts()
    .0;
    let ledger = reparse(plan.ledger_bytes());
    assert_eq!(ledger.len(), 2);
    let result = validate(&new_rb, &ledger, &sources(), None);
    let blocking: Vec<_> = result
        .state()
        .as_slice()
        .iter()
        .filter(|x| x.severity().is_blocking())
        .collect();
    assert!(blocking.is_empty(), "blocking diagnostics: {blocking:?}");
    assert!(result.state().no_errors());
}

#[test]
fn rejection_adopts_as_rejected_and_next_adoption_has_no_gap() {
    let rb = base();
    let b = batch_with(
        &rb,
        "req-2",
        &[("elig.new", "eligible2"), ("elig.new", "eligible3")],
    );
    let first = prospective_of(&rb, &b, 0);
    assert_eq!(first.revision.get(), 1);
    let reject = plan_review_with_store(
        &rb,
        &ApprovalLedger::empty(),
        &adoption("elig.new", 1, first.content_hash, reject_action()),
        &sources(),
        std::slice::from_ref(&b),
    )
    .unwrap();
    let rb2 = parse_rulebook(
        reject.rulebook_bytes().unwrap(),
        &InputId::new("rb").unwrap(),
        1 << 22,
        64,
    )
    .unwrap()
    .into_parts()
    .0;
    let ledger2 = reparse(reject.ledger_bytes());
    assert_eq!(ledger2.records()[0].kind(), RecordKind::Rejection);
    let second = prospective_of(&rb2, &b, 0);
    assert_eq!(second.revision.get(), 2);
    let plan = plan_review_with_store(
        &rb2,
        &ledger2,
        &adoption("elig.new", 2, second.content_hash, ReviewAction::Approve),
        &sources(),
        std::slice::from_ref(&b),
    )
    .unwrap();
    assert_eq!(ledger_len(&plan), 2);
}

fn ledger_len(plan: &CommitPlan) -> usize {
    plan.ledger().len()
}

#[test]
fn wrong_revision_is_target_not_found_and_wrong_hash_is_stale() {
    let rb = base();
    let b = batch_with(&rb, "req-3", &[("elig.new", "eligible2")]);
    let p = prospective_of(&rb, &b, 0);
    let store = std::slice::from_ref(&b);
    let l = ApprovalLedger::empty();
    let d = refusal(plan_review_with_store(
        &rb,
        &l,
        &adoption("elig.new", 2, p.content_hash.clone(), ReviewAction::Approve),
        &sources(),
        store,
    ));
    assert_eq!(d.code(), Code::ReviewTargetNotFound);
    assert_eq!(d.primary(), &sites().rule);
    let d = refusal(plan_review_with_store(
        &rb,
        &l,
        &adoption("elig.new", 1, sha256(b"other"), ReviewAction::Approve),
        &sources(),
        store,
    ));
    assert_eq!(d.code(), Code::ReviewStaleHash);
    assert_eq!(d.primary(), &sites().content_hash);
    let d = refusal(plan_review_with_store(
        &rb,
        &l,
        &adoption("elig.unknown", 1, p.content_hash, ReviewAction::Approve),
        &sources(),
        store,
    ));
    assert_eq!(d.code(), Code::ReviewTargetNotFound);
}

#[test]
fn proposer_as_reviewer_is_refused_with_no_bytes() {
    let rb = base();
    let b = batch_with(&rb, "req-4", &[("elig.new", "eligible2")]);
    let p = prospective_of(&rb, &b, 0);
    let mut d = adoption("elig.new", 1, p.content_hash, ReviewAction::Approve);
    d.reviewer = "ai-1".to_owned();
    let diag = refusal(plan_review_with_store(
        &rb,
        &ApprovalLedger::empty(),
        &d,
        &sources(),
        std::slice::from_ref(&b),
    ));
    assert_eq!(diag.code(), Code::ReviewSelfApproval);
}

#[test]
fn tampered_draft_hash_is_refused_as_store_invalid() {
    let rb = base();
    let mut b = batch_with(&rb, "req-5", &[("elig.new", "eligible2")]);
    let p = prospective_of(&rb, &b, 0);
    if let Some((_, h)) = b.proposals[0].rule.as_mut() {
        *h = sha256(b"tampered");
    }
    let diag = refusal(plan_review_with_store(
        &rb,
        &ApprovalLedger::empty(),
        &adoption("elig.new", 1, p.content_hash, ReviewAction::Approve),
        &sources(),
        std::slice::from_ref(&b),
    ));
    assert_eq!(diag.code(), Code::ReviewProposalStoreInvalid);
}

#[test]
fn a_draft_hashed_for_another_document_hash_is_refused_at_adoption() {
    let rb = base();
    let b = batch_with(&rb, "req-doc", &[("elig.new", "eligible2")]);
    let named = prospective_of(&rb, &b, 0).content_hash;
    let mut other = rb.clone();
    other.documents[0].sha256 = sha256(b"a different document");
    assert_ne!(other.documents[0].sha256, rb.documents[0].sha256);
    let pending = rt_proposals::pending_proposals(std::slice::from_ref(&b), &other);
    let direct = prospective_adoption(&other, &pending[0], &sites().rule).unwrap_err();
    assert_eq!(direct.code(), Code::ReviewProposalStoreInvalid);
    assert_eq!(direct.code().id(), "RT-APR-0213");
    assert_eq!(direct.primary(), &sites().rule);
    assert!(
        direct.message().contains("differs from the recomputed"),
        "{}",
        direct.message()
    );
    let planned = refusal(plan_review_with_store(
        &other,
        &ApprovalLedger::empty(),
        &adoption("elig.new", 1, named, ReviewAction::Approve),
        &sources(),
        std::slice::from_ref(&b),
    ));
    assert_eq!(planned.code(), Code::ReviewProposalStoreInvalid);
}

#[test]
fn a_draft_that_names_revision_or_origin_does_not_decode() {
    let rb = base();
    let mut b = batch_with(&rb, "req-6", &[("elig.new", "eligible2")]);
    if let Some((d, _)) = b.proposals[0].rule.as_mut() {
        d.json = d.json.replacen("\"kind\"", "\"revision\":9,\"kind\"", 1);
    }
    let pending = rt_proposals::pending_proposals(std::slice::from_ref(&b), &rb);
    let err = prospective_adoption(&rb, &pending[0], &sites().rule).unwrap_err();
    assert_eq!(err.code(), Code::ReviewProposalStoreInvalid);
}

#[test]
fn revision_exhaustion_is_rt_apr_0210() {
    let rb = base();
    let b = batch_with(&rb, "req-7", &[("elig.basic", "eligible")]);
    let mut full = rb.clone();
    let mut top = full.revisions[0].clone();
    top.key.revision = RevisionNumber::new(u32::MAX).unwrap();
    full.revisions.push(top);
    let pending = rt_proposals::pending_proposals(std::slice::from_ref(&b), &full);
    let err = prospective_adoption(&full, &pending[0], &sites().rule).unwrap_err();
    assert_eq!(err.code(), Code::ReviewRevisionExhausted);
    assert_eq!(err.code().id(), "RT-APR-0210");
    assert_eq!(err.primary(), &sites().rule);
}

#[test]
fn exhaustion_leaves_the_rulebook_unchanged() {
    let rb = base();
    let b = batch_with(&rb, "req-7b", &[("elig.basic", "eligible")]);
    let mut full = rb.clone();
    full.revisions[0].key.revision = RevisionNumber::new(u32::MAX).unwrap();
    let before = full.clone();
    let pending = rt_proposals::pending_proposals(std::slice::from_ref(&b), &full);
    assert_eq!(pending.len(), 1);
    let err = prospective_adoption(&full, &pending[0], &sites().rule).unwrap_err();
    assert_eq!(err.code(), Code::ReviewRevisionExhausted);
    assert_eq!(full, before);
}

#[test]
fn one_below_the_highest_revision_number_still_adopts_as_the_highest() {
    let rb = base();
    let b = batch_with(&rb, "req-7c", &[("elig.basic", "eligible")]);
    let mut near = rb.clone();
    near.revisions[0].key.revision = RevisionNumber::new(u32::MAX - 1).unwrap();
    let pending = rt_proposals::pending_proposals(std::slice::from_ref(&b), &near);
    let ok = prospective_adoption(&near, &pending[0], &sites().rule).unwrap();
    assert_eq!(ok.revision.get(), u32::MAX);
}

#[test]
fn an_existing_revision_still_changes_only_the_ledger() {
    let rb = base();
    let b = batch_with(&rb, "req-8", &[("elig.new", "eligible2")]);
    let plan = plan_review_with_store(
        &rb,
        &ApprovalLedger::empty(),
        &request(&rb, "elig.basic", ReviewAction::Approve),
        &sources(),
        std::slice::from_ref(&b),
    )
    .unwrap();
    assert!(plan.rulebook_bytes().is_none());
}

// Hand-derived expected values. The hash input is the canonical compact
// Rule_Content of design §4.1.1 and §4.1.7 for rule `elig.new`, written out
// byte by byte below, and the digests were computed with Python hashlib, not
// with `rule_content_hash` or `draft_hash`.
const DOC_SHA: &str = "07c5327f086fa07813c2c1c5a5f8df6801708258c7c5494d53e47da776aa71c6";
const DRAFT_GOLDEN: &str = "de9a2d628ec97ed0f2889c820de1a92a7b910df612f1f28c705ae5a26c66ba3a";
const REV1_GOLDEN: &str = "35d84a08f8c0e6611e572a681c8d8cdb6e2aa4b60c86bde713e0a8b1aca7690f";
const REV4_GOLDEN: &str = "816668e54a2a1d95956b419723996b4e4876ae6922f4081dd4ee0a70314c010a";

const DRAFT_INPUT: &str = r#"{"ir_version":"1","id":"elig.new","kind":"eligibility","target":"eligible2","type":"boolean","expr":{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}},"markers":[],"spans":[{"document":"rules.md","sha256":"07c5327f086fa07813c2c1c5a5f8df6801708258c7c5494d53e47da776aa71c6","start":9,"end":59}]}"#;

fn golden(hex: &str) -> Sha256Hex {
    Sha256Hex::parse(hex).expect("golden is 64 lowercase hex digits")
}

fn adopted_rulebook(plan: &CommitPlan) -> Rulebook {
    parse_rulebook(
        plan.rulebook_bytes()
            .expect("adoption holds rulebook bytes"),
        &InputId::new("rb").unwrap(),
        1 << 22,
        64,
    )
    .unwrap()
    .into_parts()
    .0
}

#[test]
fn golden_fixture_pins_the_document_hash_and_the_hash_inputs() {
    // The document hash and both digests are literals, so a change of the
    // canonical hash input shows here and not only in production code.
    assert_eq!(sha256(TEXT.as_bytes()), golden(DOC_SHA));
    assert_eq!(sha256(DRAFT_INPUT.as_bytes()), golden(DRAFT_GOLDEN));
    let with_revision =
        DRAFT_INPUT.replacen(r#""id":"elig.new","#, r#""id":"elig.new","revision":1,"#, 1);
    assert_eq!(sha256(with_revision.as_bytes()), golden(REV1_GOLDEN));
}

#[test]
fn adoption_hashes_equal_the_hand_derived_golden_values() {
    let rb = base();
    let mut b = batch_with(&rb, "req-g", &[("elig.new", "eligible2")]);
    // The stored Draft hash is the golden literal, so adoption only succeeds
    // when the recomputed Draft hash equals it.
    if let Some((_, h)) = b.proposals[0].rule.as_mut() {
        *h = golden(DRAFT_GOLDEN);
    }
    let pending = rt_proposals::pending_proposals(std::slice::from_ref(&b), &rb);
    let p = prospective_adoption(&rb, &pending[0], &sites().rule).unwrap();
    assert_eq!(p.revision.get(), 1);
    assert_eq!(p.content_hash, golden(REV1_GOLDEN));
    let plan = plan_review_with_store(
        &rb,
        &ApprovalLedger::empty(),
        &adoption("elig.new", 1, golden(REV1_GOLDEN), ReviewAction::Approve),
        &sources(),
        std::slice::from_ref(&b),
    )
    .unwrap();
    let new_rb = adopted_rulebook(&plan);
    assert_eq!(hash_of(&new_rb, "elig.new"), golden(REV1_GOLDEN));
    let record = &plan.ledger().records()[0];
    assert_eq!(record.kind(), RecordKind::Approval);
    assert_eq!(record.rule().as_str(), "elig.new");
    assert_eq!(record.revision().get(), 1);
    assert_eq!(record.content_hash(), &golden(REV1_GOLDEN));
}

#[test]
fn a_wrong_stored_draft_hash_is_not_accepted_as_the_golden_one() {
    let rb = base();
    let mut b = batch_with(&rb, "req-g2", &[("elig.new", "eligible2")]);
    if let Some((_, h)) = b.proposals[0].rule.as_mut() {
        *h = golden(REV1_GOLDEN);
    }
    let pending = rt_proposals::pending_proposals(std::slice::from_ref(&b), &rb);
    let err = prospective_adoption(&rb, &pending[0], &sites().rule).unwrap_err();
    assert_eq!(err.code(), Code::ReviewProposalStoreInvalid);
}

fn rule_at(revision: u32, id: &str, target: &str) -> String {
    rule(id, MANUAL, target, good_expr()).replacen(
        r#""revision":1"#,
        &format!(r#""revision":{revision}"#),
        1,
    )
}

fn hash_at(rb: &Rulebook, id: &str, revision: u32) -> Sha256Hex {
    let rev = rb
        .revisions
        .iter()
        .find(|r| r.key.id.as_str() == id && r.key.revision.get() == revision)
        .unwrap();
    rule_content_hash(&rb.ir_version, rev, &rb.documents).unwrap()
}

#[test]
fn next_number_is_one_plus_the_maximum_of_this_rule_only() {
    // Revisions of elig.new are held in memory as 2, 3, 1 (2 rejected; the
    // parser sorts them, so the order is set after parsing): min, first and
    // last would give 2, 3 and 2. Another rule has revisions 1 to 5, so a
    // maximum over all rules would give 6.
    let mut rb = rulebook(&[
        rule_at(1, "elig.new", "eligible2"),
        rule_at(2, "elig.new", "eligible2"),
        rule_at(3, "elig.new", "eligible2"),
        rule_at(1, "elig.other", "eligible9"),
        rule_at(2, "elig.other", "eligible9"),
        rule_at(3, "elig.other", "eligible9"),
        rule_at(4, "elig.other", "eligible9"),
        rule_at(5, "elig.other", "eligible9"),
    ]);
    rb.revisions[..3].rotate_left(1);
    let held: Vec<u32> = rb.revisions[..3]
        .iter()
        .map(|r| r.key.revision.get())
        .collect();
    assert_eq!(held, vec![2, 3, 1]);
    let reject_two = plan_review(
        &rb,
        &ApprovalLedger::empty(),
        &adoption("elig.new", 2, hash_at(&rb, "elig.new", 2), reject_action()),
        &sources(),
    )
    .unwrap();
    let b = batch_with(&rb, "req-m", &[("elig.new", "eligible2")]);
    let pending = rt_proposals::pending_proposals(std::slice::from_ref(&b), &rb);
    let p = prospective_adoption(&rb, &pending[0], &sites().rule).unwrap();
    assert_eq!(p.revision.get(), 4);
    assert_eq!(p.content_hash, golden(REV4_GOLDEN));
    let plan = plan_review_with_store(
        &rb,
        reject_two.ledger(),
        &adoption("elig.new", 4, golden(REV4_GOLDEN), ReviewAction::Approve),
        &sources(),
        std::slice::from_ref(&b),
    )
    .unwrap();
    assert_eq!(plan.key().revision.get(), 4);
    let new_rb = adopted_rulebook(&plan);
    let numbers: Vec<u32> = new_rb
        .revisions
        .iter()
        .filter(|r| r.key.id.as_str() == "elig.new")
        .map(|r| r.key.revision.get())
        .collect();
    assert_eq!(numbers, vec![1, 2, 3, 4]);
    // Numbers above the next one are not adoptable.
    for wrong in [5, 6] {
        let d = refusal(plan_review_with_store(
            &rb,
            reject_two.ledger(),
            &adoption(
                "elig.new",
                wrong,
                golden(REV4_GOLDEN),
                ReviewAction::Approve,
            ),
            &sources(),
            std::slice::from_ref(&b),
        ));
        assert_eq!(d.code(), Code::ReviewTargetNotFound, "revision {wrong}");
    }
}

#[test]
fn a_stored_draft_edited_after_hashing_is_refused_as_store_invalid() {
    let rb = base();
    let mut b = batch_with(&rb, "req-t", &[("elig.new", "eligible2")]);
    let p = prospective_of(&rb, &b, 0);
    let stored = b.proposals[0].rule.as_ref().unwrap().1.clone();
    if let Some((d, _)) = b.proposals[0].rule.as_mut() {
        assert!(d.json.contains(r#""value":"16""#));
        d.json = d.json.replacen(r#""value":"16""#, r#""value":"17""#, 1);
    }
    assert_eq!(b.proposals[0].rule.as_ref().unwrap().1, stored);
    let diag = refusal(plan_review_with_store(
        &rb,
        &ApprovalLedger::empty(),
        &adoption("elig.new", 1, p.content_hash, ReviewAction::Approve),
        &sources(),
        std::slice::from_ref(&b),
    ));
    assert_eq!(diag.code(), Code::ReviewProposalStoreInvalid);
    assert_eq!(diag.code().id(), "RT-APR-0213");
    assert!(
        diag.message().contains("differs from the recomputed"),
        "{}",
        diag.message()
    );
}

#[test]
fn provider_model_and_batch_only_identities_cannot_adopt() {
    let rb = base();
    let mut b = batch_with(&rb, "req-i", &[("elig.new", "eligible2")]);
    b.ai_identities
        .push(Identity::new("batch-only-ai").unwrap());
    let p = prospective_of(&rb, &b, 0);
    for name in ["mock", "m1", "batch-only-ai", "ai-1"] {
        let mut d = adoption("elig.new", 1, p.content_hash.clone(), ReviewAction::Approve);
        d.reviewer = name.to_owned();
        let diag = refusal(plan_review_with_store(
            &rb,
            &ApprovalLedger::empty(),
            &d,
            &sources(),
            std::slice::from_ref(&b),
        ));
        assert_eq!(diag.code(), Code::ReviewSelfApproval, "reviewer {name}");
        assert_eq!(diag.code().id(), "RT-APR-0201");
    }
    // An identity given only through the decision counts as well, and a
    // human name outside every set is accepted.
    let mut d = adoption("elig.new", 1, p.content_hash.clone(), ReviewAction::Approve);
    d.reviewer = "decision-only-ai".to_owned();
    d.batch_identities = AiIdentitySet::new(["decision-only-ai"]);
    let diag = refusal(plan_review_with_store(
        &rb,
        &ApprovalLedger::empty(),
        &d,
        &sources(),
        std::slice::from_ref(&b),
    ));
    assert_eq!(diag.code(), Code::ReviewSelfApproval);
    let ok = adoption("elig.new", 1, p.content_hash, ReviewAction::Approve);
    assert!(plan_review_with_store(
        &rb,
        &ApprovalLedger::empty(),
        &ok,
        &sources(),
        std::slice::from_ref(&b)
    )
    .is_ok());
}

#[test]
fn the_named_hash_selects_among_proposals_for_the_same_rule() {
    let rb = base();
    let b = batch_with(
        &rb,
        "req-s",
        &[("elig.new", "eligible2"), ("elig.new", "eligible3")],
    );
    let second = prospective_of(&rb, &b, 1);
    assert_eq!(second.revision.get(), 1);
    assert_eq!(second.adopted.content.target.as_str(), "eligible3");
    let plan = plan_review_with_store(
        &rb,
        &ApprovalLedger::empty(),
        &adoption("elig.new", 1, second.content_hash, ReviewAction::Approve),
        &sources(),
        std::slice::from_ref(&b),
    )
    .unwrap();
    let new_rb = adopted_rulebook(&plan);
    let adopted = new_rb
        .revisions
        .iter()
        .find(|r| r.key.id.as_str() == "elig.new");
    let origin = match &adopted.unwrap().origin {
        rt_ir::Origin::AiProposal(o) => o.clone(),
        other => panic!("origin must be an AI proposal: {other:?}"),
    };
    assert_eq!(origin.proposal_id, b.proposals[1].proposal_id.as_str());
    assert_eq!(adopted.unwrap().content.target.as_str(), "eligible3");
}

#[test]
fn an_unbindable_draft_is_refused_at_the_rule_site_without_an_approval_binding() {
    let rb = base();
    let b = batch_with(&rb, "req-unbound", &[("elig.new", "eligible2")]);
    let mut other = rb.clone();
    other.documents.clear();
    let pending = rt_proposals::pending_proposals(std::slice::from_ref(&b), &other);
    let direct = prospective_adoption(&other, &pending[0], &sites().rule).unwrap_err();
    assert_eq!(direct.code(), Code::ReviewBlockingDiagnostic);
    assert!(
        direct
            .message()
            .starts_with("the draft of rule 'elig.new' has no approval binding: "),
        "{}",
        direct.message()
    );
    assert_eq!(direct.primary(), &sites().rule);
}
