//! Every object of the response schema is closed (Req 23.4, 23.5; design
//! 3.7.3, 3.7.4). A field outside the schema, such as `script`, `command`
//! or `reference`, is RT-AI-0202 at every nesting level. Payloads are
//! written by hand and expected columns come from searching the payload.

use crate::response::ProposalKind;
use crate::tests_response::{col, fail, parse, proposal, rule_proposal, shape, wrap, DRAFT, SHA};
use rt_foundation::{Code, Severity};

const UNKNOWN_NAMES: [&str; 3] = ["script", "command", "reference"];
const SPAN: &str = r#"{"document":"rules.md","start":9,"end":59,"line":3,"column":1}"#;
const NUM: &str = r#"{"node":"num","t":"number","value":"1"}"#;
const FIELD: &str = r#"{"node":"field","t":"number","name":"age"}"#;
const CMP: &str = r#"{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}}"#;

/// Where a field outside the schema is reported.
#[derive(Clone, Copy, Debug)]
enum Reported {
    /// At the key, because the closed vocabulary check finds it.
    AtKey,
    /// At the start of the rule, because the Rule_IR decoder finds it.
    AtRule,
}

struct Case {
    label: &'static str,
    text: String,
    anchor: String,
    path: String,
    reported: Reported,
}

/// `text` with `"name":"x",` placed first in the object that `anchor` opens.
fn inject(text: &str, anchor: &str, name: &str) -> String {
    assert!(anchor.starts_with('{'), "{anchor}");
    assert!(text.contains(anchor), "{anchor}");
    text.replacen(anchor, &format!(r#"{{"{name}":"x",{}"#, &anchor[1..]), 1)
}

fn draft_with_left(left: &str) -> String {
    let expr = format!(r#"{{"node":"cmp","t":"boolean","op":"ge","left":{left},"right":{NUM}}}"#);
    rule_proposal(&DRAFT.replace(CMP, &expr))
}

fn rule_text(draft: &str) -> String {
    wrap(&[rule_proposal(draft)])
}

fn weighted() -> String {
    format!(
        r#"{{"id":"w.one","kind":"weighted_score","target":"score","type":"number","terms":[{{"weight":{NUM},"value":{FIELD}}}],"rounding":{{"mode":"half_even","places":2}},"markers":[{{"kind":"ambiguous","interpretations":["a","b"]}},{{"kind":"incomplete","missing":["x"]}}],"spans":[{SPAN}]}}"#
    )
}

fn ranking() -> String {
    format!(
        r#"{{"id":"r.one","kind":"ranking","target":"rank","type":"number","key":{FIELD},"direction":"desc","tie_breakers":[{{"expr":{FIELD},"direction":"asc"}}],"ties":"group","markers":[],"spans":[{SPAN}]}}"#
    )
}

fn default_rule() -> String {
    format!(
        r#"{{"id":"d.one","kind":"default","target":"base","type":"number","value":{NUM},"markers":[],"spans":[{SPAN}]}}"#
    )
}

fn derived() -> String {
    DRAFT.replace("eligibility", "derived")
}

fn unrecognized_kind() -> String {
    format!(
        r#"{{"id":"u.one","kind":"unrecognized","name":"seeding","detail":"d","target":"t","type":"boolean","markers":[],"spans":[{SPAN}]}}"#
    )
}

fn case(label: &'static str, text: String, anchor: &str, path: &str, r: Reported) -> Case {
    Case {
        label,
        text,
        anchor: anchor.to_owned(),
        path: path.to_owned(),
        reported: r,
    }
}

fn node_cases() -> Vec<Case> {
    let left = |label, node: &str, anchor: &str| {
        case(
            label,
            wrap(&[draft_with_left(node)]),
            anchor,
            "proposals[0].rule.expr.left",
            Reported::AtKey,
        )
    };
    let whole = |label, expr: &str, anchor: &str| {
        case(
            label,
            rule_text(&DRAFT.replace(CMP, expr)),
            anchor,
            "proposals[0].rule.expr",
            Reported::AtKey,
        )
    };
    let leaf = r#"{"node":"bool","t":"boolean","value":true}"#;
    vec![
        left("num", NUM, r#"{"node":"num""#),
        left("field", FIELD, r#"{"node":"field""#),
        left(
            "text",
            r#"{"node":"text","t":"text","value":"a"}"#,
            r#"{"node":"text""#,
        ),
        left(
            "date",
            r#"{"node":"date","t":"date","value":"2024-01-31"}"#,
            r#"{"node":"date""#,
        ),
        left(
            "enum",
            r#"{"node":"enum","t":"enum:level","enum":"level","member":"gold"}"#,
            r#"{"node":"enum""#,
        ),
        left(
            "ref",
            r#"{"node":"ref","t":"number","target":"score.total"}"#,
            r#"{"node":"ref""#,
        ),
        left(
            "rank_reference",
            r#"{"node":"rank_reference","t":"number","target":"rank.main"}"#,
            r#"{"node":"rank_reference""#,
        ),
        left(
            "binary",
            &format!(r#"{{"node":"binary","t":"number","op":"add","left":{NUM},"right":{NUM}}}"#),
            r#"{"node":"binary""#,
        ),
        left(
            "round",
            &format!(
                r#"{{"node":"round","t":"number","arg":{NUM},"places":2,"mode":"half_even"}}"#
            ),
            r#"{"node":"round""#,
        ),
        left(
            "agg",
            r#"{"node":"agg","t":"number","fn":"sum","collection":"c","field":"f"}"#,
            r#"{"node":"agg""#,
        ),
        left(
            "unrecognized node",
            r#"{"node":"unrecognized","name":"seeding","detail":"d"}"#,
            r#"{"node":"unrecognized""#,
        ),
        whole("cmp", CMP, r#"{"node":"cmp""#),
        whole("bool", leaf, r#"{"node":"bool""#),
        whole(
            "not",
            &format!(r#"{{"node":"not","t":"boolean","arg":{leaf}}}"#),
            r#"{"node":"not""#,
        ),
        whole(
            "and",
            &format!(r#"{{"node":"and","t":"boolean","args":[{leaf},{leaf}]}}"#),
            r#"{"node":"and""#,
        ),
        whole(
            "or",
            &format!(r#"{{"node":"or","t":"boolean","args":[{leaf},{leaf}]}}"#),
            r#"{"node":"or""#,
        ),
        whole(
            "in",
            r#"{"node":"in","t":"boolean","value":{"node":"field","t":"text","name":"c"},"set":"countries"}"#,
            r#"{"node":"in""#,
        ),
    ]
}

fn structural_cases() -> Vec<Case> {
    let kinds = [
        "clause_interpretation",
        "clause_relationship",
        "conflict_highlight",
        "candidate_test",
    ];
    let mut out = vec![
        case(
            "response",
            wrap(&[proposal("candidate_test")]),
            r#"{"schema_version""#,
            "",
            Reported::AtKey,
        ),
        case(
            "rule_suggestion proposal",
            wrap(&[rule_proposal(DRAFT)]),
            r#"{"kind":"rule_suggestion""#,
            "proposals[0]",
            Reported::AtKey,
        ),
        case(
            "evidence item",
            wrap(&[proposal("candidate_test")]),
            r#"{"document":"rules.md","sha256""#,
            "proposals[0].evidence[0]",
            Reported::AtKey,
        ),
        case(
            "eligibility rule",
            rule_text(DRAFT),
            r#"{"id":"elig.new""#,
            "proposals[0].rule",
            Reported::AtRule,
        ),
        case(
            "rule span",
            rule_text(DRAFT),
            r#"{"document":"rules.md","start":9"#,
            "proposals[0].rule.spans[0]",
            Reported::AtRule,
        ),
        case(
            "derived rule",
            rule_text(&derived()),
            r#"{"id":"elig.new""#,
            "proposals[0].rule",
            Reported::AtRule,
        ),
        case(
            "weighted_score rule",
            rule_text(&weighted()),
            r#"{"id":"w.one""#,
            "proposals[0].rule",
            Reported::AtRule,
        ),
        case(
            "weighted term",
            rule_text(&weighted()),
            r#"{"weight":"#,
            "proposals[0].rule.terms[0]",
            Reported::AtRule,
        ),
        case(
            "rounding",
            rule_text(&weighted()),
            r#"{"mode":"half_even""#,
            "proposals[0].rule.rounding",
            Reported::AtRule,
        ),
        case(
            "ambiguous marker",
            rule_text(&weighted()),
            r#"{"kind":"ambiguous""#,
            "proposals[0].rule.markers[0]",
            Reported::AtRule,
        ),
        case(
            "incomplete marker",
            rule_text(&weighted()),
            r#"{"kind":"incomplete""#,
            "proposals[0].rule.markers[1]",
            Reported::AtRule,
        ),
        case(
            "ranking rule",
            rule_text(&ranking()),
            r#"{"id":"r.one""#,
            "proposals[0].rule",
            Reported::AtRule,
        ),
        case(
            "tie breaker",
            rule_text(&ranking()),
            r#"{"expr":"#,
            "proposals[0].rule.tie_breakers[0]",
            Reported::AtRule,
        ),
        case(
            "default rule",
            rule_text(&default_rule()),
            r#"{"id":"d.one""#,
            "proposals[0].rule",
            Reported::AtRule,
        ),
        case(
            "default value literal",
            rule_text(&default_rule()),
            r#"{"node":"num""#,
            "proposals[0].rule.value",
            Reported::AtKey,
        ),
        case(
            "unrecognized rule kind",
            rule_text(&unrecognized_kind()),
            r#"{"id":"u.one""#,
            "proposals[0].rule",
            Reported::AtKey,
        ),
        case(
            "span of an unrecognized rule kind",
            rule_text(&unrecognized_kind()),
            r#"{"document":"rules.md","start":9"#,
            "proposals[0].rule.spans[0]",
            Reported::AtRule,
        ),
    ];
    for kind in kinds {
        out.push(case(
            "advisory proposal",
            wrap(&[proposal(kind)]),
            &format!(r#"{{"kind":"{kind}""#),
            "proposals[0]",
            Reported::AtKey,
        ));
    }
    out
}

fn check_case(c: &Case, name: &str) {
    let text = inject(&c.text, &c.anchor, name);
    let d = fail(&text);
    let key = format!(r#""{name}":"x""#);
    let want_col = match c.reported {
        Reported::AtKey => col(&text, &key),
        Reported::AtRule => col(&text, r#""rule":{"#).map(|c| c + 7),
    };
    let field = if c.path.is_empty() {
        name.to_owned()
    } else {
        format!("{}.{name}", c.path)
    };
    assert_eq!(
        shape(&d),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            want_col,
            Some(field)
        ),
        "{} / {name}: {}",
        c.label,
        d.message()
    );
}

#[test]
fn script_command_and_reference_are_0202_in_every_structural_object() {
    for c in structural_cases() {
        for name in UNKNOWN_NAMES {
            check_case(&c, name);
        }
    }
}

#[test]
fn script_command_and_reference_are_0202_in_every_expression_node() {
    let cases = node_cases();
    assert_eq!(cases.len(), 17);
    for c in cases {
        for name in UNKNOWN_NAMES {
            check_case(&c, name);
        }
    }
}

fn opaque_cmp(left: &str) -> String {
    let expr =
        format!(r#"{{"node":"cmp","t":"boolean","op":"run_shell","left":{left},"right":{NUM}}}"#);
    rule_proposal(&DRAFT.replace(CMP, &expr))
}

#[test]
fn an_extra_field_on_a_node_with_an_unknown_operator_is_0202_at_the_key() {
    let nodes = [
        (
            r#"{"node":"cmp","t":"boolean","op":"run_shell","script":"x","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}}"#,
            "proposals[0].rule.expr.script",
        ),
        (
            r#"{"node":"binary","t":"number","op":"run_shell","command":"x","left":{"node":"num","t":"number","value":"1"},"right":{"node":"num","t":"number","value":"1"}}"#,
            "proposals[0].rule.expr.command",
        ),
        (
            r#"{"node":"agg","t":"number","fn":"run_shell","reference":"x","collection":"c","field":"f"}"#,
            "proposals[0].rule.expr.reference",
        ),
    ];
    for (expr, field) in nodes {
        let text = wrap(&[rule_proposal(&DRAFT.replace(CMP, expr))]);
        let name = field.rsplit('.').next().unwrap();
        let d = fail(&text);
        assert_eq!(
            shape(&d),
            (
                Code::AiResponseSchemaViolation,
                Severity::Error,
                Some(1),
                col(&text, &format!(r#""{name}""#)),
                Some(field.to_owned())
            ),
            "{expr}: {}",
            d.message()
        );
    }
}

#[test]
fn an_extra_field_below_a_node_with_an_unknown_operator_is_0202_at_the_key() {
    for name in UNKNOWN_NAMES {
        let left = format!(r#"{{"node":"field","t":"number","name":"age","{name}":"x"}}"#);
        let text = wrap(&[opaque_cmp(&left)]);
        let d = fail(&text);
        assert_eq!(
            shape(&d),
            (
                Code::AiResponseSchemaViolation,
                Severity::Error,
                Some(1),
                col(&text, &format!(r#""{name}""#)),
                Some(format!("proposals[0].rule.expr.left.{name}"))
            ),
            "{name}: {}",
            d.message()
        );
    }
}

#[test]
fn an_operand_without_a_node_below_an_unknown_operator_is_0202_at_the_operand() {
    for (left, needle) in [
        (r#"{"script":"x"}"#, r#"{"script""#),
        (r#""rm -rf /""#, r#""rm -rf /""#),
        ("7", "7,"),
        ("[1]", "[1]"),
    ] {
        let text = wrap(&[opaque_cmp(left)]);
        let d = fail(&text);
        assert_eq!(
            shape(&d),
            (
                Code::AiResponseSchemaViolation,
                Severity::Error,
                Some(1),
                col(&text, needle),
                Some("proposals[0].rule.expr.left".to_owned())
            ),
            "{left}: {}",
            d.message()
        );
        assert!(d.message().contains("'node'"), "{}", d.message());
    }
}

#[test]
fn an_arbitrary_node_below_a_node_with_an_unknown_operator_is_0202() {
    let text = wrap(&[opaque_cmp(r#"{"node":"eval","code":"x"}"#)]);
    let d = fail(&text);
    assert_eq!(
        shape(&d),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            col(&text, r#""eval""#),
            Some("proposals[0].rule.expr.left.node".to_owned())
        )
    );
}

#[test]
fn a_node_with_an_unknown_but_well_formed_operator_is_carried() {
    let opaque = [
        opaque_cmp(FIELD),
        rule_proposal(&DRAFT.replace(
            CMP,
            r#"{"node":"agg","t":"number","fn":"median","collection":"c","field":"f"}"#,
        )),
        rule_proposal(&DRAFT.replace(
            CMP,
            &format!(r#"{{"node":"binary","t":"number","op":"pow","left":{NUM},"right":{NUM}}}"#),
        )),
    ];
    for proposal in opaque {
        let text = wrap(&[proposal]);
        let parsed = parse(&text).unwrap_or_else(|d| panic!("{}: {text}", d.message()));
        let ProposalKind::RuleSuggestion(rule) = &parsed.proposals[0].kind else {
            panic!("rule suggestion expected");
        };
        assert_eq!(rule.kind_name, "eligibility");
    }
}

#[test]
fn a_node_whose_operator_is_not_well_formed_is_0202() {
    let expr = CMP.replace(r#""op":"ge""#, r#""op":"Run Shell""#);
    let text = wrap(&[rule_proposal(&DRAFT.replace(CMP, &expr))]);
    let d = fail(&text);
    assert_eq!(d.code(), Code::AiResponseSchemaViolation);
    assert!(matches!(d.primary(),
        rt_foundation::Location::Input(l)
            if l.field.as_deref() == Some("proposals[0].rule.expr.op")));
}

#[test]
fn every_rule_kind_with_its_full_field_set_is_accepted() {
    let drafts = [
        ("eligibility", DRAFT.to_owned(), "elig.new"),
        ("derived", derived(), "elig.new"),
        ("weighted_score", weighted(), "w.one"),
        ("ranking", ranking(), "r.one"),
        ("default", default_rule(), "d.one"),
    ];
    for (kind, draft, id) in drafts {
        let text = wrap(&[rule_proposal(&draft)]);
        let parsed = parse(&text).unwrap_or_else(|d| panic!("{kind}: {}", d.message()));
        let ProposalKind::RuleSuggestion(rule) = &parsed.proposals[0].kind else {
            panic!("rule suggestion expected");
        };
        assert_eq!(rule.kind_name, kind);
        assert_eq!(rule.id.as_str(), id);
        assert_eq!(rule.json, draft);
    }
}

#[test]
fn typed_source_references_are_the_only_references_and_are_accepted() {
    let first = format!(
        r#"{{"document":"rules.md","sha256":"{SHA}","start":9,"end":59,"line":3,"column":1,"quote":"Applicants must be 16."}}"#
    );
    let second = format!(
        r#"{{"document":"annex-b.md","sha256":"{SHA}","start":70,"end":90,"line":5,"column":4,"quote":"Fees apply."}}"#
    );
    let text = wrap(&[format!(
        r#"{{"kind":"clause_relationship","proposer":"model-1","rationale":"why","evidence":[{first},{second}]}}"#
    )]);
    let parsed = parse(&text).unwrap();
    let evidence = &parsed.proposals[0].evidence;
    assert_eq!(evidence.len(), 2);
    assert_eq!(evidence[1].document.as_str(), "annex-b.md");
    assert_eq!(evidence[1].sha256.as_str(), SHA);
    assert_eq!(
        (
            evidence[1].start,
            evidence[1].end,
            evidence[1].line,
            evidence[1].column
        ),
        (70, 90, 5, 4)
    );
    assert_eq!(evidence[1].quote, "Fees apply.");
    let with_reference = text.replace(
        r#""quote":"Fees apply.""#,
        r#""quote":"Fees apply.","reference":"x""#,
    );
    let d = fail(&with_reference);
    assert_eq!(d.code(), Code::AiResponseSchemaViolation);
}
