//! Fixtures for the application report tests (task 9.4).

use crate::report::{
    DifferenceFacts, EligibilityEntry, PublishedDifference, PublishedSection, PublishedUnused,
    QualificationEntry, ReportApplication, ReportDoc, ReportHeader, ReportSection, ScoreEntry,
    Sha256Hex, StandingsTable, StatementId, TraceRef,
};
use crate::{verify_trace, Verification};
use rt_foundation::{
    Code, Diagnostic, DiagnosticState, DocumentId, Location, RevisionNumber, RuleId,
};
use rt_numeric::parse_literal;
use rt_srcmodel::{DocumentKind, Element, ElementKind, SourceModel, SourceModels};
use rt_trace::{
    CompareCriterion, CompareEntry, CriterionResult, DecisionTrace, DocumentHash, Entry,
    MissingInput, Outcome, Place, QuestionKind, RankDirection, ReadEntry, Reason, TraceParts,
    TraceQuestion, TraceRule, TraceSpan, TraceValue,
};

pub(crate) const CONTENT: &str = "Entrants must be 16.\u{1b}[0m\nSecond line.";

pub(crate) fn model() -> SourceModel {
    let len = u32::try_from(CONTENT.len()).unwrap();
    SourceModel::new(
        DocumentId::new("rules.md").unwrap(),
        DocumentKind::Markdown,
        CONTENT.to_owned(),
        vec![Element {
            id: 0,
            kind: ElementKind::Paragraph,
            level: None,
            label: None,
            start: 0,
            end: len,
            line: 1,
            column: 1,
            parent: None,
        }],
    )
    .unwrap()
}

pub(crate) fn models() -> SourceModels {
    let mut m = SourceModels::new();
    m.insert(model()).unwrap();
    m
}

pub(crate) fn span(start: u32, end: u32) -> TraceSpan {
    TraceSpan {
        document: DocumentId::new("rules.md").unwrap(),
        sha256: DocumentHash::new(model().sha256().as_str()).unwrap(),
        start,
        end,
        line: 1,
        column: 1,
    }
}

pub(crate) fn num(text: &str) -> TraceValue {
    TraceValue::Number(parse_literal(text).unwrap())
}

pub(crate) fn hash(digit: char) -> Sha256Hex {
    Sha256Hex::new(&digit.to_string().repeat(64)).unwrap()
}

fn rid(s: &str) -> RuleId {
    RuleId::new(s).unwrap()
}

pub(crate) fn question(kind: QuestionKind, target: &str, who: Option<&str>) -> TraceQuestion {
    TraceQuestion {
        kind,
        target: target.to_owned(),
        participant: who.map(str::to_owned),
    }
}

/// A trace of one rule at `revision` with one span over `start..end`.
pub(crate) fn trace(
    q: TraceQuestion,
    outcome: Outcome,
    rule: &str,
    revision: u32,
    span: (u32, u32),
    entries: Vec<Entry>,
    missing: Vec<MissingInput>,
) -> DecisionTrace {
    DecisionTrace::new(TraceParts {
        question: q,
        outcome,
        rules: vec![TraceRule {
            rule: rid(rule),
            revision: RevisionNumber::new(revision).unwrap(),
            spans: vec![self::span(span.0, span.1)],
        }],
        entries,
        not_applied: Vec::new(),
        missing,
        diagnostics: DiagnosticState::empty(),
    })
    .unwrap()
}

pub(crate) fn missing_outcome(who: &str, field: &str) -> Outcome {
    Outcome::indeterminate(vec![Reason::MissingInput {
        participant: who.to_owned(),
        field: field.to_owned(),
    }])
}

pub(crate) fn elig_trace(who: &str, outcome: Outcome) -> DecisionTrace {
    let missing = match &outcome {
        Outcome::Indeterminate(_) => vec![MissingInput {
            participant: who.to_owned(),
            field: "birth_date".to_owned(),
        }],
        Outcome::Decided(_) => Vec::new(),
    };
    trace(
        question(QuestionKind::Value, "elig.age", Some(who)),
        outcome,
        "elig.age",
        1,
        (0, 20),
        Vec::new(),
        missing,
    )
}

pub(crate) fn score_trace(who: &str, value: &str) -> DecisionTrace {
    trace(
        question(QuestionKind::Value, "score.total", Some(who)),
        Outcome::Decided(num(value)),
        "score.total",
        2,
        (0, 20),
        Vec::new(),
        Vec::new(),
    )
}

pub(crate) fn compare(a: &str, b: &str, first_ahead: bool) -> Entry {
    Entry::Compare(CompareEntry {
        between: (a.to_owned(), b.to_owned()),
        criteria: vec![CompareCriterion {
            index: 0,
            rule: rid("rank.total"),
            direction: RankDirection::Desc,
            left: num("9"),
            right: num(if first_ahead { "8" } else { "9" }),
            result: if first_ahead {
                CriterionResult::First
            } else {
                CriterionResult::Equal
            },
        }],
        first_ahead,
    })
}

/// Standings `1: p3`, `2: {p1, p2}`.
pub(crate) fn standings_trace() -> DecisionTrace {
    let places = vec![
        Place::new(1, vec!["p3".to_owned()]).unwrap(),
        Place::new(2, vec!["p2".to_owned(), "p1".to_owned()]).unwrap(),
    ];
    trace(
        question(QuestionKind::Rank, "rank.total", None),
        Outcome::Decided(TraceValue::Standings(places.clone())),
        "rank.total",
        1,
        (0, 20),
        vec![
            compare("p3", "p1", true),
            compare("p1", "p2", false),
            Entry::Place(places[0].clone()),
            Entry::Place(places[1].clone()),
        ],
        Vec::new(),
    )
}

pub(crate) fn standings_indeterminate_trace() -> DecisionTrace {
    trace(
        question(QuestionKind::Rank, "rank.late", None),
        missing_outcome("p4", "late_score"),
        "rank.late",
        1,
        (0, 20),
        Vec::new(),
        vec![MissingInput {
            participant: "p4".to_owned(),
            field: "late_score".to_owned(),
        }],
    )
}

pub(crate) fn qualification_trace(who: &str, outcome: Outcome) -> DecisionTrace {
    trace(
        question(QuestionKind::Value, "qual.q", Some(who)),
        outcome,
        "qual.q",
        3,
        (0, 20),
        vec![Entry::Read(ReadEntry {
            name: "points".to_owned(),
            participant: Some(who.to_owned()),
            value: num("70.5"),
            rule: rid("qual.q"),
            default_spans: Vec::new(),
        })],
        Vec::new(),
    )
}

pub(crate) fn verified(traces: &[DecisionTrace]) -> Vec<Verification> {
    traces.iter().map(|t| verify_trace(t, &models())).collect()
}

pub(crate) fn unverified(traces: &[DecisionTrace]) -> Vec<Verification> {
    traces
        .iter()
        .map(|t| verify_trace(t, &SourceModels::new()))
        .collect()
}

/// The `index`-th trace with its verification.
pub(crate) fn tref<'a>(
    index: usize,
    traces: &'a [DecisionTrace],
    vs: &'a [Verification],
) -> TraceRef<'a> {
    TraceRef::new(index, &traces[index], &vs[index]).unwrap()
}

pub(crate) fn header(app: ReportApplication) -> ReportHeader {
    ReportHeader {
        application: app,
        statements: vec![
            StatementId::new("ST-BOUNDARY"),
            StatementId::new("ST-VERIFIED-MEANING"),
        ],
        rulebook_hash: hash('1'),
        input_hashes: vec![("data", hash('2')), ("published", hash('3'))],
        synthetic_data: Some(true),
    }
}

pub(crate) fn facts(group: &[&str]) -> DifferenceFacts {
    DifferenceFacts {
        participant: "p1".to_owned(),
        target: "rank.total".to_owned(),
        published: "3".to_owned(),
        computed: Some("2".to_owned()),
        trace: 3,
        entry: Some(5),
        non_terminating: false,
        group: group.iter().map(|s| (*s).to_owned()).collect(),
    }
}

pub(crate) fn difference(kind: &str, group: &[&str]) -> PublishedDifference {
    PublishedDifference::new(kind, facts(group)).unwrap()
}

pub(crate) fn published_differs() -> PublishedSection {
    PublishedSection::Compared {
        differences: vec![
            difference("value_differs", &[]),
            PublishedDifference::new(
                "tie_broken_without_declared_criterion",
                DifferenceFacts {
                    participant: "p2".to_owned(),
                    computed: Some("2/3".to_owned()),
                    non_terminating: true,
                    ..facts(&["p2", "p1"])
                },
            )
            .unwrap(),
        ],
        unused: vec![PublishedUnused {
            participant: "p9".to_owned(),
            target: "rank.total".to_owned(),
        }],
    }
}

/// An evaluation Diagnostic of the kind the engine reports beside the
/// Indeterminate score of p2 (missing input `points`). A published-result
/// difference never belongs in the Diagnostics section (Req 31.16).
pub(crate) fn evaluation_diagnostic() -> Diagnostic {
    Diagnostic::new(
        Code::ExeMissingValue,
        "rule score.total: required input 'points' of participant 'p2' is a Missing_Value"
            .to_owned(),
        Location::None(Code::ExeMissingValue),
    )
}

/// All fixture traces of a Prize_Check document, in section order:
/// 0 eligible, 1 indeterminate, 2 score, 3 score indeterminate, 4 standings,
/// 5 standings indeterminate.
pub(crate) fn prize_traces() -> Vec<DecisionTrace> {
    vec![
        elig_trace("p1", Outcome::Decided(TraceValue::Bool(true))),
        elig_trace("p2", missing_outcome("p2", "birth_date")),
        score_trace("p1", "7.5"),
        trace(
            question(QuestionKind::Value, "score.total", Some("p2")),
            missing_outcome("p2", "points"),
            "score.total",
            2,
            (0, 20),
            Vec::new(),
            vec![MissingInput {
                participant: "p2".to_owned(),
                field: "points".to_owned(),
            }],
        ),
        standings_trace(),
        standings_indeterminate_trace(),
    ]
}

pub(crate) fn prize_doc(traces: &[DecisionTrace], vs: &[Verification]) -> ReportDoc {
    let r = |i: usize| tref(i, traces, vs);
    ReportDoc::new(
        header(ReportApplication::PrizeCheck),
        vs,
        vec![
            ReportSection::Eligibility(vec![
                EligibilityEntry::from_trace(&r(0)).unwrap(),
                EligibilityEntry::from_trace(&r(1)).unwrap(),
            ]),
            ReportSection::Scores(vec![
                ScoreEntry::from_trace(&r(2)).unwrap(),
                ScoreEntry::from_trace(&r(3)).unwrap(),
            ]),
            ReportSection::Standings(vec![
                StandingsTable::from_trace(&r(4), false).unwrap(),
                StandingsTable::from_trace(&r(5), false).unwrap(),
            ]),
            ReportSection::Published(published_differs()),
            ReportSection::Diagnostics(vec![evaluation_diagnostic()]),
        ],
    )
    .unwrap()
}

/// Fixture traces of a Bracket_Check document: 0 standings, 1 indeterminate
/// standings, 2 qualification true, 3 qualification indeterminate.
pub(crate) fn bracket_traces() -> Vec<DecisionTrace> {
    vec![
        standings_trace(),
        standings_indeterminate_trace(),
        qualification_trace("p1", Outcome::Decided(TraceValue::Bool(true))),
        qualification_trace("p2", missing_outcome("p2", "points")),
    ]
}

pub(crate) fn bracket_doc(traces: &[DecisionTrace], vs: &[Verification]) -> ReportDoc {
    let r = |i: usize| tref(i, traces, vs);
    ReportDoc::new(
        header(ReportApplication::BracketCheck),
        vs,
        vec![
            ReportSection::Standings(vec![
                StandingsTable::from_trace(&r(0), true).unwrap(),
                StandingsTable::from_trace(&r(1), true).unwrap(),
            ]),
            ReportSection::Qualification(vec![
                QualificationEntry::from_trace(&r(2)).unwrap(),
                QualificationEntry::from_trace(&r(3)).unwrap(),
            ]),
            ReportSection::Published(published_differs()),
            ReportSection::Diagnostics(vec![evaluation_diagnostic()]),
        ],
    )
    .unwrap()
}

/// A model of `rules.md` whose hash differs from the one the fixture spans
/// record, so that every span fails with RT-PRV-0002.
pub(crate) fn other_models() -> SourceModels {
    let mut m = SourceModels::new();
    m.insert(
        SourceModel::new(
            DocumentId::new("rules.md").unwrap(),
            DocumentKind::Markdown,
            "Different text entirely.".to_owned(),
            vec![Element {
                id: 0,
                kind: ElementKind::Paragraph,
                level: None,
                label: None,
                start: 0,
                end: 24,
                line: 1,
                column: 1,
                parent: None,
            }],
        )
        .unwrap(),
    )
    .unwrap();
    m
}

pub(crate) fn mismatched(traces: &[DecisionTrace]) -> Vec<Verification> {
    traces
        .iter()
        .map(|t| verify_trace(t, &other_models()))
        .collect()
}
