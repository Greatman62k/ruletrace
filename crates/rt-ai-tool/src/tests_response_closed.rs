//! Closed vocabulary of rule drafts and forbidden names behind repeated keys
//! (design 3.7.3, 3.7.4; Req 23.4, 23.5). Payloads are written by hand and
//! expected columns come from searching the payload text.

use crate::tests_response::{
    col, expect, fail, parse, proposal, rule_proposal, shape, wrap, DRAFT,
};
use rt_foundation::{Code, Severity};

const RIGHT: &str = r#"{"node":"num","t":"number","value":"16"}"#;
const RULE_PATH: &str = "proposals[0].rule";

fn unrecognized_rule(extra: &str) -> String {
    format!(
        r#"{{"id":"elig.new","kind":"unrecognized",{extra}"target":"eligible2","type":"boolean","markers":[],"spans":[{{"document":"rules.md","start":9,"end":59,"line":3,"column":1}}]}}"#
    )
}

fn with_right(node: &str) -> String {
    rule_proposal(&DRAFT.replace(RIGHT, node))
}

fn schema(text: &str, needle: &str, field: &str) {
    expect(text, Code::AiResponseSchemaViolation, needle, field);
}

#[test]
fn an_unrecognized_rule_with_a_command_field_is_rejected_at_the_key() {
    let rule = unrecognized_rule(r#""name":"x","detail":"y","command":"rm -rf /","#);
    let text = wrap(&[rule_proposal(&rule)]);
    schema(&text, r#""command""#, "proposals[0].rule.command");
}

#[test]
fn an_unrecognized_rule_with_an_expression_field_is_rejected() {
    let rule = unrecognized_rule(r#""name":"x","detail":"y","expr":{},"#);
    let text = wrap(&[rule_proposal(&rule)]);
    schema(&text, r#""expr""#, "proposals[0].rule.expr");
}

#[test]
fn an_unrecognized_rule_without_name_or_detail_is_rejected_at_the_rule() {
    for (extra, missing) in [
        (r#""detail":"y","#, "'name'"),
        (r#""name":"x","#, "'detail'"),
        ("", "'name'"),
    ] {
        let text = wrap(&[rule_proposal(&unrecognized_rule(extra))]);
        let start = col(&text, r#""rule":{"#).map(|c| c + 7);
        let d = fail(&text);
        assert_eq!(
            shape(&d),
            (
                Code::AiResponseSchemaViolation,
                Severity::Error,
                Some(1),
                start,
                Some(RULE_PATH.to_owned())
            ),
            "{extra}"
        );
        assert!(d.message().contains(missing), "{}", d.message());
    }
}

#[test]
fn an_unrecognized_rule_with_wrong_types_is_rejected_at_the_value() {
    let name_seven = wrap(&[rule_proposal(&unrecognized_rule(
        r#""name":7,"detail":[1],"#,
    ))]);
    schema(&name_seven, r#"7,"detail""#, "proposals[0].rule.name");
    let detail_list = wrap(&[rule_proposal(&unrecognized_rule(
        r#""name":"x","detail":[1],"#,
    ))]);
    schema(&detail_list, "[1],\"target\"", "proposals[0].rule.detail");
}

#[test]
fn an_unrecognized_rule_name_must_match_the_name_pattern() {
    for bad in [
        "Slot",
        "1slot",
        "",
        "slot-allocation",
        "slot allocation",
        "é",
    ] {
        let extra = format!(r#""name":"{bad}","detail":"y","#);
        let text = wrap(&[rule_proposal(&unrecognized_rule(&extra))]);
        schema(&text, &format!(r#""{bad}""#), "proposals[0].rule.name");
    }
    for good in ["a", "slot_allocation", "x9_"] {
        let extra = format!(r#""name":"{good}","detail":"y","#);
        assert!(parse(&wrap(&[rule_proposal(&unrecognized_rule(&extra))])).is_ok());
    }
}

#[test]
fn an_arbitrary_rule_kind_is_rejected_before_its_other_fields() {
    let draft = DRAFT.replace(
        r#""kind":"eligibility""#,
        r#""kind":"shell_exec","script":"x""#,
    );
    let text = wrap(&[rule_proposal(&draft)]);
    schema(&text, r#""shell_exec""#, "proposals[0].rule.kind");
}

#[test]
fn a_well_formed_but_unlisted_rule_kind_is_rejected_not_carried() {
    let draft = DRAFT.replace("eligibility", "slot_allocation");
    let text = wrap(&[rule_proposal(&draft)]);
    schema(&text, r#""slot_allocation""#, "proposals[0].rule.kind");
}

#[test]
fn an_unrecognized_node_with_a_command_field_is_rejected_at_the_key() {
    let text = wrap(&[with_right(r#"{"node":"unrecognized","command":"x"}"#)]);
    schema(
        &text,
        r#""command""#,
        "proposals[0].rule.expr.right.command",
    );
}

#[test]
fn an_unrecognized_node_may_not_carry_a_type() {
    let node = r#"{"node":"unrecognized","t":"number","name":"x","detail":"y"}"#;
    let text = wrap(&[with_right(node)]);
    schema(
        &text,
        r#""t":"number","name":"x""#,
        "proposals[0].rule.expr.right.t",
    );
}

#[test]
fn an_unrecognized_node_without_name_or_detail_is_rejected_at_the_node() {
    for (node, missing) in [
        (r#"{"node":"unrecognized","detail":"y"}"#, "'name'"),
        (r#"{"node":"unrecognized","name":"x"}"#, "'detail'"),
        (r#"{"node":"unrecognized"}"#, "'name'"),
    ] {
        let text = wrap(&[with_right(node)]);
        let d = fail(&text);
        assert_eq!(
            shape(&d),
            (
                Code::AiResponseSchemaViolation,
                Severity::Error,
                Some(1),
                col(&text, node),
                Some("proposals[0].rule.expr.right".to_owned())
            ),
            "{node}"
        );
        assert!(d.message().contains(missing), "{}", d.message());
    }
}

#[test]
fn an_unrecognized_node_with_wrong_types_or_name_is_rejected_at_the_value() {
    let name_seven = wrap(&[with_right(
        r#"{"node":"unrecognized","name":7,"detail":[1]}"#,
    )]);
    schema(
        &name_seven,
        r#"7,"detail""#,
        "proposals[0].rule.expr.right.name",
    );
    let detail_list = wrap(&[with_right(
        r#"{"node":"unrecognized","name":"x","detail":[1]}"#,
    )]);
    schema(&detail_list, "[1]}", "proposals[0].rule.expr.right.detail");
    let bad_name = wrap(&[with_right(
        r#"{"node":"unrecognized","name":"Bad-Name","detail":"y"}"#,
    )]);
    schema(
        &bad_name,
        r#""Bad-Name""#,
        "proposals[0].rule.expr.right.name",
    );
}

#[test]
fn an_arbitrary_node_name_is_rejected_at_the_node_value() {
    let text = wrap(&[with_right(r#"{"node":"shell_exec","script":"x"}"#)]);
    schema(
        &text,
        r#""shell_exec""#,
        "proposals[0].rule.expr.right.node",
    );
}

#[test]
fn an_arbitrary_node_inside_an_array_reports_its_index() {
    let draft = DRAFT.replace(
        CMP,
        r#"{"node":"and","t":"boolean","args":[{"node":"bool","t":"boolean","value":true},{"node":"eval","code":"x"}]}"#,
    );
    let text = wrap(&[rule_proposal(&draft)]);
    schema(&text, r#""eval""#, "proposals[0].rule.expr.args[1].node");
}

const CMP: &str = r#"{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}}"#;

#[test]
fn every_vocabulary_node_name_is_still_accepted() {
    let num = |v: &str| format!(r#"{{"node":"num","t":"number","value":"{v}"}}"#);
    let cmp = |left: &str| {
        format!(
            r#"{{"node":"cmp","t":"boolean","op":"ge","left":{left},"right":{}}}"#,
            num("1")
        )
    };
    let exprs = [
        cmp(&num("2")),
        cmp(r#"{"node":"field","t":"number","name":"age"}"#),
        cmp(r#"{"node":"ref","t":"number","target":"score.total"}"#),
        cmp(r#"{"node":"rank_reference","t":"number","target":"rank.main"}"#),
        cmp(&format!(
            r#"{{"node":"binary","t":"number","op":"add","left":{},"right":{}}}"#,
            num("1"),
            num("2")
        )),
        cmp(&format!(
            r#"{{"node":"round","t":"number","arg":{},"places":2,"mode":"half_even"}}"#,
            num("1")
        )),
        cmp(r#"{"node":"agg","t":"number","fn":"sum","collection":"c","field":"f"}"#),
        r#"{"node":"bool","t":"boolean","value":true}"#.to_owned(),
        format!(
            r#"{{"node":"not","t":"boolean","arg":{}}}"#,
            r#"{"node":"bool","t":"boolean","value":false}"#
        ),
        format!(
            r#"{{"node":"and","t":"boolean","args":[{0},{0}]}}"#,
            r#"{"node":"bool","t":"boolean","value":true}"#
        ),
        format!(
            r#"{{"node":"or","t":"boolean","args":[{0},{0}]}}"#,
            r#"{"node":"bool","t":"boolean","value":true}"#
        ),
        r#"{"node":"in","t":"boolean","value":{"node":"field","t":"text","name":"c"},"set":"countries"}"#
            .to_owned(),
        r#"{"node":"cmp","t":"boolean","op":"eq","left":{"node":"text","t":"text","value":"a"},"right":{"node":"field","t":"text","name":"c"}}"#
            .to_owned(),
        r#"{"node":"cmp","t":"boolean","op":"eq","left":{"node":"date","t":"date","value":"2024-01-31"},"right":{"node":"field","t":"date","name":"d"}}"#
            .to_owned(),
        r#"{"node":"cmp","t":"boolean","op":"eq","left":{"node":"enum","t":"enum:level","enum":"level","member":"gold"},"right":{"node":"field","t":"enum:level","name":"l"}}"#
            .to_owned(),
    ];
    for expr in exprs {
        let text = wrap(&[rule_proposal(&DRAFT.replace(CMP, &expr))]);
        assert!(parse(&text).is_ok(), "{expr}");
    }
}

fn forbidden_after(prefix_keys: &str) -> String {
    format!(r#"{{{prefix_keys}"status":"x"}}"#)
}

#[test]
fn a_forbidden_name_behind_many_repeated_keys_is_0204() {
    let ten: String = (0..10).map(|i| format!(r#""a":{i},"#)).collect();
    let text = forbidden_after(&ten);
    expect(
        &text,
        Code::AiResponseForbiddenField,
        r#""status""#,
        "status",
    );
}

#[test]
fn a_forbidden_name_behind_an_escaped_repeated_key_is_0204() {
    let text = forbidden_after(r#""a":1,"\u0061":2,"#);
    expect(
        &text,
        Code::AiResponseForbiddenField,
        r#""status""#,
        "status",
    );
}

#[test]
fn a_forbidden_name_behind_a_non_ascii_repeated_key_is_0204() {
    let text = forbidden_after(r#""é":1,"é":2,"#);
    expect(
        &text,
        Code::AiResponseForbiddenField,
        r#""status""#,
        "status",
    );
}

#[test]
fn a_forbidden_name_behind_a_repeated_key_with_a_quote_is_0204() {
    let text = forbidden_after(r#""a\"":1,"a\"":2,"#);
    expect(
        &text,
        Code::AiResponseForbiddenField,
        r#""status""#,
        "status",
    );
}

#[test]
fn a_forbidden_name_behind_a_nested_repeated_key_is_0204() {
    let text = r#"{"n":{"a":1,"a":2},"schema_version":"1","proposals":[],"approved_by":"x"}"#;
    expect(
        text,
        Code::AiResponseForbiddenField,
        r#""approved_by""#,
        "approved_by",
    );
}

#[test]
fn a_forbidden_name_after_a_repeated_key_in_a_proposal_is_0204() {
    let text = wrap(&[proposal("candidate_test").replace(
        r#""rationale":"why""#,
        r#""rationale":"why","rationale":"again","is_approved":true"#,
    )]);
    expect(
        &text,
        Code::AiResponseForbiddenField,
        r#""is_approved""#,
        "proposals[0].is_approved",
    );
}

#[test]
fn a_forbidden_name_after_a_repeated_key_inside_the_rule_is_0204() {
    let draft = DRAFT.replace(
        r#""target":"eligible2""#,
        r#""target":"eligible2","target":"x""#,
    );
    let one = rule_proposal(&draft);
    let head = one.strip_suffix('}').unwrap();
    let text = wrap(&[format!(r#"{head},"validation_result":"ok"}}"#)]);
    expect(
        &text,
        Code::AiResponseForbiddenField,
        r#""validation_result""#,
        "proposals[0].validation_result",
    );
}

#[test]
fn a_forbidden_name_inside_a_repeated_key_object_is_found_in_every_copy() {
    let text = r#"{"x":{"a":1},"x":{"passed":true},"schema_version":"1","proposals":[]}"#;
    expect(
        text,
        Code::AiResponseForbiddenField,
        r#""passed""#,
        "x.passed",
    );
}

#[test]
fn a_repeated_key_alone_is_0202_with_its_path() {
    let ten: String = (0..10).map(|i| format!(r#""a":{i},"#)).collect();
    let text = format!(r#"{{{ten}"schema_version":"1","proposals":[]}}"#);
    let second = text.find(r#""a":1"#).unwrap();
    let want = u32::try_from(second).unwrap() + 1;
    assert_eq!(
        shape(&fail(&text)),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            Some(want),
            Some("a".to_owned())
        )
    );
}

#[test]
fn a_repeated_key_inside_the_rule_is_0202_at_the_second_key() {
    let draft = DRAFT.replace(
        r#""target":"eligible2""#,
        r#""target":"eligible2","target":"x""#,
    );
    let text = wrap(&[rule_proposal(&draft)]);
    schema(&text, r#""target":"x""#, "proposals[0].rule.target");
}

#[test]
fn a_repeated_key_followed_by_a_syntax_error_is_0201() {
    let text = r#"{"a":1,"a":2,"#;
    assert_eq!(fail(text).code(), Code::AiResponseMalformed);
}
