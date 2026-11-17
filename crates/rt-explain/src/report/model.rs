//! The application report model (design §8.5 to §8.8, Req 31.6, 31.13, 31.14,
//! 32.1, 32.2, 32.5).
//!
//! Every value is copied from an Outcome, a Decision_Trace or the Explainer's
//! [`Verification`]. Nothing is computed, ordered or decided here. The model
//! has no field for validity, a pairing, round, bracket position, seed, slot,
//! advancement, elimination, prize, award or amount.
//!
//! The Bracket_Check tie-straddle notes belong to the deferred rank_reference
//! task 9.3 and are not modelled.

use crate::verify::{SpanVerdict, Verification, VerificationStatus};
use rt_foundation::{Code, Diagnostic, RevisionNumber, RuleId};
use rt_numeric::ExactNumber;
use rt_trace::{
    CompareEntry, DecisionTrace, Entry, Outcome, Place, QuestionKind, Reason, TraceValue,
};
use std::collections::BTreeSet;
use std::fmt;

/// A SHA-256 digest as 64 lowercase hexadecimal digits.
pub type Sha256Hex = rt_trace::DocumentHash;

/// Which application a report belongs to. It decides the field set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportApplication {
    /// The Prize_Check demonstration layer.
    PrizeCheck,
    /// The Bracket_Check demonstration layer.
    BracketCheck,
}

impl ReportApplication {
    /// The name written in the header.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::PrizeCheck => "prize-check",
            Self::BracketCheck => "bracket-check",
        }
    }
}

/// The identifier of a required statement of Req 34, for example
/// `ST-BOUNDARY`. The statement text is not part of this model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatementId(&'static str);

impl StatementId {
    /// A statement identifier.
    #[must_use]
    pub fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// The identifier text.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

/// The provenance status of the whole report. It is derived from the
/// Explainer's output and never chosen by the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProvenanceSummary {
    /// The Explainer verified every trace.
    Verified {
        /// Number of traces, all verified.
        traces: u32,
    },
    /// At least one trace is not verified, or there is no trace.
    NotVerified {
        /// Codes of the verification Diagnostics, ascending and without
        /// repeats.
        codes: Vec<Code>,
        /// Number of traces the Explainer verified.
        verified: u32,
        /// Number of traces.
        total: u32,
    },
    /// No Source_Model was supplied (Req 31.14).
    NotVerifiedNoSources,
}

impl ProvenanceSummary {
    /// Derives the summary from the Explainer's verifications.
    #[must_use]
    pub fn from_verifications(verifications: &[Verification]) -> Self {
        let no_sources = verifications.iter().any(|v| {
            v.diagnostics()
                .as_slice()
                .iter()
                .any(|d| d.code() == Code::ProvNotVerifiedNoSources)
        });
        if no_sources {
            return Self::NotVerifiedNoSources;
        }
        let total = count(verifications.len());
        let verified = count(
            verifications
                .iter()
                .filter(|v| v.status() == VerificationStatus::Verified)
                .count(),
        );
        if total > 0 && verified == total {
            return Self::Verified { traces: total };
        }
        let ids: BTreeSet<&'static str> = verifications
            .iter()
            .flat_map(|v| v.diagnostics().as_slice())
            .map(|d| d.code().id())
            .collect();
        let codes = ids.into_iter().filter_map(Code::from_id).collect();
        Self::NotVerified {
            codes,
            verified,
            total,
        }
    }
}

pub(crate) fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The report header (design §8.5). The format identifier and version are the
/// constants of the writer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportHeader {
    /// The application.
    pub application: ReportApplication,
    /// The required statements shown in the header.
    pub statements: Vec<StatementId>,
    /// Content hash of the Compiled_Rulebook.
    pub rulebook_hash: Sha256Hex,
    /// SHA-256 of the bytes of each input, named by role and never by path.
    pub input_hashes: Vec<(&'static str, Sha256Hex)>,
    /// Provenance status.
    pub provenance: ProvenanceSummary,
    /// Whether the Competition_Data is synthetic, as the caller states it. A
    /// label only.
    pub synthetic_data: Option<bool>,
}

/// Why a report part cannot be built or a document cannot be assembled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportError {
    /// The trace of this index does not have the Outcome kind the section
    /// needs.
    OutcomeKind {
        /// Index of the trace in the results file.
        trace: usize,
        /// What the section expected.
        expected: &'static str,
    },
    /// The trace has no participant.
    NoParticipant {
        /// Index of the trace.
        trace: usize,
    },
    /// The section does not belong to the application's field set.
    SectionNotAllowed {
        /// Section name.
        section: &'static str,
    },
    /// The section occurs twice.
    DuplicateSection {
        /// Section name.
        section: &'static str,
    },
    /// A Prize_Check standings table carries comparisons.
    ComparisonsNotAllowed,
    /// A difference names a kind and a code that do not belong together.
    DifferenceKindCode {
        /// The kind name given.
        kind: &'static str,
    },
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutcomeKind { trace, expected } => {
                write!(f, "trace {trace} does not carry {expected}")
            }
            Self::NoParticipant { trace } => write!(f, "trace {trace} names no participant"),
            Self::SectionNotAllowed { section } => {
                write!(f, "section {section} is not in the field set")
            }
            Self::DuplicateSection { section } => write!(f, "section {section} occurs twice"),
            Self::ComparisonsNotAllowed => {
                f.write_str("a Prize_Check standings table has no comparisons")
            }
            Self::DifferenceKindCode { kind } => {
                write!(f, "difference kind {kind} does not match its code")
            }
        }
    }
}

impl std::error::Error for ReportError {}

/// One trace with its Explainer output and its index in the results file.
#[derive(Clone, Copy, Debug)]
pub struct TraceRef<'a> {
    /// Zero-based index of the Question in the results file.
    pub index: usize,
    /// The Decision_Trace.
    pub trace: &'a DecisionTrace,
    /// The Explainer's output for it.
    pub verification: &'a Verification,
}

/// What every entry carries: the trace reference, the Explainer's verdict, the
/// rules and the sources (Req 31.14).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    /// Index of the trace in the results file.
    pub trace: usize,
    /// True only when the Explainer's status is verified.
    pub verified: bool,
    /// Applied rules: identifier and Revision_Number.
    pub rules: Vec<(RuleId, RevisionNumber)>,
    /// Every span with the text the Explainer resolved.
    pub sources: Vec<SpanVerdict>,
}

impl Evidence {
    /// Copies the evidence of one trace.
    #[must_use]
    pub fn new(r: &TraceRef<'_>) -> Self {
        Self {
            trace: r.index,
            verified: r.verification.status() == VerificationStatus::Verified,
            rules: r
                .trace
                .rules()
                .iter()
                .map(|x| (x.rule.clone(), x.revision))
                .collect(),
            sources: r.verification.spans().to_vec(),
        }
    }
}

/// One reason an Outcome is Indeterminate, copied from the Outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Missing {
    /// A required input is missing.
    Input {
        /// Participant.
        participant: String,
        /// Field name.
        field: String,
    },
    /// `min` or `max` of an empty collection.
    EmptyAggregate {
        /// Collection or field name.
        field: String,
    },
    /// A blocking Diagnostic recorded in the trace.
    Diagnosed(Code),
}

fn missing_of(outcome: &Outcome) -> Option<Vec<Missing>> {
    match outcome {
        Outcome::Decided(_) => None,
        Outcome::Indeterminate(reasons) => Some(
            reasons
                .iter()
                .map(|r| match r {
                    Reason::MissingInput { participant, field } => Missing::Input {
                        participant: participant.clone(),
                        field: field.clone(),
                    },
                    Reason::EmptyAggregate { field } => Missing::EmptyAggregate {
                        field: field.clone(),
                    },
                    Reason::Diagnosed(code) => Missing::Diagnosed(*code),
                })
                .collect(),
        ),
    }
}

fn value_participant(r: &TraceRef<'_>) -> Result<(String, String), ReportError> {
    let q = r.trace.question();
    match (&q.kind, &q.participant) {
        (QuestionKind::Value, Some(p)) => Ok((p.clone(), q.target.clone())),
        (QuestionKind::Value, None) => Err(ReportError::NoParticipant { trace: r.index }),
        (QuestionKind::Rank, _) => Err(ReportError::OutcomeKind {
            trace: r.index,
            expected: "a value question",
        }),
    }
}

/// A Boolean Target result: true, false or Indeterminate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Truth {
    /// The Outcome is true.
    True,
    /// The Outcome is false.
    False,
    /// The Outcome is Indeterminate, with its reasons.
    Indeterminate(Vec<Missing>),
}

fn truth_of(r: &TraceRef<'_>) -> Result<Truth, ReportError> {
    match r.trace.outcome() {
        Outcome::Decided(TraceValue::Bool(true)) => Ok(Truth::True),
        Outcome::Decided(TraceValue::Bool(false)) => Ok(Truth::False),
        Outcome::Decided(_) => Err(ReportError::OutcomeKind {
            trace: r.index,
            expected: "a Boolean",
        }),
        other => Ok(Truth::Indeterminate(missing_of(other).unwrap_or_default())),
    }
}

/// The eligibility Outcome of one participant for one Target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EligibilityEntry {
    /// Participant identifier.
    pub participant: String,
    /// Eligibility Target.
    pub target: String,
    /// The Outcome.
    pub result: Truth,
    /// Trace reference, verdict, rules and sources.
    pub evidence: Evidence,
}

impl EligibilityEntry {
    /// Copies the entry from a value trace whose Outcome is a Boolean or
    /// Indeterminate.
    ///
    /// # Errors
    /// [`ReportError`] when the trace is not such a trace.
    pub fn from_trace(r: &TraceRef<'_>) -> Result<Self, ReportError> {
        let (participant, target) = value_participant(r)?;
        Ok(Self {
            participant,
            target,
            result: truth_of(r)?,
            evidence: Evidence::new(r),
        })
    }
}

/// An exact score or derived value, or Indeterminate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Score {
    /// The exact value, written in canonical Exact_Number text.
    Exact(ExactNumber),
    /// The Outcome is Indeterminate, with its reasons.
    Indeterminate(Vec<Missing>),
}

/// The score of one participant for one Target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoreEntry {
    /// Participant identifier.
    pub participant: String,
    /// Score or derived Target.
    pub target: String,
    /// The Outcome.
    pub result: Score,
    /// Trace reference, verdict, rules and sources.
    pub evidence: Evidence,
}

impl ScoreEntry {
    /// Copies the entry from a value trace whose Outcome is a number or
    /// Indeterminate.
    ///
    /// # Errors
    /// [`ReportError`] when the trace is not such a trace.
    pub fn from_trace(r: &TraceRef<'_>) -> Result<Self, ReportError> {
        let (participant, target) = value_participant(r)?;
        let result = match r.trace.outcome() {
            Outcome::Decided(TraceValue::Number(n)) => Score::Exact(n.clone()),
            Outcome::Decided(_) => {
                return Err(ReportError::OutcomeKind {
                    trace: r.index,
                    expected: "a number",
                })
            }
            other => Score::Indeterminate(missing_of(other).unwrap_or_default()),
        };
        Ok(Self {
            participant,
            target,
            result,
            evidence: Evidence::new(r),
        })
    }
}

/// One position of a ranking: one participant, or one Tie_Group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandingsPlace {
    /// The Rank_Position.
    pub position: u32,
    /// The participants at the position. The writers list them in ascending
    /// identifier order, which has no ranking meaning when there are several.
    pub members: Vec<String>,
}

impl StandingsPlace {
    fn from_place(p: &Place) -> Self {
        Self {
            position: p.position(),
            members: p.members().to_vec(),
        }
    }
}

/// The Standings of one ranking Target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StandingsResult {
    /// The positions, in position order as the engine returned them.
    Places(Vec<StandingsPlace>),
    /// The ranking is Indeterminate, with its reasons.
    Indeterminate(Vec<Missing>),
}

/// The tie-breaking criteria applied to one pair of adjacent entries,
/// copied from a `compare` entry of the trace (design §8.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison(pub CompareEntry);

/// The Standings of one ranking Target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandingsTable {
    /// The ranking Target.
    pub target: String,
    /// The Outcome.
    pub result: StandingsResult,
    /// The comparisons of adjacent entries, in trace order. Written only in a
    /// Bracket_Check report.
    pub comparisons: Vec<Comparison>,
    /// Trace reference, verdict, rules and sources.
    pub evidence: Evidence,
}

impl StandingsTable {
    /// Copies the table from a rank trace whose Outcome is Standings or
    /// Indeterminate. The comparisons are copied only when `comparisons` is
    /// true.
    ///
    /// # Errors
    /// [`ReportError`] when the trace is not such a trace.
    pub fn from_trace(r: &TraceRef<'_>, comparisons: bool) -> Result<Self, ReportError> {
        let q = r.trace.question();
        if q.kind != QuestionKind::Rank {
            return Err(ReportError::OutcomeKind {
                trace: r.index,
                expected: "a rank question",
            });
        }
        let result = match r.trace.outcome() {
            Outcome::Decided(TraceValue::Standings(places)) => {
                StandingsResult::Places(places.iter().map(StandingsPlace::from_place).collect())
            }
            Outcome::Decided(_) => {
                return Err(ReportError::OutcomeKind {
                    trace: r.index,
                    expected: "Standings",
                })
            }
            other => StandingsResult::Indeterminate(missing_of(other).unwrap_or_default()),
        };
        let comparisons = if comparisons {
            r.trace
                .entries()
                .iter()
                .filter_map(|e| match e {
                    Entry::Compare(c) => Some(Comparison(c.clone())),
                    _ => None,
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(Self {
            target: q.target.clone(),
            result,
            comparisons,
            evidence: Evidence::new(r),
        })
    }
}

/// An input the trace read, copied from a `read` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputRead {
    /// Input name.
    pub name: String,
    /// Participant, when the input belongs to one.
    pub participant: Option<String>,
    /// The exact value.
    pub value: TraceValue,
}

/// The Qualification_Predicate Outcome of one participant. It is a
/// per-participant condition and never a place, slot, seed or advancement
/// (Req 32.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualificationEntry {
    /// Participant identifier.
    pub participant: String,
    /// The eligibility Target that is the predicate.
    pub target: String,
    /// The Outcome.
    pub result: Truth,
    /// The inputs the trace read, in trace order.
    pub inputs_read: Vec<InputRead>,
    /// Trace reference, verdict, rules and sources.
    pub evidence: Evidence,
}

impl QualificationEntry {
    /// Copies the entry from a value trace whose Outcome is a Boolean or
    /// Indeterminate.
    ///
    /// # Errors
    /// [`ReportError`] when the trace is not such a trace.
    pub fn from_trace(r: &TraceRef<'_>) -> Result<Self, ReportError> {
        let (participant, target) = value_participant(r)?;
        let inputs_read = r
            .trace
            .entries()
            .iter()
            .filter_map(|e| match e {
                Entry::Read(x) => Some(InputRead {
                    name: x.name.clone(),
                    participant: x.participant.clone(),
                    value: x.value.clone(),
                }),
                _ => None,
            })
            .collect();
        Ok(Self {
            participant,
            target,
            result: truth_of(r)?,
            inputs_read,
            evidence: Evidence::new(r),
        })
    }
}

/// One published value that differs from the computed one. All fields are
/// copied from the engine's `Difference`; the catalogue code follows the kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedDifference {
    /// Participant identifier.
    pub participant: String,
    /// Target.
    pub target: String,
    /// The published value, canonical text.
    pub published: String,
    /// The computed value, canonical text, or `None` when Indeterminate.
    pub computed: Option<String>,
    /// Index of the trace that produced the computed value.
    pub trace: usize,
    /// Sequence number of the trace entry, when the trace has one.
    pub entry: Option<u32>,
    /// The difference kind name: `value_differs`,
    /// `tie_broken_without_declared_criterion` or `tie_not_in_computed_result`.
    pub kind: &'static str,
    /// The catalogue code of the kind.
    pub code: Code,
    /// Whether the computed number has a non-terminating expansion.
    pub non_terminating: bool,
    /// The participants of the tie the kind refers to, empty for
    /// `value_differs`.
    pub group: Vec<String>,
}

/// The kind names with their catalogue codes (Req 13.12).
pub(crate) const DIFFERENCE_KINDS: [(&str, Code); 3] = [
    ("value_differs", Code::PublishedResultDiffers),
    (
        "tie_broken_without_declared_criterion",
        Code::PublishedTieGroupSplit,
    ),
    ("tie_not_in_computed_result", Code::PublishedTieNotComputed),
];

/// A published entry that no Question answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedUnused {
    /// Participant identifier.
    pub participant: String,
    /// Target.
    pub target: String,
}

/// The published-result consistency section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublishedSection {
    /// No Published_Result was supplied.
    NotSupplied,
    /// A Published_Result was compared. It is equal exactly when
    /// `differences` is empty.
    Compared {
        /// The differences, in the engine's order.
        differences: Vec<PublishedDifference>,
        /// Published entries that were not compared.
        unused: Vec<PublishedUnused>,
    },
}

/// A section of a report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportSection {
    /// Eligibility Outcomes (Prize_Check).
    Eligibility(Vec<EligibilityEntry>),
    /// Exact scores (Prize_Check).
    Scores(Vec<ScoreEntry>),
    /// Standings per ranking Target.
    Standings(Vec<StandingsTable>),
    /// Qualification_Predicate Outcomes (Bracket_Check).
    Qualification(Vec<QualificationEntry>),
    /// Published-result consistency.
    Published(PublishedSection),
    /// Diagnostics of the run.
    Diagnostics(Vec<Diagnostic>),
}

impl ReportSection {
    /// The section name, which is also its key in the machine form.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Eligibility(_) => "eligibility",
            Self::Scores(_) => "scores",
            Self::Standings(_) => "standings",
            Self::Qualification(_) => "qualification",
            Self::Published(_) => "published",
            Self::Diagnostics(_) => "diagnostics",
        }
    }

    /// The fixed position of the section in both writers.
    pub(crate) fn rank(&self) -> u8 {
        match self {
            Self::Eligibility(_) => 0,
            Self::Scores(_) => 1,
            Self::Standings(_) => 2,
            Self::Qualification(_) => 3,
            Self::Published(_) => 4,
            Self::Diagnostics(_) => 5,
        }
    }

    fn allowed(&self, app: ReportApplication) -> bool {
        !matches!(
            (app, self),
            (ReportApplication::PrizeCheck, Self::Qualification(_))
                | (
                    ReportApplication::BracketCheck,
                    Self::Eligibility(_) | Self::Scores(_)
                )
        )
    }
}

/// A report: a header and sections in the fixed order of the writers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportDoc {
    header: ReportHeader,
    sections: Vec<ReportSection>,
}

impl ReportDoc {
    /// Assembles a report. Sections are kept in the fixed order of the
    /// writers whatever order they are given in.
    ///
    /// # Errors
    /// [`ReportError`] when a section is not in the application's field set,
    /// occurs twice, when a Prize_Check table carries comparisons or when a
    /// difference names a kind and a code that do not belong together.
    pub fn new(
        header: ReportHeader,
        mut sections: Vec<ReportSection>,
    ) -> Result<Self, ReportError> {
        sections.sort_by_key(ReportSection::rank);
        for (i, s) in sections.iter().enumerate() {
            if !s.allowed(header.application) {
                return Err(ReportError::SectionNotAllowed { section: s.name() });
            }
            if sections.get(i + 1).is_some_and(|n| n.rank() == s.rank()) {
                return Err(ReportError::DuplicateSection { section: s.name() });
            }
            match s {
                ReportSection::Standings(tables)
                    if header.application == ReportApplication::PrizeCheck
                        && tables.iter().any(|t| !t.comparisons.is_empty()) =>
                {
                    return Err(ReportError::ComparisonsNotAllowed);
                }
                ReportSection::Published(PublishedSection::Compared { differences, .. }) => {
                    for d in differences {
                        if !DIFFERENCE_KINDS.contains(&(d.kind, d.code)) {
                            return Err(ReportError::DifferenceKindCode { kind: d.kind });
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(Self { header, sections })
    }

    /// The header.
    #[must_use]
    pub fn header(&self) -> &ReportHeader {
        &self.header
    }

    /// The sections in writer order.
    #[must_use]
    pub fn sections(&self) -> &[ReportSection] {
        &self.sections
    }
}
