//! Response parsing and strict schema tests (Req 23.4 to 23.6, 30.9,
//! 22.16; design 3.7.3 and 3.7.4). Payloads are written by hand. Expected
//! columns are found by searching the payload text, not by the parser.

use crate::response::{parse_response, ParsedResponse, ProposalKind, FORBIDDEN_FIELDS};
use proptest::prelude::*;
use rt_ai_adapter::Completion;
use rt_foundation::{Code, Diagnostic, InputId, Location, Severity};
use rt_json::{parse_keeping_duplicates, Kind, ParseLimits, Position, Value};

/// The thirteen names of design 3.7.4 step 1, written out independently of
/// the production list.
pub(crate) const FORBIDDEN_NAMES: [&str; 13] = [
    "approval",
    "approval_status",
    "status",
    "reviewer",
    "approved_by",
    "validation_status",
    "validation_result",
    "has_errors",
    "no_errors",
    "passed",
    "is_approved",
    "is_rejected",
    "is_proposed",
];

pub(crate) const LIMIT: u64 = 1 << 20;
pub(crate) const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const CMP_EXPR: &str = r#"{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}}"#;
pub(crate) const DRAFT: &str = r#"{"id":"elig.new","kind":"eligibility","target":"eligible2","type":"boolean","expr":{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}},"markers":[],"spans":[{"document":"rules.md","start":9,"end":59,"line":3,"column":1}]}"#;

pub(crate) fn rid() -> InputId {
    InputId::new("response-1").unwrap()
}

pub(crate) fn evidence() -> String {
    format!(
        r#"{{"document":"rules.md","sha256":"{SHA}","start":9,"end":59,"line":3,"column":1,"quote":"Applicants must be 16."}}"#
    )
}

pub(crate) fn proposal(kind: &str) -> String {
    format!(
        r#"{{"kind":"{kind}","proposer":"model-1","rationale":"why","evidence":[{}]}}"#,
        evidence()
    )
}

pub(crate) fn rule_proposal(draft: &str) -> String {
    format!(
        r#"{{"kind":"rule_suggestion","proposer":"model-1","rationale":"why","evidence":[{}],"rule":{draft}}}"#,
        evidence()
    )
}

pub(crate) fn wrap(proposals: &[String]) -> String {
    format!(
        r#"{{"schema_version":"1","proposals":[{}]}}"#,
        proposals.join(",")
    )
}

pub(crate) fn parse_with(
    completion: Completion,
    bytes: &[u8],
    limit: u64,
) -> Result<ParsedResponse, Diagnostic> {
    parse_response(&rid(), completion, bytes, limit)
}

pub(crate) fn parse(text: &str) -> Result<ParsedResponse, Diagnostic> {
    parse_with(Completion::Complete, text.as_bytes(), LIMIT)
}

pub(crate) fn fail(text: &str) -> Diagnostic {
    match parse(text) {
        Ok(_) => panic!("expected a rejection of {text}"),
        Err(d) => d,
    }
}

/// Code, severity, line, column and field of a diagnostic, after checking
/// that the location is an Input_Location of the response.
pub(crate) fn shape(d: &Diagnostic) -> (Code, Severity, Option<u32>, Option<u32>, Option<String>) {
    match d.primary() {
        Location::Input(l) => {
            assert_eq!(l.input, rid());
            (d.code(), d.severity(), l.line, l.column, l.field.clone())
        }
        other => panic!("not an input location: {other:?}"),
    }
}

/// 1-based column of the only occurrence of `needle` in single-line `text`.
/// A needle that occurs more than once or not at all fails the test, so
/// that an expectation never points at the wrong occurrence.
pub(crate) fn col(text: &str, needle: &str) -> Option<u32> {
    assert_eq!(
        text.matches(needle).count(),
        1,
        "needle {needle:?} must occur exactly once in the text"
    );
    col_nth(text, needle, 0)
}

/// 1-based column of occurrence `index` (0-based) of `needle`.
pub(crate) fn col_nth(text: &str, needle: &str, index: usize) -> Option<u32> {
    let (at, _) = text.match_indices(needle).nth(index).unwrap();
    Some(u32::try_from(text[..at].chars().count()).unwrap() + 1)
}

pub(crate) fn expect(text: &str, code: Code, needle: &str, field: &str) {
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
    let draft = r#"{"id":"elig.new","kind":"unrecognized","name":"slot_allocation","detail":"top four qualify","target":"eligible2","type":"boolean","markers":[],"spans":[{"document":"rules.md","start":9,"end":59,"line":3,"column":1}]}"#;
    let parsed = parse(&wrap(&[rule_proposal(draft)])).unwrap();
    let ProposalKind::RuleSuggestion(rule) = &parsed.proposals[0].kind else {
        panic!("rule suggestion expected");
    };
    assert_eq!(rule.id.as_str(), "elig.new");
    assert_eq!(rule.kind_name, "unrecognized");
    assert_eq!(rule.json, draft);
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
fn a_hundred_kibibyte_rationale_and_quote_are_accepted_whole() {
    let long = "a".repeat(100 * 1024);
    let text = wrap(&[proposal("candidate_test")
        .replace("why", &long)
        .replace("Applicants must be 16.", &long)]);
    let parsed = parse(&text).unwrap();
    assert_eq!(parsed.proposals[0].rationale, long);
    assert_eq!(parsed.proposals[0].evidence[0].quote, long);
}

#[test]
fn nesting_of_exactly_thirty_two_is_accepted_and_thirty_three_is_not() {
    let leaf = r#"{"node":"bool","t":"boolean","value":true}"#;
    let with_depth = |nots: usize| {
        let mut expr = leaf.to_owned();
        for _ in 0..nots {
            expr = format!(r#"{{"node":"not","t":"boolean","arg":{expr}}}"#);
        }
        DRAFT.replace(CMP_EXPR, &expr)
    };
    // Root 1, proposals 2, proposal 3, rule 4, outermost expr 5. The leaf
    // sits 5 + nots deep, so 27 nots put it at depth 32.
    assert!(parse(&wrap(&[rule_proposal(&with_depth(27))])).is_ok());
    let text = wrap(&[rule_proposal(&with_depth(28))]);
    let at_leaf = col(&text, r#"{"node":"bool""#);
    let field = format!("proposals[0].rule.expr{}", ".arg".repeat(28));
    assert_eq!(
        shape(&fail(&text)),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            at_leaf,
            Some(field)
        )
    );
}

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
fn the_production_list_is_exactly_the_thirteen_names_of_the_design() {
    assert_eq!(FORBIDDEN_FIELDS.len(), 13);
    let mut production: Vec<&str> = FORBIDDEN_FIELDS.to_vec();
    let mut design: Vec<&str> = FORBIDDEN_NAMES.to_vec();
    production.sort_unstable();
    design.sort_unstable();
    assert_eq!(production, design);
}

#[test]
fn every_forbidden_name_is_0204_whatever_its_value() {
    for name in FORBIDDEN_NAMES {
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
fn schema_version_must_be_one_or_two() {
    // "2" became valid with the flat schema (UD-37, task 10.4). It is
    // accepted by tests_flat; "3" takes its place here.
    for v in ["3", "", "1.0"] {
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
    assert_eq!(parse(&many(100)).unwrap().proposals.len(), 100);
    let text = many(101);
    assert_eq!(
        shape(&fail(&text)),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            col_nth(&text, r#""proposals":["#, 0).map(|c| c + 12),
            Some("proposals".to_owned())
        )
    );

    let with = |n: usize| {
        let list = vec![evidence(); n].join(",");
        let one = proposal("candidate_test");
        wrap(&[one.replace(&format!("[{}]", evidence()), &format!("[{list}]"))])
    };
    assert_eq!(parse(&with(16)).unwrap().proposals[0].evidence.len(), 16);
    for n in [0, 17] {
        let text = with(n);
        assert_eq!(
            shape(&fail(&text)),
            (
                Code::AiResponseSchemaViolation,
                Severity::Error,
                Some(1),
                col(&text, r#""evidence":"#).map(|c| c + 11),
                Some("proposals[0].evidence".to_owned())
            ),
            "{n} items"
        );
    }
}

#[test]
fn a_repeated_key_is_0202_at_the_second_key_with_its_path() {
    let top = r#"{"schema_version":"1","schema_version":"1","proposals":[]}"#;
    let second = u32::try_from(top.rfind(r#""schema_version""#).unwrap()).unwrap() + 1;
    assert_eq!(
        shape(&fail(top)),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            Some(second),
            Some("schema_version".to_owned())
        )
    );
    let nested = wrap(&[proposal("candidate_test").replace(
        r#""rationale":"why""#,
        r#""rationale":"why","rationale":"again""#,
    )]);
    let second = col(&nested, r#""rationale":"again""#);
    assert_eq!(
        shape(&fail(&nested)),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            second,
            Some("proposals[0].rationale".to_owned())
        )
    );
}

/// Column of the `{` that opens the rule object in single-line `text`.
fn rule_start(text: &str) -> Option<u32> {
    col(text, r#""rule":{"#).map(|c| c + 7)
}

#[test]
fn a_rule_that_is_not_valid_rule_ir_is_0202_at_the_key_or_the_rule() {
    let cases = [
        (r#""revision":2,"#, "proposals[0].rule.revision"),
        (
            r#""origin":{"kind":"manual","author":null},"#,
            "proposals[0].rule.origin",
        ),
    ];
    for (member, field) in cases {
        let draft = DRAFT.replacen('{', &format!("{{{member}"), 1);
        let text = wrap(&[rule_proposal(&draft)]);
        assert_eq!(
            shape(&fail(&text)),
            (
                Code::AiResponseSchemaViolation,
                Severity::Error,
                Some(1),
                col(&text, member.trim_end_matches(',')),
                Some(field.to_owned())
            ),
            "{member}"
        );
    }
    let text = wrap(&[rule_proposal(r#"{"id":"x"}"#)]);
    assert_eq!(
        shape(&fail(&text)),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            rule_start(&text),
            Some("proposals[0].rule".to_owned())
        )
    );
}

#[test]
fn a_first_decoder_diagnostic_that_is_not_an_unknown_field_stays_at_the_rule() {
    let draft = DRAFT.replacen(r#""id":"elig.new""#, r#""id":"has space","revision":2"#, 1);
    let text = wrap(&[rule_proposal(&draft)]);
    let d = fail(&text);
    let (code, severity, line, column, field) = shape(&d);
    assert_eq!(
        (code, severity, line),
        (Code::AiResponseSchemaViolation, Severity::Error, Some(1))
    );
    assert_eq!(column, rule_start(&text), "{}", d.message());
    assert_eq!(field.as_deref(), Some("proposals[0].rule.id"));
    assert!(d.message().contains("RT-IR-0010"), "{}", d.message());
}

#[test]
fn an_escaped_rule_key_is_located_by_its_character_column() {
    let key = r#""revisi\u006fn""#;
    let draft = DRAFT.replacen('{', &format!("{{{key}:2,"), 1);
    let text =
        wrap(&[rule_proposal(&draft)]).replace(r#""rationale":"why""#, r#""rationale":"why é""#);
    assert!(text.contains("é") && text.len() > text.chars().count());
    assert_eq!(
        shape(&fail(&text)),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            col(&text, key),
            Some("proposals[0].rule.revision".to_owned())
        )
    );
}

const SPAN_A: &str = r#"{"document":"rules.md","start":9,"end":59,"line":3,"column":1}"#;
const SPAN_B: &str = r#"{"document":"rules.md","start":70,"end":90,"line":5,"column":4}"#;

fn evidence_item(span: &str) -> String {
    let quote = r#","quote":"Applicants must be 16.""#;
    let fields = span.trim_end_matches('}');
    let with_hash = fields.replace(r#""start""#, &format!(r#""sha256":"{SHA}","start""#));
    format!("{with_hash}{quote}}}")
}

fn suggestion(evidence_spans: &[&str], rule_spans: &[&str]) -> String {
    let items: Vec<String> = evidence_spans.iter().map(|s| evidence_item(s)).collect();
    let draft = DRAFT.replace(
        &format!(r#""spans":[{SPAN_A}]"#),
        &format!(r#""spans":[{}]"#, rule_spans.join(",")),
    );
    wrap(&[format!(
        r#"{{"kind":"rule_suggestion","proposer":"model-1","rationale":"why","evidence":[{}],"rule":{draft}}}"#,
        items.join(",")
    )])
}

#[test]
fn matching_rule_and_evidence_spans_are_accepted() {
    assert!(parse(&suggestion(&[SPAN_A], &[SPAN_A])).is_ok());
    assert!(parse(&suggestion(&[SPAN_A, SPAN_B], &[SPAN_A, SPAN_B])).is_ok());
}

#[test]
fn rule_spans_must_equal_the_evidence_spans() {
    let moved = |from: &str, to: &str| SPAN_A.replace(from, to);
    let changed = [
        moved(r#""start":9"#, r#""start":8"#),
        moved(r#""end":59"#, r#""end":60"#),
        moved(r#""line":3"#, r#""line":4"#),
        moved(r#""column":1"#, r#""column":2"#),
        moved("rules.md", "other.md"),
    ];
    for rule_span in &changed {
        let text = suggestion(&[SPAN_A], &[rule_span]);
        assert_expected_mismatch(&text, rule_span);
    }
}

#[test]
fn only_the_column_differing_is_a_mismatch() {
    let column_only = SPAN_A.replace(r#""column":1"#, r#""column":2"#);
    assert_expected_mismatch(&suggestion(&[SPAN_A], &[&column_only]), &column_only);
}

#[test]
fn a_different_span_count_is_a_mismatch() {
    assert_expected_mismatch(&suggestion(&[SPAN_A, SPAN_B], &[SPAN_A]), "count");
    assert_expected_mismatch(&suggestion(&[SPAN_A], &[SPAN_A, SPAN_B]), "count");
}

#[test]
fn the_same_spans_in_a_different_order_are_a_mismatch() {
    assert_expected_mismatch(&suggestion(&[SPAN_A, SPAN_B], &[SPAN_B, SPAN_A]), "order");
}

fn assert_expected_mismatch(text: &str, what: &str) {
    assert_eq!(
        shape(&fail(text)),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            rule_start(text),
            Some("proposals[0].rule.spans".to_owned())
        ),
        "{what}"
    );
}

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
fn a_hundred_thousand_nested_arrays_are_0202_without_exhausting_the_stack() {
    let n = 100_000;
    let text = format!("{}{}", "[".repeat(n), "]".repeat(n));
    let d = parse_with(Completion::Complete, text.as_bytes(), 1 << 21).unwrap_err();
    assert_eq!(
        shape(&d),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            Some(33),
            Some("[0]".repeat(32))
        )
    );
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

/// Every decoded key of `value`, in document order.
fn decoded_keys(value: &Value<Position>, out: &mut Vec<String>) {
    match value.kind() {
        Kind::Array(items) => items.iter().for_each(|v| decoded_keys(v, out)),
        Kind::Object(members) => {
            for m in members {
                out.push(m.key.clone());
                decoded_keys(&m.value, out);
            }
        }
        _ => {}
    }
}

const TOKENS: [&str; 16] = [
    r#""status":"x","#,
    r#""approval":true,"#,
    r#""kind":"unrecognized","#,
    r#""node":"shell_exec","#,
    r#""command":"rm","#,
    r#""rationale":"again","#,
    r#""evidence":[],"#,
    "null",
    "[",
    "]",
    "{",
    "}",
    ",",
    r#""é""#,
    "99999999999",
    r#"\u0000"#,
];

fn bases() -> Vec<String> {
    vec![
        wrap(&[proposal("candidate_test")]),
        wrap(&[
            proposal("clause_relationship"),
            proposal("conflict_highlight"),
        ]),
        wrap(&[rule_proposal(DRAFT)]),
        suggestion(&[SPAN_A, SPAN_B], &[SPAN_A, SPAN_B]),
        wrap(&[]),
    ]
}

/// One structured mutation of a valid payload: delete, replace or insert a
/// token, duplicate a slice, truncate or flip one byte.
fn mutate(base: &str, op: u8, at: usize, len: usize, token: usize) -> Vec<u8> {
    let mut bytes = base.as_bytes().to_vec();
    let at = at % (bytes.len() + 1);
    let end = (at + len).min(bytes.len());
    let token = TOKENS[token % TOKENS.len()].as_bytes();
    match op % 6 {
        0 => {
            bytes.drain(at..end);
        }
        1 => {
            bytes.splice(at..end, token.iter().copied());
        }
        2 => {
            bytes.splice(at..at, token.iter().copied());
        }
        3 => {
            let copy = bytes[at..end].to_vec();
            bytes.splice(at..at, copy);
        }
        4 => bytes.truncate(at),
        _ => {
            if let Some(b) = bytes.get_mut(at) {
                *b ^= 1 << (len % 8);
            }
        }
    }
    bytes
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// A mutated valid payload never panics. A rejection is exactly one
    /// Severity Error diagnostic with an AI response code and a location in
    /// the response. An accepted payload has no forbidden key once every
    /// key is decoded, whatever escapes spell it.
    #[test]
    fn mutated_payloads_never_panic_and_fail_with_an_ai_error(
        base in 0usize..5,
        op in any::<u8>(),
        at in 0usize..400,
        len in 0usize..40,
        token in any::<usize>(),
        completion in 0u8..3,
    ) {
        let payload = mutate(&bases()[base], op, at, len, token);
        let completion = [Completion::Complete, Completion::Refused, Completion::Incomplete]
            [usize::from(completion)];
        match parse_with(completion, &payload, LIMIT) {
            Err(d) => {
                prop_assert!(matches!(
                    d.code(),
                    Code::AiResponseMalformed | Code::AiResponseIncomplete
                        | Code::AiResponseSchemaViolation | Code::AiResponseForbiddenField
                ));
                prop_assert_eq!(d.severity(), Severity::Error);
                prop_assert!(matches!(d.primary(), Location::Input(l) if l.input == rid()));
            }
            Ok(_) => {
                prop_assert_eq!(completion, Completion::Complete);
                let parsed = parse_keeping_duplicates(&rid(), &payload, ParseLimits::new(LIMIT, 64))
                    .expect("an accepted payload parses");
                let mut keys = Vec::new();
                decoded_keys(&parsed.tree, &mut keys);
                for name in FORBIDDEN_NAMES {
                    prop_assert!(!keys.iter().any(|k| k == name), "{name}");
                }
            }
        }
    }

    /// Whatever its value, a forbidden name anywhere gives RT-AI-0204.
    #[test]
    fn a_forbidden_name_with_any_value_is_rejected(
        index in 0..FORBIDDEN_NAMES.len(),
        value in prop_oneof![
            Just("true".to_owned()), Just("false".to_owned()), Just("null".to_owned()),
            Just("0".to_owned()), Just(r#""x""#.to_owned()), Just("{}".to_owned()),
        ],
        depth in 0usize..4,
    ) {
        let name = FORBIDDEN_NAMES[index];
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

    /// Repeated keys, in any spelling, before a forbidden name never hide
    /// it: the answer is RT-AI-0204 at the forbidden key, not RT-AI-0202.
    #[test]
    fn repeated_keys_in_any_spelling_never_hide_a_forbidden_name(
        index in 0..FORBIDDEN_NAMES.len(),
        escape_first in any::<bool>(),
        groups in proptest::collection::vec(
            (0..KEY_SPELLINGS.len(), proptest::collection::vec(0usize..4, 2..6)),
            1..4,
        ),
    ) {
        let name = FORBIDDEN_NAMES[index];
        let spelled = if escape_first {
            let first = name.chars().next().unwrap();
            format!(r#""\u{:04x}{}""#, u32::from(first), &name[1..])
        } else {
            format!(r#""{name}""#)
        };
        let mut prefix = String::new();
        for (family, spellings) in &groups {
            let family = &KEY_SPELLINGS[*family];
            for (n, pick) in spellings.iter().enumerate() {
                let key = family[pick % family.len()];
                prefix.push_str(&format!(r#"{key}:{n},"#));
            }
        }
        let text = wrap(&[proposal("candidate_test").replace(
            r#""rationale":"why""#,
            &format!(r#""rationale":"why",{prefix}{spelled}:true"#),
        )]);
        prop_assert_eq!(
            shape(&fail(&text)),
            (
                Code::AiResponseForbiddenField,
                Severity::Error,
                Some(1),
                col(&text, &spelled),
                Some(format!("proposals[0].{name}"))
            ),
            "{}", text
        );
    }
}

/// Spellings of one decoded key: plain, escaped and non-ASCII forms.
const KEY_SPELLINGS: [&[&str]; 4] = [
    &[r#""a""#, r#""\u0061""#],
    &[r#""é""#, r#""\u00e9""#, r#""\u00E9""#],
    &[r#""a\"""#, r#""a\u0022""#],
    &["\"😀\"", r#""\ud83d\ude00""#, r#""\uD83D\uDE00""#],
];
