//! Tests of the shared report assembler and the Published-section builder.
//! Expected values come from the fixtures of `tests_app_fixtures` and from
//! hand-written lists, not from the assembler.

use crate::report::{
    assemble_report, compared_section, AssembleInputs, DifferenceFacts, EligibilityEntry,
    PresentError, PublishedSection, PublishedUnused, ReportApplication, ReportError, ReportSection,
    ScoreEntry, SectionEntry, StandingsTable, StatementId,
};
use crate::tests_app_fixtures::*;
use crate::{classify_evaluation, EvaluationClass, Verification};
use rt_foundation::{Code, Diagnostic, DiagnosticState, InputId, InputLocation, Location};
use rt_trace::{DecisionTrace, Outcome, TraceValue};

const GOOD_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";

fn at_line(code: Code, line: u32, message: &str) -> Diagnostic {
    Diagnostic::new(
        code,
        message.to_owned(),
        Location::Input(InputLocation {
            input: InputId::new("in/data.csv").unwrap(),
            line: Some(line),
            column: None,
            field: None,
        }),
    )
}

fn state(items: Vec<Diagnostic>) -> DiagnosticState {
    let mut s = DiagnosticState::empty();
    for d in items {
        s.push(d);
    }
    s
}

struct Fixture {
    outcomes: Vec<Outcome>,
    traces: Vec<DecisionTrace>,
    verifications: Vec<Verification>,
    diagnostics: DiagnosticState,
    rulebook_hash: String,
}

impl Fixture {
    fn new(app_traces: Vec<DecisionTrace>) -> Self {
        let verifications = verified(&app_traces);
        Self {
            outcomes: app_traces.iter().map(|t| t.outcome().clone()).collect(),
            traces: app_traces,
            verifications,
            diagnostics: DiagnosticState::empty(),
            rulebook_hash: GOOD_HASH.to_owned(),
        }
    }

    fn inputs(&self, application: ReportApplication) -> AssembleInputs<'_> {
        AssembleInputs {
            application,
            outcomes: &self.outcomes,
            traces: &self.traces,
            verifications: &self.verifications,
            diagnostics: &self.diagnostics,
            rulebook_hash: &self.rulebook_hash,
            input_hashes: &[],
            statements: &[],
            synthetic_data: None,
        }
    }
}

fn not_supplied() -> Result<PublishedSection, PresentError> {
    Ok(PublishedSection::NotSupplied)
}

/// The entry builder of a Prize-like application over the prize fixture:
/// traces 0 and 1 eligibility, 2 and 3 scores, 4 and 5 standings.
fn prize_entry(
    index: usize,
    _trace: &DecisionTrace,
    at: &crate::report::TraceRef<'_>,
) -> Result<SectionEntry, PresentError> {
    Ok(match index {
        0 | 1 => SectionEntry::Eligibility(EligibilityEntry::from_trace(at)?),
        2 | 3 => SectionEntry::Score(ScoreEntry::from_trace(at)?),
        _ => SectionEntry::Standings(StandingsTable::from_trace(at, false)?),
    })
}

fn names(sections: &[ReportSection]) -> Vec<&'static str> {
    sections
        .iter()
        .map(|s| match s {
            ReportSection::Eligibility(_) => "eligibility",
            ReportSection::Scores(_) => "scores",
            ReportSection::Standings(_) => "standings",
            ReportSection::Qualification(_) => "qualification",
            ReportSection::Published(_) => "published",
            ReportSection::Diagnostics(_) => "diagnostics",
        })
        .collect()
}

#[test]
fn sections_follow_the_fixed_order_with_published_and_diagnostics_last() {
    let f = Fixture::new(prize_traces());
    let doc = assemble_report(
        &f.inputs(ReportApplication::PrizeCheck),
        prize_entry,
        not_supplied,
    )
    .unwrap();
    assert_eq!(
        names(doc.sections()),
        vec![
            "eligibility",
            "scores",
            "standings",
            "published",
            "diagnostics"
        ]
    );
    match &doc.sections()[0] {
        ReportSection::Eligibility(entries) => assert_eq!(entries.len(), 2),
        other => panic!("{other:?}"),
    }
}

#[test]
fn bracket_entries_give_standings_then_qualification() {
    let f = Fixture::new(bracket_traces());
    let entry = |index: usize, _t: &DecisionTrace, at: &crate::report::TraceRef<'_>| {
        Ok(if index < 2 {
            SectionEntry::Standings(StandingsTable::from_trace(at, true)?)
        } else {
            SectionEntry::Qualification(crate::report::QualificationEntry::from_trace(at)?)
        })
    };
    let doc = assemble_report(
        &f.inputs(ReportApplication::BracketCheck),
        entry,
        not_supplied,
    )
    .unwrap();
    assert_eq!(
        names(doc.sections()),
        vec!["standings", "qualification", "published", "diagnostics"]
    );
}

#[test]
fn a_count_mismatch_is_refused_before_any_entry_is_built() {
    let mut f = Fixture::new(prize_traces());
    f.verifications.pop();
    let mut calls = 0;
    let result = assemble_report(
        &f.inputs(ReportApplication::PrizeCheck),
        |i, t, at| {
            calls += 1;
            prize_entry(i, t, at)
        },
        not_supplied,
    );
    assert_eq!(
        result.unwrap_err(),
        PresentError::CountMismatch {
            outcomes: 6,
            traces: 6,
            verifications: 5
        }
    );
    assert_eq!(calls, 0);
}

#[test]
fn each_list_that_differs_in_length_is_a_count_mismatch() {
    let mut f = Fixture::new(prize_traces());
    f.outcomes.pop();
    let r = assemble_report(
        &f.inputs(ReportApplication::PrizeCheck),
        prize_entry,
        not_supplied,
    );
    assert_eq!(
        r.unwrap_err(),
        PresentError::CountMismatch {
            outcomes: 5,
            traces: 6,
            verifications: 6
        }
    );
    let mut g = Fixture::new(prize_traces());
    g.traces.pop();
    let r = assemble_report(
        &g.inputs(ReportApplication::PrizeCheck),
        prize_entry,
        not_supplied,
    );
    assert_eq!(
        r.unwrap_err(),
        PresentError::CountMismatch {
            outcomes: 6,
            traces: 5,
            verifications: 6
        }
    );
}

#[test]
fn an_outcome_that_differs_from_its_trace_is_refused_at_its_index() {
    let mut f = Fixture::new(prize_traces());
    f.outcomes[3] = Outcome::Decided(TraceValue::Bool(true));
    f.outcomes[4] = Outcome::Decided(TraceValue::Bool(false));
    let r = assemble_report(
        &f.inputs(ReportApplication::PrizeCheck),
        prize_entry,
        not_supplied,
    );
    assert_eq!(r.unwrap_err(), PresentError::OutcomeNotInTrace { trace: 3 });
}

#[test]
fn an_entry_function_refusal_is_returned_and_the_published_builder_is_not_run() {
    let f = Fixture::new(prize_traces());
    let mut ran = false;
    let r = assemble_report(
        &f.inputs(ReportApplication::PrizeCheck),
        |i, t, at| {
            if i == 2 {
                Err(PresentError::TargetNotReportable { trace: i })
            } else {
                prize_entry(i, t, at)
            }
        },
        || {
            ran = true;
            not_supplied()
        },
    );
    assert_eq!(
        r.unwrap_err(),
        PresentError::TargetNotReportable { trace: 2 }
    );
    assert!(!ran);
}

#[test]
fn a_trace_error_outranks_a_published_error() {
    let f = Fixture::new(prize_traces());
    let r = assemble_report(
        &f.inputs(ReportApplication::PrizeCheck),
        |i, t, at| {
            if i == 5 {
                Err(PresentError::TargetNotReportable { trace: 5 })
            } else {
                prize_entry(i, t, at)
            }
        },
        || Err(PresentError::ComputedValueNotComparable { question: 0 }),
    );
    assert_eq!(
        r.unwrap_err(),
        PresentError::TargetNotReportable { trace: 5 }
    );
    let g = Fixture::new(prize_traces());
    let r = assemble_report(
        &g.inputs(ReportApplication::PrizeCheck),
        prize_entry,
        || Err(PresentError::ComputedValueNotComparable { question: 7 }),
    );
    assert_eq!(
        r.unwrap_err(),
        PresentError::ComputedValueNotComparable { question: 7 }
    );
}

#[test]
fn the_header_refuses_an_invalid_rulebook_hash_and_copies_the_plain_values() {
    for bad in ["", "xyz", &"1".repeat(63), &"1".repeat(65), &"A".repeat(64)] {
        let mut f = Fixture::new(prize_traces());
        f.rulebook_hash = bad.to_owned();
        let r = assemble_report(
            &f.inputs(ReportApplication::PrizeCheck),
            prize_entry,
            not_supplied,
        );
        assert_eq!(r.unwrap_err(), PresentError::RulebookHash, "{bad:?}");
    }
    let f = Fixture::new(prize_traces());
    let statements = [StatementId::new("ST-BOUNDARY")];
    let hashes = [("data", hash('2'))];
    let mut inputs = f.inputs(ReportApplication::PrizeCheck);
    inputs.statements = &statements;
    inputs.input_hashes = &hashes;
    inputs.synthetic_data = Some(true);
    let doc = assemble_report(&inputs, prize_entry, not_supplied).unwrap();
    let header = doc.header();
    assert_eq!(header.application, ReportApplication::PrizeCheck);
    assert_eq!(header.rulebook_hash, hash('1'));
    assert_eq!(header.statements, statements.to_vec());
    assert_eq!(header.input_hashes, hashes.to_vec());
    assert_eq!(header.synthetic_data, Some(true));
}

#[test]
fn the_report_model_refusal_is_returned_as_a_report_error() {
    // The Prize field set has no Qualification section.
    let f = Fixture::new(vec![qualification_trace(
        "p1",
        Outcome::Decided(TraceValue::Bool(true)),
    )]);
    let entry = |_i: usize, _t: &DecisionTrace, at: &crate::report::TraceRef<'_>| {
        Ok(SectionEntry::Qualification(
            crate::report::QualificationEntry::from_trace(at)?,
        ))
    };
    let r = assemble_report(
        &f.inputs(ReportApplication::PrizeCheck),
        entry,
        not_supplied,
    );
    assert!(matches!(
        r.unwrap_err(),
        PresentError::Report(ReportError::SectionNotAllowed { .. })
    ));
}

fn diagnostics_section(doc: &crate::report::ReportDoc) -> &[Diagnostic] {
    match doc.sections().last() {
        Some(ReportSection::Diagnostics(list)) => list,
        other => panic!("last section is not Diagnostics: {other:?}"),
    }
}

// RN-38, UD-40 section 3: an Indeterminate Outcome must not conceal an Error
// Diagnostic of the evaluation.
#[test]
fn an_error_diagnostic_is_listed_beside_an_indeterminate_outcome() {
    let mut f = Fixture::new(vec![elig_trace("p2", missing_outcome("p2", "birth_date"))]);
    let error = at_line(Code::AggregateEmptyCollection, 4, "empty collection");
    assert_eq!(error.severity(), rt_foundation::Severity::Error);
    f.diagnostics = state(vec![error.clone()]);
    let entry = |_i: usize, _t: &DecisionTrace, at: &crate::report::TraceRef<'_>| {
        Ok(SectionEntry::Eligibility(EligibilityEntry::from_trace(at)?))
    };
    let doc = assemble_report(
        &f.inputs(ReportApplication::PrizeCheck),
        entry,
        not_supplied,
    )
    .unwrap();
    assert!(matches!(f.outcomes[0], Outcome::Indeterminate(_)));
    assert!(diagnostics_section(&doc).contains(&error));
    assert_eq!(
        classify_evaluation(&f.outcomes, &f.diagnostics, &f.verifications),
        EvaluationClass::Indeterminate
    );
}

#[test]
fn the_diagnostics_section_merges_evaluation_and_verification_diagnostics() {
    let mut f = Fixture::new(prize_traces());
    f.verifications = unverified(&f.traces);
    let first = at_line(Code::AggregateEmptyCollection, 2, "second by line");
    let second = at_line(Code::ValConflictResolved, 1, "first by line");
    // Listed twice and out of order: shown once, in canonical order.
    f.diagnostics = state(vec![first.clone(), second.clone(), first.clone()]);
    let doc = assemble_report(
        &f.inputs(ReportApplication::PrizeCheck),
        prize_entry,
        not_supplied,
    )
    .unwrap();
    let shown = diagnostics_section(&doc);
    let verification_codes: Vec<Code> = f
        .verifications
        .iter()
        .flat_map(|v| v.diagnostics().as_slice().iter().map(Diagnostic::code))
        .collect();
    assert!(!verification_codes.is_empty());
    assert_eq!(shown.iter().filter(|d| **d == first).count(), 1);
    assert_eq!(shown.iter().filter(|d| **d == second).count(), 1);
    // Verification Diagnostics have a non-input location and sort after
    // input locations; the two input-located ones keep line order.
    let positions: Vec<usize> = [&second, &first]
        .iter()
        .map(|d| shown.iter().position(|s| s == *d).unwrap())
        .collect();
    assert!(positions[0] < positions[1]);
    for code in verification_codes {
        assert!(shown.iter().any(|d| d.code() == code), "{code:?}");
    }
}

#[test]
fn a_run_without_diagnostics_has_an_empty_diagnostics_section() {
    let f = Fixture::new(vec![score_trace("p1", "7.5")]);
    let entry = |_i: usize, _t: &DecisionTrace, at: &crate::report::TraceRef<'_>| {
        Ok(SectionEntry::Score(ScoreEntry::from_trace(at)?))
    };
    let doc = assemble_report(
        &f.inputs(ReportApplication::PrizeCheck),
        entry,
        not_supplied,
    )
    .unwrap();
    assert!(diagnostics_section(&doc).is_empty());
}

fn unused(p: &str, t: &str) -> PublishedUnused {
    PublishedUnused {
        participant: p.to_owned(),
        target: t.to_owned(),
    }
}

#[test]
fn compared_section_keeps_the_order_of_differences_and_unused_rows() {
    let second = DifferenceFacts {
        participant: "p2".to_owned(),
        computed: None,
        ..facts(&[])
    };
    let section = compared_section(
        vec![
            ("value_differs", facts(&[])),
            ("value_differs", second.clone()),
        ],
        vec![unused("p9", "a"), unused("p8", "b")],
    )
    .unwrap();
    let PublishedSection::Compared {
        differences,
        unused: rows,
    } = section
    else {
        panic!("not compared");
    };
    assert_eq!(differences.len(), 2);
    assert_eq!(differences[0].participant(), "p1");
    assert_eq!(differences[1].participant(), "p2");
    assert_eq!(differences[1].computed(), None);
    assert_eq!(rows, vec![unused("p9", "a"), unused("p8", "b")]);
}

#[test]
fn compared_section_without_differences_is_compared_and_equal() {
    let section = compared_section(Vec::new(), Vec::new()).unwrap();
    assert_eq!(
        section,
        PublishedSection::Compared {
            differences: Vec::new(),
            unused: Vec::new()
        }
    );
}

#[test]
fn compared_section_refuses_an_unknown_kind_with_its_name() {
    let r = compared_section(vec![("not_a_kind", facts(&[]))], Vec::new());
    assert_eq!(
        r.unwrap_err(),
        PresentError::Report(ReportError::UnknownDifferenceKind {
            kind: "not_a_kind".to_owned()
        })
    );
}
