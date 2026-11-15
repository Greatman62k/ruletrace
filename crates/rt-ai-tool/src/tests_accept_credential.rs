//! The credential never appears in a rejected batch's Diagnostics, whatever
//! stage rejects it (Req 28.9; steering: no canary in any output). Each
//! position places the canary where a decoder or checker might echo it.

use crate::accept::{accept_batch, AcceptInput};
use crate::credential::Credential;
use crate::out::render_diagnostic;
use crate::plan::ClauseId;
use crate::redact::Redactor;
use crate::tests_provenance::{doc, models, RULES_HASH};
use crate::tests_response::{rid, wrap, LIMIT};
use rt_ai_adapter::Completion;
use rt_foundation::{Code, Diagnostic, ExitCode, InputId, Location};
use rt_hash::sha256;
use rt_ir::parse_rulebook;

const CANARY: &str = "sk-canary-Zq7x93LmN";
const QUOTE: &str = "Gamma δ delta.";
const SPAN: &str = r#"{"document":"rules.md","start":12,"end":27,"line":2,"column":1}"#;

fn rulebook() -> rt_ir::Rulebook {
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

fn evidence() -> String {
    format!(
        r#"{{"document":"rules.md","sha256":"{RULES_HASH}","start":12,"end":27,"line":2,"column":1,"quote":"{QUOTE}"}}"#
    )
}

const EXPR: &str = r#"{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}}"#;

fn rule_text(id: &str, kind: &str, expr: &str, extra: &str) -> String {
    format!(
        r#"{{"id":"{id}",{extra}"kind":"{kind}","target":"eligible2","type":"boolean","expr":{expr},"markers":[],"spans":[{SPAN}]}}"#
    )
}

fn good_rule() -> String {
    rule_text("elig.new", "eligibility", EXPR, "")
}

fn proposal(extra: &str, rule: &str) -> String {
    format!(
        r#"{{"kind":"rule_suggestion","proposer":"model-1","rationale":"why",{extra}"evidence":[{}],"rule":{rule}}}"#,
        evidence()
    )
}

fn run(payload: &str, completion: Completion) -> Result<(), Vec<Diagnostic>> {
    let id = rid();
    let rb = rulebook();
    let all = models();
    let req = sha256(b"request-bytes");
    let clauses = vec![ClauseId {
        document: doc("rules.md"),
        start: 12,
        end: 27,
    }];
    let redactor = Redactor::new(&Credential::new(CANARY.to_owned()).unwrap());
    accept_batch(&AcceptInput {
        response_id: &id,
        request_id: &req,
        completion,
        payload: payload.as_bytes(),
        response_limit: LIMIT,
        models: &all,
        clauses: &clauses,
        declarations: &rb,
        redactor: &redactor,
    })
    .map(|_| ())
}

fn rejected_text(found: &[Diagnostic]) -> Vec<String> {
    let mut texts = vec![format!("{found:?}")];
    for d in found {
        texts.push(format!("{d:?}"));
        texts.push(d.message().to_owned());
        if let Location::Input(at) = d.primary() {
            texts.push(at.field.clone().unwrap_or_default());
        }
        texts.push(render_diagnostic(d));
    }
    texts
}

fn leaks(found: &[Diagnostic]) -> bool {
    rejected_text(found).iter().any(|t| t.contains(CANARY))
}

fn assert_credential_free(label: &str, found: &[Diagnostic]) {
    assert!(!leaks(found), "{label}: {found:?}");
}

fn leak_positions() -> Vec<(&'static str, String)> {
    let key = |extra: String| wrap(&[proposal(&extra, &good_rule())]);
    let in_rule = |rule: String| wrap(&[proposal("", &rule)]);
    vec![
        ("unknown proposal key", key(format!(r#""{CANARY}":1,"#))),
        (
            "unknown response key",
            format!(
                r#"{{"schema_version":"1","{CANARY}":1,"proposals":[{}]}}"#,
                proposal("", &good_rule())
            ),
        ),
        (
            "unknown proposal kind",
            wrap(&[proposal("", &good_rule()).replace("rule_suggestion", CANARY)]),
        ),
        (
            "unknown rule kind",
            in_rule(rule_text("elig.new", CANARY, EXPR, "")),
        ),
        (
            "unknown expression node",
            in_rule(rule_text(
                "elig.new",
                "eligibility",
                &EXPR.replace(r#""node":"cmp""#, &format!(r#""node":"{CANARY}""#)),
                "",
            )),
        ),
        (
            "unknown rule key",
            in_rule(rule_text(
                "elig.new",
                "eligibility",
                EXPR,
                &format!(r#""{CANARY}":1,"#),
            )),
        ),
        (
            "unknown expression key",
            in_rule(rule_text(
                "elig.new",
                "eligibility",
                &EXPR.replace(r#""op":"ge""#, &format!(r#""op":"ge","{CANARY}":1"#)),
                "",
            )),
        ),
        (
            "document id",
            wrap(&[proposal("", &good_rule()).replace(
                r#""document":"rules.md""#,
                &format!(r#""document":"{CANARY}.md""#),
            )]),
        ),
        (
            "duplicate proposal key",
            key(format!(r#""{CANARY}":1,"{CANARY}":2,"#)),
        ),
        (
            "duplicate rule key",
            in_rule(rule_text(
                "elig.new",
                "eligibility",
                EXPR,
                &format!(r#""{CANARY}":1,"{CANARY}":2,"#),
            )),
        ),
    ]
}

#[test]
fn no_rejection_position_echoes_the_credential() {
    let mut leaking = Vec::new();
    for (label, payload) in leak_positions() {
        let found = run(&payload, Completion::Complete).expect_err(label);
        if leaks(&found) {
            leaking.push(label);
        }
    }
    assert!(leaking.is_empty(), "positions that echo: {leaking:?}");
}

#[test]
fn a_credential_in_any_rejected_position_gives_one_refusal_without_it() {
    for (label, payload) in leak_positions() {
        let found = run(&payload, Completion::Complete).expect_err(label);
        assert_eq!(found.len(), 1, "{label}: {found:?}");
        assert_eq!(found[0].code(), Code::AiResponseCredentialEcho, "{label}");
        assert_eq!(found[0].code().exit_code(), ExitCode::Response, "{label}");
        let Location::Input(at) = found[0].primary() else {
            panic!("{label}: not an input location");
        };
        assert!(at.field.is_some(), "{label}");
    }
}

#[test]
fn the_named_field_is_the_nearest_credential_free_ancestor() {
    let payload = wrap(&[proposal(&format!(r#""{CANARY}":1,"#), &good_rule())]);
    let found = run(&payload, Completion::Complete).unwrap_err();
    let Location::Input(at) = found[0].primary() else {
        panic!("not an input location");
    };
    assert_eq!(at.field.as_deref(), Some("proposals[0]"));
}

#[test]
fn rejections_that_do_not_echo_stay_credential_free_and_keep_their_code() {
    let rule = good_rule();
    let cases: Vec<(&str, String, Completion, Code)> = vec![
        (
            "forbidden field value",
            wrap(&[proposal(&format!(r#""status":"{CANARY}","#), &rule)]),
            Completion::Complete,
            Code::AiResponseForbiddenField,
        ),
        (
            "bad rule id",
            wrap(&[proposal(
                "",
                &rule_text(&format!("{CANARY} bad"), "eligibility", EXPR, ""),
            )]),
            Completion::Complete,
            Code::AiResponseSchemaViolation,
        ),
        (
            "claimed hash",
            wrap(&[proposal("", &rule)]).replace(RULES_HASH, CANARY),
            Completion::Complete,
            Code::AiResponseSchemaViolation,
        ),
        (
            "wrong type",
            wrap(&[proposal("", &rule)]).replace(
                r#""start":12,"end":27,"line":2,"column":1,"quote""#,
                &format!(r#""start":"{CANARY}","end":27,"line":2,"column":1,"quote""#),
            ),
            Completion::Complete,
            Code::AiResponseSchemaViolation,
        ),
        (
            "quote mismatch",
            wrap(&[proposal("", &rule)]).replace(QUOTE, CANARY),
            Completion::Complete,
            Code::AiProposalQuoteMismatch,
        ),
        (
            "malformed payload",
            format!(r#"{{"schema_version":"1","proposals":["{CANARY}"#),
            Completion::Complete,
            Code::AiResponseMalformed,
        ),
        (
            "bad schema version",
            wrap(&[proposal("", &rule)]).replace(
                r#""schema_version":"1""#,
                &format!(r#""schema_version":"{CANARY}""#),
            ),
            Completion::Complete,
            Code::AiResponseSchemaViolation,
        ),
        (
            "refusal",
            wrap(&[proposal("", &rule)]).replace("why", CANARY),
            Completion::Refused,
            Code::AiResponseIncomplete,
        ),
    ];
    for (label, payload, completion, expected) in cases {
        let found = run(&payload, completion).expect_err(label);
        assert_credential_free(label, &found);
        assert_eq!(found.len(), 1, "{label}: {found:?}");
        assert_eq!(found[0].code(), expected, "{label}: {found:?}");
    }
}
