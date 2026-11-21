//! Tests of unrequested published rows (user decision B, 2026-11-17, design
//! §8.3) and of Missing_Value handling in the canonical bytes (decision D).
//! Expected values and positions are written by hand.

use super::tests_csv::{csv, view, View};
use super::tests_map::{base, kinds, parts, requested, schema, HEADER};
use super::*;
use crate::{ParticipantId, TargetName};
use rt_foundation::{Code, DiagnosticState, Location};

fn pid(s: &str) -> ParticipantId {
    ParticipantId::new(s).unwrap()
}

fn tn(s: &str) -> TargetName {
    TargetName::new(s).unwrap()
}

fn publ(text: &str, names: &[&str]) -> Result<MappedPublished, DiagnosticState> {
    map_published(
        &csv(text).unwrap(),
        &kinds(),
        &requested(names),
        &base(&["p1", "p2", "p3"]),
    )
}

type Unused = (String, String, u32, u32, UnusedReason);

fn listed(m: &MappedPublished) -> Vec<Unused> {
    m.unused
        .iter()
        .map(|u| {
            (
                u.participant.as_str().to_owned(),
                u.target.as_str().to_owned(),
                u.line,
                u.column,
                u.reason,
            )
        })
        .collect()
}

fn unused(p: &str, t: &str, line: u32, reason: UnusedReason) -> Unused {
    (p.to_owned(), t.to_owned(), line, 1, reason)
}

fn at_target(code: Code, line: u32, column: u32) -> View {
    (code, Some(line), Some(column), Some("target".to_owned()))
}

const HEAD: &str = "participant,target,value\n";

#[test]
fn unrequested_reportable_targets_are_listed_with_the_not_requested_reason() {
    let file = format!("{HEAD}p1,score,7.5\np2,standing,1\np1,eligible,true\np3,derived,2\n");
    let m = publ(&file, &["eligible"]).unwrap();
    assert_eq!(
        listed(&m),
        vec![
            unused("p1", "score", 2, UnusedReason::NotRequested),
            unused("p2", "standing", 3, UnusedReason::NotRequested),
            unused("p3", "derived", 5, UnusedReason::NotRequested),
        ]
    );
    assert_eq!(m.result.entries().len(), 1);
}

#[test]
fn an_unrequested_unreportable_target_is_listed_not_rejected() {
    let file = format!("{HEAD}p1,fallback,1\np2,eligible,true\n");
    let m = publ(&file, &["eligible"]).unwrap();
    assert_eq!(
        listed(&m),
        vec![unused(
            "p1",
            "fallback",
            2,
            UnusedReason::NotRequestedKindNotReportable
        )]
    );
    assert_eq!(m.result.entries().len(), 1);
    assert!(m
        .result
        .entries()
        .contains_key(&(pid("p2"), tn("eligible"))));
}

#[test]
fn the_value_of_an_unrequested_unreportable_target_is_not_type_checked() {
    for value in ["1", "anything at all", "1e3", "\"\"", ""] {
        let file = format!("{HEAD}p1,fallback,{value}\n");
        let m = publ(&file, &[]).unwrap_or_else(|e| panic!("{value:?}: {:?}", view(&e)));
        assert_eq!(
            listed(&m),
            vec![unused(
                "p1",
                "fallback",
                2,
                UnusedReason::NotRequestedKindNotReportable
            )],
            "{value:?}"
        );
    }
}

#[test]
fn a_requested_unreportable_target_is_rt_app_0006_at_the_target_cell() {
    let file = format!("{HEAD}p1,fallback,1\n");
    let e = view(&publ(&file, &["fallback"]).unwrap_err());
    assert_eq!(e, vec![at_target(Code::AppTargetKindMismatch, 2, 4)]);
}

#[test]
fn requestedness_decides_between_listing_and_rt_app_0006_for_the_same_row() {
    let file = format!("{HEAD}p1,fallback,1\np2,fallback,2\n");
    assert!(publ(&file, &["eligible"]).is_ok());
    let e = view(&publ(&file, &["fallback"]).unwrap_err());
    assert_eq!(
        e,
        vec![
            at_target(Code::AppTargetKindMismatch, 2, 4),
            at_target(Code::AppTargetKindMismatch, 3, 4),
        ]
    );
}

#[test]
fn an_undeclared_target_is_rt_app_0005_whatever_is_requested() {
    let file = format!("{HEAD}p1,nope,1\n");
    for names in [&[][..], &["eligible"][..]] {
        let e = view(&publ(&file, names).unwrap_err());
        assert_eq!(e, vec![at_target(Code::AppTargetNotDeclared, 2, 4)]);
    }
}

#[test]
fn a_requested_reportable_value_is_checked_and_an_unrequested_one_still_is() {
    let file = format!("{HEAD}p1,score,xx\n");
    for names in [&["score"][..], &["eligible"][..]] {
        let e = view(&publ(&file, names).unwrap_err());
        assert_eq!(
            e,
            vec![(
                Code::AppInputValueInvalid,
                Some(2),
                Some(10),
                Some("value".to_owned())
            )]
        );
    }
}

#[test]
fn an_unrequested_unreportable_row_still_needs_a_known_participant() {
    let file = format!("{HEAD}p9,fallback,1\n");
    let e = view(&publ(&file, &[]).unwrap_err());
    assert_eq!(
        e,
        vec![(
            Code::AppInputValueInvalid,
            Some(2),
            Some(1),
            Some("participant".to_owned())
        )]
    );
}

fn duplicate_of(state: &DiagnosticState, first: (u32, u32), second: (u32, u32)) {
    assert_eq!(state.as_slice().len(), 1);
    let d = &state.as_slice()[0];
    assert_eq!(d.code(), Code::AppInputValueInvalid);
    match (d.primary(), d.related()) {
        (Location::Input(p), [Location::Input(r)]) => {
            assert_eq!((p.line, p.column), (Some(second.0), Some(second.1)));
            assert_eq!(p.field.as_deref(), Some("target"));
            assert_eq!((r.line, r.column), (Some(first.0), Some(first.1)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_repeated_pair_of_an_unrequested_reportable_target_is_rt_app_0004() {
    let file = format!("{HEAD}p1,score,1\np2,score,2\np1,score,3\n");
    let e = publ(&file, &["eligible"]).unwrap_err();
    duplicate_of(&e, (2, 4), (4, 4));
}

#[test]
fn a_repeated_pair_of_an_unrequested_unreportable_target_is_rt_app_0004() {
    let file = format!("{HEAD}p1,fallback,1\np1,fallback,2\n");
    let e = publ(&file, &[]).unwrap_err();
    duplicate_of(&e, (2, 4), (3, 4));
}

#[test]
fn a_repeated_pair_is_found_across_used_and_unused_rows_in_one_file() {
    let file = format!("{HEAD}p1,eligible,true\np1,fallback,1\np1,score,1\np1,eligible,false\n");
    let e = publ(&file, &["eligible"]).unwrap_err();
    duplicate_of(&e, (2, 4), (5, 4));
}

#[test]
fn the_same_participant_with_different_used_and_unused_targets_is_not_a_repeat() {
    let file = format!("{HEAD}p1,eligible,true\np1,fallback,1\np1,score,1\n");
    let m = publ(&file, &["eligible"]).unwrap();
    assert_eq!(m.unused.len(), 2);
    assert_eq!(m.result.entries().len(), 1);
}

#[test]
fn unused_rows_do_not_change_the_canonical_published_output() {
    let used = format!("{HEAD}p1,eligible,true\np2,eligible,false\n");
    let mixed = format!(
        "{HEAD}p1,fallback,junk\np1,eligible,true\np1,score,7.5\np2,standing,2\np2,eligible,false\n"
    );
    let a = publ(&used, &["eligible"]).unwrap();
    let b = publ(&mixed, &["eligible"]).unwrap();
    assert!(a.unused.is_empty());
    assert_eq!(b.unused.len(), 3);
    assert_eq!(published_bytes(&a.result), published_bytes(&b.result));
    let want = "{\n  \"format\": \"ruletrace-published\",\n  \"version\": 1,\n  \"entries\": [\n    {\n      \"participant\": \"p1\",\n      \"target\": \"eligible\",\n      \"value\": true\n    },\n    {\n      \"participant\": \"p2\",\n      \"target\": \"eligible\",\n      \"value\": false\n    }\n  ]\n}\n";
    assert_eq!(String::from_utf8(published_bytes(&b.result)).unwrap(), want);
}

// Missing_Value handling in the canonical bytes (decision D).

const EMPTY_P1: &str = "{\n  \"format\": \"ruletrace-data\",\n  \"version\": 1,\n  \"participants\": [\n    {\n      \"id\": \"p1\",\n      \"fields\": {},\n      \"collections\": {}\n    }\n  ]\n}\n";

#[test]
fn missing_cells_never_appear_in_the_canonical_bytes_as_a_default() {
    for file in [
        format!("{HEADER}\np1,,,,,\n"),
        format!("{HEADER}\np1,,,,,\"\"\n"),
        format!("{HEADER}\r\np1,,,,,\r\n"),
    ] {
        let d = parts(&file).unwrap();
        assert_eq!(String::from_utf8(data_bytes(&d)).unwrap(), EMPTY_P1);
    }
}

#[test]
fn explicit_zero_false_and_space_stay_distinct_from_missing_in_the_bytes() {
    let d = parts(&format!("{HEADER}\np1,0,false,,,\" \"\n")).unwrap();
    let bytes = String::from_utf8(data_bytes(&d)).unwrap();
    assert!(bytes.contains("\"age\": \"0\""), "{bytes}");
    assert!(bytes.contains("\"licence\": false"), "{bytes}");
    assert!(bytes.contains("\"note\": \" \""), "{bytes}");
    assert!(
        !bytes.contains("born") && !bytes.contains("level"),
        "{bytes}"
    );
    assert_ne!(bytes, EMPTY_P1);
}

#[test]
fn an_empty_cell_gives_missing_for_every_declared_type_whatever_its_column() {
    let types: [(&str, &str); 5] = [
        ("age", "7"),
        ("licence", "true"),
        ("born", "2020-02-29"),
        ("level", "Silver"),
        ("note", "x"),
    ];
    for (i, (name, _)) in types.iter().enumerate() {
        let cells: Vec<&str> = types
            .iter()
            .enumerate()
            .map(|(j, (_, v))| if i == j { "" } else { *v })
            .collect();
        let row = cells.join(",");
        let d = parts(&format!("{HEADER}\np1,{row}\n")).unwrap();
        let p = d.participant(&pid("p1")).unwrap();
        assert_eq!(p.field(name), None, "{name}");
        assert_eq!(p.fields().len(), 4, "{name}");
    }
}

#[test]
fn an_empty_collection_row_and_an_absent_collection_stay_distinct_in_the_bytes() {
    let file = "participant,collection,row,place,medal,reason\n\
                p1,results,empty,,,\n";
    let d = map_collections(&csv(file).unwrap(), &schema(), base(&["p1", "p2"])).unwrap();
    let bytes = String::from_utf8(data_bytes(&d)).unwrap();
    let want = "{\n  \"format\": \"ruletrace-data\",\n  \"version\": 1,\n  \"participants\": [\n    {\n      \"id\": \"p1\",\n      \"fields\": {},\n      \"collections\": {\n        \"results\": []\n      }\n    },\n    {\n      \"id\": \"p2\",\n      \"fields\": {},\n      \"collections\": {}\n    }\n  ]\n}\n";
    assert_eq!(bytes, want);
}

#[test]
fn an_element_row_of_empty_cells_is_one_empty_element_not_an_empty_collection() {
    let file = "participant,collection,row,place,medal,reason\n\
                p1,results,element,,,\n";
    let d = map_collections(&csv(file).unwrap(), &schema(), base(&["p1"])).unwrap();
    let bytes = String::from_utf8(data_bytes(&d)).unwrap();
    assert!(
        bytes.contains("\"results\": [\n          {}\n        ]"),
        "{bytes}"
    );
}
