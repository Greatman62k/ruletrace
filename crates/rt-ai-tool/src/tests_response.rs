//! Response parsing and strict schema tests (Req 23.4 to 23.6, 30.9,
//! 22.16; design 3.7.3 and 3.7.4). Payloads are written by hand. Expected
//! columns are found by searching the payload text, not by the parser.

use crate::response::{
    parse_response, ParsedResponse, ProposalKind, FORBIDDEN_FIELDS, MAX_EVIDENCE, MAX_PROPOSALS,
};
use proptest::prelude::*;
use rt_ai_adapter::Completion;
use rt_foundation::{Code, Diagnostic, InputId, Location, Severity};

const LIMIT: u64 = 1 << 20;
const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const DRAFT: &str = r#"{"id":"elig.new","kind":"eligibility","target":"eligible2","type":"boolean","expr":{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}},"markers":[],"spans":[{"document":"rules.md","start":9,"end":59,"line":3,"column":1}]}"#;

fn rid() -> InputId {
    InputId::new("response-1").unwrap()
}

fn evidence() -> String {
    format!(
        r#"{{"document":"rules.md","sha256":"{SHA}","start":9,"end":59,"line":3,"column":1,"quote":"Applicants must be 16."}}"#
    )
}

fn proposal(kind: &str) -> String {
    format!(
        r#"{{"kind":"{kind}","proposer":"model-1","rationale":"why","evidence":[{}]}}"#,
        evidence()
    )
}

fn rule_proposal(draft: &str) -> String {
    format!(
        r#"{{"kind":"rule_suggestion","proposer":"model-1","rationale":"why","evidence":[{}],"rule":{draft}}}"#,
        evidence()
    )
}

fn wrap(proposals: &[String]) -> String {
    format!(
        r#"{{"schema_version":"1","proposals":[{}]}}"#,
        proposals.join(",")
    )
}

fn parse_with(
    completion: Completion,
    bytes: &[u8],
    limit: u64,
) -> Result<ParsedResponse, Diagnostic> {
    parse_response(&rid(), completion, bytes, limit)
}

fn parse(text: &str) -> Result<ParsedResponse, Diagnostic> {
    parse_with(Completion::Complete, text.as_bytes(), LIMIT)
}

fn fail(text: &str) -> Diagnostic {
    match parse(text) {
        Ok(_) => panic!("expected a rejection of {text}"),
        Err(d) => d,
    }
}

/// Code, severity, line, column and field of a diagnostic, after checking
/// that the location is an Input_Location of the response.
fn shape(d: &Diagnostic) -> (Code, Severity, Option<u32>, Option<u32>, Option<String>) {
    match d.primary() {
        Location::Input(l) => {
            assert_eq!(l.input, rid());
            (d.code(), d.severity(), l.line, l.column, l.field.clone())
        }
        other => panic!("not an input location: {other:?}"),
    }
}

/// 1-based column of the first occurrence of `needle` in single-line `text`.
fn col(text: &str, needle: &str) -> Option<u32> {
    let at = text.find(needle).unwrap();
    Some(u32::try_from(text[..at].chars().count()).unwrap() + 1)
}

fn expect(text: &str, code: Code, needle: &str, field: &str) {
    let d = fail(text);
    assert_eq!(
        shape(&d),
        (
            code,
            Severity::Error,
            Some(1),
            col(text, needle),
            Some(field.to_owned())
        ),
        "{}",
        d.message()
    );
}

// ----- valid responses, one per kind -----

#[test]
fn each_advisory_kind_decodes_to_exact_values() {
    for (name, want) in [
        ("clause_interpretation", ProposalKind::ClauseInterpretation),
        ("clause_relationship", ProposalKind::ClauseRelationship),
        ("conflict_highlight", ProposalKind::ConflictHighlight),
        ("candidate_test", ProposalKind::CandidateTest),
    ] {
        let parsed = parse(&wrap(&[proposal(name)])).unwrap();
        assert_eq!(parsed.proposals.len(), 1);
        let p = &parsed.proposals[0];
        assert_eq!(p.kind, want);
        assert_eq!(p.kind.name(), name);
        assert_eq!(p.proposer.as_str(), "model-1");
        assert_eq!(p.rationale, "why");
        assert_eq!(p.evidence.len(), 1);
        let e = &p.evidence[0];
        assert_eq!(e.document.as_str(), "rules.md");
        assert_eq!(e.sha256.as_str(), SHA);
        assert_eq!((e.start, e.end, e.line, e.column), (9, 59, 3, 1));
        assert_eq!(e.quote, "Applicants must be 16.");
    }
}

#[test]
fn a_rule_suggestion_decodes_with_its_draft() {
    let parsed = parse(&wrap(&[rule_proposal(DRAFT)])).unwrap();
    let ProposalKind::RuleSuggestion(rule) = &parsed.proposals[0].kind else {
        panic!("rule suggestion expected");
    };
    assert_eq!(rule.id.as_str(), "elig.new");
    assert_eq!(rule.kind_name, "eligibility");
    assert_eq!(rule.json, DRAFT);
}

#[test]
fn proposals_keep_response_order() {
    let text = wrap(&[proposal("candidate_test"), proposal("clause_relationship")]);
    let kinds: Vec<_> = parse(&text)
        .unwrap()
        .proposals
        .iter()
        .map(|p| p.kind.name())
        .collect();
    assert_eq!(kinds, ["candidate_test", "clause_relationship"]);
}

#[test]
fn an_empty_proposal_list_is_a_valid_response() {
    assert!(parse(&wrap(&[])).unwrap().proposals.is_empty());
}

#[test]
fn an_unrecognized_rule_kind_is_carried_not_rejected() {
    let draft = DRAFT.replace(
        r#""kind":"eligibility","target":"eligible2","type":"boolean","expr":"#,
        r#""kind":"unrecognized","name":"slot_allocation","detail":"top four qualify","target":"eligible2","type":"boolean","expr":"#,
    );
    let draft = draft.replace(
        r#"{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}}"#,
        "{}",
    );
    let draft = draft.replace(r#""expr":{},"#, "");
    let parsed = parse(&wrap(&[rule_proposal(&draft)])).unwrap();
    let ProposalKind::RuleSuggestion(rule) = &parsed.proposals[0].kind else {
        panic!("rule suggestion expected");
    };
    assert_eq!(rule.kind_name, "unrecognized");
    assert!(rule.json.contains(r#""name":"slot_allocation""#));
}

#[test]
fn an_unrecognized_expression_node_is_carried_not_rejected() {
    let draft = DRAFT.replace(
        r#"{"node":"num","t":"number","value":"16"}"#,
        r#"{"node":"unrecognized","name":"rank_reference","detail":"top four"}"#,
    );
    let parsed = parse(&wrap(&[rule_proposal(&draft)])).unwrap();
    let ProposalKind::RuleSuggestion(rule) = &parsed.proposals[0].kind else {
        panic!("rule suggestion expected");
    };
    assert_eq!(rule.kind_name, "eligibility");
    assert!(rule
        .json
        .contains(r#"{"node":"unrecognized","name":"rank_reference","detail":"top four"}"#));
}

#[test]
fn a_response_of_exactly_the_limit_is_accepted() {
    let text = wrap(&[]);
    let n = u64::try_from(text.len()).unwrap();
    assert!(parse_with(Completion::Complete, text.as_bytes(), n).is_ok());
    let d = parse_with(Completion::Complete, text.as_bytes(), n - 1).unwrap_err();
    assert_eq!(d.code(), Code::AiResponseTooLarge);
}

#[test]
fn nesting_of_exactly_thirty_two_is_accepted_and_thirty_three_is_not() {
    let nested = |extra: usize| {
        let open = "[".repeat(extra);
        let close = "]".repeat(extra);
        DRAFT.replace(
            r#""kind":"eligibility","#,
            &format!(r#""kind":"deep_kind","nest":{open}{close},"#),
        )
    };
    // root 1, proposals 2, proposal 3, rule 4, so 28 arrays reach depth 32.
    assert!(parse(&wrap(&[rule_proposal(&nested(28))])).is_ok());
    let text = wrap(&[rule_proposal(&nested(29))]);
    let d = fail(&text);
    let (code, sev, _, _, field) = shape(&d);
    assert_eq!(
        (code, sev),
        (Code::AiResponseSchemaViolation, Severity::Error)
    );
    assert_eq!(field.as_deref(), Some("proposals[0].rule.nest[0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0][0]"));
}

// ----- one negative test per failure class -----

#[test]
fn too_large_is_0203_with_the_response_id_and_no_position() {
    let text = wrap(&[proposal("candidate_test")]);
    let limit = u64::try_from(text.len()).unwrap() - 1;
    let d = parse_with(Completion::Complete, text.as_bytes(), limit).unwrap_err();
    assert_eq!(
        shape(&d),
        (Code::AiResponseTooLarge, Severity::Error, None, None, None)
    );
    assert!(d.message().contains(&limit.to_string()), "{}", d.message());
}

#[test]
fn invalid_utf8_is_0201_at_the_first_bad_byte() {
    let mut bytes = br#"{"a":""#.to_vec();
    bytes.push(0xFF);
    bytes.extend_from_slice(br#""}"#);
    let d = parse_with(Completion::Complete, &bytes, LIMIT).unwrap_err();
    assert_eq!(
        shape(&d),
        (
            Code::AiResponseMalformed,
            Severity::Error,
            Some(1),
            Some(7),
            Some("byte 6".to_owned())
        )
    );
}

#[test]
fn unparseable_payloads_are_0201() {
    let valid = wrap(&[proposal("candidate_test")]);
    let cases = [
        String::new(),
        "not json".to_owned(),
        valid[..valid.len() - 1].to_owned(),
        format!("{valid} trailing"),
        r#"{"schema_version":"1","proposals":[,]}"#.to_owned(),
    ];
    for text in cases {
        let d = fail(&text);
        assert_eq!(d.code(), Code::AiResponseMalformed, "{text}");
        assert_eq!(d.severity(), Severity::Error);
        assert!(matches!(d.primary(), Location::Input(l) if l.input == rid()));
    }
}

#[test]
fn a_parse_error_reports_its_position_and_byte_offset() {
    let text = r#"{"schema_version":"1","proposals":[,]}"#;
    let d = fail(text);
    let comma = u32::try_from(text.find("[,").unwrap() + 1).unwrap();
    assert_eq!(
        shape(&d),
        (
            Code::AiResponseMalformed,
            Severity::Error,
            Some(1),
            Some(comma + 1),
            Some(format!("byte {comma}"))
        )
    );
}

#[test]
fn refusal_and_incomplete_are_0205() {
    let text = wrap(&[proposal("candidate_test")]);
    for completion in [Completion::Refused, Completion::Incomplete] {
        let d = parse_with(completion, text.as_bytes(), LIMIT).unwrap_err();
        assert_eq!(
            shape(&d),
            (
                Code::AiResponseIncomplete,
                Severity::Error,
                None,
                None,
                None
            )
        );
    }
}

#[test]
fn every_forbidden_name_is_0204_whatever_its_value() {
    for name in FORBIDDEN_FIELDS {
        for value in ["true", "false", r#""approved""#, "null", "[]"] {
            let text = proposal("candidate_test").replace(
                r#""rationale":"why""#,
                &format!(r#""rationale":"why","{name}":{value}"#),
            );
            let text = wrap(&[text]);
            expect(
                &text,
                Code::AiResponseForbiddenField,
                &format!(r#""{name}""#),
                &format!("proposals[0].{name}"),
            );
        }
    }
}

#[test]
fn a_forbidden_name_is_found_at_any_depth() {
    let in_evidence = wrap(&[
        proposal("candidate_test").replace(r#""start":9"#, r#""reviewer":"alice","start":9"#)
    ]);
    expect(
        &in_evidence,
        Code::AiResponseForbiddenField,
        r#""reviewer""#,
        "proposals[0].evidence[0].reviewer",
    );
    let in_rule = wrap(&[rule_proposal(&DRAFT.replace(
        r#""markers":[]"#,
        r#""markers":[],"nest":[{"has_errors":false}]"#,
    ))]);
    expect(
        &in_rule,
        Code::AiResponseForbiddenField,
        r#""has_errors""#,
        "proposals[0].rule.nest[0].has_errors",
    );
    let top = r#"{"approval":"x","schema_version":"1","proposals":[]}"#;
    expect(
        top,
        Code::AiResponseForbiddenField,
        r#""approval""#,
        "approval",
    );
}

#[test]
fn the_first_forbidden_field_in_document_order_is_reported() {
    let text = wrap(&[proposal("candidate_test").replace(
        r#""kind":"candidate_test","#,
        r#""kind":"candidate_test","nest":{"passed":true},"reviewer":"x","#,
    )]);
    expect(
        &text,
        Code::AiResponseForbiddenField,
        r#""passed""#,
        "proposals[0].nest.passed",
    );
}

#[test]
fn a_name_that_only_contains_a_forbidden_word_is_a_schema_violation() {
    let text = wrap(&[proposal("candidate_test").replace(
        r#""rationale":"why""#,
        r#""rationale":"why","approval_note":"x""#,
    )]);
    expect(
        &text,
        Code::AiResponseSchemaViolation,
        r#""approval_note""#,
        "proposals[0].approval_note",
    );
}

#[test]
fn a_forbidden_word_as_a_value_is_not_a_forbidden_field() {
    let text = wrap(&[proposal("candidate_test").replace("why", "approval reviewer status")]);
    let parsed = parse(&text).unwrap();
    assert_eq!(parsed.proposals[0].rationale, "approval reviewer status");
}

#[test]
fn unknown_fields_are_0202_at_the_key() {
    for name in ["script", "command", "reference", "extra"] {
        let text = wrap(&[proposal("candidate_test").replace(
            r#""rationale":"why""#,
            &format!(r#""rationale":"why","{name}":"x""#),
        )]);
        expect(
            &text,
            Code::AiResponseSchemaViolation,
            &format!(r#""{name}""#),
            &format!("proposals[0].{name}"),
        );
    }
    let top = r#"{"schema_version":"1","proposals":[],"extra":1}"#;
    expect(top, Code::AiResponseSchemaViolation, r#""extra""#, "extra");
}

#[test]
fn a_field_of_another_kind_is_unknown() {
    let text = wrap(&[proposal("candidate_test").replace(
        r#""rationale":"why""#,
        &format!(r#""rationale":"why","rule":{DRAFT}"#),
    )]);
    expect(
        &text,
        Code::AiResponseSchemaViolation,
        r#""rule""#,
        "proposals[0].rule",
    );
}

#[test]
fn missing_fields_are_0202_at_the_owning_object() {
    let text = wrap(&[proposal("candidate_test").replace(r#","rationale":"why""#, "")]);
    let d = fail(&text);
    assert_eq!(d.code(), Code::AiResponseSchemaViolation);
    let first = text.find(r#"{"kind""#).unwrap();
    assert_eq!(
        shape(&d),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            Some(u32::try_from(first).unwrap() + 1),
            Some("proposals[0]".to_owned())
        )
    );
    assert!(d.message().contains("rationale"), "{}", d.message());
    let rule_missing = wrap(&[proposal("rule_suggestion")]);
    let d = fail(&rule_missing);
    assert!(d.message().contains("'rule'"), "{}", d.message());
    let no_proposals = r#"{"schema_version":"1"}"#;
    let d = fail(no_proposals);
    assert_eq!(
        shape(&d),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            Some(1),
            None
        )
    );
}

#[test]
fn wrong_types_are_0202_at_the_value() {
    let sha_from = format!(r#""sha256":"{SHA}""#);
    let cases = [
        (r#""start":9"#, r#""start":"9""#, r#""9""#, "start"),
        (r#""start":9"#, r#""start":-1"#, "-1", "start"),
        (r#""start":9"#, r#""start":1.5"#, "1.5", "start"),
        (
            r#""start":9"#,
            r#""start":4294967296"#,
            "4294967296",
            "start",
        ),
        (r#""rationale":"why""#, r#""rationale":7"#, "7", "rationale"),
        (
            r#""proposer":"model-1""#,
            r#""proposer":null"#,
            "null",
            "proposer",
        ),
        (sha_from.as_str(), r#""sha256":"ABC""#, r#""ABC""#, "sha256"),
    ];
    for (from, to, value, key) in cases {
        let text = wrap(&[proposal("candidate_test").replace(from, to)]);
        let d = fail(&text);
        // The value is the tail of the replacement text.
        let value_at = col(&text, to)
            .map(|c| c + u32::try_from(to.chars().count() - value.chars().count()).unwrap());
        let field = match key {
            "start" | "sha256" => format!("proposals[0].evidence[0].{key}"),
            _ => format!("proposals[0].{key}"),
        };
        assert_eq!(
            shape(&d),
            (
                Code::AiResponseSchemaViolation,
                Severity::Error,
                Some(1),
                value_at,
                Some(field)
            ),
            "{to}: {}",
            d.message()
        );
    }
    let arr = r#"{"schema_version":"1","proposals":{}}"#;
    expect(arr, Code::AiResponseSchemaViolation, "{}", "proposals");
    let d = fail("[]");
    assert_eq!(
        shape(&d),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            Some(1),
            None
        )
    );
    assert!(d.message().starts_with("the response must be an object"));
}

#[test]
fn schema_version_must_be_one() {
    for v in ["2", "", "1.0"] {
        let text = format!(r#"{{"schema_version":"{v}","proposals":[]}}"#);
        let d = fail(&text);
        assert_eq!(
            shape(&d),
            (
                Code::AiResponseSchemaViolation,
                Severity::Error,
                Some(1),
                col(&text, &format!(r#""{v}""#)),
                Some("schema_version".to_owned())
            )
        );
    }
}

#[test]
fn only_the_five_kinds_exist() {
    for kind in ["approval", "unrecognized", "rule", "", "RULE_SUGGESTION"] {
        let text = wrap(&[proposal("candidate_test").replace("candidate_test", kind)]);
        let d = fail(&text);
        assert_eq!(
            shape(&d),
            (
                Code::AiResponseSchemaViolation,
                Severity::Error,
                Some(1),
                col(&text, &format!(r#""{kind}""#)),
                Some("proposals[0].kind".to_owned())
            ),
            "{kind}"
        );
    }
}

#[test]
fn proposer_identity_is_one_to_two_hundred_scalar_values() {
    let ok = "é".repeat(200);
    let long = "é".repeat(201);
    for (who, good) in [(ok.as_str(), true), (long.as_str(), false), ("", false)] {
        let text = wrap(&[proposal("candidate_test").replace("model-1", who)]);
        assert_eq!(
            parse(&text).is_ok(),
            good,
            "{} scalar values",
            who.chars().count()
        );
        if !good {
            let d = fail(&text);
            assert_eq!(d.code(), Code::AiResponseSchemaViolation);
            assert!(matches!(d.primary(), Location::Input(l)
                if l.field.as_deref() == Some("proposals[0].proposer")));
        }
    }
}

#[test]
fn proposal_and_evidence_counts_are_bounded() {
    let many = |n: usize| wrap(&vec![proposal("candidate_test"); n]);
    assert_eq!(
        parse(&many(MAX_PROPOSALS)).unwrap().proposals.len(),
        MAX_PROPOSALS
    );
    let d = fail(&many(MAX_PROPOSALS + 1));
    assert_eq!(d.code(), Code::AiResponseSchemaViolation);
    assert!(matches!(d.primary(), Location::Input(l)
        if l.field.as_deref() == Some("proposals")));

    let with = |n: usize| {
        let list = vec![evidence(); n].join(",");
        let one = proposal("candidate_test");
        wrap(&[one.replace(&format!("[{}]", evidence()), &format!("[{list}]"))])
    };
    assert_eq!(
        parse(&with(MAX_EVIDENCE)).unwrap().proposals[0]
            .evidence
            .len(),
        MAX_EVIDENCE
    );
    for n in [0, MAX_EVIDENCE + 1] {
        let d = fail(&with(n));
        assert_eq!(d.code(), Code::AiResponseSchemaViolation, "{n}");
        assert!(matches!(d.primary(), Location::Input(l)
            if l.field.as_deref() == Some("proposals[0].evidence")));
    }
}

#[test]
fn a_repeated_key_is_0202_at_the_second_key() {
    let text = r#"{"schema_version":"1","schema_version":"1","proposals":[]}"#;
    let second = text.rfind(r#""schema_version""#).unwrap();
    let d = fail(text);
    let (code, sev, line, column, _) = shape(&d);
    assert_eq!(
        (code, sev, line),
        (Code::AiResponseSchemaViolation, Severity::Error, Some(1))
    );
    assert_eq!(column, Some(u32::try_from(second).unwrap() + 1));
}

#[test]
fn a_rule_that_is_not_valid_rule_ir_is_0202() {
    for member in [
        r#""revision":2,"#,
        r#""origin":{"kind":"manual","author":null},"#,
    ] {
        let draft = DRAFT.replacen('{', &format!("{{{member}"), 1);
        let text = wrap(&[rule_proposal(&draft)]);
        let d = fail(&text);
        assert_eq!(d.code(), Code::AiResponseSchemaViolation, "{member}");
        assert_eq!(d.severity(), Severity::Error);
        assert!(matches!(d.primary(), Location::Input(l)
            if l.input == rid() && l.line == Some(1)
                && l.field.as_deref().is_some_and(|f| f.starts_with("proposals[0].rule"))));
    }
    let text = wrap(&[rule_proposal(r#"{"id":"x"}"#)]);
    assert_eq!(fail(&text).code(), Code::AiResponseSchemaViolation);
}

#[test]
fn rule_spans_must_equal_the_evidence_spans() {
    for changed in [
        DRAFT.replace(r#""start":9"#, r#""start":8"#),
        DRAFT.replace(r#""end":59"#, r#""end":60"#),
        DRAFT.replace(r#""line":3"#, r#""line":4"#),
        DRAFT.replace("rules.md", "other.md"),
    ] {
        let text = wrap(&[rule_proposal(&changed)]);
        let d = fail(&text);
        assert_eq!(
            (d.code(), d.severity()),
            (Code::AiResponseSchemaViolation, Severity::Error),
            "{changed}"
        );
        assert!(
            matches!(d.primary(), Location::Input(l)
            if l.field.as_deref() == Some("proposals[0].rule.spans")),
            "{:?}",
            d.primary()
        );
    }
}

// ----- precedence (Req 23.5) -----

#[test]
fn too_large_outranks_malformed() {
    let d = parse_with(Completion::Complete, b"not json at all", 3).unwrap_err();
    assert_eq!(d.code(), Code::AiResponseTooLarge);
}

#[test]
fn too_large_outranks_invalid_utf8() {
    let d = parse_with(Completion::Complete, &[0xFF, 0xFE, 0xFD, 0xFC], 3).unwrap_err();
    assert_eq!(d.code(), Code::AiResponseTooLarge);
}

#[test]
fn too_large_outranks_refusal_and_forbidden() {
    let text = wrap(&[proposal("candidate_test").replace(
        r#""rationale":"why""#,
        r#""rationale":"why","approval":true"#,
    )]);
    let d = parse_with(Completion::Refused, text.as_bytes(), 10).unwrap_err();
    assert_eq!(d.code(), Code::AiResponseTooLarge);
}

#[test]
fn malformed_outranks_refusal() {
    for c in [Completion::Refused, Completion::Incomplete] {
        let d = parse_with(c, b"{\"a\":", LIMIT).unwrap_err();
        assert_eq!(d.code(), Code::AiResponseMalformed);
        let d = parse_with(c, &[0xFF, 0xFE], LIMIT).unwrap_err();
        assert_eq!(d.code(), Code::AiResponseMalformed);
    }
}

#[test]
fn malformed_outranks_forbidden() {
    let text = r#"{"schema_version":"1","approval":true,"proposals":[,"#;
    assert_eq!(fail(text).code(), Code::AiResponseMalformed);
}

#[test]
fn refusal_outranks_forbidden_and_schema_violations() {
    let forbidden = wrap(&[proposal("candidate_test").replace(
        r#""rationale":"why""#,
        r#""rationale":"why","has_errors":false"#,
    )]);
    let schema = wrap(&[proposal("candidate_test").replace(r#""rationale":"why","#, "")]);
    let repeated = r#"{"schema_version":"1","schema_version":"1","proposals":[]}"#;
    for text in [forbidden, schema, repeated.to_owned()] {
        for c in [Completion::Refused, Completion::Incomplete] {
            let d = parse_with(c, text.as_bytes(), LIMIT).unwrap_err();
            assert_eq!(d.code(), Code::AiResponseIncomplete, "{text}");
        }
    }
}

#[test]
fn forbidden_outranks_schema_violations_of_every_kind() {
    let forbid = r#""no_errors":true"#;
    let cases = [
        // Unknown field earlier than the forbidden one.
        format!(r#"{{"schema_version":"1","zzz":1,"proposals":[],{forbid}}}"#),
        // Missing field and wrong version.
        format!(r#"{{"schema_version":"9",{forbid}}}"#),
        // Wrong type earlier in the document.
        format!(r#"{{"schema_version":7,"proposals":[{{{forbid}}}]}}"#),
        // Repeated key before the forbidden field.
        format!(r#"{{"schema_version":"1","schema_version":"1","proposals":[],{forbid}}}"#),
        // Two repeated keys before the forbidden field.
        format!(r#"{{"a":1,"a":2,"b":1,"b":2,{forbid}}}"#),
        // The forbidden field is itself repeated.
        r#"{"schema_version":"1","proposals":[],"passed":true,"passed":false}"#.to_owned(),
    ];
    for text in cases {
        assert_eq!(fail(&text).code(), Code::AiResponseForbiddenField, "{text}");
    }
}

#[test]
fn forbidden_outranks_excess_nesting() {
    let deep = format!("{}{}", "[".repeat(40), "]".repeat(40));
    let text = format!(r#"{{"x":{deep},"is_approved":true}}"#);
    assert_eq!(fail(&text).code(), Code::AiResponseForbiddenField);
    let text = format!(r#"{{"x":{deep}}}"#);
    assert_eq!(fail(&text).code(), Code::AiResponseSchemaViolation);
}

#[test]
fn flags_that_agree_with_the_validator_are_still_rejected() {
    // A clean candidate_test agrees with has_errors:false, no_errors:true,
    // status:"proposed" and approval_status:"Proposed".
    for (name, value) in [
        ("has_errors", "false"),
        ("no_errors", "true"),
        ("status", r#""proposed""#),
        ("approval_status", r#""Proposed""#),
        ("validation_status", r#""valid""#),
        ("is_proposed", "true"),
        ("is_approved", "false"),
    ] {
        let text = wrap(&[proposal("candidate_test").replace(
            r#""rationale":"why""#,
            &format!(r#""rationale":"why","{name}":{value}"#),
        )]);
        assert_eq!(fail(&text).code(), Code::AiResponseForbiddenField, "{name}");
    }
}

#[test]
fn a_failure_returns_no_partial_result() {
    let good = proposal("candidate_test");
    let bad = proposal("candidate_test").replace(r#""rationale":"why","#, "");
    let result = parse(&wrap(&[good, bad]));
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), Code::AiResponseSchemaViolation);
}

// ----- properties -----

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Any payload yields either a parsed response or exactly one error that
    /// is Severity Error with a response Input_Location. Arbitrary bytes
    /// never panic.
    #[test]
    fn arbitrary_bytes_never_panic_and_fail_with_an_ai_error(
        bytes in proptest::collection::vec(any::<u8>(), 0..200),
        completion in 0u8..3,
    ) {
        let completion = [Completion::Complete, Completion::Refused, Completion::Incomplete]
            [usize::from(completion)];
        if let Err(d) = parse_with(completion, &bytes, LIMIT) {
            prop_assert!(matches!(
                d.code(),
                Code::AiResponseMalformed | Code::AiResponseIncomplete
                    | Code::AiResponseSchemaViolation | Code::AiResponseForbiddenField
            ));
            prop_assert_eq!(d.severity(), Severity::Error);
            prop_assert!(matches!(d.primary(), Location::Input(l) if l.input == rid()));
        }
    }

    /// Whatever its value, a forbidden name anywhere gives RT-AI-0204.
    #[test]
    fn a_forbidden_name_with_any_value_is_rejected(
        index in 0..FORBIDDEN_FIELDS.len(),
        value in prop_oneof![
            Just("true".to_owned()), Just("false".to_owned()), Just("null".to_owned()),
            Just("0".to_owned()), Just(r#""x""#.to_owned()), Just("{}".to_owned()),
        ],
        depth in 0usize..4,
    ) {
        let name = FORBIDDEN_FIELDS[index];
        let mut inner = format!(r#""{name}":{value}"#);
        for _ in 0..depth {
            inner = format!(r#""n":{{{inner}}}"#);
        }
        let text = wrap(&[proposal("candidate_test").replace(
            r#""rationale":"why""#,
            &format!(r#""rationale":"why",{inner}"#),
        )]);
        prop_assert_eq!(fail(&text).code(), Code::AiResponseForbiddenField);
    }
}
