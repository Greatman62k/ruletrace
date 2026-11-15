//! Tests of the flat node-table form, Proposal_Schema version "2" (UD-37,
//! Req 23.39, design 3.7.3). Expressions are built from a small model in
//! this file, which writes both the nested text and the flat table, so the
//! expected text is not derived from the lowering under test.

use crate::accept::{accept_batch, AcceptInput, AcceptedBatch};
use crate::plan::ClauseId;
use crate::proposal_schema::proposal_schema_bytes;
use crate::redact::Redactor;
use crate::tests_proposal_schema::tree;
use crate::tests_provenance::{doc, models, RULES_HASH};
use crate::tests_response::{col, fail, parse_with, rid, shape, LIMIT};
use crate::vocab::Ud32Option;
use proptest::prelude::*;
use rt_ai_adapter::Completion;
use rt_foundation::{Code, ExitCode, InputId, Severity};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, Rulebook};
use rt_json::{Kind, Position, Value};

const SPAN: &str = r#"{"document":"rules.md","start":12,"end":27,"line":2,"column":1}"#;
const QUOTE: &str = "Gamma δ delta.";

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

fn evidence() -> String {
    format!(
        r#"{{"document":"rules.md","sha256":"{RULES_HASH}","start":12,"end":27,"line":2,"column":1,"quote":"{QUOTE}"}}"#
    )
}

fn suggestion(rule: &str) -> String {
    format!(
        r#"{{"kind":"rule_suggestion","proposer":"model-1","rationale":"why","evidence":[{}],"rule":{rule}}}"#,
        evidence()
    )
}

fn payload(version: &str, rule: &str) -> String {
    format!(
        r#"{{"schema_version":"{version}","proposals":[{}]}}"#,
        suggestion(rule)
    )
}

fn accepted(text: &str) -> AcceptedBatch {
    let id = rid();
    let rb = rulebook();
    let all = models();
    let req = sha256(b"request-bytes");
    let clauses = vec![ClauseId {
        document: doc("rules.md"),
        start: 12,
        end: 27,
    }];
    accept_batch(&AcceptInput {
        response_id: &id,
        request_id: &req,
        completion: Completion::Complete,
        payload: text.as_bytes(),
        response_limit: LIMIT,
        models: &all,
        clauses: &clauses,
        declarations: &rb,
        redactor: &Redactor::no_credential(),
        revise_rule: None,
    })
    .unwrap_or_else(|d| panic!("rejected: {d:?}"))
}

#[derive(Clone, Debug)]
enum E {
    Num(u8),
    Bool(bool),
    Field,
    Not(Box<E>),
    And(Vec<E>),
    Or(Vec<E>),
    Bin(&'static str, Box<E>, Box<E>),
    Cmp(&'static str, Box<E>, Box<E>),
    In(Box<E>),
    Agg(&'static str, bool),
    Round(Box<E>, u8),
}

impl E {
    fn kids(&self) -> Vec<&E> {
        match self {
            E::Not(a) | E::In(a) | E::Round(a, _) => vec![a],
            E::And(v) | E::Or(v) => v.iter().collect(),
            E::Bin(_, l, r) | E::Cmp(_, l, r) => vec![l, r],
            _ => Vec::new(),
        }
    }

    fn nested(&self) -> String {
        let k: Vec<String> = self.kids().iter().map(|e| e.nested()).collect();
        self.write(&k)
    }

    // Writes the node with `k` standing for its children, in key order.
    fn write(&self, k: &[String]) -> String {
        match self {
            E::Num(n) => format!(r#"{{"node":"num","t":"number","value":"{n}"}}"#),
            E::Bool(b) => format!(r#"{{"node":"bool","t":"boolean","value":{b}}}"#),
            E::Field => r#"{"node":"field","t":"number","name":"age"}"#.to_owned(),
            E::Not(_) => format!(r#"{{"node":"not","t":"boolean","arg":{}}}"#, k[0]),
            E::And(_) => format!(r#"{{"node":"and","t":"boolean","args":[{}]}}"#, k.join(",")),
            E::Or(_) => format!(r#"{{"node":"or","t":"boolean","args":[{}]}}"#, k.join(",")),
            E::Bin(op, ..) => format!(
                r#"{{"node":"binary","t":"number","op":"{op}","left":{},"right":{}}}"#,
                k[0], k[1]
            ),
            E::Cmp(op, ..) => format!(
                r#"{{"node":"cmp","t":"boolean","op":"{op}","left":{},"right":{}}}"#,
                k[0], k[1]
            ),
            E::In(_) => format!(
                r#"{{"node":"in","t":"boolean","value":{},"set":"s"}}"#,
                k[0]
            ),
            E::Agg(f, with_field) => {
                let field = if *with_field { r#""f""# } else { "null" };
                format!(
                    r#"{{"node":"agg","t":"number","fn":"{f}","collection":"c","field":{field}}}"#
                )
            }
            E::Round(_, p) => format!(
                r#"{{"node":"round","t":"number","arg":{},"places":{p},"mode":"half_even"}}"#,
                k[0]
            ),
        }
    }

    fn depth(&self) -> usize {
        1 + self.kids().iter().map(|e| e.depth()).max().unwrap_or(0)
    }
}

/// Table in preorder (`breadth` false) or breadth-first order. A child
/// index is greater than its parent's in both.
fn table(root: &E, breadth: bool) -> Vec<String> {
    let mut order: Vec<&E> = vec![root];
    let mut kids: Vec<Vec<usize>> = Vec::new();
    if breadth {
        let mut i = 0;
        while i < order.len() {
            let mut mine = Vec::new();
            for k in order[i].kids() {
                mine.push(order.len());
                order.push(k);
            }
            kids.push(mine);
            i += 1;
        }
    } else {
        fn visit<'a>(e: &'a E, order: &mut Vec<&'a E>, kids: &mut Vec<Vec<usize>>) -> usize {
            let me = order.len();
            order.push(e);
            kids.push(Vec::new());
            let mut mine = Vec::new();
            for k in e.kids() {
                mine.push(visit(k, order, kids));
            }
            kids[me] = mine;
            me
        }
        order.clear();
        visit(root, &mut order, &mut kids);
    }
    order
        .iter()
        .zip(&kids)
        .map(|(e, k)| {
            let text: Vec<String> = k.iter().map(ToString::to_string).collect();
            e.write(&text)
        })
        .collect()
}

fn nested_rule(e: &E) -> String {
    format!(
        r#"{{"id":"elig.new","kind":"eligibility","target":"eligible2","type":"boolean","expr":{},"markers":[],"spans":[{SPAN}]}}"#,
        e.nested()
    )
}

fn flat_rule(e: &E, breadth: bool) -> String {
    format!(
        r#"{{"id":"elig.new","kind":"eligibility","target":"eligible2","type":"boolean","expr":0,"nodes":[{}],"markers":[],"spans":[{SPAN}]}}"#,
        table(e, breadth).join(",")
    )
}

/// Stores `nested` as version 1 and `flat` as version 2 and checks that the
/// stored draft text, its hash and its findings are the same. The stored
/// text is also the canonical text that `nested` was written in.
fn same_rule(nested: &str, flat: &str) -> AcceptedBatch {
    let a = accepted(&payload("1", nested));
    let b = accepted(&payload("2", flat));
    let (da, ha) = a.proposals()[0].rule.as_ref().unwrap();
    let (db, hb) = b.proposals()[0].rule.as_ref().unwrap();
    assert_eq!(da.json, nested, "the nested draft is stored canonically");
    assert_eq!(db, da);
    assert_eq!(hb, ha);
    assert_eq!(b.proposals()[0].outcome, a.proposals()[0].outcome);
    b
}

fn chain(depth: usize) -> E {
    let mut e = E::Bool(true);
    for _ in 1..depth {
        e = E::Not(Box::new(e));
    }
    e
}

#[test]
fn an_expression_of_depth_16_lowers_to_the_same_rule_and_hash() {
    let e = chain(16);
    assert_eq!(e.depth(), 16);
    for breadth in [false, true] {
        let b = same_rule(&nested_rule(&e), &flat_rule(&e, breadth));
        assert!(b.proposals()[0].outcome.is_empty(), "{breadth}");
    }
}

#[test]
fn an_expression_of_depth_17_reaches_the_validator_as_rt_lim_0003() {
    let e = chain(17);
    assert_eq!(e.depth(), 17);
    let b = same_rule(&nested_rule(&e), &flat_rule(&e, false));
    let found = &b.proposals()[0].outcome;
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code, Code::LimitExpressionTooDeep);
    assert_eq!(found[0].code.id(), "RT-LIM-0003");
    assert_eq!(found[0].code.severity(), Severity::Error);
}

#[test]
fn a_flat_table_deeper_than_the_nested_container_limit_still_reaches_the_validator() {
    // 17 levels of `and` use 34 containers in nested form, above the limit
    // of 32 for a response, so the nested form is refused at the parser.
    let mut e = E::Bool(true);
    for _ in 1..17 {
        e = E::And(vec![e, E::Bool(false)]);
    }
    let nested = fail(&payload("1", &nested_rule(&e)));
    assert_eq!(nested.code(), Code::AiResponseSchemaViolation);
    let b = accepted(&payload("2", &flat_rule(&e, false)));
    let found = &b.proposals()[0].outcome;
    assert!(found.iter().any(|d| d.code == Code::LimitExpressionTooDeep));
}

fn leaf() -> impl Strategy<Value = E> {
    prop_oneof![
        (0u8..100).prop_map(E::Num),
        any::<bool>().prop_map(E::Bool),
        Just(E::Field),
        (
            prop::sample::select(vec!["sum", "min", "max", "count"]),
            any::<bool>()
        )
            .prop_map(|(f, w)| E::Agg(f, w)),
    ]
}

fn expr() -> impl Strategy<Value = E> {
    leaf().prop_recursive(15, 200, 4, |inner| {
        prop_oneof![
            inner.clone().prop_map(|e| E::Not(Box::new(e))),
            prop::collection::vec(inner.clone(), 2..5).prop_map(E::And),
            prop::collection::vec(inner.clone(), 2..5).prop_map(E::Or),
            (
                prop::sample::select(vec!["add", "sub", "mul", "div"]),
                inner.clone(),
                inner.clone()
            )
                .prop_map(|(op, l, r)| E::Bin(op, Box::new(l), Box::new(r))),
            (
                prop::sample::select(vec!["eq", "ne", "lt", "le", "gt", "ge"]),
                inner.clone(),
                inner.clone()
            )
                .prop_map(|(op, l, r)| E::Cmp(op, Box::new(l), Box::new(r))),
            inner.clone().prop_map(|e| E::In(Box::new(e))),
            (inner, 0u8..28).prop_map(|(e, p)| E::Round(Box::new(e), p)),
        ]
    })
}

/// A spine of exactly `depth` nodes with random operators and siblings.
fn spine(depth: usize) -> impl Strategy<Value = E> {
    let step = (0usize..5, leaf());
    prop::collection::vec(step, depth - 1).prop_map(move |steps| {
        let mut e = E::Bool(true);
        for (kind, other) in steps {
            e = match kind {
                0 => E::Not(Box::new(e)),
                1 => E::And(vec![other, e]),
                2 => E::Bin("add", Box::new(e), Box::new(other)),
                3 => E::Cmp("lt", Box::new(other), Box::new(e)),
                _ => E::Round(Box::new(e), 2),
            };
        }
        e
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn any_expression_round_trips_through_either_table_order(e in expr()) {
        same_rule(&nested_rule(&e), &flat_rule(&e, false));
        same_rule(&nested_rule(&e), &flat_rule(&e, true));
    }

    #[test]
    fn any_expression_in_a_table_conforms_to_the_generated_schema(e in expr()) {
        prop_assert!(conforming(&payload("2", &flat_rule(&e, false))));
        prop_assert!(conforming(&payload("2", &flat_rule(&e, true))));
    }

    #[test]
    fn a_spine_of_depth_16_or_17_round_trips(
        e in prop_oneof![spine(16), spine(17)],
        breadth in any::<bool>(),
    ) {
        let want = e.depth();
        prop_assert!(want == 16 || want == 17);
        let b = same_rule(&nested_rule(&e), &flat_rule(&e, breadth));
        let too_deep = b.proposals()[0]
            .outcome
            .iter()
            .any(|d| d.code == Code::LimitExpressionTooDeep);
        prop_assert_eq!(too_deep, want == 17);
    }
}

/// Pairs of a nested rule and its flat form, one for each other rule kind.
fn kind_cases() -> Vec<(String, String)> {
    let lit = |n: &str| format!(r#"{{"node":"num","t":"number","value":"{n}"}}"#);
    let field = r#"{"node":"field","t":"number","name":"age"}"#;
    let tail = format!(r#""markers":[],"spans":[{SPAN}]"#);
    let head = |kind: &str, ty: &str| {
        format!(r#"{{"id":"elig.new","kind":"{kind}","target":"eligible2","type":"{ty}","#)
    };
    vec![
        (
            format!(
                r#"{}"terms":[{{"weight":{},"value":{field}}},{{"weight":{},"value":{}}}],"rounding":{{"mode":"half_even","places":2}},{tail}}}"#,
                head("weighted_score", "number"),
                lit("2"),
                lit("3"),
                lit("4")
            ),
            format!(
                r#"{}"terms":[{{"weight":0,"value":1}},{{"weight":2,"value":3}}],"rounding":{{"mode":"half_even","places":2}},"nodes":[{},{field},{},{}],{tail}}}"#,
                head("weighted_score", "number"),
                lit("2"),
                lit("3"),
                lit("4")
            ),
        ),
        (
            format!(
                r#"{}"key":{field},"direction":"desc","tie_breakers":[{{"expr":{},"direction":"asc"}}],"ties":"group",{tail}}}"#,
                head("ranking", "ranking"),
                lit("1")
            ),
            format!(
                r#"{}"key":0,"direction":"desc","tie_breakers":[{{"expr":1,"direction":"asc"}}],"ties":"group","nodes":[{field},{}],{tail}}}"#,
                head("ranking", "ranking"),
                lit("1")
            ),
        ),
        (
            format!(
                r#"{}"value":{},{tail}}}"#,
                head("default", "number"),
                lit("0")
            ),
            format!(
                r#"{}"value":0,"nodes":[{}],{tail}}}"#,
                head("default", "number"),
                lit("0")
            ),
        ),
        (
            format!(
                r#"{}"name":"slot_allocation","detail":"top four",{tail}}}"#,
                head("unrecognized", "boolean")
            ),
            format!(
                r#"{}"name":"slot_allocation","detail":"top four",{tail}}}"#,
                head("unrecognized", "boolean")
            ),
        ),
    ]
}

#[test]
fn every_rule_kind_lowers_to_its_nested_form() {
    for (nested, flat) in kind_cases() {
        same_rule(&nested, &flat);
    }
}

/// True when `value` is described by `schema`. Supports exactly the
/// keywords that the generated schema uses.
fn conforms(schema: &Value<Position>, value: &Value<Position>) -> bool {
    let member = |v: &'static str| schema.get(v).map(|m| &m.value);
    if let Some(alternatives) = member("anyOf").and_then(Value::as_array) {
        return alternatives.iter().any(|a| conforms(a, value));
    }
    let kind = member("type").and_then(Value::as_str);
    let allowed = member("enum").and_then(Value::as_array);
    let in_enum = |v: &Value<Position>| {
        allowed.is_none_or(|list| list.iter().any(|e| e.as_str() == v.as_str()))
    };
    match (kind, value.kind()) {
        (Some("string"), Kind::String(_)) => in_enum(value),
        (Some("integer"), Kind::Integer(_)) | (Some("boolean"), Kind::Bool(_)) => true,
        (Some("null"), Kind::Null) => true,
        (Some("array"), Kind::Array(items)) => {
            member("items").is_some_and(|inner| items.iter().all(|i| conforms(inner, i)))
        }
        (Some("object"), Kind::Object(fields)) => {
            let props = member("properties").map(|p| p.as_object().unwrap_or_default());
            let required = member("required").and_then(Value::as_array);
            let props = props.unwrap_or_default();
            let closed = fields.iter().all(|f| props.iter().any(|p| p.key == f.key));
            let complete = required
                .unwrap_or_default()
                .iter()
                .all(|r| fields.iter().any(|f| Some(f.key.as_str()) == r.as_str()));
            closed
                && complete
                && fields.iter().all(|f| {
                    props
                        .iter()
                        .find(|p| p.key == f.key)
                        .is_some_and(|p| conforms(&p.value, &f.value))
                })
        }
        _ => false,
    }
}

fn schema() -> Value<Position> {
    tree(&proposal_schema_bytes(Ud32Option::RankReference).unwrap())
}

fn conforming(text: &str) -> bool {
    conforms(&schema(), &tree(text.as_bytes()))
}

#[test]
fn flat_payloads_that_the_parser_accepts_conform_to_the_generated_schema() {
    let mut texts: Vec<String> = kind_cases()
        .into_iter()
        .map(|(_, flat)| payload("2", &flat))
        .collect();
    texts.push(payload("2", &flat_rule(&chain(16), false)));
    for text in texts {
        assert!(parse_with(Completion::Complete, text.as_bytes(), LIMIT).is_ok());
        assert!(conforming(&text), "{text}");
    }
}

#[test]
fn the_conformance_check_rejects_a_wrong_shape() {
    let flat = flat_rule(&chain(2), false);
    assert!(conforming(&payload("2", &flat)));
    // The nested form, an extra member, a missing member, a wrong tag and
    // a wrong type each leave the schema.
    let broken = [
        payload("2", &nested_rule(&chain(2))),
        payload(
            "2",
            &flat.replace(r#""markers":[],"#, r#""markers":[],"script":"x","#),
        ),
        payload("2", &flat.replace(r#""markers":[],"#, "")),
        payload("2", &flat.replace(r#""node":"not""#, r#""node":"shell""#)),
        payload("2", &flat.replace(r#""expr":0"#, r#""expr":"0""#)),
        payload("1", &flat),
    ];
    for text in broken {
        assert!(!conforming(&text), "{text}");
    }
}

#[test]
fn a_flat_unrecognized_node_and_a_null_aggregate_field_lower_unchanged() {
    let nested = r#"{"id":"elig.new","kind":"eligibility","target":"eligible2","type":"boolean","expr":{"node":"and","t":"boolean","args":[{"node":"unrecognized","name":"slot_allocation","detail":"top four"},{"node":"cmp","t":"boolean","op":"gt","left":{"node":"agg","t":"number","fn":"count","collection":"c","field":null},"right":{"node":"num","t":"number","value":"1"}}]},"markers":[{"kind":"ambiguous","interpretations":["a","b"]}],"spans":[{"document":"rules.md","start":12,"end":27,"line":2,"column":1}]}"#;
    let flat = r#"{"id":"elig.new","kind":"eligibility","target":"eligible2","type":"boolean","expr":0,"nodes":[{"node":"and","t":"boolean","args":[1,2]},{"node":"unrecognized","name":"slot_allocation","detail":"top four"},{"node":"cmp","t":"boolean","op":"gt","left":3,"right":4},{"node":"agg","t":"number","fn":"count","collection":"c","field":null},{"node":"num","t":"number","value":"1"}],"markers":[{"kind":"ambiguous","interpretations":["a","b"]}],"spans":[{"document":"rules.md","start":12,"end":27,"line":2,"column":1}]}"#;
    same_rule(nested, flat);
}

// A rule over a table, with one `not` node 0 over node 1 over a bool.
fn table_rule(nodes: &str) -> String {
    format!(
        r#"{{"id":"elig.new","kind":"eligibility","target":"eligible2","type":"boolean","expr":0,"nodes":[{nodes}],"markers":[],"spans":[{SPAN}]}}"#
    )
}

const NOT_0_1: &str = r#"{"node":"not","t":"boolean","arg":1}"#;
const BOOL: &str = r#"{"node":"bool","t":"boolean","value":true}"#;

fn flat_payload(rule: &str) -> String {
    payload("2", rule)
}

/// `at` is a context that occurs once in `text` and the value that the
/// Diagnostic must point at, found by its last occurrence in the context.
fn expect_flat(text: &str, at: (&str, &str), field: &str, message: &str) {
    let (context, value) = at;
    let column =
        col(text, context).map(|c| c + u32::try_from(context.rfind(value).unwrap()).unwrap());
    let d = fail(text);
    assert_eq!(
        shape(&d),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            column,
            Some(field.to_owned())
        ),
        "{}",
        d.message()
    );
    assert!(d.message().contains(message), "{}", d.message());
    assert_eq!(d.code().exit_code(), ExitCode::Response);
}

#[test]
fn a_valid_two_node_table_is_accepted() {
    let text = flat_payload(&table_rule(&format!("{NOT_0_1},{BOOL}")));
    assert!(parse_with(Completion::Complete, text.as_bytes(), LIMIT).is_ok());
}

#[test]
fn a_node_that_names_itself_is_refused_at_its_operand() {
    let text = flat_payload(&table_rule(r#"{"node":"not","t":"boolean","arg":0}"#));
    expect_flat(
        &text,
        (r#""arg":0}"#, "0"),
        "proposals[0].rule.nodes[0].arg",
        "greater index",
    );
}

#[test]
fn a_cycle_is_refused_at_the_edge_that_points_back() {
    let text = flat_payload(&table_rule(&format!(
        r#"{NOT_0_1},{{"node":"not","t":"boolean","arg":0}}"#
    )));
    expect_flat(
        &text,
        (r#""arg":0}"#, "0"),
        "proposals[0].rule.nodes[1].arg",
        "greater index",
    );
}

#[test]
fn a_child_with_a_lower_index_than_its_parent_is_refused() {
    let text = flat_payload(&table_rule(&format!(
        r#"{{"node":"not","t":"boolean","arg":2}},{BOOL},{{"node":"not","t":"boolean","arg":1}}"#
    )));
    expect_flat(
        &text,
        (r#""arg":1}"#, "1"),
        "proposals[0].rule.nodes[2].arg",
        "greater index",
    );
}

#[test]
fn a_shared_node_is_refused_at_its_second_reference() {
    let text = flat_payload(&table_rule(&format!(
        r#"{{"node":"and","t":"boolean","args":[1,1]}},{BOOL}"#
    )));
    expect_flat(
        &text,
        (r#""args":[1,1]"#, "1"),
        "proposals[0].rule.nodes[0].args[1]",
        "named more than once",
    );
}

#[test]
fn a_node_that_the_rule_names_twice_is_refused() {
    let rule = format!(
        r#"{{"id":"elig.new","kind":"ranking","target":"t","type":"ranking","key":0,"direction":"asc","tie_breakers":[{{"expr":0,"direction":"asc"}}],"ties":"group","nodes":[{BOOL}],"markers":[],"spans":[{SPAN}]}}"#
    );
    let text = flat_payload(&rule);
    expect_flat(
        &text,
        (r#""expr":0"#, "0"),
        "proposals[0].rule.tie_breakers[0].expr",
        "named more than once",
    );
}

#[test]
fn an_unreferenced_node_is_refused_at_the_node() {
    let text = flat_payload(&table_rule(&format!(r#"{BOOL},{BOOL}"#)));
    let d = fail(&text);
    assert_eq!(d.code(), Code::AiResponseSchemaViolation);
    let (_, _, line, column, field) = shape(&d);
    assert_eq!(line, Some(1));
    assert_eq!(field.as_deref(), Some("proposals[0].rule.nodes[1]"));
    let second = text.rfind(BOOL).unwrap();
    let chars = u32::try_from(text[..second].chars().count()).unwrap();
    assert_eq!(column, Some(chars + 1));
    assert!(d.message().contains("node 1 is not named"));
}

#[test]
fn an_out_of_range_index_is_refused_at_the_index() {
    let text = flat_payload(&table_rule(r#"{"node":"not","t":"boolean","arg":7}"#));
    expect_flat(
        &text,
        (r#""arg":7}"#, "7"),
        "proposals[0].rule.nodes[0].arg",
        "out of range",
    );
    let rule = table_rule(BOOL).replace(r#""expr":0"#, r#""expr":3"#);
    expect_flat(
        &flat_payload(&rule),
        (r#""expr":3,"#, "3"),
        "proposals[0].rule.expr",
        "out of range",
    );
}

#[test]
fn an_index_that_is_not_a_non_negative_integer_is_refused() {
    for bad in [r#""0""#, "-1", "0.5"] {
        let context = format!(r#""expr":{bad},"#);
        let rule = table_rule(BOOL).replace(r#""expr":0,"#, &context);
        expect_flat(
            &flat_payload(&rule),
            (&context, bad),
            "proposals[0].rule.expr",
            "non-negative integer",
        );
    }
    let nested_operand = table_rule(
        r#"{"node":"not","t":"boolean","arg":{"node":"bool","t":"boolean","value":true}}"#,
    );
    expect_flat(
        &flat_payload(&nested_operand),
        (r#""arg":{"node":"bool""#, "{"),
        "proposals[0].rule.nodes[0].arg",
        "node index",
    );
}

#[test]
fn a_nested_rule_in_a_version_2_response_and_the_reverse_are_refused() {
    let nested = nested_rule(&E::Bool(true));
    expect_flat(
        &payload("2", &nested),
        (r#""expr":{"node":"bool""#, "{"),
        "proposals[0].rule.expr",
        "non-negative integer",
    );
    let flat = flat_rule(&E::Bool(true), false);
    let d = fail(&payload("1", &flat));
    assert_eq!(d.code(), Code::AiResponseSchemaViolation);
    let context = r#""rule":{"id""#;
    assert_eq!(
        shape(&d),
        (
            Code::AiResponseSchemaViolation,
            Severity::Error,
            Some(1),
            col(&payload("1", &flat), context).map(|c| c + 7),
            Some("proposals[0].rule.expr".to_owned())
        ),
        "{}",
        d.message()
    );
}

#[test]
fn a_node_name_outside_the_vocabulary_is_refused_in_a_table() {
    let text = flat_payload(&table_rule(r#"{"node":"shell_exec","t":"boolean"}"#));
    expect_flat(
        &text,
        (r#""node":"shell_exec""#, r#""shell_exec""#),
        "proposals[0].rule.nodes[0].node",
        "unknown expression node",
    );
}

#[test]
fn an_unknown_member_in_a_table_node_is_refused() {
    let text = flat_payload(&table_rule(
        r#"{"node":"bool","t":"boolean","value":true,"script":"x"}"#,
    ));
    expect_flat(
        &text,
        (r#""script":"x""#, r#""script""#),
        "proposals[0].rule.nodes[0].script",
        "unknown field",
    );
}

#[test]
fn only_versions_1_and_2_are_accepted() {
    for version in ["0", "3", "1.0", "", "2 "] {
        let text = payload(version, &flat_rule(&E::Bool(true), false));
        let d = fail(&text);
        assert_eq!(
            shape(&d),
            (
                Code::AiResponseSchemaViolation,
                Severity::Error,
                Some(1),
                col(&text, &format!(r#""{version}""#)),
                Some("schema_version".to_owned())
            ),
            "{version:?}"
        );
    }
}

#[test]
fn a_hostile_chain_of_twenty_thousand_nodes_is_refused_without_overflow() {
    let count = 20_000usize;
    let nodes: Vec<String> = (0..count)
        .map(|i| {
            if i + 1 == count {
                BOOL.to_owned()
            } else {
                format!(r#"{{"node":"not","t":"boolean","arg":{}}}"#, i + 1)
            }
        })
        .collect();
    let text = flat_payload(&table_rule(&nodes.join(",")));
    let d = fail(&text);
    assert_eq!(d.code(), Code::AiResponseSchemaViolation);
    assert_eq!(d.severity(), Severity::Error);
    assert!(d.message().contains("not valid Rule_IR"), "{}", d.message());
}
