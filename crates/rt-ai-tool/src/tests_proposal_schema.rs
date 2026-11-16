//! Tests of the provider-neutral Proposal schema (UD-37, Req 23.39, design
//! 3.7.3). The limits are measured by a counter in this file that reads
//! only the schema bytes and shares no code with the generator.

use crate::proposal_schema::{proposal_schema, proposal_schema_bytes};
use crate::vocab::Ud32Option;
use rt_json::{parse, write_pretty, Kind, ParseLimits, Position, Value};

const GOLDEN: &str =
    include_str!("../../../tests/fixtures/proposal-schema/proposal-schema-v2.json");

const MAX_DEPTH: usize = 10;
const MAX_PROPERTIES: usize = 5_000;
const MAX_NAME_CHARS: usize = 120_000;
const MAX_ENUM_VALUES: usize = 1_000;

pub(crate) fn tree(bytes: &[u8]) -> Value<Position> {
    let id = rt_foundation::InputId::new("schema").unwrap();
    parse(&id, bytes, ParseLimits::new(1 << 24, 256)).unwrap()
}

#[derive(Default, Debug)]
pub(crate) struct Measure {
    pub(crate) depth: usize,
    pub(crate) properties: usize,
    pub(crate) name_chars: usize,
    pub(crate) description_chars: usize,
    pub(crate) largest_enum: usize,
    pub(crate) refs: usize,
    pub(crate) forbidden_combinators: usize,
    pub(crate) open_objects: usize,
    pub(crate) required_mismatches: usize,
    pub(crate) nested_root_any_of: bool,
}

fn members(v: &Value<Position>) -> &[rt_json::Member<Position>] {
    v.as_object().unwrap_or_default()
}

fn get<'a>(v: &'a Value<Position>, key: &str) -> Option<&'a Value<Position>> {
    members(v).iter().find(|m| m.key == key).map(|m| &m.value)
}

/// Depth counts every schema level: the root is 1, and a property schema,
/// an `items` schema and each `anyOf` alternative is one level below the
/// schema that holds it. `anyOf` is counted as a level, which is stricter
/// than a count of data nesting.
fn walk(v: &Value<Position>, depth: usize, m: &mut Measure) {
    m.depth = m.depth.max(depth);
    for member in members(v) {
        if member.key == "$ref" || member.key == "$defs" {
            m.refs += 1;
        }
        if member.key == "oneOf" || member.key == "allOf" {
            m.forbidden_combinators += 1;
        }
    }
    if let Some(text) = get(v, "description").and_then(Value::as_str) {
        m.description_chars += text.chars().count();
    }
    if let Some(Kind::Array(options)) = get(v, "enum").map(Value::kind) {
        m.largest_enum = m.largest_enum.max(options.len());
        for o in options {
            m.name_chars += o.as_str().map_or(0, |s| s.chars().count());
        }
    }
    if get(v, "type").and_then(Value::as_str) == Some("object") {
        let props = get(v, "properties").map(members).unwrap_or_default();
        let mut keys: Vec<&str> = props.iter().map(|p| p.key.as_str()).collect();
        let mut required: Vec<&str> = get(v, "required")
            .and_then(Value::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        keys.sort_unstable();
        required.sort_unstable();
        if keys != required {
            m.required_mismatches += 1;
        }
        if !matches!(
            get(v, "additionalProperties").map(Value::kind),
            Some(Kind::Bool(false))
        ) {
            m.open_objects += 1;
        }
        m.properties += props.len();
        for p in props {
            m.name_chars += p.key.chars().count();
            walk(&p.value, depth + 1, m);
        }
    }
    if let Some(items) = get(v, "items") {
        walk(items, depth + 1, m);
    }
    if let Some(alts) = get(v, "anyOf").and_then(Value::as_array) {
        for a in alts {
            walk(a, depth + 1, m);
        }
    }
}

pub(crate) fn measure(root: &Value<Position>) -> Measure {
    let mut m = Measure::default();
    walk(root, 1, &mut m);
    m.nested_root_any_of =
        get(root, "anyOf").is_some() || get(root, "type").and_then(Value::as_str) != Some("object");
    m
}

/// Every limit and strict-mode rule that the schema breaks.
pub(crate) fn violations(m: &Measure) -> Vec<&'static str> {
    let mut out = Vec::new();
    let mut check = |broken: bool, what: &'static str| {
        if broken {
            out.push(what);
        }
    };
    check(m.depth > MAX_DEPTH, "depth");
    check(m.properties > MAX_PROPERTIES, "properties");
    check(m.name_chars > MAX_NAME_CHARS, "name characters");
    // Descriptions are not named in the provider limit; counting them keeps
    // the total string length of the schema within it under any reading.
    check(
        m.name_chars + m.description_chars > MAX_NAME_CHARS,
        "name and description characters",
    );
    check(m.largest_enum > MAX_ENUM_VALUES, "enum values");
    check(m.refs > 0, "$ref or $defs");
    check(m.forbidden_combinators > 0, "oneOf or allOf");
    check(m.open_objects > 0, "additionalProperties");
    check(m.required_mismatches > 0, "required");
    check(m.nested_root_any_of, "root");
    out
}

fn generated(option: Ud32Option) -> Value<Position> {
    tree(&proposal_schema_bytes(option).unwrap())
}

#[test]
fn the_generated_schema_is_within_every_limit_with_the_measured_numbers() {
    let m = measure(&generated(Ud32Option::RankReference));
    assert_eq!(violations(&m), Vec::<&str>::new(), "{m:?}");
    // Exact bounds, not a loose ceiling: a change of the shape shows here.
    assert_eq!(m.depth, 10, "{m:?}");
    assert!(m.properties < MAX_PROPERTIES, "{m:?}");
    assert_eq!(m.refs, 0);
    assert_eq!(m.largest_enum, 6);
}

#[test]
fn the_threshold_only_vocabulary_has_no_rank_reference_node() {
    let with = measure(&generated(Ud32Option::RankReference));
    let without = measure(&generated(Ud32Option::ThresholdOnly));
    assert_eq!(violations(&without), Vec::<&str>::new());
    // One node variant of three properties in each of five rule variants.
    assert_eq!(with.properties - without.properties, 15);
    let text =
        String::from_utf8(proposal_schema_bytes(Ud32Option::ThresholdOnly).unwrap()).unwrap();
    assert!(!text.contains("rank_reference"));
}

#[test]
fn the_measurer_detects_each_kind_of_violation() {
    let base = generated(Ud32Option::RankReference);
    let text = rt_json::write_compact(&base);
    let deeper = r#""quote":{"type":"array","items":{"type":"array","items":{"type":"array","items":{"type":"array","items":{"type":"string"}}}}}"#;
    let many = (0..1001)
        .map(|i| format!("\"v{i}\""))
        .collect::<Vec<_>>()
        .join(",");
    let mutants: [(&str, String, &str); 8] = [
        (
            "a reference",
            text.replacen(
                r#"{"type":"object","#,
                r##"{"$ref":"#/$defs/x","type":"object","##,
                1,
            ),
            "$ref or $defs",
        ),
        (
            "an open object",
            text.replacen(
                r#""additionalProperties":false"#,
                r#""additionalProperties":true"#,
                1,
            ),
            "additionalProperties",
        ),
        (
            "a property left out of required",
            text.replacen(r#""required":["schema_version","#, r#""required":["#, 1),
            "required",
        ),
        (
            "a union at the root",
            text.replacen(r#"{"type":"object","#, r#"{"anyOf":[],"type":"object","#, 1),
            "root",
        ),
        (
            "oneOf",
            text.replacen(r#""anyOf""#, r#""oneOf""#, 1),
            "oneOf or allOf",
        ),
        (
            "an eleventh level",
            text.replacen(
                r#""quote":{"type":"string","description":"The document bytes from start to end, copied exactly."}"#,
                deeper,
                1,
            ),
            "depth",
        ),
        (
            "descriptions past the string length limit",
            text.replacen(
                r#""description":"Document identifier"#,
                &format!(r#""description":"{}"#, "d".repeat(MAX_NAME_CHARS)),
                1,
            ),
            "name and description characters",
        ),
        (
            "an oversized enum",
            text.replacen(r#""enum":["2"]"#, &format!(r#""enum":[{many}]"#), 1),
            "enum values",
        ),
    ];
    for (label, mutated, expected) in mutants {
        assert_ne!(mutated, text, "{label} was not applied");
        let found = violations(&measure(&tree(mutated.as_bytes())));
        assert!(found.contains(&expected), "{label}: {found:?}");
    }
}

#[test]
fn the_measurer_counts_depth_and_properties_on_a_hand_built_schema() {
    let text = r#"{"type":"object","additionalProperties":false,"required":["a","b"],"properties":{"a":{"type":"array","items":{"anyOf":[{"type":"string","enum":["x","y"]},{"type":"null"}]}},"b":{"type":"integer"}}}"#;
    let m = measure(&tree(text.as_bytes()));
    // root 1, a 2, items 3, anyOf alternative 4.
    assert_eq!(m.depth, 4);
    assert_eq!(m.properties, 2);
    assert_eq!(m.largest_enum, 2);
    assert_eq!(m.name_chars, 4);
    assert_eq!(m.description_chars, 0);
    assert!(violations(&m).is_empty());
    let described = measure(&tree(
        br#"{"type":"object","description":"abcde","additionalProperties":false,"required":["a"],"properties":{"a":{"type":"string","description":"xyz"}}}"#,
    ));
    assert_eq!(described.description_chars, 8);
    assert_eq!(described.depth, 2);
}

#[test]
fn every_node_variant_has_exactly_the_decoder_fields() {
    let root = generated(Ud32Option::RankReference);
    let proposals = get(get(&root, "properties").unwrap(), "proposals").unwrap();
    let variants = get(get(proposals, "items").unwrap(), "anyOf")
        .and_then(Value::as_array)
        .unwrap();
    // The first rule variant after the four advisory ones is eligibility.
    let rule = get(get(&variants[4], "properties").unwrap(), "rule").unwrap();
    let table = get(get(rule, "properties").unwrap(), "nodes").unwrap();
    let nodes = get(get(table, "items").unwrap(), "anyOf")
        .and_then(Value::as_array)
        .unwrap();
    let mut seen: Vec<(String, Vec<String>)> = Vec::new();
    for n in nodes {
        let props = get(n, "properties").unwrap();
        let name = get(props, "node").and_then(|t| get(t, "enum")).unwrap();
        let name = name.as_array().unwrap()[0].as_str().unwrap().to_owned();
        let mut keys: Vec<String> = members(props).iter().map(|m| m.key.clone()).collect();
        keys.sort();
        seen.push((name, keys));
    }
    let mut expected: Vec<(String, Vec<String>)> = rt_ir::NODE_FIELDS
        .iter()
        .map(|(n, f)| {
            let mut keys: Vec<String> = f.iter().map(|k| (*k).to_owned()).collect();
            keys.sort();
            ((*n).to_owned(), keys)
        })
        .collect();
    expected.push((
        "unrecognized".to_owned(),
        vec!["detail".to_owned(), "name".to_owned(), "node".to_owned()],
    ));
    assert_eq!(seen, expected);
}

#[test]
fn the_rule_variants_are_the_five_kinds_and_the_unrecognized_kind() {
    let root = generated(Ud32Option::RankReference);
    let proposals = get(get(&root, "properties").unwrap(), "proposals").unwrap();
    let variants = get(get(proposals, "items").unwrap(), "anyOf")
        .and_then(Value::as_array)
        .unwrap();
    let kind_of = |v: &Value<Position>| -> String {
        let kind = get(get(v, "properties").unwrap(), "kind").unwrap();
        get(kind, "enum").and_then(Value::as_array).unwrap()[0]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let top: Vec<String> = variants.iter().map(&kind_of).collect();
    assert_eq!(
        top,
        [
            "clause_interpretation",
            "clause_relationship",
            "conflict_highlight",
            "candidate_test",
            "rule_suggestion",
            "rule_suggestion",
            "rule_suggestion",
            "rule_suggestion",
            "rule_suggestion",
            "rule_suggestion",
        ]
    );
    let rules: Vec<String> = variants[4..]
        .iter()
        .map(|v| kind_of(get(get(v, "properties").unwrap(), "rule").unwrap()))
        .collect();
    assert_eq!(
        rules,
        [
            "eligibility",
            "derived",
            "weighted_score",
            "ranking",
            "default",
            "unrecognized"
        ]
    );
}

#[test]
fn the_schema_bytes_equal_the_reviewed_golden_file() {
    let bytes = proposal_schema_bytes(Ud32Option::RankReference).unwrap();
    let value = proposal_schema(Ud32Option::RankReference).unwrap();
    // The golden file is the pretty form so that a reviewer can read a
    // change. The compact bytes are what is sent.
    assert_eq!(format!("{}\n", write_pretty(&value)), GOLDEN);
    let compact = rt_json::write_compact(&tree(GOLDEN.as_bytes()));
    assert_eq!(compact.as_bytes(), bytes.as_slice());
}

#[test]
fn the_schema_is_deterministic() {
    let a = proposal_schema_bytes(Ud32Option::RankReference).unwrap();
    let b = proposal_schema_bytes(Ud32Option::RankReference).unwrap();
    assert_eq!(a, b);
}
