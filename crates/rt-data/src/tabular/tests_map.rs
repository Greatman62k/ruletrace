//! Mapping tests. Expected values and positions are written by hand from the
//! CSV text, the design table of §8.3 and Requirements 8.27, 22.22 and 31.8.

use super::tests_csv::{csv, view, View};
use super::*;
use crate::{
    parse_data, parse_published, CompetitionData, DataLimits, DataValue, Element, Participant,
    ParticipantId, PublishedNumber, PublishedResult, PublishedValue, Question, TargetName,
};
use rt_foundation::{Code, DiagnosticState, InputId, Location};
use std::collections::{BTreeMap, BTreeSet};

fn pid(s: &str) -> ParticipantId {
    ParticipantId::new(s).unwrap()
}

fn tn(s: &str) -> TargetName {
    TargetName::new(s).unwrap()
}

fn enum_of(members: &[&str]) -> FieldType {
    FieldType::Enum(members.iter().map(|m| (*m).to_owned()).collect())
}

pub(super) fn schema() -> InputSchema {
    let medal = enum_of(&["Gold", "Silver"]);
    InputSchema::new(
        [
            ("age".to_owned(), FieldType::Number),
            ("licence".to_owned(), FieldType::Boolean),
            ("born".to_owned(), FieldType::Date),
            ("level".to_owned(), medal.clone()),
            ("note".to_owned(), FieldType::Text),
        ],
        [
            (
                "results".to_owned(),
                vec![
                    ("place".to_owned(), FieldType::Number),
                    ("medal".to_owned(), medal),
                ],
            ),
            (
                "penalties".to_owned(),
                vec![("reason".to_owned(), FieldType::Text)],
            ),
        ],
    )
}

pub(super) const HEADER: &str = "participant,age,licence,born,level,note";

pub(super) fn parts(text: &str) -> Result<CompetitionData, DiagnosticState> {
    map_participants(&csv(text).unwrap(), &schema(), &MappingOptions::default())
}

fn bad(text: &str) -> Vec<View> {
    view(&parts(text).unwrap_err())
}

fn text(s: &str) -> DataValue {
    DataValue::Text(s.to_owned())
}

fn value_err(line: u32, column: u32, field: &str) -> View {
    (
        Code::AppInputValueInvalid,
        Some(line),
        Some(column),
        Some(field.to_owned()),
    )
}

/// A participants file whose only non-empty cell is column `index` (0-based
/// after `participant`), so that the cell starts at column `4 + index`.
pub(super) fn row_with(index: usize, raw: &str) -> String {
    let mut cells = vec![String::new(); 5];
    cells[index] = raw.to_owned();
    if cells[4].is_empty() {
        cells[4] = "\"\"".to_owned();
    }
    format!("{HEADER}\np1,{}\n", cells.join(","))
}

pub(super) fn base(ids: &[&str]) -> CompetitionData {
    CompetitionData::new(
        ids.iter()
            .map(|i| (pid(i), Participant::default()))
            .collect(),
    )
}

#[test]
fn maps_every_type_and_keeps_explicit_zero_false_and_text() {
    let d = parts(&format!(
        "{HEADER}\np1,17,true,2010-01-31,Gold,hello\np2,0,false,,,\"\"\n"
    ))
    .unwrap();
    let p1 = d.participant(&pid("p1")).unwrap();
    assert_eq!(p1.field("age"), Some(&text("17")));
    assert_eq!(p1.field("licence"), Some(&DataValue::Bool(true)));
    assert_eq!(p1.field("born"), Some(&text("2010-01-31")));
    assert_eq!(p1.field("level"), Some(&text("Gold")));
    assert_eq!(p1.field("note"), Some(&text("hello")));
    let p2 = d.participant(&pid("p2")).unwrap();
    assert_eq!(p2.field("age"), Some(&text("0")));
    assert_eq!(p2.field("licence"), Some(&DataValue::Bool(false)));
    assert_eq!(p2.field("born"), None);
    assert_eq!(p2.field("level"), None);
    assert_eq!(p2.field("note"), None);
    assert!(p2.collections().is_empty());
}

#[test]
fn an_empty_cell_is_missing_for_every_type_and_never_a_default() {
    let d = parts(&format!("{HEADER}\np1,,,,,\"\"\n")).unwrap();
    let p = d.participant(&pid("p1")).unwrap();
    assert!(p.fields().is_empty());
    for name in ["age", "licence", "born", "level", "note"] {
        assert_eq!(p.field(name), None, "{name}");
    }
}

#[test]
fn a_number_keeps_the_literal_the_file_gives() {
    let d = parts(&row_with(0, "007")).unwrap();
    assert_eq!(
        d.participant(&pid("p1")).unwrap().field("age"),
        Some(&text("007"))
    );
}

#[test]
fn text_is_kept_whole_including_spaces_and_line_ends() {
    let d = parts(&row_with(4, "\"  a\n b  \"")).unwrap();
    assert_eq!(
        d.participant(&pid("p1")).unwrap().field("note"),
        Some(&text("  a\n b  "))
    );
    let d = parts(&row_with(4, " ")).unwrap();
    assert_eq!(
        d.participant(&pid("p1")).unwrap().field("note"),
        Some(&text(" "))
    );
}

#[test]
fn the_task_observable_a_missing_column_a_duplicate_and_a_bad_number() {
    let file = "participant,age,licence,born,level\n\
                p1,17,true,2010-01-01,Gold\n\
                p2,1e3,false,2010-02-02,Silver\n\
                p1,18,true,2010-03-03,Gold\n";
    let state = parts(file).unwrap_err();
    let got = view(&state);
    assert_eq!(
        got,
        vec![
            (
                Code::AppInputFieldMissing,
                Some(1),
                Some(1),
                Some("note".to_owned())
            ),
            value_err(3, 4, "age"),
            (
                Code::AppInputDuplicateParticipant,
                Some(4),
                Some(1),
                Some("participant".to_owned())
            ),
        ]
    );
    let dup = &state.as_slice()[2];
    match &dup.related()[0] {
        Location::Input(l) => assert_eq!(
            (l.line, l.column, l.field.as_deref()),
            (Some(2), Some(1), Some("participant"))
        ),
        other => panic!("{other:?}"),
    }
    assert!(state.has_errors());
}

#[test]
fn rt_data_depends_on_no_engine_or_compiler_crate() {
    // UD-41 (C1) approves the one added edge to rt-compiled, which is read
    // for declarations only. No engine, compiler, validator or explain crate
    // may appear. The manifest is the evidence the crate cannot name one.
    let manifest = include_str!("../../Cargo.toml");
    let deps = manifest
        .split("[dev-dependencies]")
        .next()
        .unwrap()
        .split("[dependencies]")
        .nth(1)
        .unwrap();
    let names: BTreeSet<&str> = deps
        .lines()
        .filter_map(|l| l.split('=').next())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();
    assert_eq!(
        names,
        BTreeSet::from(["rt-compiled", "rt-foundation", "rt-json", "rt-numeric"])
    );
}

// One test per row of the error table (design §8.3).

#[test]
fn rt_app_0001_unknown_column_without_ignore() {
    let file = format!("{HEADER},extra\np1,1,true,2010-01-01,Gold,x,y\n");
    assert_eq!(
        bad(&file),
        vec![(
            Code::AppInputMalformed,
            Some(1),
            Some(41),
            Some("extra".to_owned())
        )]
    );
}

#[test]
fn ignore_column_skips_an_unknown_column_and_its_cells() {
    let file = format!("{HEADER},extra\np1,1,true,2010-01-01,Gold,x,\"not checked \"\n");
    let options = MappingOptions {
        ignore_columns: BTreeSet::from(["extra".to_owned()]),
    };
    let d = map_participants(&csv(&file).unwrap(), &schema(), &options).unwrap();
    assert_eq!(d.participant(&pid("p1")).unwrap().fields().len(), 5);
}

#[test]
fn ignore_column_does_not_hide_a_missing_declared_input() {
    let file = "participant,age,licence,born,level\np1,1,true,2010-01-01,Gold\n";
    let options = MappingOptions {
        ignore_columns: BTreeSet::from(["note".to_owned()]),
    };
    let e = map_participants(&csv(file).unwrap(), &schema(), &options).unwrap_err();
    assert_eq!(
        view(&e),
        vec![(
            Code::AppInputFieldMissing,
            Some(1),
            Some(1),
            Some("note".to_owned())
        )]
    );
}

#[test]
fn ignore_column_does_not_stop_a_declared_column_from_being_checked() {
    let file = format!("{HEADER}\np1,x,,,,\"\"\n");
    let options = MappingOptions {
        ignore_columns: BTreeSet::from(["age".to_owned()]),
    };
    let e = map_participants(&csv(&file).unwrap(), &schema(), &options).unwrap_err();
    assert_eq!(view(&e), vec![value_err(2, 4, "age")]);
}

#[test]
fn a_repeated_or_empty_header_name_is_malformed() {
    let e = bad(&format!("{HEADER},age\np1,1,true,2010-01-01,Gold,x,2\n"));
    assert_eq!(
        e,
        vec![(
            Code::AppInputMalformed,
            Some(1),
            Some(41),
            Some("age".to_owned())
        )]
    );
    let e = bad(&format!("{HEADER},\"\"\np1,1,true,2010-01-01,Gold,x,2\n"));
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].0, Code::AppInputMalformed);
    assert_eq!((e[0].1, e[0].2), (Some(1), Some(41)));
}

#[test]
fn a_hand_built_ragged_row_is_malformed_not_a_panic() {
    let mut t = csv(&format!("{HEADER}\np1,1,true,2010-01-01,Gold,x\n")).unwrap();
    t.rows[0].pop();
    let e = map_participants(&t, &schema(), &MappingOptions::default()).unwrap_err();
    assert_eq!(e.as_slice().len(), 1);
    assert_eq!(e.as_slice()[0].code(), Code::AppInputMalformed);
}

#[test]
fn rt_app_0002_participant_column_absent() {
    let file = "age,licence,born,level,note\n1,true,2010-01-01,Gold,x\n";
    assert_eq!(
        bad(file),
        vec![(
            Code::AppInputFieldMissing,
            Some(1),
            Some(1),
            Some("participant".to_owned())
        )]
    );
}

#[test]
fn rt_app_0002_every_absent_input_and_participant_are_reported() {
    let file = "age,licence,born,level\n1,true,2010-01-01,Gold\n";
    let missing = |name: &str| {
        (
            Code::AppInputFieldMissing,
            Some(1),
            Some(1),
            Some(name.to_owned()),
        )
    };
    assert_eq!(bad(file), vec![missing("note"), missing("participant")]);
}

#[test]
fn rt_app_0003_duplicate_participant_with_the_first_row_related() {
    let file = format!("{HEADER}\np1,1,,,,\"\"\np2,2,,,,\"\"\np1,3,,,,\"\"\n");
    let e = parts(&file).unwrap_err();
    assert_eq!(e.as_slice().len(), 1);
    let d = &e.as_slice()[0];
    assert_eq!(d.code(), Code::AppInputDuplicateParticipant);
    match (d.primary(), d.related()) {
        (Location::Input(p), [Location::Input(r)]) => {
            assert_eq!((p.line, p.column), (Some(4), Some(1)));
            assert_eq!((r.line, r.column), (Some(2), Some(1)));
            assert_eq!(r.field.as_deref(), Some("participant"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn participant_identifiers_are_compared_bytewise() {
    let file = format!("{HEADER}\np1,,,,,\"\"\nP1,,,,,\"\"\ne\u{301},,,,,\"\"\n\u{e9},,,,,\"\"\n");
    assert_eq!(parts(&file).unwrap().participants().len(), 4);
}

#[test]
fn participant_identifier_rules() {
    let long = "\u{e9}".repeat(200);
    let file = format!("{HEADER}\n{long},,,,,\"\"\n");
    assert!(parts(&file).is_ok());
    let file = format!("{HEADER}\n{},,,,,\"\"\n", "\u{e9}".repeat(201));
    assert_eq!(bad(&file), vec![value_err(2, 1, "participant")]);
    let file = format!("{HEADER}\n,,,,,\"\"\n");
    assert_eq!(bad(&file), vec![value_err(2, 1, "participant")]);
    let file = format!("{HEADER}\n\"a\tb\",,,,,\"\"\n");
    assert_eq!(bad(&file), vec![value_err(2, 1, "participant")]);
}

fn accepts(index: usize, raw: &str, expect: DataValue, field: &str) {
    let d = parts(&row_with(index, raw)).unwrap_or_else(|e| panic!("{raw:?}: {:?}", view(&e)));
    assert_eq!(
        d.participant(&pid("p1")).unwrap().field(field),
        Some(&expect),
        "{raw:?}"
    );
}

fn rejects(index: usize, raw: &str, field: &str) {
    let column = 4 + u32::try_from(index).unwrap();
    assert_eq!(
        bad(&row_with(index, raw)),
        vec![value_err(2, column, field)],
        "{raw:?}"
    );
}

#[test]
fn rt_app_0004_number_forms_of_requirement_8_27() {
    let digits38 = "12345678901234567890123456789012345678";
    let frac28 = "0.1234567890123456789012345678";
    let mixed38 = "9999999999.1234567890123456789012345678";
    for ok in [
        "0", "-0", "17", "-17", "3.5", "0.125", "007", "-0.50", digits38, frac28, mixed38,
    ] {
        accepts(0, ok, text(ok), "age");
    }
    let digits39 = "123456789012345678901234567890123456789";
    let frac29 = "0.12345678901234567890123456789";
    for no in [
        "1e3",
        "1E3",
        "1e+3",
        "2.5e-1",
        "+5",
        "\"1,000\"",
        "1 000",
        "NaN",
        "nan",
        "Infinity",
        "inf",
        "-",
        ".",
        ".5",
        "5.",
        " 5",
        "5 ",
        "1/2",
        "0x10",
        "--1",
        "1.2.3",
        "\u{ff11}\u{ff12}",
        "\u{663}",
        digits39,
        frac29,
    ] {
        rejects(0, no, "age");
    }
}

#[test]
fn rt_app_0004_boolean_is_exactly_true_or_false() {
    accepts(1, "true", DataValue::Bool(true), "licence");
    accepts(1, "false", DataValue::Bool(false), "licence");
    for no in [
        "TRUE", "True", "False", "1", "0", "yes", "t", " true", "false ",
    ] {
        rejects(1, no, "licence");
    }
}

#[test]
fn rt_app_0004_dates_are_valid_calendar_dates() {
    for ok in [
        "2026-02-28",
        "2028-02-29",
        "2000-02-29",
        "0001-01-01",
        "9999-12-31",
        "2026-04-30",
    ] {
        accepts(2, ok, text(ok), "born");
    }
    for no in [
        "2026-02-29",
        "1900-02-29",
        "2100-02-29",
        "2026-13-01",
        "2026-00-10",
        "2026-04-31",
        "2026-04-00",
        "2026-4-1",
        "2026/04/01",
        "20260401",
        "0000-01-01",
        " 2026-04-01",
        "2026-04-01 ",
        "\u{ff12}026-04-01",
        "2026-04-01T00:00",
    ] {
        rejects(2, no, "born");
    }
}

#[test]
fn rt_app_0004_enum_is_a_declared_member_case_sensitive() {
    accepts(3, "Gold", text("Gold"), "level");
    accepts(3, "Silver", text("Silver"), "level");
    for no in ["gold", "GOLD", "Gold ", " Gold", "Bronze", "Gol"] {
        rejects(3, no, "level");
    }
}

#[test]
fn rt_app_0004_every_bad_cell_of_a_file_is_reported() {
    let file = format!("{HEADER}\np1,x,TRUE,2026-02-29,gold,\"\"\n");
    assert_eq!(
        bad(&file),
        vec![
            value_err(2, 4, "age"),
            value_err(2, 6, "licence"),
            value_err(2, 11, "born"),
            value_err(2, 22, "level")
        ]
    );
}

#[test]
fn reserved_column_names_are_rejected_with_the_dgn_codes() {
    let flags = [
        "has_errors",
        "no_errors",
        "validation_status",
        "validation_result",
        "passed",
    ];
    let lifecycle = [
        "approval_status",
        "is_approved",
        "is_rejected",
        "is_proposed",
        "approval_record",
    ];
    for (names, code) in [
        (&flags[..], Code::DiagnosticStateContradiction),
        (&lifecycle[..], Code::LifecycleFlagContradiction),
    ] {
        for name in names {
            for cell in ["true", "false", "anything"] {
                let file = format!("participant,age,{name}\np1,5,{cell}\n");
                assert_eq!(
                    bad(&file),
                    vec![(code, Some(1), Some(17), Some((*name).to_owned()))],
                    "{name} {cell}"
                );
            }
        }
    }
}

#[test]
fn both_flag_columns_give_one_dgn_0002_naming_both() {
    let e = parts("participant,no_errors,has_errors\np1,true,false\n").unwrap_err();
    assert_eq!(e.as_slice().len(), 1);
    let d = &e.as_slice()[0];
    assert_eq!(d.code(), Code::DiagnosticStateBothFlags);
    assert!(d.message().contains("no_errors") && d.message().contains("has_errors"));
    match (d.primary(), d.related()) {
        (Location::Input(p), [Location::Input(r)]) => {
            assert_eq!(
                (p.column, p.field.as_deref()),
                (Some(13), Some("no_errors"))
            );
            assert_eq!(
                (r.column, r.field.as_deref()),
                (Some(23), Some("has_errors"))
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn each_reserved_occurrence_gives_exactly_one_diagnostic() {
    let e = bad("participant,is_approved,approval_status\np1,true,Approved\n");
    assert_eq!(
        e,
        vec![
            (
                Code::LifecycleFlagContradiction,
                Some(1),
                Some(13),
                Some("is_approved".to_owned())
            ),
            (
                Code::LifecycleFlagContradiction,
                Some(1),
                Some(25),
                Some("approval_status".to_owned())
            ),
        ]
    );
    let e = bad("participant,passed,validation_status\np1,true,passed\n");
    assert_eq!(e.len(), 2);
    assert!(e.iter().all(|v| v.0 == Code::DiagnosticStateContradiction));
}

#[test]
fn a_reserved_column_runs_no_later_stage_of_the_file() {
    let file = "participant,age,bogus,has_errors\np1,1e3,x,false\np1,1,y,true\n";
    assert_eq!(
        bad(file),
        vec![(
            Code::DiagnosticStateContradiction,
            Some(1),
            Some(23),
            Some("has_errors".to_owned())
        )]
    );
}

#[test]
fn a_reserved_name_is_matched_exactly_so_other_spellings_are_unknown_columns() {
    let file = format!("{HEADER},Has_Errors\np1,1,,,,x,y\n");
    let e = bad(&file);
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].0, Code::AppInputMalformed);
}

// Collections.

const COLLECTIONS: &str = "participant,collection,row,place,medal,reason";

fn colls(text: &str, base: CompetitionData) -> Result<CompetitionData, DiagnosticState> {
    map_collections(&csv(text).unwrap(), &schema(), base)
}

fn colls_bad(text: &str) -> Vec<View> {
    view(&colls(text, base(&["p1", "p2"])).unwrap_err())
}

fn element(pairs: &[(&str, &str)]) -> Element {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), text(v)))
        .collect()
}

#[test]
fn collections_element_empty_and_absent_are_three_different_things() {
    let file = format!(
        "{COLLECTIONS}\n\
         p1,results,element,1,Gold,\"\"\n\
         p1,results,element,2,,\"\"\n\
         p2,results,empty,,,\"\"\n\
         p1,penalties,empty,,,\"\"\n"
    );
    let d = colls(&file, base(&["p1", "p2", "p3"])).unwrap();
    let p1 = d.participant(&pid("p1")).unwrap();
    let a = element(&[("place", "1"), ("medal", "Gold")]);
    let b = element(&[("place", "2")]);
    assert_eq!(p1.collection("results"), Some(&[a, b][..]));
    assert_eq!(p1.collection("penalties"), Some(&[][..]));
    let p2 = d.participant(&pid("p2")).unwrap();
    assert_eq!(p2.collection("results"), Some(&[][..]));
    assert_eq!(p2.collection("penalties"), None);
    let p3 = d.participant(&pid("p3")).unwrap();
    assert_eq!(p3.collection("results"), None);
    assert!(p3.collections().is_empty());
}

#[test]
fn an_element_row_with_no_cells_is_one_empty_element() {
    let file = format!("{COLLECTIONS}\np1,results,element,,,\"\"\n");
    let d = colls(&file, base(&["p1"])).unwrap();
    let got = d.participant(&pid("p1")).unwrap().collection("results");
    assert_eq!(got, Some(&[Element::new()][..]));
}

#[test]
fn collection_rows_keep_the_fields_and_other_collections_of_the_base() {
    let mut fields = BTreeMap::new();
    fields.insert("age".to_owned(), text("9"));
    let mut cols = BTreeMap::new();
    cols.insert("penalties".to_owned(), vec![element(&[("reason", "late")])]);
    let base = CompetitionData::new(BTreeMap::from([(
        pid("p1"),
        Participant::new(fields, cols),
    )]));
    let file = format!("{COLLECTIONS}\np1,results,empty,,,\"\"\n");
    let d = colls(&file, base).unwrap();
    let p = d.participant(&pid("p1")).unwrap();
    assert_eq!(p.field("age"), Some(&text("9")));
    assert_eq!(
        p.collection("penalties"),
        Some(&[element(&[("reason", "late")])][..])
    );
    assert_eq!(p.collection("results"), Some(&[][..]));
}

#[test]
fn identical_element_rows_stay_two_elements() {
    let file = format!("{COLLECTIONS}\np1,results,element,1,,\"\"\np1,results,element,1,,\"\"\n");
    let d = colls(&file, base(&["p1"])).unwrap();
    assert_eq!(
        d.participant(&pid("p1")).unwrap().collection("results"),
        Some(&[element(&[("place", "1")]), element(&[("place", "1")])][..])
    );
}

#[test]
fn collections_rt_app_0004_unknown_participant() {
    let file = format!("{COLLECTIONS}\np9,results,element,1,Gold,\"\"\n");
    assert_eq!(colls_bad(&file), vec![value_err(2, 1, "participant")]);
}

#[test]
fn collections_rt_app_0004_undeclared_collection() {
    let file = format!("{COLLECTIONS}\np1,medals,element,1,Gold,\"\"\n");
    assert_eq!(colls_bad(&file), vec![value_err(2, 4, "collection")]);
}

#[test]
fn collections_rt_app_0004_row_must_be_element_or_empty() {
    for raw in ["both", "Element", "EMPTY", "", " element"] {
        let file = format!("{COLLECTIONS}\np1,results,{raw},,,\"\"\n");
        assert_eq!(colls_bad(&file), vec![value_err(2, 12, "row")], "{raw:?}");
    }
}

#[test]
fn collections_rt_app_0004_an_empty_row_has_only_empty_cells() {
    let file = format!("{COLLECTIONS}\np1,results,empty,1,,\"\"\n");
    assert_eq!(colls_bad(&file), vec![value_err(2, 18, "place")]);
}

#[test]
fn collections_rt_app_0004_element_cells_follow_their_declared_types() {
    let file = format!("{COLLECTIONS}\np1,results,element,x,gold,\"\"\n");
    assert_eq!(
        colls_bad(&file),
        vec![value_err(2, 20, "place"), value_err(2, 22, "medal")]
    );
}

#[test]
fn collections_rt_app_0004_a_cell_for_a_field_of_another_collection() {
    let file = format!("{COLLECTIONS}\np1,results,element,1,Gold,oops\n");
    assert_eq!(colls_bad(&file), vec![value_err(2, 27, "reason")]);
}

#[test]
fn collections_rt_app_0004_an_empty_row_beside_an_element_row_contradicts() {
    let file = format!("{COLLECTIONS}\np1,results,element,1,,\"\"\np1,results,empty,,,\"\"\n");
    let e = colls(&file, base(&["p1"])).unwrap_err();
    assert_eq!(e.as_slice().len(), 1);
    let d = &e.as_slice()[0];
    assert_eq!(d.code(), Code::AppInputValueInvalid);
    match (d.primary(), d.related()) {
        (Location::Input(p), [Location::Input(r)]) => {
            assert_eq!(
                (p.line, p.column, p.field.as_deref()),
                (Some(3), Some(12), Some("row"))
            );
            assert_eq!(
                (r.line, r.column, r.field.as_deref()),
                (Some(2), Some(12), Some("row"))
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn collections_header_faults() {
    let e = view(
        &colls(
            "collection,row,place,medal,reason\nresults,empty,,,\"\"\n",
            base(&[]),
        )
        .unwrap_err(),
    );
    assert_eq!(
        e,
        vec![(
            Code::AppInputFieldMissing,
            Some(1),
            Some(1),
            Some("participant".to_owned())
        )]
    );
    let e = view(&colls("participant,collection,row,place,medal\n", base(&[])).unwrap_err());
    assert_eq!(
        e,
        vec![(
            Code::AppInputFieldMissing,
            Some(1),
            Some(1),
            Some("reason".to_owned())
        )]
    );
    let e = view(&colls(&format!("{COLLECTIONS},zzz\n"), base(&[])).unwrap_err());
    assert_eq!(
        e,
        vec![(
            Code::AppInputMalformed,
            Some(1),
            Some(47),
            Some("zzz".to_owned())
        )]
    );
}

#[test]
fn collections_reserved_header_name_gives_a_dgn_code() {
    let e = view(
        &colls(
            &format!("{COLLECTIONS},approval_status\np1,results,empty,,,\"\",x\n"),
            base(&["p1"]),
        )
        .unwrap_err(),
    );
    assert_eq!(
        e,
        vec![(
            Code::LifecycleFlagContradiction,
            Some(1),
            Some(47),
            Some("approval_status".to_owned())
        )]
    );
}

// Published.

pub(super) fn kinds() -> TargetKinds {
    TargetKinds::new([
        (tn("eligible"), TargetKind::Eligibility),
        (tn("score"), TargetKind::Score),
        (tn("derived"), TargetKind::Derived),
        (tn("standing"), TargetKind::Ranking),
        (tn("fallback"), TargetKind::Other),
    ])
}

pub(super) fn requested(names: &[&str]) -> BTreeSet<TargetName> {
    names.iter().map(|n| tn(n)).collect()
}

fn publ(text: &str, names: &[&str]) -> Result<MappedPublished, DiagnosticState> {
    map_published(
        &csv(text).unwrap(),
        &kinds(),
        &requested(names),
        &base(&["p1", "p2", "p3"]),
    )
}

fn publ_bad(text: &str) -> Vec<View> {
    view(&publ(text, &["eligible", "score", "derived", "standing"]).unwrap_err())
}

pub(super) fn num(s: &str) -> PublishedValue {
    PublishedValue::Number(PublishedNumber::parse(s).unwrap())
}

const FILE: &str = "participant,target,value\n\
                    p1,standing,1\n\
                    p2,standing,1\n\
                    p3,standing,3\n\
                    p1,eligible,true\n\
                    p2,eligible,false\n\
                    p1,score,7.50\n\
                    p1,derived,-2.25\n";

#[test]
fn published_values_by_kind_with_a_tie() {
    let m = publ(FILE, &["eligible", "score", "derived", "standing"]).unwrap();
    assert!(m.unused.is_empty());
    let e = m.result.entries();
    assert_eq!(e.len(), 7);
    let get = |p: &str, t: &str| e.get(&(pid(p), tn(t))).cloned();
    assert_eq!(get("p1", "standing"), Some(num("1")));
    assert_eq!(get("p2", "standing"), Some(num("1")));
    assert_eq!(get("p3", "standing"), Some(num("3")));
    assert_eq!(get("p1", "eligible"), Some(PublishedValue::Bool(true)));
    assert_eq!(get("p2", "eligible"), Some(PublishedValue::Bool(false)));
    assert_eq!(get("p1", "score"), Some(num("7.5")));
    assert_eq!(get("p1", "derived"), Some(num("-2.25")));
}

#[test]
fn published_rows_of_declared_targets_that_were_not_requested_are_listed_not_dropped() {
    let m = publ(FILE, &["eligible"]).unwrap();
    assert_eq!(m.result.entries().len(), 2);
    let got: Vec<_> = m
        .unused
        .iter()
        .map(|u| (u.participant.as_str(), u.target.as_str(), u.line, u.column))
        .collect();
    assert_eq!(
        got,
        vec![
            ("p1", "standing", 2, 1),
            ("p2", "standing", 3, 1),
            ("p3", "standing", 4, 1),
            ("p1", "score", 7, 1),
            ("p1", "derived", 8, 1),
        ]
    );
}

#[test]
fn an_unused_published_row_is_still_checked() {
    let file = "participant,target,value\np1,score,xx\n";
    let e = view(&publ(file, &["eligible"]).unwrap_err());
    assert_eq!(e, vec![value_err(2, 10, "value")]);
}

#[test]
fn published_rt_app_0005_undeclared_target_at_the_cell() {
    let file = "participant,target,value\np1,nope,1\n";
    assert_eq!(
        publ_bad(file),
        vec![(
            Code::AppTargetNotDeclared,
            Some(2),
            Some(4),
            Some("target".to_owned())
        )]
    );
}

#[test]
fn published_rt_app_0006_requested_target_of_an_unreportable_kind() {
    let file = "participant,target,value\np1,fallback,1\n";
    let requested_all = &["eligible", "score", "derived", "standing", "fallback"];
    assert_eq!(
        view(&publ(file, requested_all).unwrap_err()),
        vec![(
            Code::AppTargetKindMismatch,
            Some(2),
            Some(4),
            Some("target".to_owned())
        )]
    );
}

#[test]
fn published_rt_app_0004_unknown_participant() {
    let file = "participant,target,value\np9,eligible,true\n";
    assert_eq!(publ_bad(file), vec![value_err(2, 1, "participant")]);
}

#[test]
fn published_rt_app_0004_eligibility_is_true_or_false() {
    for no in ["TRUE", "False", "1", " true", "\"\""] {
        let file = format!("participant,target,value\np1,eligible,{no}\n");
        assert_eq!(publ_bad(&file), vec![value_err(2, 13, "value")], "{no:?}");
    }
}

#[test]
fn published_rt_app_0004_numbers_follow_the_decimal_rules() {
    for target in ["score", "derived"] {
        let col = 4 + u32::try_from(target.len()).unwrap() + 1;
        for no in ["1e3", "+1", "NaN", "7.", " 7", "1,5", ""] {
            let raw = if no.contains(',') || no.is_empty() {
                format!("\"{no}\"")
            } else {
                no.to_owned()
            };
            let file = format!("participant,target,value\np1,{target},{raw}\n");
            assert_eq!(publ_bad(&file), vec![value_err(2, col, "value")], "{no:?}");
        }
    }
}

#[test]
fn published_rt_app_0004_a_rank_position_is_a_positive_integer() {
    for no in ["0", "-1", "1.5", "1.0", "abc", "\"\"", " 1", "1e0", "+1"] {
        let file = format!("participant,target,value\np1,standing,{no}\n");
        assert_eq!(publ_bad(&file), vec![value_err(2, 13, "value")], "{no:?}");
    }
    let m = publ("participant,target,value\np1,standing,12\n", &["standing"]).unwrap();
    assert_eq!(m.result.entries().values().next(), Some(&num("12")));
}

#[test]
fn published_rt_app_0004_a_repeated_pair_is_reported_at_the_second_row() {
    let file = "participant,target,value\np1,eligible,true\np2,eligible,true\np1,eligible,false\n";
    let e = publ(file, &["eligible"]).unwrap_err();
    assert_eq!(e.as_slice().len(), 1);
    let d = &e.as_slice()[0];
    assert_eq!(d.code(), Code::AppInputValueInvalid);
    match (d.primary(), d.related()) {
        (Location::Input(p), [Location::Input(r)]) => {
            assert_eq!((p.line, p.column), (Some(4), Some(4)));
            assert_eq!(p.field.as_deref(), Some("target"));
            assert_eq!((r.line, r.column), (Some(2), Some(4)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn published_header_faults() {
    let e = view(&publ("participant,target\np1,eligible\n", &["eligible"]).unwrap_err());
    assert_eq!(
        e,
        vec![(
            Code::AppInputFieldMissing,
            Some(1),
            Some(1),
            Some("value".to_owned())
        )]
    );
    let e = view(&publ("participant,target,value,extra\np1,eligible,true,x\n", &[]).unwrap_err());
    assert_eq!(
        e,
        vec![(
            Code::AppInputMalformed,
            Some(1),
            Some(26),
            Some("extra".to_owned())
        )]
    );
}

#[test]
fn published_reserved_names_are_rejected_in_the_header_and_as_a_target() {
    let e = view(
        &publ(
            "participant,target,value,no_errors\np1,eligible,true,true\n",
            &[],
        )
        .unwrap_err(),
    );
    assert_eq!(
        e,
        vec![(
            Code::DiagnosticStateContradiction,
            Some(1),
            Some(26),
            Some("no_errors".to_owned())
        )]
    );
    for (name, code) in [
        ("has_errors", Code::DiagnosticStateContradiction),
        ("passed", Code::DiagnosticStateContradiction),
        ("is_approved", Code::LifecycleFlagContradiction),
        ("approval_record", Code::LifecycleFlagContradiction),
    ] {
        let file = format!("participant,target,value\np1,{name},true\np2,eligible,1x\n");
        assert_eq!(
            publ_bad(&file),
            vec![(code, Some(2), Some(4), Some("target".to_owned()))],
            "{name}"
        );
    }
}

// Canonical writers against the committed demonstration files.

const DEMO: &str = "../../demo/stand-in-hand/data/canonical";

fn demo_schema() -> InputSchema {
    InputSchema::new(
        [
            ("age".to_owned(), FieldType::Number),
            ("licence".to_owned(), FieldType::Boolean),
            ("judges".to_owned(), FieldType::Number),
            ("technical".to_owned(), FieldType::Number),
        ],
        [],
    )
}

fn read_demo(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(DEMO)
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn demo_participants(file: &str) -> CompetitionData {
    map_participants(
        &csv(file).unwrap(),
        &demo_schema(),
        &MappingOptions::default(),
    )
    .unwrap()
}

fn demo_file_data(name: &str) -> CompetitionData {
    let id = InputId::new(name).unwrap();
    parse_data(&id, &read_demo(name), DataLimits::default()).unwrap()
}

// The committed demo data and published files list fields and entries in
// authoring order, so they are not byte fixed points of the canonical writer
// (only questions.json is). The mapped values are therefore compared with the
// values of the committed files, and the bytes with the writer's output.

#[test]
fn mapped_demo_data_equals_the_committed_demo_data_with_a_missing_age() {
    let d = demo_participants(
        "participant,age,licence,judges,technical\n\
         p1,17,true,8,7\n\
         p2,,true,7,9\n\
         p3,15,true,6,6\n\
         p4,21,true,9,6\n\
         p5,19,true,6,6\n",
    );
    let committed = demo_file_data("data.json");
    assert_eq!(d, committed);
    assert_eq!(d.participant(&pid("p2")).unwrap().field("age"), None);
    assert_eq!(data_bytes(&d), data_bytes(&committed));
}

#[test]
fn mapped_demo_data_equals_the_committed_complete_demo_data_in_any_row_order() {
    let d = demo_participants(
        "participant,age,licence,judges,technical\n\
         p5,19,true,6,6\n\
         p2,20,true,7,9\n\
         p1,17,true,8,7\n\
         p4,21,true,9,6\n\
         p3,15,true,6,6\n",
    );
    assert_eq!(d, demo_file_data("data-complete.json"));
}

#[test]
fn data_bytes_are_the_hand_written_canonical_form() {
    let d = demo_participants("participant,age,licence,judges,technical\np1,17,true,,7\n");
    let want = "{\n  \"format\": \"ruletrace-data\",\n  \"version\": 1,\n  \"participants\": [\n    {\n      \"id\": \"p1\",\n      \"fields\": {\n        \"age\": \"17\",\n        \"licence\": true,\n        \"technical\": \"7\"\n      },\n      \"collections\": {}\n    }\n  ]\n}\n";
    assert_eq!(String::from_utf8(data_bytes(&d)).unwrap(), want);
}

fn demo_kinds() -> TargetKinds {
    TargetKinds::new([
        (tn("eligible"), TargetKind::Eligibility),
        (tn("score"), TargetKind::Score),
        (tn("standing"), TargetKind::Ranking),
    ])
}

fn demo_published(csv_text: &str) -> PublishedResult {
    let d = base(&["p1", "p2", "p3", "p4", "p5"]);
    map_published(
        &csv(csv_text).unwrap(),
        &demo_kinds(),
        &requested(&["eligible", "score"]),
        &d,
    )
    .unwrap()
    .result
}

fn demo_file_published(name: &str) -> PublishedResult {
    let id = InputId::new(name).unwrap();
    parse_published(&id, &read_demo(name), crate::default_parse_limits()).unwrap()
}

#[test]
fn mapped_published_equals_the_committed_demo_published_files() {
    let rows = "participant,target,value\n\
                p5,score,6\n\
                p1,eligible,true\n\
                p3,eligible,false\n\
                p4,eligible,true\n\
                p5,eligible,true\n\
                p1,score,7.6\n\
                p2,score,7.8\n\
                p3,score,6\n";
    let plain = demo_published(&format!("{rows}p4,score,7.8\n"));
    let seeded = demo_published(&format!("{rows}p4,score,7.9\n"));
    assert_eq!(plain, demo_file_published("published.json"));
    assert_eq!(seeded, demo_file_published("published-seeded.json"));
    assert_ne!(plain, seeded);
}

#[test]
fn published_bytes_are_the_hand_written_canonical_form() {
    let r = demo_published("participant,target,value\np1,score,7.50\np1,eligible,true\n");
    let want = "{\n  \"format\": \"ruletrace-published\",\n  \"version\": 1,\n  \"entries\": [\n    {\n      \"participant\": \"p1\",\n      \"target\": \"eligible\",\n      \"value\": true\n    },\n    {\n      \"participant\": \"p1\",\n      \"target\": \"score\",\n      \"value\": \"7.5\"\n    }\n  ]\n}\n";
    assert_eq!(String::from_utf8(published_bytes(&r)).unwrap(), want);
}

#[test]
fn questions_bytes_equal_the_committed_demo_questions() {
    let mut questions = Vec::new();
    for target in ["eligible", "score"] {
        for p in ["p1", "p2", "p3", "p4", "p5"] {
            questions.push(Question::Value {
                target: tn(target),
                participant: pid(p),
            });
        }
    }
    questions.push(Question::Rank {
        target: tn("standing"),
        participants: None,
    });
    assert_eq!(questions_bytes(&questions), read_demo("questions.json"));
}

#[test]
fn mapped_data_survives_the_canonical_round_trip() {
    let file = format!("{HEADER}\np1,17,true,2010-01-31,Gold,hello\np2,0,false,,,\"\"\n");
    let d = parts(&file).unwrap();
    let input = InputId::new("d.json").unwrap();
    let back = parse_data(&input, &data_bytes(&d), DataLimits::default()).unwrap();
    assert_eq!(back, d);
    assert_eq!(data_bytes(&back), data_bytes(&d));
}

#[test]
fn mapped_published_survives_the_canonical_round_trip() {
    let m = publ(FILE, &["eligible", "score", "derived", "standing"]).unwrap();
    let input = InputId::new("p.json").unwrap();
    let back = parse_published(
        &input,
        &published_bytes(&m.result),
        crate::default_parse_limits(),
    )
    .unwrap();
    assert_eq!(back, m.result);
}
