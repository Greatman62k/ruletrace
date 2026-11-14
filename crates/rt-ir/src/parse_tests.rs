//! Tests of the Rule_IR parser and the canonical printers (task 4.5).

use super::*;
use rt_foundation::{Code, Diagnostic, InputId, Location};
use rt_json::ParseLimits;

const HASH: &str = "eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a";

/// The `elig.basic` rule of design §4.1.7, compact, in canonical key order.
const ELIG: &str = r#"{"id":"elig.basic","revision":1,"origin":{"kind":"manual","author":null},"kind":"eligibility","target":"eligible","type":"boolean","expr":{"node":"and","t":"boolean","args":[{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}},{"node":"field","t":"boolean","name":"licence"}]},"markers":[],"spans":[{"document":"rules.md","start":21,"end":91,"line":3,"column":1}]}"#;

fn input() -> InputId {
    InputId::new("rulebook.json").unwrap()
}

fn compact_doc(rules: &[&str]) -> String {
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"spring-open","version":"2026.1","documents":[{{"id":"rules.md","sha256":"{HASH}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}},{{"name":"licence","type":"boolean"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{}],"precedence":[]}}"#,
        rules.join(",")
    )
}

/// The canonical pretty form of a compact document, made by `rt-json` and not by the printer
/// under test.
fn pretty_of(compact: &str) -> String {
    let v = rt_json::parse(&input(), compact.as_bytes(), ParseLimits::new(1 << 24, 64)).unwrap();
    rt_json::write_pretty(&v)
}

fn parse(text: &str) -> Result<Rulebook, Vec<Diagnostic>> {
    parse_rulebook(text.as_bytes(), &input(), 8 << 20, 64)
        .map(|s| s.into_parts().0)
        .map_err(|r| r.into_diagnostics().as_slice().to_vec())
}

fn fail(text: &str) -> Vec<Diagnostic> {
    match parse(text) {
        Ok(_) => panic!("expected a rejection of {text}"),
        Err(d) => d,
    }
}

fn line_col(d: &Diagnostic) -> (u32, u32) {
    match d.primary() {
        Location::Input(l) => (l.line.unwrap(), l.column.unwrap()),
        other => panic!("no input location: {other:?}"),
    }
}

fn field(d: &Diagnostic) -> Option<String> {
    match d.primary() {
        Location::Input(l) => l.field.clone(),
        _ => None,
    }
}

/// 1-based line and column of the first occurrence of `needle` in `text`.
fn pos_of(text: &str, needle: &str) -> (u32, u32) {
    let at = text.find(needle).unwrap();
    let before = &text[..at];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
    (u32::try_from(line).unwrap(), u32::try_from(column).unwrap())
}

fn codes(d: &[Diagnostic]) -> Vec<Code> {
    d.iter().map(Diagnostic::code).collect()
}

fn round_trip(text: &str) -> Rulebook {
    let a = parse(text).unwrap();
    let pretty = print_pretty(&a).unwrap();
    let b = parse(&pretty).unwrap();
    assert_eq!(a, b, "pretty round trip");
    assert_eq!(print_pretty(&b).unwrap(), pretty, "pretty is a fixed point");
    let compact = print_compact(&a).unwrap();
    assert_eq!(parse(&compact).unwrap(), a, "compact round trip");
    assert_eq!(print_compact(&b).unwrap(), compact);
    a
}

// ----- worked example -----

#[test]
fn worked_example_pretty_bytes_are_reproduced() {
    let golden = pretty_of(&compact_doc(&[ELIG]));
    let rb = parse(&golden).unwrap();
    assert_eq!(print_pretty(&rb).unwrap(), golden);
    assert_eq!(print_compact(&rb).unwrap(), compact_doc(&[ELIG]));
    assert_eq!(rb.revisions.len(), 1);
    assert_eq!(rb.revisions[0].content.body.kind_name(), "eligibility");
    let at = rb.revisions[0].loc.position().unwrap();
    assert_eq!(
        (at.line, at.column),
        pos_of(&golden, "{\n      \"id\": \"elig.basic\"")
    );
}

#[test]
fn design_excerpt_with_inline_objects_prints_canonically() {
    let excerpt = r#"{
  "format": "ruletrace-rulebook",
  "ir_version": "1",
  "name": "spring-open",
  "version": "2026.1",
  "documents": [
    { "id": "rules.md", "sha256": "eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a" }
  ],
  "types": {
    "inputs": [ { "name": "age", "type": "number" }, { "name": "licence", "type": "boolean" } ],
    "enums": [], "sets": [], "collections": []
  },
  "rules": [
    {
      "id": "elig.basic", "revision": 1,
      "origin": { "kind": "manual", "author": null },
      "kind": "eligibility", "target": "eligible", "type": "boolean",
      "expr": {
        "node": "and", "t": "boolean",
        "args": [
          { "node": "cmp", "t": "boolean", "op": "ge",
            "left": { "node": "field", "t": "number", "name": "age" },
            "right": { "node": "num", "t": "number", "value": "16" } },
          { "node": "field", "t": "boolean", "name": "licence" }
        ]
      },
      "markers": [],
      "spans": [ { "document": "rules.md", "start": 21, "end": 91, "line": 3, "column": 1 } ]
    }
  ],
  "precedence": []
}
"#;
    let rb = parse(excerpt).unwrap();
    assert_eq!(print_pretty(&rb).unwrap(), pretty_of(&compact_doc(&[ELIG])));
    // The compact Rule_Content of design §4.1.7 reads the same expression text.
    let expr = r#""expr":{"node":"and","t":"boolean","args":[{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}},{"node":"field","t":"boolean","name":"licence"}]}"#;
    assert!(print_compact(&rb).unwrap().contains(expr));
}

#[test]
fn printing_sorts_rules_by_id_bytes_then_revision() {
    let r = |id: &str, rev: u32| {
        ELIG.replace("elig.basic", id)
            .replace("\"revision\":1", &format!("\"revision\":{rev}"))
    };
    let shuffled = compact_doc(&[&r("b", 1), &r("a-", 1), &r("a", 2), &r("a", 1)]);
    let rb = parse(&shuffled).unwrap();
    // Document order is kept by the parser.
    assert_eq!(rb.revisions[0].key.id.as_str(), "b");
    let sorted = compact_doc(&[&r("a", 1), &r("a", 2), &r("a-", 1), &r("b", 1)]);
    assert_eq!(print_compact(&rb).unwrap(), sorted);
}

#[test]
fn documents_print_sorted_by_identifier_bytes() {
    let doc = compact_doc(&[ELIG]).replace(
        &format!(r#"[{{"id":"rules.md","sha256":"{HASH}"}}]"#),
        &format!(r#"[{{"id":"z.md","sha256":"{HASH}"}},{{"id":"a.md","sha256":"{HASH}"}}]"#),
    );
    let rb = parse(&doc).unwrap();
    let out = print_compact(&rb).unwrap();
    assert!(out.find("a.md").unwrap() < out.find("z.md").unwrap());
}

// ----- all constructs round trip -----

fn full_document() -> String {
    let weighted = r#"{"id":"score.total","revision":1,"origin":{"kind":"manual","author":"sam"},"kind":"weighted_score","target":"score","type":"number","terms":[{"weight":{"node":"num","t":"number","value":"0.6"},"value":{"node":"field","t":"number","name":"judges"}},{"weight":{"node":"num","t":"number","value":"0.4"},"value":{"node":"agg","t":"number","fn":"sum","collection":"marks","field":"points"}}],"rounding":{"mode":"half_even","places":2},"markers":[{"kind":"ambiguous","interpretations":["a","b"]},{"kind":"incomplete","missing":["cap"]}],"spans":[{"document":"rules.md","start":92,"end":171,"line":4,"column":1}]}"#;
    let ranking = r#"{"id":"rank.main","revision":1,"origin":{"kind":"ai_proposal","proposer":"ai-1","provider":"mock","model":"m1","adapter_version":"0.1","request_hash":"eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a","response_hash":"eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a","proposal_id":"p-1","ai_identities":["ai-1"]},"kind":"ranking","target":"rank","type":"ranking","key":{"node":"ref","t":"number","target":"score"},"direction":"desc","tie_breakers":[{"expr":{"node":"round","t":"number","arg":{"node":"binary","t":"number","op":"div","left":{"node":"num","value":"1"},"right":{"node":"num","value":"3"}},"places":4,"mode":"toward_zero"},"direction":"asc"}],"ties":"group","markers":[],"spans":[{"document":"rules.md","start":1,"end":2,"line":1,"column":1}]}"#;
    let default = r#"{"id":"def.level","revision":1,"origin":{"kind":"manual","author":null},"kind":"default","target":"level","type":"enum:Level","value":{"node":"enum","t":"enum:Level","enum":"Level","member":"low"},"markers":[],"spans":[{"document":"rules.md","start":1,"end":2,"line":1,"column":1}]}"#;
    let derived = r#"{"id":"der.when","revision":1,"origin":{"kind":"manual","author":null},"kind":"derived","target":"when","type":"number","expr":{"node":"not","t":"boolean","arg":{"node":"or","t":"boolean","args":[{"node":"in","t":"boolean","value":{"node":"field","t":"text","name":"club"},"set":"clubs"},{"node":"cmp","t":"boolean","op":"lt","left":{"node":"date","t":"date","value":"2024-02-29"},"right":{"node":"date","t":"date","value":"2025-01-01"}},{"node":"bool","t":"boolean","value":true},{"node":"text","t":"text","value":"q\"\\\n"}]}},"markers":[],"spans":[{"document":"rules.md","start":1,"end":2,"line":1,"column":1}]}"#;
    let types = r#"{"inputs":[{"name":"age","type":"number"},{"name":"club","type":"text"}],"enums":[{"name":"Level","members":["low","high"]}],"sets":[{"name":"clubs","element_type":"text","members":[{"node":"text","t":"text","value":"A"},{"node":"text","t":"text","value":"B"}]},{"name":"levels","element_type":"enum:Level","members":[{"node":"enum","t":"enum:Level","enum":"Level","member":"high"}]}],"collections":[{"name":"marks","fields":[{"name":"points","type":"number"}]}]}"#;
    let precedence = r#"[{"id":"prec.1","target":"score","order":["score.total","other"],"spans":[{"document":"rules.md","start":1,"end":2,"line":1,"column":1}]}]"#;
    compact_doc(&[default, derived, ranking, weighted])
        .replace(
            r#""types":{"inputs":[{"name":"age","type":"number"},{"name":"licence","type":"boolean"}],"enums":[],"sets":[],"collections":[]}"#,
            &format!(r#""types":{types}"#),
        )
        .replace(r#""precedence":[]"#, &format!(r#""precedence":{precedence}"#))
}

#[test]
fn every_construct_survives_a_round_trip() {
    let doc = pretty_of(&full_document());
    let rb = round_trip(&doc);
    assert_eq!(rb.revisions.len(), 4);
    assert_eq!(rb.types.sets.len(), 2);
    assert_eq!(rb.precedence.len(), 1);
    // Parsing the compact text and the pretty text gives the same Rulebook.
    assert_eq!(parse(&full_document()).unwrap(), rb);
    // Number literals are canonical decimals.
    let compact = print_compact(&rb).unwrap();
    assert!(compact.contains(r#""value":"0.6""#));
}

#[test]
fn a_literal_is_normalised_to_its_canonical_decimal() {
    let doc = compact_doc(&[&ELIG.replace(r#""value":"16""#, r#""value":"16.500""#)]);
    let out = print_compact(&parse(&doc).unwrap()).unwrap();
    assert!(out.contains(r#""value":"16.5""#));
}

// ----- unrecognized constructs -----

#[test]
fn unrecognized_kind_and_node_are_carried_without_diagnostics() {
    let weird_rule = r#"{"id":"w.1","revision":1,"origin":{"kind":"manual","author":null},"kind":"sliding_window","target":"t","type":"number","size":3,"ratio":-1.50,"nested":{"a":[1,2.5e3,null,true]},"markers":[],"spans":[{"document":"rules.md","start":1,"end":2,"line":1,"column":1}]}"#;
    let weird_node = ELIG.replace(
        r#"{"node":"field","t":"boolean","name":"licence"}"#,
        r#"{"node":"fuzzy","t":"boolean","degree":0.50,"items":["x"]}"#,
    );
    let doc = compact_doc(&[
        ELIG,
        &weird_node.replace("elig.basic", "elig.two"),
        weird_rule,
    ]);
    let rb = round_trip(&doc);
    match &rb.revisions[2].content.body {
        RuleBody::Unrecognized(o) => {
            assert_eq!(o.name.as_str(), "sliding_window");
            assert_eq!(
                o.raw(),
                r#"{"size":3,"ratio":-1.50,"nested":{"a":[1,2.5e3,null,true]}}"#
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(print_compact(&rb).unwrap().contains(r#""ratio":-1.50"#));
    let RuleBody::Eligibility { expr } = &rb.revisions[1].content.body else {
        panic!("eligibility expected");
    };
    let ExprKind::And(args) = &expr.kind else {
        panic!("and expected");
    };
    assert!(matches!(&args[1].kind, ExprKind::Opaque(o) if o.name.as_str() == "fuzzy"));
    assert_eq!(args[1].ty, Some(Type::Boolean));
}

#[test]
fn unknown_operator_names_are_carried_as_unrecognized_nodes() {
    let doc = compact_doc(&[&ELIG.replace(r#""op":"ge""#, r#""op":"approx""#)]);
    let rb = round_trip(&doc);
    let RuleBody::Eligibility { expr } = &rb.revisions[0].content.body else {
        panic!();
    };
    let ExprKind::And(args) = &expr.kind else {
        panic!();
    };
    assert!(matches!(&args[0].kind, ExprKind::Opaque(o) if o.name.as_str() == "cmp"));
}

#[test]
fn malformed_kind_and_node_names_are_errors() {
    for bad in ["Sliding", "1x", "a-b", ""] {
        let rule = ELIG.replace(r#""kind":"eligibility""#, &format!(r#""kind":"{bad}""#));
        let d = fail(&compact_doc(&[&rule]));
        assert_eq!(codes(&d), [Code::IrFieldInvalid], "{bad}");
    }
    let rule = ELIG.replace(r#""node":"cmp""#, r#""node":"Cmp""#);
    assert_eq!(codes(&fail(&compact_doc(&[&rule]))), [Code::IrFieldInvalid]);
}

// ----- version -----

#[test]
fn unsupported_version_gives_exactly_one_unsupported_diagnostic() {
    let text =
        pretty_of(&compact_doc(&[ELIG])).replace(r#""ir_version": "1""#, r#""ir_version": "2""#);
    let d = fail(&text);
    assert_eq!(codes(&d), [Code::IrVersionUnsupported]);
    assert!(d[0].message().contains("\"2\""));
    assert!(d[0].message().contains("supported versions: \"1\""));
    assert_eq!(line_col(&d[0]), pos_of(&text, "\"ir_version\""));
}

#[test]
fn a_bare_number_version_is_unsupported_not_accepted() {
    let text = compact_doc(&[ELIG]).replace(r#""ir_version":"1""#, r#""ir_version":1"#);
    assert_eq!(codes(&fail(&text)), [Code::IrVersionUnsupported]);
}

#[test]
fn missing_version_is_reported_at_the_header() {
    let text = pretty_of(&compact_doc(&[ELIG])).replace("  \"ir_version\": \"1\",\n", "");
    let d = fail(&text);
    assert_eq!(codes(&d), [Code::IrVersionMissingOrRepeated]);
    assert!(d[0].message().contains("missing"));
    assert_eq!(line_col(&d[0]), (1, 1));
}

#[test]
fn repeated_version_is_reported_at_the_second_with_the_first_related() {
    let text = pretty_of(&compact_doc(&[ELIG])).replace(
        "  \"ir_version\": \"1\",\n",
        "  \"ir_version\": \"1\",\n  \"ir_version\": \"1\",\n",
    );
    let d = fail(&text);
    assert_eq!(codes(&d), [Code::IrVersionMissingOrRepeated]);
    assert!(d[0].message().contains("repeated"));
    assert_eq!(line_col(&d[0]), (4, 3));
    match d[0].related() {
        [Location::Input(first)] => assert_eq!((first.line, first.column), (Some(3), Some(3))),
        other => panic!("{other:?}"),
    }
}

#[test]
fn another_repeated_key_stays_a_syntax_error() {
    let text = compact_doc(&[ELIG]).replace(r#""name":"spring-open""#, r#""name":"a","name":"b""#);
    assert_eq!(codes(&fail(&text)), [Code::IrSyntaxError]);
}

// ----- syntax and limits -----

#[test]
fn syntax_errors_limits_and_boundaries() {
    let text = pretty_of(&compact_doc(&[ELIG]));
    let broken = text
        .replacen("\"name\":", "name:", 1)
        .replacen("\"name\": ", "name: ", 1);
    let d = fail(&broken);
    assert_eq!(codes(&d), [Code::IrSyntaxError]);
    assert_eq!(line_col(&d[0]), pos_of(&broken, "name: "));

    let bytes = text.as_bytes();
    let exact = u64::try_from(bytes.len()).unwrap();
    assert!(parse_rulebook(bytes, &input(), exact, 64).is_ok());
    let small = parse_rulebook(bytes, &input(), exact - 1, 64).unwrap_err();
    assert_eq!(
        small.diagnostics().as_slice()[0].code(),
        Code::LimitFileTooLarge
    );
    // The deepest container of the example is at depth 7; the limit itself is accepted.
    assert!(parse_rulebook(bytes, &input(), exact, 7).is_ok());
    let deep = parse_rulebook(bytes, &input(), exact, 6).unwrap_err();
    assert_eq!(
        deep.diagnostics().as_slice()[0].code(),
        Code::LimitNestingTooDeep
    );
}

#[test]
fn an_overridden_depth_limit_is_capped_so_deep_input_cannot_exhaust_the_stack() {
    let depth = 50_000;
    let mut text = String::from(r#"{"format":"ruletrace-rulebook","ir_version":"1","x":"#);
    text.push_str(&"[".repeat(depth));
    text.push_str(&"]".repeat(depth));
    text.push('}');
    let r = parse_rulebook(text.as_bytes(), &input(), u64::MAX, u32::MAX).unwrap_err();
    assert_eq!(
        r.diagnostics().as_slice()[0].code(),
        Code::LimitNestingTooDeep
    );
}

// ----- flag and lifecycle keys -----

#[test]
fn lifecycle_and_flag_keys_are_rejected_without_an_unsupported_diagnostic() {
    let text = pretty_of(&compact_doc(&[ELIG])).replace(
        "      \"kind\": \"eligibility\",",
        "      \"approval_status\": \"Approved\",\n      \"is_approved\": true,\n      \"kind\": \"eligibility\",",
    );
    let d = fail(&text);
    assert_eq!(codes(&d), [Code::LifecycleFlagContradiction]);
    assert!(d[0].message().contains("approval_status, is_approved"));
    assert_eq!(line_col(&d[0]), pos_of(&text, "\"approval_status\""));
    assert_eq!(field(&d[0]).as_deref(), Some("rules[0].approval_status"));

    let text = compact_doc(&[ELIG]).replacen('{', r#"{"has_errors":false,"no_errors":true,"#, 1);
    assert_eq!(codes(&fail(&text)), [Code::DiagnosticStateBothFlags]);
    let text = compact_doc(&[ELIG]).replacen('{', r#"{"validation_status":"passed","#, 1);
    assert_eq!(codes(&fail(&text)), [Code::DiagnosticStateContradiction]);
}

#[test]
fn flag_keys_inside_unrecognized_constructs_are_rejected() {
    let rule = r#"{"id":"w.1","revision":1,"origin":{"kind":"manual","author":null},"kind":"future","target":"t","cfg":{"is_proposed":false},"markers":[],"spans":[]}"#;
    assert_eq!(
        codes(&fail(&compact_doc(&[rule]))),
        [Code::LifecycleFlagContradiction]
    );
}

// ----- unknown names -----

#[test]
fn each_unknown_name_gives_one_unsupported_diagnostic_with_its_position() {
    let text = pretty_of(&compact_doc(&[ELIG]))
        .replace(
            "  \"precedence\": []",
            "  \"script\": \"rm -rf /\",\n  \"precedence\": []",
        )
        .replace(
            "            \"op\": \"ge\",",
            "            \"op\": \"ge\",\n            \"eval\": \"x\",",
        );
    let d = fail(&text);
    assert_eq!(codes(&d), [Code::IrUnknownField, Code::IrUnknownField]);
    assert_eq!(line_col(&d[1]), pos_of(&text, "\"script\""));
    assert_eq!(field(&d[1]).as_deref(), Some("script"));
    assert_eq!(line_col(&d[0]), pos_of(&text, "\"eval\""));
    assert!(d[0].message().contains("'eval'"));
    assert!(d[1].message().contains("'script'"));
}

#[test]
fn an_unknown_rounding_mode_is_unsupported() {
    let rule = r#"{"id":"s.1","revision":1,"origin":{"kind":"manual","author":null},"kind":"weighted_score","target":"s","type":"number","terms":[],"rounding":{"mode":"ceiling","places":2},"markers":[],"spans":[]}"#;
    let d = fail(&compact_doc(&[rule]));
    assert_eq!(codes(&d), [Code::IrUnknownField]);
    assert!(d[0].message().contains("ceiling"));
}

// ----- numbers -----

#[test]
fn bare_and_unrepresentable_numbers_give_one_error_each_with_the_literal() {
    let make = |value: &str| ELIG.replace(r#""value":"16""#, &format!(r#""value":{value}"#));
    for (literal, shown) in [
        ("16", "16"),
        ("16.5", "16.5"),
        ("1e3", "1e3"),
        ("-2", "-2"),
        ("\"1e3\"", "1e3"),
        ("\"1/3\"", "1/3"),
        ("\"+1\"", "+1"),
        ("\"NaN\"", "NaN"),
        (
            "\"123456789012345678901234567890123456789\"",
            "12345678901234567890123456789012345678",
        ),
        (
            "\"0.00000000000000000000000000001\"",
            "0.00000000000000000000000000001",
        ),
    ] {
        let text = pretty_of(&compact_doc(&[&make(literal)]));
        let d = fail(&text);
        assert_eq!(codes(&d), [Code::IrNumericLiteralInvalid], "{literal}");
        assert!(
            d[0].message().contains(shown),
            "{literal}: {}",
            d[0].message()
        );
        let needle = format!("\"value\": {literal}");
        let (line, column) = pos_of(&text, &needle);
        let skip = u32::try_from("\"value\": ".len()).unwrap();
        assert_eq!(line_col(&d[0]), (line, column + skip), "{literal}");
    }
}

#[test]
fn every_bad_literal_of_one_document_is_reported() {
    let rule = ELIG.replace(r#""value":"16""#, r#""value":16"#);
    let second = r#"{"id":"d.1","revision":1,"origin":{"kind":"manual","author":null},"kind":"derived","target":"d","type":"number","expr":{"node":"num","t":"number","value":"1/3"},"markers":[],"spans":[]}"#;
    let d = fail(&compact_doc(&[&rule, second]));
    assert_eq!(
        codes(&d),
        [Code::IrNumericLiteralInvalid, Code::IrNumericLiteralInvalid]
    );
}

#[test]
fn literal_limits_are_accepted_at_the_boundary() {
    let at_limit = format!("{}.{}", "1".repeat(10), "2".repeat(28));
    let doc = compact_doc(&[&ELIG.replace(r#""value":"16""#, &format!(r#""value":"{at_limit}""#))]);
    assert!(parse(&doc).is_ok());
}

// ----- identifiers, revisions, documents -----

#[test]
fn duplicate_revisions_are_reported_at_each_repeat_with_the_first_related() {
    let text = pretty_of(&compact_doc(&[ELIG, ELIG, ELIG]));
    let d = fail(&text);
    assert_eq!(
        codes(&d),
        [Code::IrDuplicateRevision, Code::IrDuplicateRevision]
    );
    assert!(d[0].message().contains("'elig.basic' revision 1"));
    assert_eq!(d[0].rules().len(), 1);
    let first = pos_of(&text, "{\n      \"id\": \"elig.basic\"");
    match d[0].related() {
        [Location::Input(l)] => assert_eq!((l.line, l.column), (Some(first.0), Some(first.1))),
        other => panic!("{other:?}"),
    }
    assert_eq!(field(&d[0]).as_deref(), Some("rules[1]"));
    assert_eq!(field(&d[1]).as_deref(), Some("rules[2]"));
}

#[test]
fn revision_numbers_must_start_at_one_and_have_no_gap() {
    let rev = |n: &str| ELIG.replace("\"revision\":1", &format!("\"revision\":{n}"));
    for bad in ["0", "4294967296", "9007199254740992", "-1", "1.5"] {
        let d = fail(&compact_doc(&[&rev(bad)]));
        assert_eq!(codes(&d), [Code::IrRevisionSequenceInvalid], "{bad}");
    }
    let d = fail(&compact_doc(&[&rev("1"), &rev("3")]));
    assert_eq!(codes(&d), [Code::IrRevisionSequenceInvalid]);
    assert!(d[0].message().contains("expected revision 2, found 3"));
    let d = fail(&compact_doc(&[&rev("2")]));
    assert_eq!(codes(&d), [Code::IrRevisionSequenceInvalid]);
    assert!(parse(&compact_doc(&[&rev("2"), &rev("1")])).is_ok());
    assert!(parse(&compact_doc(&[&rev("1"), &rev("2")])).is_ok());
}

#[test]
fn document_declarations_are_checked() {
    let base = compact_doc(&[ELIG]);
    let bad_hash = base.replace(HASH, &HASH.to_uppercase());
    let d = fail(&bad_hash);
    assert_eq!(codes(&d), [Code::IrDocumentHashInvalid]);
    assert!(d[0].message().contains("'rules.md'"));
    assert!(d[0].message().contains("malformed"));
    assert_eq!(field(&d[0]).as_deref(), Some("documents[0]"));

    let short = base.replace(HASH, &HASH[..63]);
    assert_eq!(codes(&fail(&short)), [Code::IrDocumentHashInvalid]);

    let dup = base.replace(
        &format!(r#"{{"id":"rules.md","sha256":"{HASH}"}}"#),
        &format!(r#"{{"id":"rules.md","sha256":"{HASH}"}},{{"id":"rules.md","sha256":"{HASH}"}}"#),
    );
    let d = fail(&dup);
    assert_eq!(codes(&d), [Code::IrDocumentIdDuplicate]);
    assert!(d[0].message().contains("'rules.md'"));
    assert_eq!(d[0].related().len(), 1);
    assert_eq!(field(&d[0]).as_deref(), Some("documents[1]"));
}

#[test]
fn missing_and_mistyped_fields_are_field_errors() {
    let no_target = ELIG.replace(r#""target":"eligible","#, "");
    let d = fail(&compact_doc(&[&no_target]));
    assert_eq!(codes(&d), [Code::IrFieldInvalid]);
    assert!(d[0].message().contains("'target'"));
    assert_eq!(field(&d[0]).as_deref(), Some("rules[0]"));

    let typed = ELIG.replace(r#""start":21"#, r#""start":"21""#);
    let d = fail(&compact_doc(&[&typed]));
    assert_eq!(codes(&d), [Code::IrFieldInvalid]);
    assert_eq!(field(&d[0]).as_deref(), Some("rules[0].spans[0].start"));

    let bad_type = ELIG.replace(r#""type":"boolean","expr""#, r#""type":"float","expr""#);
    assert_eq!(
        codes(&fail(&compact_doc(&[&bad_type]))),
        [Code::IrFieldInvalid]
    );

    let bad_id = ELIG.replace("elig.basic", "Elig");
    assert_eq!(
        codes(&fail(&compact_doc(&[&bad_id]))),
        [Code::IrFieldInvalid]
    );

    let bad_format = compact_doc(&[ELIG]).replace("ruletrace-rulebook", "other");
    assert_eq!(codes(&fail(&bad_format)), [Code::IrFieldInvalid]);

    let bad_places = r#"{"id":"s.1","revision":1,"origin":{"kind":"manual","author":null},"kind":"weighted_score","target":"s","type":"number","terms":[],"rounding":{"mode":"half_even","places":29},"markers":[],"spans":[]}"#;
    assert_eq!(
        codes(&fail(&compact_doc(&[bad_places]))),
        [Code::IrFieldInvalid]
    );
    let no_mode = bad_places
        .replace(r#""mode":"half_even","#, "")
        .replace("29", "2");
    assert_eq!(
        codes(&fail(&compact_doc(&[&no_mode]))),
        [Code::IrFieldInvalid]
    );
}

#[test]
fn an_omitted_type_and_omitted_spans_are_left_for_the_validator() {
    let rule = ELIG
        .replace(r#""type":"boolean","expr""#, r#""expr""#)
        .replace(
            r#"{"node":"field","t":"boolean","name":"licence"}"#,
            r#"{"node":"field","name":"licence"}"#,
        );
    let rule = rule.replace(
        r#","spans":[{"document":"rules.md","start":21,"end":91,"line":3,"column":1}]"#,
        "",
    );
    let rb = round_trip(&compact_doc(&[&rule]));
    let c = &rb.revisions[0].content;
    assert_eq!(c.ty, None);
    assert!(c.spans.is_empty());
    let RuleBody::Eligibility { expr } = &c.body else {
        panic!();
    };
    let ExprKind::And(args) = &expr.kind else {
        panic!();
    };
    assert_eq!(args[1].ty, None);
}

// ----- collected diagnostics and order -----

#[test]
fn all_errors_of_a_document_are_returned_in_canonical_order() {
    let text = pretty_of(&compact_doc(&[ELIG, &ELIG.replace("elig.basic", "x.two")]))
        .replace(HASH, "bad")
        .replace(r#""value": "16""#, r#""value": 16"#)
        .replace(
            "  \"precedence\": []",
            "  \"zzz\": 1,\n  \"precedence\": []",
        );
    let d = fail(&text);
    let mut sorted = d.clone();
    sorted.sort();
    assert_eq!(d, sorted);
    let got = codes(&d);
    assert!(got.contains(&Code::IrDocumentHashInvalid));
    assert_eq!(
        got.iter()
            .filter(|c| **c == Code::IrNumericLiteralInvalid)
            .count(),
        2
    );
    assert!(got.contains(&Code::IrUnknownField));
}

#[test]
fn parsing_is_deterministic_for_the_same_bytes() {
    let text = pretty_of(&compact_doc(&[ELIG])).replace("16", "1e3");
    let a = fail(&text);
    let b = fail(&text);
    assert_eq!(a, b);
    let ok = pretty_of(&full_document());
    assert_eq!(parse(&ok).unwrap(), parse(&ok).unwrap());
}

// ----- count limits -----

fn span_list(n: usize) -> String {
    vec![r#"{"document":"rules.md","start":1,"end":2,"line":1,"column":1}"#; n].join(",")
}

#[test]
fn span_count_limit_is_checked_by_the_parser_and_the_printer() {
    let with = |n: usize| {
        let rule = ELIG.replace(
            r#"{"document":"rules.md","start":21,"end":91,"line":3,"column":1}"#,
            &span_list(n),
        );
        compact_doc(&[&rule])
    };
    let rb = parse(&with(64)).unwrap();
    assert_eq!(rb.revisions[0].content.spans.len(), 64);
    let d = fail(&with(65));
    assert_eq!(codes(&d), [Code::LimitCountExceeded]);
    assert_eq!(field(&d[0]).as_deref(), Some("rules[0].spans"));

    let mut rb = rb;
    let extra = rb.revisions[0].content.spans[0].clone();
    rb.revisions[0].content.spans.push(extra);
    let e = print_pretty(&rb).unwrap_err();
    assert_eq!(codes(&e), [Code::LimitCountExceeded]);
    assert!(print_compact(&rb).is_err());
}

#[test]
fn revision_and_rule_count_limits() {
    // 100,001 revisions of 10,001 rules would be large; use the fields the limits count.
    let rule = |id: usize| {
        format!(
            r#"{{"id":"r{id}","revision":1,"origin":{{"kind":"manual","author":null}},"kind":"future","target":"t","markers":[],"spans":[]}}"#
        )
    };
    let at_limit: Vec<String> = (0..10_000).map(rule).collect();
    let refs: Vec<&str> = at_limit.iter().map(String::as_str).collect();
    let rb = parse(&compact_doc(&refs)).unwrap();
    assert_eq!(rb.revisions.len(), 10_000);
    assert!(print_compact(&rb).is_ok());

    let over: Vec<String> = (0..10_001).map(rule).collect();
    let refs: Vec<&str> = over.iter().map(String::as_str).collect();
    let d = fail(&compact_doc(&refs));
    assert_eq!(codes(&d), [Code::LimitCountExceeded]);
    assert!(d[0].message().contains("10000 rules"));
    assert_eq!(field(&d[0]).as_deref(), Some("rules[10000]"));
    let mut rb = rb;
    rb.revisions.push(
        parse(&compact_doc(&[&rule(10_000)]))
            .unwrap()
            .revisions
            .remove(0),
    );
    assert_eq!(
        codes(&print_compact(&rb).unwrap_err()),
        [Code::LimitCountExceeded]
    );
}

// ----- writer agrees with rt-json -----

#[test]
fn the_private_writer_matches_the_rt_json_writer() {
    let text = full_document();
    let v = rt_json::parse(&input(), text.as_bytes(), ParseLimits::new(1 << 24, 64)).unwrap();
    let rb = parse(&text).unwrap();
    // The canonical key order equals the order of the input, so the printer must reproduce the
    // output of the independent rt-json writer byte for byte.
    assert_eq!(print_compact(&rb).unwrap(), rt_json::write_compact(&v));
    assert_eq!(print_pretty(&rb).unwrap(), rt_json::write_pretty(&v));
}

#[test]
fn control_characters_in_strings_are_escaped_like_rt_json() {
    let doc = compact_doc(&[ELIG]).replace("spring-open", "a\\u0001\\u007f\\u0085\\n\\\"é");
    let rb = parse(&doc).unwrap();
    assert_eq!(rb.name, "a\u{1}\u{7f}\u{85}\n\"é");
    let v = rt_json::parse(&input(), doc.as_bytes(), ParseLimits::new(1 << 24, 64)).unwrap();
    assert_eq!(print_compact(&rb).unwrap(), rt_json::write_compact(&v));
}

#[test]
fn flag_keys_are_not_reintroduced_by_the_printer() {
    let rb = parse(&full_document()).unwrap();
    let out = print_pretty(&rb).unwrap();
    for key in rt_json::FLAG_KEYS
        .iter()
        .chain(rt_json::LIFECYCLE_KEYS.iter())
    {
        assert!(!out.contains(&format!("\"{key}\"")), "{key}");
    }
}
