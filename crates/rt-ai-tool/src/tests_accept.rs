//! Tests of the accepted batch (design 3.7.4; Req 23.8, 23.10, 30.1 to
//! 30.5, 30.7, 30.9, 22.16; UD-26, UD-36). Payloads are written by hand
//! and every expected code, severity and location is stated, not derived.

use crate::accept::{accept_batch, AcceptInput, AcceptedBatch};
use crate::credential::Credential;
use crate::out::render_diagnostic;
use crate::plan::ClauseId;
use crate::redact::Redactor;
use crate::tests_provenance::{doc, models, RULES_HASH};
use crate::tests_response::{rid, wrap, LIMIT};
use rt_ai_adapter::Completion;
use rt_foundation::{Code, Diagnostic, ExitCode, InputId, Location, Severity};
use rt_hash::{sha256, Sha256Hex};
use rt_ir::{draft_hash, parse_rulebook, Rulebook};
use rt_proposals::{
    parse_batch, print_batch, Batch, Completion as StoredCompletion, ProposalKind, StoredLocation,
    StoredResponse,
};

const CANARY: &str = "sk-canary-Zq7x93LmN";
const QUOTE: &str = "Gamma δ delta.";
const SPAN: &str = r#"{"document":"rules.md","start":12,"end":27,"line":2,"column":1}"#;

fn rulebook() -> Rulebook {
    let text = format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"n","version":"1","documents":[{{"id":"rules.md","sha256":"{RULES_HASH}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[],"precedence":[]}}"#
    );
    parse_rulebook(
        text.as_bytes(),
        &InputId::new("rb.json").unwrap(),
        8 << 20,
        64,
    )
    .map(|s| s.into_parts().0)
    .map_err(|r| r.into_diagnostics().as_slice().to_vec())
    .unwrap()
}

fn request() -> Sha256Hex {
    sha256(b"request-bytes")
}

fn clauses() -> Vec<ClauseId> {
    vec![ClauseId {
        document: doc("rules.md"),
        start: 12,
        end: 27,
    }]
}

fn redactor() -> Redactor {
    Redactor::new(&Credential::new(CANARY.to_owned()).unwrap())
}

fn evidence_with(quote: &str) -> String {
    format!(
        r#"{{"document":"rules.md","sha256":"{RULES_HASH}","start":12,"end":27,"line":2,"column":1,"quote":"{quote}"}}"#
    )
}

fn advisory(kind: &str) -> String {
    format!(
        r#"{{"kind":"{kind}","proposer":"model-1","rationale":"why","evidence":[{}]}}"#,
        evidence_with(QUOTE)
    )
}

fn rule_with(kind_and_body: &str) -> String {
    format!(r#"{{"id":"elig.new",{kind_and_body},"markers":[],"spans":[{SPAN}]}}"#)
}

fn eligibility(field: &str) -> String {
    rule_with(&format!(
        r#""kind":"eligibility","target":"eligible2","type":"boolean","expr":{{"node":"cmp","t":"boolean","op":"ge","left":{{"node":"field","t":"number","name":"{field}"}},"right":{{"node":"num","t":"number","value":"16"}}}}"#
    ))
}

fn unsupported() -> String {
    rule_with(
        r#""kind":"unrecognized","name":"rank_reference","detail":"the top four qualify","target":"eligible2","type":"boolean""#,
    )
}

fn suggestion_with(rule: &str, rationale: &str, quote: &str) -> String {
    format!(
        r#"{{"kind":"rule_suggestion","proposer":"model-1","rationale":"{rationale}","evidence":[{}],"rule":{rule}}}"#,
        evidence_with(quote)
    )
}

fn suggestion(rule: &str) -> String {
    suggestion_with(rule, "why", QUOTE)
}

fn accept_with(
    payload: &str,
    completion: Completion,
    redactor: &Redactor,
) -> Result<AcceptedBatch, Vec<Diagnostic>> {
    let id = rid();
    let rb = rulebook();
    let all = models();
    let req = request();
    let cl = clauses();
    accept_batch(&AcceptInput {
        response_id: &id,
        request_id: &req,
        completion,
        payload: payload.as_bytes(),
        response_limit: LIMIT,
        models: &all,
        clauses: &cl,
        declarations: &rb,
        redactor,
    })
}

fn accept(payload: &str) -> Result<AcceptedBatch, Vec<Diagnostic>> {
    accept_with(payload, Completion::Complete, &Redactor::no_credential())
}

fn accepted(payload: &str) -> AcceptedBatch {
    accept(payload).unwrap_or_else(|d| panic!("rejected: {d:?}"))
}

fn rejected(payload: &str) -> Vec<Diagnostic> {
    match accept(payload) {
        Ok(_) => panic!("expected a rejection of {payload}"),
        Err(d) => d,
    }
}

fn only_code(found: &[Diagnostic], code: Code, exit: ExitCode) {
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code(), code);
    assert_eq!(found[0].severity(), Severity::Error);
    assert_eq!(code.exit_code(), exit);
}

#[test]
fn a_valid_rule_suggestion_is_stored_with_no_findings() {
    let text = wrap(&[suggestion(&eligibility("age"))]);
    let batch = accepted(&text);
    assert_eq!(batch.proposals().len(), 1);
    let p = &batch.proposals()[0];
    assert_eq!(p.kind, ProposalKind::RuleSuggestion);
    assert!(p.outcome.is_empty(), "{:?}", p.outcome);
    assert!(!batch.has_blocking_findings());
    assert!(batch.blocking_warning().is_none());
    assert_eq!(batch.payload(), text);
    assert_eq!(batch.response_sha256(), &sha256(text.as_bytes()));
    assert_eq!(p.proposal_id, sha256(format!("{}:0", request()).as_bytes()));
    let (draft, hash) = p.rule.as_ref().unwrap();
    assert_eq!(draft.id.as_str(), "elig.new");
    let rb = rulebook();
    let content = rt_ir::parse_draft(
        draft.json.as_bytes(),
        &rid(),
        rt_foundation::RevisionNumber::FIRST,
        rt_ir::Origin::Manual { author: None },
        LIMIT,
        32,
    )
    .unwrap()
    .into_parts()
    .0
    .content;
    let declared = draft_hash(&rb.ir_version, &draft.id, &content, &rb.documents).unwrap();
    assert_eq!(hash, &declared);
}

#[test]
fn a_well_formed_unsupported_clause_is_kept_as_a_non_executable_proposal() {
    let batch = accepted(&wrap(&[suggestion(&unsupported())]));
    let p = &batch.proposals()[0];
    assert_eq!(p.outcome.len(), 1, "{:?}", p.outcome);
    let finding = &p.outcome[0];
    assert_eq!(finding.code, Code::ValUnsupportedConstruct);
    assert_eq!(finding.code.severity(), Severity::Unsupported);
    assert_eq!(
        finding.location,
        StoredLocation::Source {
            document: doc("rules.md"),
            start: 12,
            end: 27
        }
    );
    assert!(p.rule.is_some());
    assert!(batch.has_blocking_findings());
}

#[test]
fn blocking_findings_give_one_warning_that_does_not_change_the_exit_code() {
    let batch = accepted(&wrap(&[
        suggestion(&unsupported()),
        suggestion(&eligibility("missing")),
    ]));
    let warning = batch.blocking_warning().unwrap();
    assert_eq!(warning.code(), Code::AiBatchHasBlockingProposals);
    assert_eq!(warning.severity(), Severity::Warning);
    assert_eq!(warning.code().exit_code(), ExitCode::Success);
}

#[test]
fn an_invalid_rule_is_stored_with_its_finding_and_the_batch_is_accepted() {
    let batch = accepted(&wrap(&[
        suggestion(&eligibility("age")),
        suggestion(&eligibility("missing")),
    ]));
    assert!(batch.proposals()[0].outcome.is_empty());
    let codes: Vec<Code> = batch.proposals()[1]
        .outcome
        .iter()
        .map(|d| d.code)
        .collect();
    assert_eq!(codes, vec![Code::ValUndeclaredReference]);
    assert!(batch.has_blocking_findings());
}

#[test]
fn an_ambiguity_marker_is_a_stored_blocking_finding() {
    let rule = rule_with(
        r#""kind":"eligibility","target":"eligible2","type":"boolean","expr":{"node":"bool","t":"boolean","value":true}"#,
    )
    .replace(
        r#""markers":[]"#,
        r#""markers":[{"kind":"ambiguous","interpretations":["a","b"]}]"#,
    );
    let batch = accepted(&wrap(&[suggestion(&rule)]));
    let outcome = &batch.proposals()[0].outcome;
    assert_eq!(outcome.len(), 1, "{outcome:?}");
    assert_eq!(outcome[0].code, Code::ValAmbiguousInterpretation);
    assert_eq!(outcome[0].code.severity(), Severity::Ambiguity);
}

#[test]
fn advisory_kinds_create_no_draft_and_no_findings() {
    let kinds = [
        "clause_interpretation",
        "clause_relationship",
        "conflict_highlight",
        "candidate_test",
    ];
    let mut items: Vec<String> = kinds.iter().map(|k| advisory(k)).collect();
    items.push(suggestion(&eligibility("age")));
    let batch = accepted(&wrap(&items));
    assert_eq!(batch.proposals().len(), 5);
    let with_draft = batch.proposals().iter().filter(|p| p.rule.is_some());
    assert_eq!(with_draft.count(), 1);
    for p in &batch.proposals()[..4] {
        assert!(p.rule.is_none());
        assert!(p.outcome.is_empty());
    }
    assert!(batch.proposals()[4].rule.is_some());
}

#[test]
fn a_stored_batch_round_trips_through_the_store_format() {
    let text = wrap(&[
        advisory("candidate_test"),
        suggestion(&unsupported()),
        suggestion(&eligibility("age")),
    ]);
    let a = accepted(&text);
    let batch = Batch {
        request_id: a.request_id().clone(),
        response_sha256: a.response_sha256().clone(),
        response: StoredResponse {
            completion: a.completion(),
            payload: a.payload().to_owned(),
        },
        provider: "mock".to_owned(),
        model: "m1".to_owned(),
        adapter_version: "0.1".to_owned(),
        ai_identities: vec![rt_ir::Identity::new("ai-1").unwrap()],
        created_at: "2026-11-14T08:00:00Z".to_owned(),
        proposals: a.proposals().to_vec(),
    };
    let printed = print_batch(&batch);
    let parsed = parse_batch(&InputId::new("store").unwrap(), printed.as_bytes()).unwrap();
    assert_eq!(parsed, batch);
    assert_eq!(parsed.response.completion, StoredCompletion::Complete);
}

#[test]
fn the_quote_one_byte_off_rejects_the_whole_batch_and_keeps_nothing() {
    let good = suggestion(&eligibility("age"));
    let unsupported_ok = suggestion(&unsupported());
    let off = suggestion_with(&eligibility("age"), "why", "Gamma δ delta!");
    let found = rejected(&wrap(&[good, unsupported_ok, off]));
    only_code(&found, Code::AiProposalQuoteMismatch, ExitCode::Blocked);
    assert_eq!(found[0].code().id(), "RT-PRV-0104");
}

#[test]
fn every_protocol_failure_rejects_the_whole_batch_with_its_own_code() {
    let good = suggestion(&eligibility("age"));
    let wrong_hash = good.replace(RULES_HASH, &"0".repeat(64));
    let found = rejected(&wrap(&[good.clone(), wrong_hash]));
    only_code(&found, Code::AiProposalHashMismatch, ExitCode::Blocked);

    let unknown_field = good.replace(r#""rationale":"why""#, r#""rationale":"why","extra":1"#);
    let found = rejected(&wrap(&[good.clone(), unknown_field]));
    only_code(&found, Code::AiResponseSchemaViolation, ExitCode::Response);

    let forbidden = good.replace(r#""rationale":"why""#, r#""rationale":"why","status":"ok""#);
    let found = rejected(&wrap(&[good.clone(), forbidden]));
    only_code(&found, Code::AiResponseForbiddenField, ExitCode::Response);

    let found = rejected("{\"schema_version\":\"1\",\"proposals\":[");
    only_code(&found, Code::AiResponseMalformed, ExitCode::Response);

    let text = wrap(&[good]);
    let found = accept_with(&text, Completion::Refused, &Redactor::no_credential()).unwrap_err();
    only_code(&found, Code::AiResponseIncomplete, ExitCode::Response);
}

#[test]
fn a_provider_supplied_flag_is_rejected_whatever_its_value() {
    for flag in ["approval", "has_errors", "no_errors", "validation_status"] {
        let good = suggestion(&eligibility("age"));
        let flagged = good.replace(
            r#""rationale":"why""#,
            &format!(r#""rationale":"why","{flag}":false"#),
        );
        let found = rejected(&wrap(&[good, flagged]));
        only_code(&found, Code::AiResponseForbiddenField, ExitCode::Response);
    }
}

fn canary_proposal(rationale: &str) -> String {
    suggestion_with(&eligibility("age"), rationale, QUOTE)
}

fn assert_refused(found: &[Diagnostic], field: &str) {
    only_code(found, Code::AiResponseCredentialEcho, ExitCode::Response);
    assert_eq!(found[0].code().id(), "RT-AI-0206");
    let Location::Input(at) = found[0].primary() else {
        panic!("not an input location");
    };
    assert_eq!(at.field.as_deref(), Some(field));
    let shown = [
        format!("{found:?}"),
        found[0].message().to_owned(),
        render_diagnostic(&found[0]),
    ];
    for text in shown {
        assert!(!text.contains(CANARY), "{text}");
    }
}

fn accept_canary(payload: &str) -> Vec<Diagnostic> {
    accept_with(payload, Completion::Complete, &redactor()).unwrap_err()
}

#[test]
fn a_payload_with_the_credential_is_refused_at_start_middle_and_end() {
    let plain = canary_proposal("why");
    let first = plain.replace(
        r#""proposer":"model-1""#,
        &format!(r#""proposer":"{CANARY}""#),
    );
    let found = accept_canary(&wrap(&[first, plain.clone()]));
    assert_refused(&found, "payload");

    let middle = canary_proposal(&format!("before {CANARY} after"));
    let found = accept_canary(&wrap(&[plain.clone(), middle, plain.clone()]));
    assert_refused(&found, "payload");

    let last = canary_proposal(&format!("ends with {CANARY}"));
    let found = accept_canary(&wrap(&[plain, last]));
    assert_refused(&found, "payload");
}

#[test]
fn the_bearer_form_and_the_escaped_form_are_refused() {
    let found = accept_canary(&wrap(&[canary_proposal(&format!("Bearer {CANARY}"))]));
    assert_refused(&found, "payload");

    let quoted = Redactor::new(&Credential::new("ab\"cd".to_owned()).unwrap());
    let text = wrap(&[canary_proposal(r#"x ab\"cd y"#)]);
    let found = accept_with(&text, Completion::Complete, &quoted).unwrap_err();
    only_code(&found, Code::AiResponseCredentialEcho, ExitCode::Response);
}

#[test]
fn a_credential_spelled_with_json_escapes_is_found_in_the_decoded_text() {
    let escaped = CANARY.replace("Zq7", r"\u005aq\u0037");
    assert!(!escaped.contains(CANARY));
    let text = wrap(&[canary_proposal(&format!("see {escaped}"))]);
    assert!(!text.contains(CANARY));
    let found = accept_canary(&text);
    assert_refused(&found, "proposals[0].rationale");

    let plain = canary_proposal("why");
    let proposer = plain.replace(
        r#""proposer":"model-1""#,
        &format!(r#""proposer":"{escaped}""#),
    );
    let found = accept_canary(&wrap(&[plain, proposer]));
    assert_refused(&found, "proposals[1].proposer");
}

#[test]
fn a_payload_without_the_credential_is_accepted_with_a_credential_configured() {
    let text = wrap(&[canary_proposal("sk-canary is only a prefix")]);
    let batch = accept_with(&text, Completion::Complete, &redactor()).unwrap();
    assert_eq!(batch.proposals().len(), 1);
}

#[test]
fn provenance_is_reported_before_the_credential_check() {
    let off = suggestion_with(&eligibility("age"), CANARY, "Gamma δ delta!");
    let found = accept_with(&wrap(&[off]), Completion::Complete, &redactor()).unwrap_err();
    only_code(&found, Code::AiProposalQuoteMismatch, ExitCode::Blocked);
    assert!(!format!("{found:?}").contains(CANARY));
}

#[test]
fn a_schema_failure_is_reported_before_the_credential_check() {
    let bad = canary_proposal(CANARY).replace(r#""kind":"rule_suggestion""#, r#""kind":"nope""#);
    let found = accept_with(&wrap(&[bad]), Completion::Complete, &redactor()).unwrap_err();
    only_code(&found, Code::AiResponseSchemaViolation, ExitCode::Response);
    assert!(!format!("{found:?}").contains(CANARY));
}

#[test]
fn a_credential_echoed_by_a_stored_finding_is_refused_at_the_finding() {
    let echoing = Redactor::new(&Credential::new("undeclared field".to_owned()).unwrap());
    let text = wrap(&[
        suggestion(&eligibility("age")),
        suggestion(&eligibility("missing")),
    ]);
    let found = accept_with(&text, Completion::Complete, &echoing).unwrap_err();
    only_code(&found, Code::AiResponseCredentialEcho, ExitCode::Response);
    let Location::Input(at) = found[0].primary() else {
        panic!("not an input location");
    };
    assert_eq!(at.field.as_deref(), Some("proposals[1].outcome[0]"));
    assert!(!format!("{found:?}").contains("undeclared field"));
}
