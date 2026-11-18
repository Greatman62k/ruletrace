//! Tests of the Prepare group: Target errors, mapping pass-through, Question
//! construction and canonical serialization. Expected positions and values
//! are derived by hand from the CSV text and from the rules of
//! `tests_fixture`.

use crate::tests_fixture::{
    compiled, full_request, id, run_with, sources, target, CSV_FULL, CSV_MISSING_JUDGES, CSV_TIE,
};
use crate::{prepare, BracketRequest, FileInputs, NamedFile, Rejection, ResultClass};
use rt_data::tabular::MappingOptions;
use rt_data::{
    default_parse_limits, parse_data, parse_published, parse_questions, DataLimits, ParticipantId,
    Question,
};
use rt_foundation::{CliLocation, Code, DiagnosticState, InputLocation, Location};

fn rejected(result: Result<crate::Evaluated, Rejection>) -> DiagnosticState {
    match result {
        Err(Rejection::InvalidInput(state)) => state,
        Err(other) => panic!("not invalid input: {other:?}"),
        Ok(_) => panic!("the run reached the engine"),
    }
}

fn cli(position: u32, option: &str) -> Location {
    Location::Cli(CliLocation::Argument {
        position,
        name: Some(option.to_owned()),
    })
}

fn input(file: &str, line: u32, column: u32, field: &str) -> Location {
    Location::Input(InputLocation {
        input: id(file),
        line: Some(line),
        column: Some(column),
        field: Some(field.to_owned()),
    })
}

fn seen(state: &DiagnosticState) -> Vec<(Code, Location)> {
    state
        .as_slice()
        .iter()
        .map(|d| (d.code(), d.primary().clone()))
        .collect()
}

fn request(ranking: Option<(&str, u32)>, qualification: Option<(&str, u32)>) -> BracketRequest {
    let group = |g: Option<(&str, u32)>| g.map(|(t, p)| (target(t), p)).into_iter().collect();
    BracketRequest {
        ranking: group(ranking),
        qualification: group(qualification),
    }
}

#[test]
fn each_target_error_has_its_code_and_the_cli_location_of_its_argument() {
    // The rulebook declares `eligible` and `qualified` (eligibility),
    // `score` (weighted score) and `standing` (ranking).
    let cases = [
        (
            request(Some(("nosuch", 3)), None),
            Code::AppTargetNotDeclared,
            cli(3, "--ranking"),
        ),
        (
            request(Some(("score", 5)), None),
            Code::AppTargetKindMismatch,
            cli(5, "--ranking"),
        ),
        (
            request(Some(("qualified", 6)), None),
            Code::AppTargetKindMismatch,
            cli(6, "--ranking"),
        ),
        (
            request(None, Some(("nosuch", 7))),
            Code::AppTargetNotDeclared,
            cli(7, "--qualification"),
        ),
        (
            request(None, Some(("score", 9))),
            Code::AppTargetKindMismatch,
            cli(9, "--qualification"),
        ),
        // A ranking Target is not a Qualification_Predicate. No rank cutoff
        // exists in this version, so a rank-based request is this error.
        (
            request(None, Some(("standing", 11))),
            Code::AppTargetKindMismatch,
            cli(11, "--qualification"),
        ),
    ];
    for (req, code, at) in cases {
        let state = rejected(run_with(CSV_FULL, None, &req, &sources()));
        assert_eq!(seen(&state), vec![(code, at)], "{req:?}");
    }
}

#[test]
fn a_bad_target_beside_good_ones_gives_no_evaluation_at_all() {
    // `standing` and `qualified` are valid. If the run evaluated them anyway,
    // `run_with` would return an `Evaluated` and `rejected` would panic.
    let req = BracketRequest {
        ranking: [(target("standing"), 2)].into(),
        qualification: [(target("qualified"), 4), (target("nosuch"), 5)].into(),
    };
    let state = rejected(run_with(CSV_FULL, None, &req, &sources()));
    assert_eq!(
        seen(&state),
        vec![(Code::AppTargetNotDeclared, cli(5, "--qualification"))]
    );
}

#[test]
fn target_errors_and_mapping_errors_are_returned_together_in_canonical_order() {
    // Line 2 has `abc` in the `age` column: line 2, column 4 (`p1,` is three
    // characters). Input_Locations sort before Cli_Locations (Req 7.10).
    let csv = "participant,age,licence,judges,technical\np1,abc,true,8,7\n";
    let req = BracketRequest {
        ranking: [(target("score"), 2)].into(),
        qualification: [(target("nosuch"), 3)].into(),
    };
    let state = rejected(run_with(csv, None, &req, &sources()));
    assert_eq!(
        seen(&state),
        vec![
            (
                Code::AppInputValueInvalid,
                input("participants.csv", 2, 4, "age")
            ),
            (Code::AppTargetKindMismatch, cli(2, "--ranking")),
            (Code::AppTargetNotDeclared, cli(3, "--qualification")),
        ]
    );
}

#[test]
fn mapping_errors_pass_through_with_the_code_and_location_of_the_cell() {
    let header = "participant,age,licence,judges,technical\n";
    let cases = [
        // `x` is not a number: line 2, column 4, field `age`.
        (
            format!("{header}p1,x,true,8,7\n"),
            Code::AppInputValueInvalid,
            input("participants.csv", 2, 4, "age"),
        ),
        // `technical` is absent: reported at the first header cell.
        (
            "participant,age,licence,judges\np1,17,true,8\n".to_owned(),
            Code::AppInputFieldMissing,
            input("participants.csv", 1, 1, "technical"),
        ),
        // The second `p1` is on line 3, column 1.
        (
            format!("{header}p1,17,true,8,7\np1,18,true,8,7\n"),
            Code::AppInputDuplicateParticipant,
            input("participants.csv", 3, 1, "participant"),
        ),
    ];
    for (csv, code, at) in cases {
        let state = rejected(run_with(&csv, None, &full_request(), &sources()));
        assert_eq!(seen(&state), vec![(code, at)], "{csv}");
    }
}

#[test]
fn a_published_row_for_an_unknown_participant_is_invalid_input_before_any_engine_call() {
    // Line 2, column 1 holds `p9`, which the participants file lacks.
    let published = "participant,target,value\np9,standing,1\n";
    let state = rejected(run_with(
        CSV_FULL,
        Some(published),
        &full_request(),
        &sources(),
    ));
    assert_eq!(
        seen(&state),
        vec![(
            Code::AppInputValueInvalid,
            input("published.csv", 2, 1, "participant")
        )]
    );
}

#[test]
fn a_rejection_is_classified_as_invalid_input_and_lists_its_diagnostics() {
    let compiled = compiled();
    let (pid, options) = (id("participants.csv"), MappingOptions::default());
    let files = FileInputs {
        participants: NamedFile {
            bytes: b"participant,age\n",
            name: &pid,
        },
        collections: None,
        published: None,
        options: &options,
    };
    let rejection = prepare(&compiled, &full_request(), &files).unwrap_err();
    assert_eq!(rejection.class(), ResultClass::InvalidInput);
    // Three declared inputs are absent from the header.
    let diagnostics = rejection.diagnostics();
    let missing: Vec<&str> = diagnostics
        .as_slice()
        .iter()
        .filter_map(|d| match d.primary() {
            Location::Input(i) => i.field.as_deref(),
            _ => None,
        })
        .collect();
    assert_eq!(missing, vec!["judges", "licence", "technical"]);
}

fn participant(name: &str) -> ParticipantId {
    ParticipantId::new(name).unwrap()
}

fn value(target_name: &str, who: &str) -> Question {
    Question::Value {
        target: target(target_name),
        participant: participant(who),
    }
}

#[test]
fn questions_are_one_ranking_over_everyone_then_one_per_qualification_and_participant() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let expected = vec![
        Question::Rank {
            target: target("standing"),
            participants: None,
        },
        value("qualified", "p1"),
        value("qualified", "p2"),
        value("qualified", "p3"),
        value("qualified", "p4"),
    ];
    assert_eq!(run.prepared().questions(), expected.as_slice());
}

#[test]
fn ranking_questions_name_no_participant_set_whatever_the_outcomes_are() {
    // With p2's judges missing, the ranking and p2's qualification are
    // Indeterminate and p1 and p3 do not qualify. The ranking Question is the
    // same as in the complete run: no Outcome decides who is ranked.
    let ranks = |run: &crate::Evaluated| -> Vec<Question> {
        run.prepared()
            .questions()
            .iter()
            .filter(|q| matches!(q, Question::Rank { .. }))
            .cloned()
            .collect()
    };
    let expected = vec![Question::Rank {
        target: target("standing"),
        participants: None,
    }];
    for csv in [CSV_FULL, CSV_TIE, CSV_MISSING_JUDGES] {
        let run = run_with(csv, None, &full_request(), &sources()).unwrap();
        assert_eq!(ranks(&run), expected, "{csv}");
    }
    let complete = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let missing = run_with(CSV_MISSING_JUDGES, None, &full_request(), &sources()).unwrap();
    assert_eq!(
        complete.prepared().questions(),
        missing.prepared().questions()
    );
}

#[test]
fn canonical_bytes_parse_back_to_the_prepared_values() {
    let published = "participant,target,value\np1,qualified,true\np2,standing,2\n";
    let run = run_with(CSV_FULL, Some(published), &full_request(), &sources()).unwrap();
    let prepared = run.prepared();
    let data = parse_data(
        &id("data.json"),
        &prepared.data_bytes(),
        DataLimits::default(),
    )
    .unwrap();
    assert_eq!(&data, prepared.data());
    let questions = parse_questions(
        &id("questions.json"),
        &prepared.questions_bytes(),
        default_parse_limits(),
    )
    .unwrap();
    assert_eq!(questions.as_slice(), prepared.questions());
    let bytes = prepared.published_bytes().unwrap();
    let parsed = parse_published(&id("published.json"), &bytes, default_parse_limits()).unwrap();
    assert_eq!(Some(&parsed), prepared.published());
    assert_eq!(parsed.entries().len(), 2);
    let none = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    assert!(none.prepared().published_bytes().is_none());
}

#[test]
fn input_hashes_follow_the_roles_in_a_fixed_order() {
    let compiled = compiled();
    let (p, c, q) = (id("p.csv"), id("c.csv"), id("q.csv"));
    let options = MappingOptions::default();
    let files = FileInputs {
        participants: NamedFile {
            bytes: CSV_FULL.as_bytes(),
            name: &p,
        },
        collections: Some(NamedFile {
            bytes: b"participant,collection,row\n",
            name: &c,
        }),
        published: Some(NamedFile {
            bytes: b"participant,target,value\n",
            name: &q,
        }),
        options: &options,
    };
    let prepared = prepare(&compiled, &full_request(), &files).unwrap();
    let roles: Vec<&str> = prepared.input_hashes().iter().map(|(r, _)| *r).collect();
    assert_eq!(roles, vec!["participants", "collections", "published"]);
    // `printf 'participant,collection,row\n' | sha256sum`
    let collections = prepared.input_hashes().get(1).unwrap().1.as_str();
    assert_eq!(
        collections,
        "f2d15f71f542b4aefe3d0d6f54ae656c4f770faa17609ae8384552f49381d8a8"
    );
}
