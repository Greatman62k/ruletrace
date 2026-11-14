//! Tests of `plan_review`.

use super::*;
use rt_foundation::{Code, DocumentId, InputId, RevisionNumber, RuleId, Severity};
use rt_hash::{sha256, Sha256Hex};
use rt_ir::{parse_rulebook, rule_content_hash, Rulebook};
use rt_json::ParseLimits;
use rt_ledger::{parse_ledger, print_ledger, AiIdentitySet, ApprovalLedger, RecordKind};
use rt_srcmodel::{DocumentKind, SourceModel, SourceModels};
use rt_validate::validate;

const TEXT: &str = "# Rules\n\n1. A competitor is eligible if age is at least 16.\n";
const CLAUSE: &str = "1. A competitor is eligible if age is at least 16.";
const AI: &str = r#"{"kind":"ai_proposal","proposer":"ai-1","provider":"mock","model":"m1","adapter_version":"0.1","request_hash":"eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a","response_hash":"eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a","proposal_id":"p-1","ai_identities":["ai-1"]}"#;
const MANUAL: &str = r#"{"kind":"manual","author":null}"#;

fn span() -> String {
    let start = TEXT.find(CLAUSE).unwrap();
    let end = start + CLAUSE.len();
    format!(r#"{{"document":"rules.md","start":{start},"end":{end},"line":3,"column":1}}"#)
}

fn good_expr() -> &'static str {
    r#"{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}}"#
}

/// `name` (text) compared with a number: one type error naming the rule.
fn bad_expr() -> &'static str {
    r#"{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"text","name":"name"},"right":{"node":"num","t":"number","value":"16"}}"#
}

fn rule(id: &str, origin: &str, target: &str, expr: &str) -> String {
    format!(
        r#"{{"id":"{id}","revision":1,"origin":{origin},"kind":"eligibility","target":"{target}","type":"boolean","expr":{expr},"markers":[],"spans":[{}]}}"#,
        span()
    )
}

fn rulebook(rules: &[String]) -> Rulebook {
    let doc = format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}},{{"name":"name","type":"text"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{}],"precedence":[]}}"#,
        sha256(TEXT.as_bytes()),
        rules.join(",")
    );
    match parse_rulebook(
        doc.as_bytes(),
        &InputId::new("rb.json").unwrap(),
        1 << 22,
        64,
    ) {
        Ok(s) => s.into_parts().0,
        Err(r) => panic!("fixture: {:?}", r.diagnostics().as_slice()),
    }
}

fn sources() -> SourceModels {
    let mut s = SourceModels::new();
    s.insert(
        SourceModel::new(
            DocumentId::new("rules.md").unwrap(),
            DocumentKind::Markdown,
            TEXT.to_owned(),
            Vec::new(),
        )
        .unwrap(),
    )
    .unwrap();
    s
}

fn hash_of(rb: &Rulebook, id: &str) -> Sha256Hex {
    let rev = rb
        .revisions
        .iter()
        .find(|r| r.key.id.as_str() == id)
        .unwrap();
    rule_content_hash(&rb.ir_version, rev, &rb.documents).unwrap()
}

fn request(rb: &Rulebook, id: &str, action: ReviewAction) -> ReviewDecision {
    ReviewDecision {
        rule: RuleId::new(id).unwrap(),
        revision: RevisionNumber::new(1).unwrap(),
        content_hash: hash_of(rb, id),
        reviewer: "A. Reviewer".to_owned(),
        time: "2026-11-14T09:00:00Z".to_owned(),
        action,
        batch_identities: AiIdentitySet::default(),
    }
}

fn reject_action() -> ReviewAction {
    ReviewAction::Reject {
        reason: "not a rule".to_owned(),
    }
}

fn refusal(r: Result<CommitPlan, rt_foundation::Rejected>) -> Diagnostic {
    let d = r.expect_err("must refuse").into_diagnostics();
    assert_eq!(d.as_slice().len(), 1, "exactly one Diagnostic: {d:?}");
    d.as_slice()[0].clone()
}

use rt_foundation::Diagnostic;

fn reparse(bytes: &[u8]) -> ApprovalLedger {
    parse_ledger(
        &InputId::new("l").unwrap(),
        bytes,
        ParseLimits::new(1 << 20, 64),
    )
    .unwrap()
}

fn basic() -> Rulebook {
    rulebook(&[rule("elig.basic", MANUAL, "eligible", good_expr())])
}

#[test]
fn approval_appends_exactly_one_record_and_validate_passes() {
    let rb = basic();
    let old = ApprovalLedger::empty();
    let plan = plan_review(
        &rb,
        &old,
        &request(&rb, "elig.basic", ReviewAction::Approve),
        &sources(),
    )
    .unwrap();
    let old_bytes = print_ledger(&old).into_bytes();
    assert!(plan.ledger_bytes().starts_with(&old_bytes));
    let new = reparse(plan.ledger_bytes());
    assert_eq!(new.len(), 1);
    assert_eq!(new.records()[0].kind(), RecordKind::Approval);
    assert_eq!(new.records()[0].time(), "2026-11-14T09:00:00Z");
    assert_eq!(new.records()[0].hash(), plan.record_hash());
    assert!(validate(&rb, &new, &sources(), None).state().no_errors());
}

#[test]
fn second_record_chains_to_the_first_and_planning_is_deterministic() {
    let rb = rulebook(&[
        rule("elig.a", MANUAL, "ea", good_expr()),
        rule("elig.b", MANUAL, "eb", good_expr()),
    ]);
    let first = plan_review(
        &rb,
        &ApprovalLedger::empty(),
        &request(&rb, "elig.a", ReviewAction::Approve),
        &sources(),
    )
    .unwrap();
    let req = request(&rb, "elig.b", ReviewAction::Approve);
    let second = plan_review(&rb, first.ledger(), &req, &sources()).unwrap();
    let again = plan_review(&rb, first.ledger(), &req, &sources()).unwrap();
    assert_eq!(second.ledger_bytes(), again.ledger_bytes());
    assert!(second.ledger_bytes().starts_with(first.ledger_bytes()));
    let r = &second.ledger().records()[1];
    assert_eq!(r.seq(), 2);
    assert_eq!(r.prev(), first.record_hash());
}

#[test]
fn rejection_records_the_reason_and_derives_rejected() {
    let rb = basic();
    let plan = plan_review(
        &rb,
        &ApprovalLedger::empty(),
        &request(&rb, "elig.basic", reject_action()),
        &sources(),
    )
    .unwrap();
    let rec = &plan.ledger().records()[0];
    assert_eq!(rec.kind(), RecordKind::Rejection);
    assert_eq!(rec.reason(), Some("not a rule"));
}

#[test]
fn stale_hash_is_one_diagnostic_and_no_bytes() {
    let rb = basic();
    let mut req = request(&rb, "elig.basic", ReviewAction::Approve);
    req.content_hash = sha256(b"other");
    let d = refusal(plan_review(&rb, &ApprovalLedger::empty(), &req, &sources()));
    assert_eq!(d.code(), Code::ReviewStaleHash);
    assert_eq!(d.severity(), Severity::Error);
}

#[test]
fn missing_target_revision_and_rule() {
    let rb = basic();
    let mut req = request(&rb, "elig.basic", ReviewAction::Approve);
    req.revision = RevisionNumber::new(2).unwrap();
    let d = refusal(plan_review(&rb, &ApprovalLedger::empty(), &req, &sources()));
    assert_eq!(d.code(), Code::ReviewTargetNotFound);
    let mut req = request(&rb, "elig.basic", ReviewAction::Approve);
    req.rule = RuleId::new("nope").unwrap();
    let d = refusal(plan_review(&rb, &ApprovalLedger::empty(), &req, &sources()));
    assert_eq!(d.code(), Code::ReviewTargetNotFound);
}

#[test]
fn reviewer_must_be_valid_and_not_an_ai_identity() {
    let rb = rulebook(&[rule("elig.ai", AI, "eligible", good_expr())]);
    for (name, code) in [
        ("", Code::ReviewReviewerInvalid),
        (&"x".repeat(201), Code::ReviewReviewerInvalid),
        ("ai-1", Code::ReviewSelfApproval),
        ("mock", Code::ReviewSelfApproval),
        ("m1", Code::ReviewSelfApproval),
    ] {
        let mut req = request(&rb, "elig.ai", ReviewAction::Approve);
        req.reviewer = name.to_owned();
        let d = refusal(plan_review(&rb, &ApprovalLedger::empty(), &req, &sources()));
        assert_eq!(d.code(), code, "reviewer {name:?}");
    }
    // Exact comparison: a different spelling is accepted (UD-23, R-8).
    let mut req = request(&rb, "elig.ai", ReviewAction::Approve);
    req.reviewer = "AI-1".to_owned();
    assert!(plan_review(&rb, &ApprovalLedger::empty(), &req, &sources()).is_ok());
    // An identity of the batch counts too.
    let mut req = request(&rb, "elig.ai", ReviewAction::Approve);
    req.reviewer = "batch-bot".to_owned();
    req.batch_identities = AiIdentitySet::new(["batch-bot"]);
    let d = refusal(plan_review(&rb, &ApprovalLedger::empty(), &req, &sources()));
    assert_eq!(d.code(), Code::ReviewSelfApproval);
}

#[test]
fn reason_and_time_are_checked() {
    let rb = basic();
    for reason in [String::new(), "r".repeat(2001)] {
        let req = request(&rb, "elig.basic", ReviewAction::Reject { reason });
        let d = refusal(plan_review(&rb, &ApprovalLedger::empty(), &req, &sources()));
        assert_eq!(d.code(), Code::ReviewReasonInvalid);
    }
    let ok = request(
        &rb,
        "elig.basic",
        ReviewAction::Reject {
            reason: "r".repeat(2000),
        },
    );
    assert!(plan_review(&rb, &ApprovalLedger::empty(), &ok, &sources()).is_ok());
    let mut req = request(&rb, "elig.basic", ReviewAction::Approve);
    req.time = "yesterday".to_owned();
    let d = refusal(plan_review(&rb, &ApprovalLedger::empty(), &req, &sources()));
    assert_eq!(d.code(), Code::CliArgumentInvalid);
}

#[test]
fn a_decided_revision_cannot_be_decided_again() {
    let rb = basic();
    let approve = request(&rb, "elig.basic", ReviewAction::Approve);
    let approved = plan_review(&rb, &ApprovalLedger::empty(), &approve, &sources()).unwrap();
    for action in [ReviewAction::Approve, reject_action()] {
        let d = refusal(plan_review(
            &rb,
            approved.ledger(),
            &request(&rb, "elig.basic", action),
            &sources(),
        ));
        assert_eq!(d.code(), Code::ReviewInvalidTransition);
    }
    let rejected = plan_review(
        &rb,
        &ApprovalLedger::empty(),
        &request(&rb, "elig.basic", reject_action()),
        &sources(),
    )
    .unwrap();
    for action in [ReviewAction::Approve, reject_action()] {
        let d = refusal(plan_review(
            &rb,
            rejected.ledger(),
            &request(&rb, "elig.basic", action),
            &sources(),
        ));
        assert_eq!(d.code(), Code::ReviewInvalidTransition);
    }
}

#[test]
fn missing_or_mismatched_source_model_is_refused() {
    let rb = basic();
    let req = request(&rb, "elig.basic", ReviewAction::Approve);
    let d = refusal(plan_review(
        &rb,
        &ApprovalLedger::empty(),
        &req,
        &SourceModels::new(),
    ));
    assert_eq!(d.code(), Code::ReviewSourceModelMissing);
    let mut other = SourceModels::new();
    other
        .insert(
            SourceModel::new(
                DocumentId::new("rules.md").unwrap(),
                DocumentKind::Markdown,
                format!("{TEXT} "),
                Vec::new(),
            )
            .unwrap(),
        )
        .unwrap();
    let d = refusal(plan_review(&rb, &ApprovalLedger::empty(), &req, &other));
    assert_eq!(d.code(), Code::ReviewSourceModelMissing);
}

#[test]
fn blocking_diagnostic_of_the_rule_refuses_approval_but_not_rejection() {
    let rb = rulebook(&[rule("bad.type", MANUAL, "eligible", bad_expr())]);
    let d = refusal(plan_review(
        &rb,
        &ApprovalLedger::empty(),
        &request(&rb, "bad.type", ReviewAction::Approve),
        &sources(),
    ));
    assert_eq!(d.code(), Code::ReviewBlockingDiagnostic);
    assert!(plan_review(
        &rb,
        &ApprovalLedger::empty(),
        &request(&rb, "bad.type", reject_action()),
        &sources()
    )
    .is_ok());
}

#[test]
fn blocking_diagnostic_of_another_rule_does_not_refuse() {
    let rb = rulebook(&[
        rule("elig.a", MANUAL, "ea", good_expr()),
        rule("bad.type", MANUAL, "eb", bad_expr()),
    ]);
    assert!(plan_review(
        &rb,
        &ApprovalLedger::empty(),
        &request(&rb, "elig.a", ReviewAction::Approve),
        &sources()
    )
    .is_ok());
}

#[test]
fn refusal_carries_no_bytes_and_leaves_inputs_unchanged() {
    let rb = basic();
    let before = print_ledger(&ApprovalLedger::empty());
    let mut req = request(&rb, "elig.basic", ReviewAction::Approve);
    req.content_hash = sha256(b"x");
    let ledger = ApprovalLedger::empty();
    assert!(plan_review(&rb, &ledger, &req, &sources()).is_err());
    assert_eq!(print_ledger(&ledger), before);
    assert_eq!(rb, basic());
}

#[path = "tests_adopt.rs"]
mod adopt_tests;
