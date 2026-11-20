//! Tests of `map_files`. Expected values are written by hand from the CSV
//! text and the rules of design §8.3: collections and published rows name
//! participants of the participants file, so they are mapped only after it
//! mapped, while the CSV syntax of every file is checked either way.

use super::tests_map::{kinds, requested, schema};
use super::*;
use crate::{DataValue, ParticipantId, PublishedNumber, PublishedValue};
use rt_foundation::{Code, DiagnosticState, InputId, Location};

const PARTICIPANTS: &str = "participant,age,licence,born,level,note\n\
                            p1,17,true,,,\"\"\n\
                            p2,9,false,,,\"\"\n";
const COLLECTIONS: &str = "participant,collection,row,place,medal,reason\n\
                           p1,results,element,1,Gold,\"\"\n";
const PUBLISHED: &str = "participant,target,value\n\
                         p1,score,7.50\n\
                         p2,eligible,false\n";
// A blank line is a syntax fault at line 2, column 1.
const BROKEN: &str = "a,b\n\n1,2\n";

fn id(name: &str) -> InputId {
    InputId::new(name).unwrap()
}

fn pid(s: &str) -> ParticipantId {
    ParticipantId::new(s).unwrap()
}

struct Run {
    mapped: Option<MappedFiles>,
    problems: DiagnosticState,
}

fn run(participants: &str, collections: Option<&str>, published: Option<&str>) -> Run {
    let (p_id, c_id, u_id) = (id("in/p.csv"), id("in/c.csv"), id("in/u.csv"));
    let options = MappingOptions::default();
    let files = FileInputs {
        participants: NamedFile {
            bytes: participants.as_bytes(),
            name: &p_id,
        },
        collections: collections.map(|t| NamedFile {
            bytes: t.as_bytes(),
            name: &c_id,
        }),
        published: published.map(|t| NamedFile {
            bytes: t.as_bytes(),
            name: &u_id,
        }),
        options: &options,
    };
    let mut problems = DiagnosticState::empty();
    let mapped = map_files(
        &schema(),
        &kinds(),
        &requested(&["score"]),
        &files,
        &mut problems,
    );
    Run { mapped, problems }
}

/// `(code, input name, line, column)` of each Diagnostic, in state order.
fn view(state: &DiagnosticState) -> Vec<(Code, String, Option<u32>, Option<u32>)> {
    state
        .as_slice()
        .iter()
        .map(|d| match d.primary() {
            Location::Input(l) => (d.code(), l.input.as_str().to_owned(), l.line, l.column),
            other => panic!("not an input location: {other:?}"),
        })
        .collect()
}

#[test]
fn participants_alone_map_to_data_with_no_published_result() {
    let r = run(PARTICIPANTS, None, None);
    assert!(r.problems.as_slice().is_empty());
    let mapped = r.mapped.unwrap();
    assert!(mapped.published.is_none());
    let p1 = mapped.data.participant(&pid("p1")).unwrap();
    assert_eq!(p1.field("age"), Some(&DataValue::Text("17".to_owned())));
    assert_eq!(p1.field("licence"), Some(&DataValue::Bool(true)));
    assert!(p1.collections().is_empty());
    assert!(mapped.data.participant(&pid("p2")).is_some());
}

#[test]
fn collections_are_added_to_the_participants_data() {
    let mapped = run(PARTICIPANTS, Some(COLLECTIONS), None).mapped.unwrap();
    let p1 = mapped.data.participant(&pid("p1")).unwrap();
    assert_eq!(p1.collection("results").map(<[_]>::len), Some(1));
    assert_eq!(p1.field("age"), Some(&DataValue::Text("17".to_owned())));
    let p2 = mapped.data.participant(&pid("p2")).unwrap();
    assert_eq!(p2.collection("results"), None);
}

#[test]
fn the_published_file_maps_requested_rows_and_lists_the_others_as_unused() {
    let mapped = run(PARTICIPANTS, None, Some(PUBLISHED)).mapped.unwrap();
    let published = mapped.published.unwrap();
    let keys: Vec<_> = published.result.entries().keys().collect();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].0.as_str(), "p1");
    assert_eq!(keys[0].1.as_str(), "score");
    assert_eq!(published.unused.len(), 1);
    assert_eq!(published.unused[0].participant.as_str(), "p2");
    assert_eq!(published.unused[0].target.as_str(), "eligible");
    assert_eq!(
        (published.unused[0].line, published.unused[0].column),
        (3, 1)
    );
}

#[test]
fn all_three_files_map_together() {
    let mapped = run(PARTICIPANTS, Some(COLLECTIONS), Some(PUBLISHED))
        .mapped
        .unwrap();
    let p1 = mapped.data.participant(&pid("p1")).unwrap();
    assert_eq!(p1.collection("results").map(<[_]>::len), Some(1));
    assert_eq!(mapped.published.unwrap().result.entries().len(), 1);
}

#[test]
fn a_participants_fault_still_reports_the_syntax_of_the_other_files() {
    let bad_participants = "participant,age,licence,born,level,note\np1,17,maybe,,,\"\"\n";
    let r = run(bad_participants, Some(BROKEN), Some(BROKEN));
    assert!(r.mapped.is_none());
    assert_eq!(
        view(&r.problems),
        vec![
            (
                Code::AppInputValueInvalid,
                "in/p.csv".to_owned(),
                Some(2),
                Some(7)
            ),
            (
                Code::AppInputMalformed,
                "in/c.csv".to_owned(),
                Some(2),
                Some(1)
            ),
            (
                Code::AppInputMalformed,
                "in/u.csv".to_owned(),
                Some(2),
                Some(1)
            ),
        ]
    );
}

#[test]
fn collections_and_published_rows_are_not_mapped_when_the_participants_file_failed() {
    let bad_participants = "participant,age,licence,born,level,note\np1,17,maybe,,,\"\"\n";
    // Both files are well formed and name a participant that does not exist
    // in the file that failed to map. That fault must not be reported.
    let collections = "participant,collection,row,place,medal,reason\np9,results,empty,,,\"\"\n";
    let published = "participant,target,value\np9,score,1\n";
    let r = run(bad_participants, Some(collections), Some(published));
    assert!(r.mapped.is_none());
    assert_eq!(
        view(&r.problems),
        vec![(
            Code::AppInputValueInvalid,
            "in/p.csv".to_owned(),
            Some(2),
            Some(7)
        )]
    );
}

#[test]
fn a_published_row_may_name_a_participant_of_the_participants_file_only() {
    let published = "participant,target,value\np9,score,1\n";
    let r = run(PARTICIPANTS, None, Some(published));
    assert!(r.mapped.is_none());
    assert_eq!(
        view(&r.problems),
        vec![(
            Code::AppInputValueInvalid,
            "in/u.csv".to_owned(),
            Some(2),
            Some(1)
        )]
    );
}

#[test]
fn a_collections_fault_and_a_published_fault_are_both_collected() {
    let collections = "participant,collection,row,place,medal,reason\np9,results,empty,,,\"\"\n";
    let published = "participant,target,value\np9,score,1\n";
    let r = run(PARTICIPANTS, Some(collections), Some(published));
    assert!(r.mapped.is_none());
    assert_eq!(
        view(&r.problems),
        vec![
            (
                Code::AppInputValueInvalid,
                "in/c.csv".to_owned(),
                Some(2),
                Some(1)
            ),
            (
                Code::AppInputValueInvalid,
                "in/u.csv".to_owned(),
                Some(2),
                Some(1)
            ),
        ]
    );
}

#[test]
fn a_collections_fault_alone_gives_no_mapped_files() {
    let r = run(PARTICIPANTS, Some(BROKEN), None);
    assert!(r.mapped.is_none());
    assert_eq!(
        view(&r.problems),
        vec![(
            Code::AppInputMalformed,
            "in/c.csv".to_owned(),
            Some(2),
            Some(1)
        )]
    );
}

#[test]
fn a_published_fault_alone_gives_no_mapped_files() {
    let r = run(PARTICIPANTS, Some(COLLECTIONS), Some(BROKEN));
    assert!(r.mapped.is_none());
    assert_eq!(
        view(&r.problems),
        vec![(
            Code::AppInputMalformed,
            "in/u.csv".to_owned(),
            Some(2),
            Some(1)
        )]
    );
}

#[test]
fn faults_already_in_problems_are_kept_and_extended() {
    let (p_id, options) = (id("in/p.csv"), MappingOptions::default());
    let files = FileInputs {
        participants: NamedFile {
            bytes: BROKEN.as_bytes(),
            name: &p_id,
        },
        collections: None,
        published: None,
        options: &options,
    };
    let mut problems = DiagnosticState::empty();
    problems.push(rt_foundation::Diagnostic::new(
        Code::AppTargetNotDeclared,
        "earlier".to_owned(),
        Location::None(Code::AppTargetNotDeclared),
    ));
    let mapped = map_files(&schema(), &kinds(), &requested(&[]), &files, &mut problems);
    assert!(mapped.is_none());
    let codes: Vec<Code> = problems.as_slice().iter().map(|d| d.code()).collect();
    assert_eq!(
        codes,
        vec![Code::AppTargetNotDeclared, Code::AppInputMalformed]
    );
}

#[test]
fn a_published_number_keeps_its_decimal_value() {
    let mapped = run(PARTICIPANTS, None, Some(PUBLISHED)).mapped.unwrap();
    let published = mapped.published.unwrap();
    let value = published.result.entries().values().next().unwrap();
    assert_eq!(
        *value,
        PublishedValue::Number(PublishedNumber::parse("7.5").unwrap())
    );
}
