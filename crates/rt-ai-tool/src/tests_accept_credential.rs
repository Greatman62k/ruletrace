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

const SHORT: &str = "sk-canary-Zq7x93LmN";
const REALISTIC: &str = "sk-proj-Ab3dE_fG7hJ-kL9mN2pQ_rS5tU8vW-xY1zA4bC_dE6fG9hJ-kL2mN5pQ_rS8tU1vW-xY4zA7bC_dE0fG3hJ-kL6mN9pQ_rS2tU5vW8x";
const DOTTED: &str = "aaa111.bbb222.ccc333";
const BRACKETED: &str = "tok[abc]xyz9";
const ESCAPABLE: &str = "k\"e\\y-δζξ-Ω9\"q\\w-εη7";

/// Every canary shape that a decoder or checker could echo differently.
fn canaries() -> Vec<(&'static str, &'static str)> {
    vec![
        ("short", SHORT),
        ("realistic", REALISTIC),
        ("dotted", DOTTED),
        ("bracketed", BRACKETED),
        ("escapable", ESCAPABLE),
    ]
}

/// The canary as it must be written inside a JSON string.
fn js(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

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

fn run(canary: &str, payload: &str, completion: Completion) -> Result<(), Vec<Diagnostic>> {
    let id = rid();
    let rb = rulebook();
    let all = models();
    let req = sha256(b"request-bytes");
    let clauses = vec![ClauseId {
        document: doc("rules.md"),
        start: 12,
        end: 27,
    }];
    let redactor = Redactor::new(&Credential::new(canary.to_owned()).unwrap());
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

// The first run of at least 8 characters of the canary, or the whole
// canary when it is shorter, that any output text contains. A naive scan
// that shares no code with the production Redactor. The canary is also
// tried as `Debug` writes it, because Debug escapes quotes and backslashes.
fn leaked_window(canary: &str, found: &[Diagnostic]) -> Option<String> {
    let debug = format!("{canary:?}");
    let forms = [canary.to_owned(), debug[1..debug.len() - 1].to_owned()];
    let texts = rejected_text(found);
    for form in forms {
        let chars: Vec<char> = form.chars().collect();
        let width = chars.len().min(8);
        for start in 0..=(chars.len() - width) {
            let window: String = chars[start..start + width].iter().collect();
            if texts.iter().any(|t| t.contains(&window)) {
                return Some(window);
            }
        }
    }
    None
}

fn assert_credential_free(label: &str, canary: &str, found: &[Diagnostic]) {
    let window = leaked_window(canary, found);
    assert!(window.is_none(), "{label}: {window:?} in {found:?}");
}

// True when the path is built only from names that RuleTrace itself
// writes: `response`, `payload`, `proposals[n]`, `evidence[n]`, `rule`,
// and the fixed member names of a proposal.
fn is_structural(field: &str) -> bool {
    let index = |s: &str, name: &str| {
        s.strip_prefix(name)
            .and_then(|r| r.strip_prefix('['))
            .and_then(|r| r.strip_suffix(']'))
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    };
    field.split('.').all(|seg| {
        [
            "response",
            "payload",
            "rule",
            "kind",
            "proposer",
            "rationale",
            "expr",
            "node",
            "document",
            "quote",
        ]
        .contains(&seg)
            || index(seg, "proposals")
            || index(seg, "evidence")
            || index(seg, "outcome")
    })
}

fn leak_positions(c: &str) -> Vec<(&'static str, String)> {
    let j = js(c);
    let key = |extra: String| wrap(&[proposal(&extra, &good_rule())]);
    let in_rule = |rule: String| wrap(&[proposal("", &rule)]);
    vec![
        ("unknown proposal key", key(format!(r#""{j}":1,"#))),
        (
            "unknown response key",
            format!(
                r#"{{"schema_version":"1","{j}":1,"proposals":[{}]}}"#,
                proposal("", &good_rule())
            ),
        ),
        (
            "unknown proposal kind",
            wrap(&[proposal("", &good_rule()).replace("rule_suggestion", &j)]),
        ),
        (
            "unknown rule kind",
            in_rule(rule_text("elig.new", &j, EXPR, "")),
        ),
        (
            "unknown expression node",
            in_rule(rule_text(
                "elig.new",
                "eligibility",
                &EXPR.replace(r#""node":"cmp""#, &format!(r#""node":"{j}""#)),
                "",
            )),
        ),
        (
            "unknown rule key",
            in_rule(rule_text(
                "elig.new",
                "eligibility",
                EXPR,
                &format!(r#""{j}":1,"#),
            )),
        ),
        (
            "unknown expression key",
            in_rule(rule_text(
                "elig.new",
                "eligibility",
                &EXPR.replace(r#""op":"ge""#, &format!(r#""op":"ge","{j}":1"#)),
                "",
            )),
        ),
        (
            "document id",
            wrap(&[proposal("", &good_rule()).replace(
                r#""document":"rules.md""#,
                &format!(r#""document":"{j}.md""#),
            )]),
        ),
        (
            "duplicate proposal key",
            key(format!(r#""{j}":1,"{j}":2,"#)),
        ),
        (
            "duplicate rule key",
            in_rule(rule_text(
                "elig.new",
                "eligibility",
                EXPR,
                &format!(r#""{j}":1,"{j}":2,"#),
            )),
        ),
    ]
}

#[test]
fn the_realistic_canary_is_longer_than_the_echo_limit() {
    assert!(REALISTIC.chars().count() >= 100);
    assert!(REALISTIC.starts_with("sk-proj-"));
}

#[test]
fn no_rejection_position_echoes_any_part_of_the_credential() {
    let mut leaking = Vec::new();
    for (name, canary) in canaries() {
        for (label, payload) in leak_positions(canary) {
            let found = run(canary, &payload, Completion::Complete).expect_err(label);
            if leaked_window(canary, &found).is_some() {
                leaking.push(format!("{name}/{label}"));
            }
        }
    }
    assert!(leaking.is_empty(), "positions that echo: {leaking:?}");
}

#[test]
fn a_credential_in_any_rejected_position_gives_one_refusal_without_it() {
    for (name, canary) in canaries() {
        for (label, payload) in leak_positions(canary) {
            let at_label = format!("{name}/{label}");
            let found = run(canary, &payload, Completion::Complete).expect_err(&at_label);
            assert_eq!(found.len(), 1, "{at_label}: {found:?}");
            assert_eq!(
                found[0].code(),
                Code::AiResponseCredentialEcho,
                "{at_label}"
            );
            assert_eq!(
                found[0].code().exit_code(),
                ExitCode::Response,
                "{at_label}"
            );
            let Location::Input(at) = found[0].primary() else {
                panic!("{at_label}: not an input location");
            };
            let field = at.field.as_deref().expect(&at_label);
            assert!(is_structural(field), "{at_label}: field {field:?}");
        }
    }
}

#[test]
fn the_named_field_is_the_nearest_structural_ancestor() {
    for (name, canary) in canaries() {
        let payload = wrap(&[proposal(&format!(r#""{}":1,"#, js(canary)), &good_rule())]);
        let found = run(canary, &payload, Completion::Complete).unwrap_err();
        let Location::Input(at) = found[0].primary() else {
            panic!("{name}: not an input location");
        };
        assert_eq!(at.field.as_deref(), Some("proposals[0]"), "{name}");
    }
}

#[test]
fn the_structural_check_rejects_a_field_cut_inside_a_key() {
    assert!(is_structural("proposals[0].evidence[2].quote"));
    assert!(!is_structural("proposals[0].aaa111.bbb222"));
    assert!(!is_structural("proposals[0].tok"));
    assert!(!is_structural("proposals[x]"));
}

#[test]
fn the_window_scan_detects_a_partial_echo() {
    let d = Diagnostic::new(
        Code::AiResponseSchemaViolation,
        format!("unknown proposal kind '{}...'", &REALISTIC[..60]),
        Location::None(Code::AiResponseSchemaViolation),
    );
    assert!(leaked_window(REALISTIC, &[d]).is_some());
    let clean = Diagnostic::new(
        Code::AiResponseSchemaViolation,
        "unknown proposal kind at proposals[0]".to_owned(),
        Location::None(Code::AiResponseSchemaViolation),
    );
    assert!(leaked_window(REALISTIC, &[clean]).is_none());
}

#[test]
fn rejections_that_do_not_echo_stay_credential_free_and_keep_their_code() {
    let rule = good_rule();
    for (name, canary) in canaries() {
        let j = js(canary);
        let cases: Vec<(&str, String, Completion, Code)> = vec![
            (
                "forbidden field value",
                wrap(&[proposal(&format!(r#""status":"{j}","#), &rule)]),
                Completion::Complete,
                Code::AiResponseForbiddenField,
            ),
            (
                "bad rule id",
                wrap(&[proposal(
                    "",
                    &rule_text(&format!("{j} bad"), "eligibility", EXPR, ""),
                )]),
                Completion::Complete,
                Code::AiResponseSchemaViolation,
            ),
            (
                "claimed hash",
                wrap(&[proposal("", &rule)]).replace(RULES_HASH, &j),
                Completion::Complete,
                Code::AiResponseSchemaViolation,
            ),
            (
                "wrong type",
                wrap(&[proposal("", &rule)]).replace(
                    r#""start":12,"end":27,"line":2,"column":1,"quote""#,
                    &format!(r#""start":"{j}","end":27,"line":2,"column":1,"quote""#),
                ),
                Completion::Complete,
                Code::AiResponseSchemaViolation,
            ),
            (
                "quote mismatch",
                wrap(&[proposal("", &rule)]).replace(QUOTE, &j),
                Completion::Complete,
                Code::AiProposalQuoteMismatch,
            ),
            (
                "malformed payload",
                format!(r#"{{"schema_version":"1","proposals":["{j}"#),
                Completion::Complete,
                Code::AiResponseMalformed,
            ),
            (
                "bad schema version",
                wrap(&[proposal("", &rule)]).replace(
                    r#""schema_version":"1""#,
                    &format!(r#""schema_version":"{j}""#),
                ),
                Completion::Complete,
                Code::AiResponseSchemaViolation,
            ),
            (
                "refusal",
                wrap(&[proposal("", &rule)]).replace("why", &j),
                Completion::Refused,
                Code::AiResponseIncomplete,
            ),
        ];
        for (label, payload, completion, expected) in cases {
            let at_label = format!("{name}/{label}");
            let found = run(canary, &payload, completion).expect_err(&at_label);
            assert_credential_free(&at_label, canary, &found);
            assert_eq!(found.len(), 1, "{at_label}: {found:?}");
            assert_eq!(found[0].code(), expected, "{at_label}: {found:?}");
        }
    }
}
