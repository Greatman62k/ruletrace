//! The mapping schema derives from the Compiled_Rulebook (UD-41, C1).
//! Expected values are written by hand from the fixture text.

use super::*;
use crate::TargetName;
use rt_compiled::{parse_compiled, CompiledRulebook};
use rt_foundation::InputId;
use std::collections::BTreeMap;

const SLICE: &str = include_str!("../../../../tests/fixtures/slice-basic/expected/compiled.json");

fn load(text: &str) -> CompiledRulebook {
    let id = InputId::new("compiled.json").unwrap();
    match parse_compiled(text.as_bytes(), &id, 1 << 20, 64) {
        Ok(rb) => rb,
        Err(_) => panic!("fixture variant must parse: {text}"),
    }
}

fn edit(text: &str, from: &str, to: &str) -> String {
    assert_eq!(text.matches(from).count(), 1, "edit anchor {from}");
    text.replacen(from, to, 1)
}

fn tn(s: &str) -> TargetName {
    TargetName::new(s).unwrap()
}

fn kinds_of(pairs: &[(&str, TargetKind)]) -> BTreeMap<String, TargetKind> {
    pairs.iter().map(|(n, k)| ((*n).to_owned(), *k)).collect()
}

// The fixture plus an enum, text and date input and a collection, so the
// enum members of both are resolved from `enums`.
fn with_enum_and_collection() -> String {
    edit(
        SLICE,
        r#"{"name":"technical","type":"number"}],"collections":[],"enums":[]"#,
        concat!(
            r#"{"name":"technical","type":"number"},"#,
            r#"{"name":"level","type":"enum:0"},{"name":"note","type":"text"},"#,
            r#"{"name":"born","type":"date"}],"#,
            r#""collections":[{"name":"results","fields":["#,
            r#"{"name":"place","type":"number"},{"name":"medal","type":"enum:0"}]}],"#,
            r#""enums":[{"name":"Medal","members":["Gold","Silver"]}]"#
        ),
    )
}

#[test]
fn input_schema_lists_the_declared_inputs_with_their_types() {
    let schema = input_schema(&load(SLICE)).unwrap();
    let expected = InputSchema::new(
        [
            ("age".to_owned(), FieldType::Number),
            ("licence".to_owned(), FieldType::Boolean),
            ("judges".to_owned(), FieldType::Number),
            ("technical".to_owned(), FieldType::Number),
        ],
        [],
    );
    assert_eq!(schema, expected);
}

#[test]
fn input_schema_resolves_enum_members_text_date_and_collection_fields() {
    let medal = FieldType::Enum(["Gold", "Silver"].map(str::to_owned).into());
    let schema = input_schema(&load(&with_enum_and_collection())).unwrap();
    let expected = InputSchema::new(
        [
            ("age".to_owned(), FieldType::Number),
            ("licence".to_owned(), FieldType::Boolean),
            ("judges".to_owned(), FieldType::Number),
            ("technical".to_owned(), FieldType::Number),
            ("level".to_owned(), medal.clone()),
            ("note".to_owned(), FieldType::Text),
            ("born".to_owned(), FieldType::Date),
        ],
        [(
            "results".to_owned(),
            vec![
                ("place".to_owned(), FieldType::Number),
                ("medal".to_owned(), medal),
            ],
        )],
    );
    assert_eq!(schema, expected);
}

#[test]
fn target_kinds_name_the_kind_of_every_target() {
    let kinds = target_kinds_by_name(&load(SLICE));
    let expected = kinds_of(&[
        ("eligible", TargetKind::Eligibility),
        ("score", TargetKind::Score),
        ("standing", TargetKind::Ranking),
    ]);
    assert_eq!(kinds, expected);
}

#[test]
fn target_kinds_carry_valid_target_names() {
    let kinds = target_kinds(&load(SLICE)).unwrap();
    let expected = TargetKinds::new([
        (tn("eligible"), TargetKind::Eligibility),
        (tn("score"), TargetKind::Score),
        (tn("standing"), TargetKind::Ranking),
    ]);
    assert_eq!(kinds, expected);
}

#[test]
fn a_target_with_rules_of_two_kinds_is_other() {
    // The ranking rule now also targets `score`, which the weighted score
    // rule defines: two kinds under one name.
    let text = edit(SLICE, r#""target":"standing""#, r#""target":"score""#);
    let kinds = target_kinds_by_name(&load(&text));
    let expected = kinds_of(&[
        ("eligible", TargetKind::Eligibility),
        ("score", TargetKind::Other),
    ]);
    assert_eq!(kinds, expected);
}

// The mapper's internal error for a Ranking-typed input is a second line of
// defence: the Compiled_Rulebook model itself refuses such a declaration
// (rt-compiled check.rs), so no value reaches `input_schema` carrying one.
#[test]
fn a_ranking_typed_input_never_reaches_the_mapper() {
    let text = edit(
        SLICE,
        r#"{"name":"technical","type":"number"}]"#,
        r#"{"name":"technical","type":"number"},{"name":"table","type":"ranking"}]"#,
    );
    let id = InputId::new("compiled.json").unwrap();
    let Err(state) = parse_compiled(text.as_bytes(), &id, 1 << 20, 64) else {
        panic!("a Ranking-typed input must not parse");
    };
    let found = state.as_slice();
    assert_eq!(found.len(), 1);
    assert!(found[0].message().contains("inputs[4].type"));
}
