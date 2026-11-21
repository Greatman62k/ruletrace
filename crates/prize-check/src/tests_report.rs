//! Tests of the Present and Evaluate groups against values derived by hand
//! from the rules of `slice-basic` (see `tests_fixture`).
//!
//! Hand derivation, scores `0.6 * judges + 0.4 * technical`:
//! p1 `4.8 + 2.8 = 7.6`, p2 `4.2 + 3.6 = 7.8`, p3 `3.6 + 2.4 = 6`,
//! p4 `5.4 + 2.4 = 7.8`. Eligibility `age >= 16 and licence`: p1 and p2 true,
//! p3 false (age 15), p4 false (no licence). Ranking by score descending,
//! then judges descending: p2 and p4 tie on 7.8 and p4 has the higher judges
//! score (9 against 7), so the positions are p4, p2, p1, p3.

use crate::tests_fixture::{
    compiled, full_request, id, run_with, sources, target, CSV_FULL, CSV_MISSING_AGE,
};
use crate::{
    evaluate, present, FileInputs, NamedFile, PresentError, PresentInputs, PrizeInputs,
    PrizeRequest, ResultClass,
};
use rt_data::tabular::MappingOptions;
use rt_engine::{Outcome, TraceValue};
use rt_explain::report::{
    EligibilityEntry, Missing, ProvenanceSummary, PublishedSection, ReportApplication, ReportDoc,
    ReportSection, Score, ScoreEntry, StandingsResult, StandingsTable, Truth,
};
use rt_numeric::parse_literal;
use rt_srcmodel::SourceModels;

const CSV_FULL_SHA256: &str = "faf87de810a2f0727fa557fbe0bddd15d4c22309b7740fd1f6e5a382511f7bcf";

fn eligibility(doc: &ReportDoc) -> &[EligibilityEntry] {
    doc.sections()
        .iter()
        .find_map(|s| match s {
            ReportSection::Eligibility(e) => Some(e.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
}

fn scores(doc: &ReportDoc) -> &[ScoreEntry] {
    doc.sections()
        .iter()
        .find_map(|s| match s {
            ReportSection::Scores(e) => Some(e.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
}

fn standings(doc: &ReportDoc) -> &[StandingsTable] {
    doc.sections()
        .iter()
        .find_map(|s| match s {
            ReportSection::Standings(e) => Some(e.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
}

fn published(doc: &ReportDoc) -> &PublishedSection {
    doc.sections()
        .iter()
        .find_map(|s| match s {
            ReportSection::Published(p) => Some(p),
            _ => None,
        })
        .unwrap()
}

#[test]
fn csv_is_mapped_evaluated_and_reported_with_hand_derived_values_in_outcome_order() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let doc = run.report().unwrap();
    let outcomes = &run.evaluation().outcomes;
    assert_eq!(outcomes.len(), 9);

    let expected_elig = [("p1", true), ("p2", true), ("p3", false), ("p4", false)];
    let elig = eligibility(doc);
    assert_eq!(elig.len(), 4);
    for (i, (entry, (who, value))) in elig.iter().zip(expected_elig).enumerate() {
        assert_eq!(
            (
                entry.participant(),
                entry.target(),
                entry.evidence().trace()
            ),
            (who, "eligible", i)
        );
        let truth = if value { Truth::True } else { Truth::False };
        assert_eq!(entry.result(), &truth);
        assert_eq!(
            outcomes.get(i),
            Some(&Outcome::Decided(TraceValue::Bool(value)))
        );
    }

    let expected_scores = [("p1", "7.6"), ("p2", "7.8"), ("p3", "6"), ("p4", "7.8")];
    let found = scores(doc);
    assert_eq!(found.len(), 4);
    for (i, (entry, (who, value))) in found.iter().zip(expected_scores).enumerate() {
        let number = parse_literal(value).unwrap();
        assert_eq!(
            (
                entry.participant(),
                entry.target(),
                entry.evidence().trace()
            ),
            (who, "score", i + 4)
        );
        assert_eq!(entry.result(), &Score::Exact(number.clone()));
        assert_eq!(
            outcomes.get(i + 4),
            Some(&Outcome::Decided(TraceValue::Number(number)))
        );
    }

    let tables = standings(doc);
    assert_eq!(tables.len(), 1);
    let table = tables.first().unwrap();
    assert_eq!((table.target(), table.evidence().trace()), ("standing", 8));
    let StandingsResult::Places(places) = table.result() else {
        panic!("the ranking is decided");
    };
    let got: Vec<(u32, Vec<&str>)> = places
        .iter()
        .map(|p| (p.position, p.members.iter().map(String::as_str).collect()))
        .collect();
    assert_eq!(
        got,
        vec![
            (1, vec!["p4"]),
            (2, vec!["p2"]),
            (3, vec!["p1"]),
            (4, vec!["p3"])
        ]
    );
    assert_eq!(run.class(), ResultClass::Complete);
}

#[test]
fn header_carries_the_rulebook_hash_the_input_hash_and_verified_provenance() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let doc = run.report().unwrap();
    let header = doc.header();
    assert_eq!(header.application, ReportApplication::PrizeCheck);
    assert_eq!(
        header.rulebook_hash.as_str(),
        "315323aaa60b86e8212d8f82e76f8197467b5923ebe162c19d1399521df0b7d5"
    );
    let inputs: Vec<(&str, &str)> = header
        .input_hashes
        .iter()
        .map(|(role, hash)| (*role, hash.as_str()))
        .collect();
    assert_eq!(inputs, vec![("participants", CSV_FULL_SHA256)]);
    assert!(matches!(
        doc.provenance(),
        ProvenanceSummary::Verified { traces: 9, .. }
    ));
}

#[test]
fn an_empty_cell_stays_missing_and_the_report_names_the_missing_input() {
    let run = run_with(CSV_MISSING_AGE, None, &full_request(), &sources()).unwrap();
    let p2 = rt_data::ParticipantId::new("p2").unwrap();
    let participant = run.prepared().data().participant(&p2).unwrap();
    assert!(participant.field("age").is_none());
    assert!(participant.field("judges").is_some());

    let doc = run.report().unwrap();
    let elig = eligibility(doc);
    let p2_entry = elig.get(1).unwrap();
    assert_eq!(p2_entry.participant(), "p2");
    assert_eq!(
        p2_entry.result(),
        &Truth::Indeterminate(vec![Missing::Input {
            participant: "p2".to_owned(),
            field: "age".to_owned()
        }])
    );
    // The other three participants keep their hand-derived results.
    let others: Vec<(&str, &Truth)> = elig
        .iter()
        .filter(|e| e.participant() != "p2")
        .map(|e| (e.participant(), e.result()))
        .collect();
    assert_eq!(
        others,
        vec![
            ("p1", &Truth::True),
            ("p3", &Truth::False),
            ("p4", &Truth::False)
        ]
    );
    // The score and the ranking do not read `age`, so they stay decided.
    assert!(scores(doc)
        .iter()
        .all(|s| matches!(s.result(), Score::Exact(_))));
    assert_eq!(run.class(), ResultClass::Indeterminate);
}

#[test]
fn a_run_without_source_models_says_not_verified_and_stays_complete() {
    let run = run_with(CSV_FULL, None, &full_request(), &SourceModels::new()).unwrap();
    let doc = run.report().unwrap();
    assert!(matches!(
        doc.provenance(),
        ProvenanceSummary::NotVerifiedNoSources {
            without_sources: 9,
            verified: 0,
            total: 9,
            ..
        }
    ));
    assert!(eligibility(doc).iter().all(|e| !e.evidence().verified()));
    // PROV_NOT_VERIFIED_NO_SOURCES is a Warning, so the class is unchanged.
    assert_eq!(run.class(), ResultClass::Complete);
}

#[test]
fn published_differences_are_the_engines_and_unrequested_rows_are_listed() {
    let csv = "participant,target,value\np1,eligible,true\np2,score,7.9\np2,standing,1\n";
    let run = run_with(CSV_FULL, Some(csv), &full_request(), &sources()).unwrap();
    let doc = run.report().unwrap();
    let PublishedSection::Compared {
        differences,
        unused,
    } = published(doc)
    else {
        panic!("a published file was supplied");
    };
    assert!(unused.is_empty());
    let got: Vec<(&str, &str, &str, Option<&str>, &str)> = differences
        .iter()
        .map(|d| {
            (
                d.participant(),
                d.target(),
                d.published(),
                d.computed(),
                d.kind().name(),
            )
        })
        .collect();
    // p2 scored 7.8 (published 7.9) and stands at position 2 (published 1);
    // p1's eligibility equals the published `true`.
    assert_eq!(
        got,
        vec![
            ("p2", "score", "7.9", Some("7.8"), "value_differs"),
            ("p2", "standing", "1", Some("2"), "value_differs"),
        ]
    );
    // Each difference points at the trace of the Question that computed it.
    let traces: Vec<usize> = differences.iter().map(|d| d.trace()).collect();
    assert_eq!(traces, vec![5, 8]);
}

#[test]
fn no_published_file_is_reported_as_not_supplied() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    assert_eq!(
        published(run.report().unwrap()),
        &PublishedSection::NotSupplied
    );
}

#[test]
fn a_published_row_for_a_declared_target_that_was_not_requested_is_listed_not_dropped() {
    let only_elig = PrizeRequest {
        eligibility: [(target("eligible"), 2)].into(),
        ..PrizeRequest::default()
    };
    let csv = "participant,target,value\np1,score,7.6\np1,eligible,true\n";
    let run = run_with(CSV_FULL, Some(csv), &only_elig, &sources()).unwrap();
    assert_eq!(run.prepared().not_requested().len(), 1);
    let PublishedSection::Compared {
        differences,
        unused,
    } = published(run.report().unwrap())
    else {
        panic!("a published file was supplied");
    };
    assert!(differences.is_empty());
    let rows: Vec<(&str, &str)> = unused
        .iter()
        .map(|u| (u.participant.as_str(), u.target.as_str()))
        .collect();
    assert_eq!(rows, vec![("p1", "score")]);
}

#[test]
fn two_runs_render_the_same_bytes() {
    let render = || {
        let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
        let doc = run.report().unwrap();
        (
            rt_explain::report::render_human(doc),
            rt_explain::report::render_json(doc),
        )
    };
    assert_eq!(render(), render());
}

fn inputs_of<'a>(
    compiled: &'a rt_compiled::CompiledRulebook,
    run: &'a crate::Evaluated,
    outcomes: &'a [Outcome],
) -> PresentInputs<'a> {
    let evaluation = run.evaluation();
    PresentInputs {
        compiled,
        outcomes,
        traces: &evaluation.traces,
        verifications: run.verifications(),
        diagnostics: &evaluation.diagnostics,
        comparison: None,
        not_requested: &[],
        input_hashes: &[],
        statements: &[],
        synthetic_data: None,
    }
}

#[test]
fn present_refuses_lists_of_different_length_and_builds_no_partial_report() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let evaluation = run.evaluation();
    let compiled = compiled();
    let inputs = |outcomes| inputs_of(&compiled, &run, outcomes);
    let short = evaluation.outcomes.get(..8).unwrap();
    assert_eq!(
        present(&inputs(short)).unwrap_err(),
        PresentError::CountMismatch {
            outcomes: 8,
            traces: 9,
            verifications: 9
        }
    );
    assert!(present(&inputs(&evaluation.outcomes)).is_ok());
}

#[test]
fn present_refuses_an_outcome_that_differs_from_its_trace() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let evaluation = run.evaluation();
    let mut altered = evaluation.outcomes.clone();
    // Flip p1's eligibility from true to false.
    *altered.first_mut().unwrap() = Outcome::Decided(TraceValue::Bool(false));
    let compiled = compiled();
    let result = present(&PresentInputs {
        compiled: &compiled,
        outcomes: &altered,
        traces: &evaluation.traces,
        verifications: run.verifications(),
        diagnostics: &evaluation.diagnostics,
        comparison: None,
        not_requested: &[],
        input_hashes: &[],
        statements: &[],
        synthetic_data: None,
    });
    assert_eq!(
        result.unwrap_err(),
        PresentError::OutcomeNotInTrace { trace: 0 }
    );
}

#[test]
fn present_refuses_a_trace_for_a_target_the_rulebook_does_not_declare() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let evaluation = run.evaluation();
    // The `slice-missing` rulebook has no ranking Target `standing`.
    let other = rt_compiled::parse_compiled(
        include_bytes!("../../../tests/fixtures/slice-missing/expected/compiled.json"),
        &id("other.json"),
        64 * 1024 * 1024,
        rt_foundation::MAX_CONTAINER_DEPTH,
    )
    .unwrap();
    let result = present(&PresentInputs {
        compiled: &other,
        outcomes: &evaluation.outcomes,
        traces: &evaluation.traces,
        verifications: run.verifications(),
        diagnostics: &evaluation.diagnostics,
        comparison: None,
        not_requested: &[],
        input_hashes: &[],
        statements: &[],
        synthetic_data: None,
    });
    assert_eq!(
        result.unwrap_err(),
        PresentError::TargetNotReportable { trace: 8 }
    );
}

#[test]
fn a_request_without_targets_is_not_an_error_and_evaluates_nothing() {
    let compiled = compiled();
    let (pid, options) = (id("participants.csv"), MappingOptions::default());
    let sources = sources();
    let run = evaluate(&PrizeInputs {
        compiled: &compiled,
        request: &PrizeRequest::default(),
        files: FileInputs {
            participants: NamedFile {
                bytes: CSV_FULL.as_bytes(),
                name: &pid,
            },
            collections: None,
            published: None,
            options: &options,
        },
        sources: &sources,
        statements: &[],
        synthetic_data: None,
    })
    .unwrap();
    assert!(run.evaluation().outcomes.is_empty());
    assert!(run.prepared().questions().is_empty());
}
