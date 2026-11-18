//! Tests of the trailing delimiter rule (user decision A, 2026-11-17): a data
//! row that ends in an unquoted separator has an empty final cell, which is a
//! Missing_Value. Positions are counted by hand from the literal text.

use super::tests_csv::{csv, view, View};
use super::tests_map::{base, kinds, parts, requested, schema, HEADER};
use super::*;
use crate::{DataValue, ParticipantId};
use rt_foundation::Code;

fn cells(row: &[Cell]) -> Vec<(&str, u32, u32)> {
    row.iter()
        .map(|c| (c.text.as_str(), c.line, c.column))
        .collect()
}

fn pid(s: &str) -> ParticipantId {
    ParticipantId::new(s).unwrap()
}

fn malformed(line: u32, column: u32, field: Option<&str>) -> View {
    (
        Code::AppInputMalformed,
        Some(line),
        Some(column),
        field.map(str::to_owned),
    )
}

#[test]
fn a_trailing_separator_is_an_empty_final_cell_for_every_line_end() {
    for (text, rows) in [
        ("a,b\n1,\n", 1),
        ("a,b\r\n1,\r\n", 1),
        ("a,b\n1,", 1),
        ("a,b\r\n1,", 1),
        ("a,b\n1,\n3,\n", 2),
    ] {
        let t = csv(text).unwrap_or_else(|e| panic!("{text:?}: {:?}", view(&e)));
        assert_eq!(t.rows.len(), rows, "{text:?}");
        assert_eq!(cells(&t.rows[0]), vec![("1", 2, 1), ("", 2, 3)], "{text:?}");
    }
    let t = csv("a,b\n1,\n3,\n").unwrap();
    assert_eq!(cells(&t.rows[1]), vec![("3", 3, 1), ("", 3, 3)]);
}

#[test]
fn a_quoted_empty_final_cell_is_the_same_empty_cell() {
    let t = csv("a,b\n1,\"\"\n").unwrap();
    assert_eq!(cells(&t.rows[0]), vec![("1", 2, 1), ("", 2, 3)]);
}

#[test]
fn a_row_of_only_separators_has_that_many_empty_cells() {
    let t = csv("a,b,c\n,,\n").unwrap();
    assert_eq!(cells(&t.rows[0]), vec![("", 2, 1), ("", 2, 2), ("", 2, 3)]);
}

#[test]
fn a_trailing_separator_after_a_quoted_cell_is_an_empty_final_cell() {
    let t = csv("a,b\n\"x,y\",\n").unwrap();
    assert_eq!(cells(&t.rows[0]), vec![("x,y", 2, 1), ("", 2, 7)]);
}

#[test]
fn a_row_one_cell_short_is_still_an_error_with_the_missing_column_named() {
    let got = view(&csv("a,b,c\n1,2\n").unwrap_err());
    assert_eq!(got, vec![malformed(2, 4, Some("c"))]);
    let got = view(&csv("a,b,c\n1,\n").unwrap_err());
    assert_eq!(got, vec![malformed(2, 3, Some("c"))]);
}

#[test]
fn a_row_with_too_many_cells_counts_the_empty_final_cell() {
    let got = view(&csv("a,b\n1,2,\n").unwrap_err());
    assert_eq!(got, vec![malformed(2, 5, None)]);
    let got = view(&csv("a\n,\n").unwrap_err());
    assert_eq!(got, vec![malformed(2, 2, None)]);
    let got = view(&csv("a,b\n1,2,3,\n").unwrap_err());
    assert_eq!(got, vec![malformed(2, 5, None)]);
}

#[test]
fn a_header_ending_in_a_separator_is_reported_once_and_no_row_is_read() {
    let got = view(&csv("a,b,\n1,2,\n3,4,\n").unwrap_err());
    assert_eq!(got, vec![malformed(1, 4, None)]);
}

#[test]
fn a_lone_empty_cell_line_is_still_a_blank_line() {
    let got = view(&csv("a\n\n").unwrap_err());
    assert_eq!(got, vec![malformed(2, 1, None)]);
}

// Mapping.

#[test]
fn a_trailing_empty_final_cell_is_missing_for_each_declared_type() {
    let all = ["age", "licence", "born", "level", "note"];
    let values = [
        ("age", "17"),
        ("licence", "true"),
        ("born", "2010-01-31"),
        ("level", "Gold"),
        ("note", "hi"),
    ];
    for last in all {
        let others: Vec<(&str, &str)> =
            values.iter().copied().filter(|(n, _)| *n != last).collect();
        let head: Vec<&str> = others.iter().map(|(n, _)| *n).collect();
        let vals: Vec<&str> = others.iter().map(|(_, v)| *v).collect();
        let file = format!(
            "participant,{},{last}\np1,{},\n",
            head.join(","),
            vals.join(",")
        );
        let d = parts(&file).unwrap_or_else(|e| panic!("{last}: {:?}", view(&e)));
        let p = d.participant(&pid("p1")).unwrap();
        assert_eq!(p.field(last), None, "{last} must be missing");
        for (name, _) in &others {
            assert!(p.field(name).is_some(), "{name} kept beside missing {last}");
        }
        assert_eq!(p.fields().len(), 4, "{last}");
    }
}

#[test]
fn every_cell_empty_gives_a_participant_with_no_fields() {
    let d = parts(&format!("{HEADER}\np1,,,,,\n")).unwrap();
    let p = d.participant(&pid("p1")).unwrap();
    assert!(p.fields().is_empty());
    assert!(p.collections().is_empty());
}

#[test]
fn an_empty_final_cell_never_becomes_a_default_value() {
    let d = parts(&format!("{HEADER}\np1,0,false,,Gold,\np2,,,,,\"\"\n")).unwrap();
    let p1 = d.participant(&pid("p1")).unwrap();
    assert_eq!(p1.field("note"), None);
    assert_eq!(p1.field("age"), Some(&DataValue::Text("0".to_owned())));
    assert_eq!(p1.field("licence"), Some(&DataValue::Bool(false)));
    let p2 = d.participant(&pid("p2")).unwrap();
    assert_eq!(p2.field("note"), None);
    assert_eq!(d.participant(&pid("p1")).unwrap().field("born"), None);
}

#[test]
fn a_trailing_and_a_quoted_empty_final_cell_give_the_same_data() {
    let a = parts(&format!("{HEADER}\np1,1,true,,Gold,\n")).unwrap();
    let b = parts(&format!("{HEADER}\np1,1,true,,Gold,\"\"\n")).unwrap();
    assert_eq!(a, b);
    assert_eq!(data_bytes(&a), data_bytes(&b));
}

#[test]
fn a_participants_header_ending_in_a_separator_is_one_malformed_error() {
    let e = view(&csv(&format!("{HEADER},\np1,1,true,,Gold,x,\n")).unwrap_err());
    assert_eq!(e, vec![malformed(1, 40, None)]);
}

#[test]
fn a_participants_row_with_too_many_cells_via_a_trailing_separator_is_an_error() {
    let e = view(&csv(&format!("{HEADER}\np1,1,true,,Gold,x,\n")).unwrap_err());
    assert_eq!(e, vec![malformed(2, 19, None)]);
}

const COLLECTIONS: &str = "participant,collection,row,place,medal,reason";

fn colls(text: &str) -> crate::CompetitionData {
    map_collections(&csv(text).unwrap(), &schema(), base(&["p1", "p2"])).unwrap()
}

#[test]
fn collections_trailing_empty_final_cells_are_missing_fields_and_empty_rows() {
    let file = format!(
        "{COLLECTIONS}\n\
         p1,results,element,1,Gold,\n\
         p1,results,element,,,\n\
         p2,penalties,empty,,,\n\
         p2,results,empty,,,\n"
    );
    let d = colls(&file);
    let p1 = d.participant(&pid("p1")).unwrap();
    let results = p1.collection("results").unwrap();
    assert_eq!(results.len(), 2);
    // Elements are held sorted, so the order of the two rows is not asserted.
    let mut sizes: Vec<usize> = results.iter().map(|e| e.len()).collect();
    sizes.sort_unstable();
    assert_eq!(sizes, vec![0, 2]);
    assert_eq!(p1.collection("penalties"), None);
    let p2 = d.participant(&pid("p2")).unwrap();
    assert_eq!(p2.collection("penalties"), Some(&[][..]));
    assert_eq!(p2.collection("results"), Some(&[][..]));
}

#[test]
fn collections_trailing_empty_element_field_is_missing_for_a_text_field() {
    let file = format!("{COLLECTIONS}\np1,penalties,element,,,\n");
    let d = colls(&file);
    let got = d.participant(&pid("p1")).unwrap().collection("penalties");
    assert_eq!(got.map(|e| e.len()), Some(1));
    assert!(got.unwrap()[0].is_empty());
}

#[test]
fn collections_an_empty_row_with_a_value_in_the_last_cell_is_still_an_error() {
    let file = format!("{COLLECTIONS}\np1,results,empty,,,x\n");
    let e = view(&map_collections(&csv(&file).unwrap(), &schema(), base(&["p1"])).unwrap_err());
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].0, Code::AppInputValueInvalid);
}

#[test]
fn collections_a_header_ending_in_a_separator_is_one_malformed_error() {
    let e = view(&csv(&format!("{COLLECTIONS},\np1,results,empty,,,,\n")).unwrap_err());
    assert_eq!(e, vec![malformed(1, 46, None)]);
}

fn publ(text: &str) -> Result<MappedPublished, rt_foundation::DiagnosticState> {
    map_published(
        &csv(text).unwrap(),
        &kinds(),
        &requested(&["eligible", "score", "derived", "standing"]),
        &base(&["p1"]),
    )
}

#[test]
fn an_empty_published_value_stays_invalid_with_or_without_a_trailing_separator() {
    for target in ["eligible", "score", "derived", "standing"] {
        let col = 4 + u32::try_from(target.len()).unwrap() + 1;
        let want = vec![(
            Code::AppInputValueInvalid,
            Some(2),
            Some(col),
            Some("value".to_owned()),
        )];
        for tail in [",\n", ",", ",\r\n", ",\"\"\n"] {
            let file = format!("participant,target,value\np1,{target}{tail}");
            let got = view(&publ(&file).unwrap_err());
            assert_eq!(got, want, "{target} {tail:?}");
        }
    }
}

#[test]
fn a_published_row_with_a_value_and_a_trailing_separator_has_too_many_cells() {
    let e = view(&csv("participant,target,value\np1,eligible,true,\n").unwrap_err());
    assert_eq!(e, vec![malformed(2, 18, None)]);
}
