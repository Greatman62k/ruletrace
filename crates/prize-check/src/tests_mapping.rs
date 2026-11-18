//! Tests of the Prepare group: Target errors, mapping pass-through, Question
//! construction and canonical serialization. Expected positions and values
//! are derived by hand from the CSV text and from the rules of `slice-basic`.

use crate::tests_fixture::{
    compiled, compiled_with_derived_score, full_request, id, run_on, run_with, sources, target,
    CSV_FULL, CSV_MISSING_AGE,
};
use crate::{prepare, FileInputs, NamedFile, PrizeRequest, Rejection, ResultClass};
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

fn request(
    eligibility: Option<(&str, u32)>,
    score: Option<(&str, u32)>,
    ranking: Option<(&str, u32)>,
) -> PrizeRequest {
    let group = |g: Option<(&str, u32)>| g.map(|(t, p)| (target(t), p)).into_iter().collect();
    PrizeRequest {
        eligibility: group(eligibility),
        score: group(score),
        ranking: group(ranking),
    }
}

#[test]
fn each_target_error_has_its_code_and_the_cli_location_of_its_argument() {
    // `slice-basic` declares `eligible` (eligibility), `score` (weighted
    // score) and `standing` (ranking).
    let cases = [
        (
            request(Some(("nosuch", 3)), None, None),
            Code::AppTargetNotDeclared,
            cli(3, "--eligibility"),
        ),
        (
            request(Some(("score", 5)), None, None),
            Code::AppTargetKindMismatch,
            cli(5, "--eligibility"),
        ),
        (
            request(None, Some(("eligible", 7)), None),
            Code::AppTargetKindMismatch,
            cli(7, "--score"),
        ),
        (
            request(None, Some(("standing", 9)), None),
            Code::AppTargetKindMismatch,
            cli(9, "--score"),
        ),
        (
            request(None, None, Some(("score", 11))),
            Code::AppTargetKindMismatch,
            cli(11, "--ranking"),
        ),
        (
            request(None, None, Some(("nosuch", 12))),
            Code::AppTargetNotDeclared,
            cli(12, "--ranking"),
        ),
    ];
    for (req, code, at) in cases {
        let state = rejected(run_with(CSV_FULL, None, &req, &sources()));
        assert_eq!(seen(&state), vec![(code, at)], "{req:?}");
    }
}

#[test]
fn a_bad_target_beside_good_ones_gives_no_evaluation_at_all() {
    // `eligible` and `standing` are valid. If the run evaluated them anyway,
    // `run_with` would return an `Evaluated` and `rejected` would panic.
    let req = PrizeRequest {
        eligibility: [(target("eligible"), 2), (target("nosuch"), 3)].into(),
        score: Default::default(),
        ranking: [(target("standing"), 6)].into(),
    };
    let state = rejected(run_with(CSV_FULL, None, &req, &sources()));
    assert_eq!(
        seen(&state),
        vec![(Code::AppTargetNotDeclared, cli(3, "--eligibility"))]
    );
}

#[test]
fn target_errors_and_mapping_errors_are_returned_together_in_canonical_order() {
    // Line 2 has `abc` in the `age` column: line 2, column 4 (`p1,` is three
    // characters). Input_Locations sort before Cli_Locations (Req 7.10).
    let csv = "participant,age,licence,judges,technical\np1,abc,true,8,7\n";
    let req = PrizeRequest {
        eligibility: [(target("eligible"), 2), (target("nosuch"), 3)].into(),
        score: [(target("eligible"), 4)].into(),
        ranking: Default::default(),
    };
    let state = rejected(run_with(csv, None, &req, &sources()));
    assert_eq!(
        seen(&state),
        vec![
            (
                Code::AppInputValueInvalid,
                input("participants.csv", 2, 4, "age")
            ),
            (Code::AppTargetNotDeclared, cli(3, "--eligibility")),
            (Code::AppTargetKindMismatch, cli(4, "--score")),
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
        // `extra` starts at column 42: the header up to its comma is 41 characters.
        (
            "participant,age,licence,judges,technical,extra\np1,17,true,8,7,x\n".to_owned(),
            Code::AppInputMalformed,
            input("participants.csv", 1, 42, "extra"),
        ),
    ];
    for (csv, code, at) in cases {
        let state = rejected(run_with(&csv, None, &full_request(), &sources()));
        assert_eq!(seen(&state), vec![(code, at)], "{csv}");
    }
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
fn questions_are_one_per_target_and_participant_then_one_ranking_over_everyone() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let expected = vec![
        value("eligible", "p1"),
        value("eligible", "p2"),
        value("eligible", "p3"),
        value("eligible", "p4"),
        value("score", "p1"),
        value("score", "p2"),
        value("score", "p3"),
        value("score", "p4"),
        Question::Rank {
            target: target("standing"),
            participants: None,
        },
    ];
    assert_eq!(run.prepared().questions(), expected.as_slice());
}

#[test]
fn ranking_questions_name_no_participant_set_whatever_the_outcomes_are() {
    // With p2's age missing, p2's eligibility is Indeterminate and p3 and p4
    // are not eligible. The ranking Question is the same as in the complete
    // run: no Outcome decides who is ranked.
    let complete = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let missing = run_with(CSV_MISSING_AGE, None, &full_request(), &sources()).unwrap();
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
    assert_eq!(ranks(&complete), expected);
    assert_eq!(ranks(&missing), expected);
    assert_eq!(
        complete.prepared().questions(),
        missing.prepared().questions()
    );
}

#[test]
fn a_derived_target_is_accepted_by_the_score_option_and_reported_exactly() {
    // `score` is redefined as `judges`: p1 8, p2 7, p3 6, p4 9.
    let derived = compiled_with_derived_score();
    let req = request(None, Some(("score", 4)), None);
    let run = run_on(&derived, CSV_FULL, None, &req, &sources()).unwrap();
    let got: Vec<String> = run
        .prepared()
        .questions()
        .iter()
        .map(|q| format!("{q:?}"))
        .collect();
    assert_eq!(got.len(), 4);
    let doc = run.report().unwrap();
    let values: Vec<(String, String)> = doc
        .sections()
        .iter()
        .filter_map(|s| match s {
            rt_explain::report::ReportSection::Scores(e) => Some(e),
            _ => None,
        })
        .flatten()
        .map(|e| match e.result() {
            rt_explain::report::Score::Exact(n) => {
                (e.participant().to_owned(), rt_numeric::to_canonical(n))
            }
            other => panic!("{other:?}"),
        })
        .collect();
    let expected: Vec<(String, String)> = [("p1", "8"), ("p2", "7"), ("p3", "6"), ("p4", "9")]
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect();
    assert_eq!(values, expected);
    // The derived Target is not an eligibility predicate.
    let bad = request(Some(("score", 2)), None, None);
    let state = rejected(run_on(&derived, CSV_FULL, None, &bad, &sources()));
    assert_eq!(
        seen(&state),
        vec![(Code::AppTargetKindMismatch, cli(2, "--eligibility"))]
    );
}

#[test]
fn canonical_bytes_parse_back_to_the_prepared_values() {
    let published = "participant,target,value\np1,eligible,true\np2,score,7.8\n";
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
