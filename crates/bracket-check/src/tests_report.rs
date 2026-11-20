//! Tests of the Present and Evaluate groups against values derived by hand
//! from the rules of `tests_fixture`.
//!
//! Hand derivation, scores `0.6 * judges + 0.4 * technical`:
//! p1 `4.8 + 2.8 = 7.6`, p2 `4.2 + 3.6 = 7.8`, p3 `3.6 + 2.4 = 6`,
//! p4 `5.4 + 2.4 = 7.8`. Qualification `score >= 7.7`: p1 false, p2 true,
//! p3 false, p4 true. Ranking by score descending, then judges descending:
//! p2 and p4 tie on 7.8 and p4 has the higher judges score (9 against 7), so
//! the positions are p4, p2, p1, p3. With [`CSV_TIE`] p4 has judges 7 and
//! technical 9 like p2, so p2 and p4 are one Tie_Group at position 1, and
//! the next positions are 3 (two participants are ahead) and 4.

use crate::tests_fixture::{
    compiled, compiled_basic, full_request, id, run_with, sources, target, CSV_FULL,
    CSV_MISSING_JUDGES, CSV_TIE,
};
use crate::{
    evaluate, prepare, present, BracketInputs, BracketRequest, Evaluated, FileInputs, NamedFile,
    PresentError, PresentInputs, ResultClass,
};
use rt_compiled::CompiledRulebook;
use rt_data::tabular::MappingOptions;
use rt_data::{ParticipantId, Question};
use rt_engine::{Outcome, TraceValue};
use rt_explain::report::{
    render_human, render_json, Missing, ProvenanceSummary, QualificationEntry, ReportApplication,
    ReportDoc, ReportSection, StandingsResult, StandingsTable, Truth,
};
use rt_explain::verify_trace;
use rt_foundation::Code;
use rt_numeric::parse_literal;
use rt_srcmodel::SourceModels;

const BRACKET_SCOPE: &str = "Standings are calculated solely from the declared ranking rules; this report does not apply eligibility filtering.";

/// Bytes 92 to 170 of `rules.md`.
const SCORE_CLAUSE: &str =
    "2. The score is 0.6 times the judges score plus 0.4 times the technical score.";

fn standings(doc: &ReportDoc) -> &[StandingsTable] {
    doc.sections()
        .iter()
        .find_map(|s| match s {
            ReportSection::Standings(e) => Some(e.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
}

fn qualification(doc: &ReportDoc) -> &[QualificationEntry] {
    doc.sections()
        .iter()
        .find_map(|s| match s {
            ReportSection::Qualification(e) => Some(e.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
}

fn places(table: &StandingsTable) -> Vec<(u32, Vec<&str>)> {
    let StandingsResult::Places(places) = table.result() else {
        panic!("the ranking is decided");
    };
    places
        .iter()
        .map(|p| (p.position, p.members.iter().map(String::as_str).collect()))
        .collect()
}

fn number(text: &str) -> TraceValue {
    TraceValue::Number(parse_literal(text).unwrap())
}

#[test]
fn a_tie_group_is_one_entry_with_its_members_and_the_next_position_skips_it() {
    let run = run_with(CSV_TIE, None, &full_request(), &sources()).unwrap();
    let doc = run.report().unwrap();
    let tables = standings(doc);
    assert_eq!(tables.len(), 1);
    let table = tables.first().unwrap();
    assert_eq!((table.target(), table.evidence().trace()), ("standing", 0));
    assert_eq!(
        places(table),
        vec![(1, vec!["p2", "p4"]), (3, vec!["p1"]), (4, vec!["p3"])]
    );
    assert_eq!(run.class(), ResultClass::Complete);
}

#[test]
fn a_broken_tie_lists_every_participant_at_its_own_position() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let table = standings(run.report().unwrap()).first().unwrap();
    assert_eq!(
        places(table),
        vec![
            (1, vec!["p4"]),
            (2, vec!["p2"]),
            (3, vec!["p1"]),
            (4, vec!["p3"])
        ]
    );
}

/// Rule, revision, start, end, line, column and resolved text of one span.
type SpanRow<'a> = (&'a str, Option<u32>, u32, u32, u32, u32, Option<&'a str>);

type Criterion = (u32, &'static str, TraceValue, TraceValue, &'static str);
type Pair = (
    (String, String),
    bool,
    Vec<(u32, String, TraceValue, TraceValue, String)>,
);

fn pairs(table: &StandingsTable) -> Vec<Pair> {
    table
        .comparisons()
        .iter()
        .map(|c| {
            let e = &c.0;
            let criteria = e
                .criteria
                .iter()
                .map(|k| {
                    assert_eq!(k.rule.as_str(), "rank.standing");
                    (
                        k.index,
                        k.direction.name().to_owned(),
                        k.left.clone(),
                        k.right.clone(),
                        k.result.name().to_owned(),
                    )
                })
                .collect();
            (e.between.clone(), e.first_ahead, criteria)
        })
        .collect()
}

fn pair(between: (&str, &str), first_ahead: bool, rows: Vec<Criterion>) -> Pair {
    (
        (between.0.to_owned(), between.1.to_owned()),
        first_ahead,
        rows.into_iter()
            .map(|(i, d, l, r, res)| (i, d.to_owned(), l, r, res.to_owned()))
            .collect(),
    )
}

#[test]
fn the_tie_breaking_criteria_are_applied_in_order_with_the_compared_values() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let table = standings(run.report().unwrap()).first().unwrap();
    let expected = vec![
        pair(
            ("p4", "p2"),
            true,
            vec![
                (0, "desc", number("7.8"), number("7.8"), "equal"),
                (1, "desc", number("9"), number("7"), "first"),
            ],
        ),
        pair(
            ("p2", "p1"),
            true,
            vec![(0, "desc", number("7.8"), number("7.6"), "first")],
        ),
        pair(
            ("p1", "p3"),
            true,
            vec![(0, "desc", number("7.6"), number("6"), "first")],
        ),
    ];
    assert_eq!(pairs(table), expected);
}

#[test]
fn a_tie_group_shows_every_criterion_equal_and_no_member_ahead() {
    let run = run_with(CSV_TIE, None, &full_request(), &sources()).unwrap();
    let table = standings(run.report().unwrap()).first().unwrap();
    let expected = vec![
        pair(
            ("p2", "p4"),
            false,
            vec![
                (0, "desc", number("7.8"), number("7.8"), "equal"),
                (1, "desc", number("7"), number("7"), "equal"),
            ],
        ),
        pair(
            ("p4", "p1"),
            true,
            vec![(0, "desc", number("7.8"), number("7.6"), "first")],
        ),
        pair(
            ("p1", "p3"),
            true,
            vec![(0, "desc", number("7.6"), number("6"), "first")],
        ),
    ];
    assert_eq!(pairs(table), expected);
}

#[test]
fn an_indeterminate_position_names_the_missing_input() {
    let run = run_with(CSV_MISSING_JUDGES, None, &full_request(), &sources()).unwrap();
    let table = standings(run.report().unwrap()).first().unwrap();
    // p2 has no judges value, so its score and with it the ranking key is
    // missing. The engine adds its RT-EXE-0006 reason.
    assert_eq!(
        table.result(),
        &StandingsResult::Indeterminate(vec![
            Missing::Input {
                participant: "p2".to_owned(),
                field: "judges".to_owned()
            },
            Missing::Diagnosed(Code::ExeRankMissingValue),
        ])
    );
    assert!(table.comparisons().is_empty());
    assert_eq!(run.class(), ResultClass::Indeterminate);
}

#[test]
fn each_participant_has_a_qualification_outcome_with_rule_revision_span_and_inputs() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let doc = run.report().unwrap();
    let entries = qualification(doc);
    let expected = [
        ("p1", Truth::False, "8", "7"),
        ("p2", Truth::True, "7", "9"),
        ("p3", Truth::False, "6", "6"),
        ("p4", Truth::True, "9", "6"),
    ];
    assert_eq!(entries.len(), 4);
    for (i, (entry, (who, truth, judges, technical))) in entries.iter().zip(expected).enumerate() {
        assert_eq!((entry.participant(), entry.target()), (who, "qualified"));
        assert_eq!(entry.result(), &truth);
        assert_eq!(entry.evidence().trace(), i + 1);
        assert!(entry.evidence().verified());
        let read: Vec<(&str, Option<&str>, TraceValue)> = entry
            .inputs_read()
            .iter()
            .map(|r| (r.name.as_str(), r.participant.as_deref(), r.value.clone()))
            .collect();
        assert_eq!(
            read,
            vec![
                ("judges", Some(who), number(judges)),
                ("technical", Some(who), number(technical)),
            ]
        );
        let rules: Vec<(&str, u32)> = entry
            .evidence()
            .rules()
            .iter()
            .map(|(rule, revision)| (rule.as_str(), revision.get()))
            .collect();
        assert_eq!(rules, vec![("elig.qual", 1), ("score.weighted", 1)]);
        let spans: Vec<SpanRow<'_>> = entry
            .evidence()
            .sources()
            .iter()
            .map(|s| {
                let span = s.span();
                (
                    s.rule().as_str(),
                    s.revision().map(|r| r.get()),
                    span.start,
                    span.end,
                    span.line,
                    span.column,
                    s.text(),
                )
            })
            .collect();
        assert_eq!(
            spans,
            vec![
                ("elig.qual", Some(1), 92, 170, 4, 1, Some(SCORE_CLAUSE)),
                ("score.weighted", Some(1), 92, 170, 4, 1, Some(SCORE_CLAUSE)),
            ]
        );
    }
    // The Outcomes of the engine are the ones the report shows, in order.
    let outcomes: Vec<Outcome> = run.evaluation().outcomes.iter().skip(1).cloned().collect();
    assert_eq!(
        outcomes,
        vec![
            Outcome::Decided(TraceValue::Bool(false)),
            Outcome::Decided(TraceValue::Bool(true)),
            Outcome::Decided(TraceValue::Bool(false)),
            Outcome::Decided(TraceValue::Bool(true)),
        ]
    );
}

#[test]
fn an_indeterminate_qualification_names_the_missing_input_and_the_others_stay_decided() {
    let run = run_with(CSV_MISSING_JUDGES, None, &full_request(), &sources()).unwrap();
    let entries = qualification(run.report().unwrap());
    let seen: Vec<(&str, &Truth)> = entries
        .iter()
        .map(|e| (e.participant(), e.result()))
        .collect();
    let missing = Truth::Indeterminate(vec![Missing::Input {
        participant: "p2".to_owned(),
        field: "judges".to_owned(),
    }]);
    assert_eq!(
        seen,
        vec![
            ("p1", &Truth::False),
            ("p2", &missing),
            ("p3", &Truth::False),
            ("p4", &Truth::True),
        ]
    );
    // Only the technical value of p2 was read before the score was missing.
    let p2 = entries.get(1).unwrap();
    let read: Vec<&str> = p2.inputs_read().iter().map(|r| r.name.as_str()).collect();
    assert_eq!(read, vec!["technical"]);
}

#[test]
fn the_report_is_a_bracket_report_with_the_fixed_scope_statement_and_only_its_sections() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let doc = run.report().unwrap();
    assert_eq!(doc.header().application, ReportApplication::BracketCheck);
    let names: Vec<&str> = doc.sections().iter().map(ReportSection::name).collect();
    assert_eq!(
        names,
        vec!["standings", "qualification", "published", "diagnostics"]
    );
    assert!(render_human(doc).contains(BRACKET_SCOPE));
    let json = String::from_utf8(render_json(doc)).unwrap();
    assert!(json.contains(&format!("\"ranking_scope\":\"{BRACKET_SCOPE}\"")));
    assert!(json.contains("\"application\":\"bracket-check\""));
}

#[test]
fn a_ranking_only_request_has_no_qualification_section_and_the_reverse() {
    let ranking = BracketRequest {
        ranking: full_request().ranking,
        ..BracketRequest::default()
    };
    let run = run_with(CSV_FULL, None, &ranking, &sources()).unwrap();
    assert!(qualification(run.report().unwrap()).is_empty());
    assert_eq!(run.evaluation().outcomes.len(), 1);
    let qualifying = BracketRequest {
        qualification: full_request().qualification,
        ..BracketRequest::default()
    };
    let run = run_with(CSV_FULL, None, &qualifying, &sources()).unwrap();
    assert!(standings(run.report().unwrap()).is_empty());
    assert_eq!(run.evaluation().outcomes.len(), 4);
}

#[test]
fn header_carries_the_rulebook_hash_the_input_hash_and_verified_provenance() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let doc = run.report().unwrap();
    assert_eq!(
        doc.header().rulebook_hash.as_str(),
        "315323aaa60b86e8212d8f82e76f8197467b5923ebe162c19d1399521df0b7d5"
    );
    let inputs: Vec<(&str, &str)> = doc
        .header()
        .input_hashes
        .iter()
        .map(|(role, hash)| (*role, hash.as_str()))
        .collect();
    assert_eq!(
        inputs,
        vec![(
            "participants",
            "faf87de810a2f0727fa557fbe0bddd15d4c22309b7740fd1f6e5a382511f7bcf"
        )]
    );
    assert!(matches!(
        doc.provenance(),
        ProvenanceSummary::Verified { traces: 5, .. }
    ));
}

#[test]
fn a_run_without_source_models_says_not_verified_and_stays_complete() {
    let run = run_with(CSV_FULL, None, &full_request(), &SourceModels::new()).unwrap();
    let doc = run.report().unwrap();
    assert!(matches!(
        doc.provenance(),
        ProvenanceSummary::NotVerifiedNoSources {
            without_sources: 5,
            verified: 0,
            total: 5,
            ..
        }
    ));
    assert!(qualification(doc).iter().all(|e| !e.evidence().verified()));
    assert_eq!(run.class(), ResultClass::Complete);
}

#[test]
fn two_runs_render_the_same_bytes() {
    let render = || {
        let run = run_with(CSV_TIE, None, &full_request(), &sources()).unwrap();
        let doc = run.report().unwrap();
        (render_human(doc), render_json(doc))
    };
    assert_eq!(render(), render());
}

#[test]
fn entries_follow_the_order_of_the_outcomes() {
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let doc = run.report().unwrap();
    let mut traces: Vec<usize> = standings(doc)
        .iter()
        .map(|t| t.evidence().trace())
        .collect();
    traces.extend(qualification(doc).iter().map(|e| e.evidence().trace()));
    assert_eq!(traces, vec![0, 1, 2, 3, 4]);
}

fn present_inputs<'a>(run: &'a Evaluated, compiled: &'a CompiledRulebook) -> PresentInputs<'a> {
    let evaluation = run.evaluation();
    PresentInputs {
        compiled,
        outcomes: &evaluation.outcomes,
        traces: &evaluation.traces,
        verifications: run.verifications(),
        diagnostics: &evaluation.diagnostics,
        comparison: evaluation.comparison.as_ref(),
        not_requested: run.prepared().not_requested(),
        input_hashes: run.prepared().input_hashes(),
        statements: &[],
        synthetic_data: None,
    }
}

#[test]
fn present_refuses_lists_of_different_length_and_builds_no_partial_report() {
    let compiled = compiled();
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let mut inputs = present_inputs(&run, &compiled);
    inputs.outcomes = inputs.outcomes.get(..4).unwrap();
    assert_eq!(
        present(&inputs).unwrap_err(),
        PresentError::CountMismatch {
            outcomes: 4,
            traces: 5,
            verifications: 5
        }
    );
}

#[test]
fn present_refuses_an_outcome_that_differs_from_its_trace() {
    let compiled = compiled();
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let mut flipped = run.evaluation().outcomes.clone();
    *flipped.get_mut(1).unwrap() = Outcome::Decided(TraceValue::Bool(true));
    let mut inputs = present_inputs(&run, &compiled);
    inputs.outcomes = &flipped;
    assert_eq!(
        present(&inputs).unwrap_err(),
        PresentError::OutcomeNotInTrace { trace: 1 }
    );
}

#[test]
fn present_refuses_a_trace_for_a_target_the_rulebook_does_not_declare() {
    // `slice-basic` does not declare `qualified`. Trace 0 (`standing`) is
    // reportable, trace 1 is not.
    let run = run_with(CSV_FULL, None, &full_request(), &sources()).unwrap();
    let basic = compiled_basic();
    let inputs = present_inputs(&run, &basic);
    assert_eq!(
        present(&inputs).unwrap_err(),
        PresentError::TargetNotReportable { trace: 1 }
    );
}

#[test]
fn a_score_trace_is_not_a_standings_report_entry() {
    // `score` is a weighted score, a kind that only the Prize report holds.
    let compiled = compiled();
    let (p_id, options) = (id("participants.csv"), MappingOptions::default());
    let files = FileInputs {
        participants: NamedFile {
            bytes: CSV_FULL.as_bytes(),
            name: &p_id,
        },
        collections: None,
        published: None,
        options: &options,
    };
    let prepared = prepare(&compiled, &full_request(), &files).unwrap();
    let question = Question::Value {
        target: target("score"),
        participant: ParticipantId::new("p1").unwrap(),
    };
    let evaluation = rt_engine::evaluate(&compiled, prepared.data(), &[question], None);
    let none = SourceModels::new();
    let verifications: Vec<_> = evaluation
        .traces
        .iter()
        .map(|t| verify_trace(t, &none))
        .collect();
    let result = present(&PresentInputs {
        compiled: &compiled,
        outcomes: &evaluation.outcomes,
        traces: &evaluation.traces,
        verifications: &verifications,
        diagnostics: &evaluation.diagnostics,
        comparison: None,
        not_requested: &[],
        input_hashes: prepared.input_hashes(),
        statements: &[],
        synthetic_data: None,
    });
    assert_eq!(
        result.unwrap_err(),
        PresentError::TargetNotReportable { trace: 0 }
    );
}

#[test]
fn a_request_without_targets_is_not_an_error_and_evaluates_nothing() {
    let run = run_with(
        CSV_FULL,
        None,
        &BracketRequest::default(),
        &SourceModels::new(),
    )
    .unwrap();
    assert!(run.evaluation().outcomes.is_empty());
    let names: Vec<&str> = run
        .report()
        .unwrap()
        .sections()
        .iter()
        .map(ReportSection::name)
        .collect();
    assert_eq!(names, vec!["published", "diagnostics"]);
}

#[test]
fn the_caller_states_whether_the_data_is_synthetic_and_the_report_only_labels_it() {
    let compiled = compiled();
    let (p_id, options, none) = (id("p.csv"), MappingOptions::default(), SourceModels::new());
    let run = evaluate(&BracketInputs {
        compiled: &compiled,
        request: &full_request(),
        files: FileInputs {
            participants: NamedFile {
                bytes: CSV_FULL.as_bytes(),
                name: &p_id,
            },
            collections: None,
            published: None,
            options: &options,
        },
        sources: &none,
        statements: &[],
        synthetic_data: Some(true),
    })
    .unwrap();
    assert_eq!(run.report().unwrap().header().synthetic_data, Some(true));
}
