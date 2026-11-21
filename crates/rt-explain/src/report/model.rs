//! The application report model (design §8.5 to §8.8, Req 31.6, 31.13, 31.14,
//! 32.1, 32.2, 32.5).
//!
//! Every value is copied from an Outcome, a Decision_Trace or the Explainer's
//! [`Verification`]. Nothing is computed, ordered or decided here. The model
//! has no field for validity, a pairing, round, bracket position, seed, slot,
//! advancement, elimination, prize, award or amount.
//!
//! An entry is bound to the verification at its trace index by equal span
//! verdicts and an equal verified flag. The trace itself (question,
//! participant) is not compared beyond its spans.
//!
//! The Bracket_Check tie-straddle notes belong to the deferred rank_reference
//! task 9.3 and are not modelled.

use crate::report::difference::{difference_kind, DifferenceKindInfo};
use crate::verify::{SpanVerdict, Verification, VerificationStatus};
use rt_foundation::{escape_controls, Code, Diagnostic, RevisionNumber, RuleId};
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

/// The provenance status of the whole report. Only
/// [`ProvenanceSummary::from_verifications`] and [`ReportDoc::new`] produce
/// one, so no caller can state a status the Explainer did not give (Req 31.14).
///
/// Every variant is `non_exhaustive`: no other crate can build one.
///
/// ```compile_fail
/// use rt_explain::report::ProvenanceSummary;
/// let _forged = ProvenanceSummary::Verified { traces: 1 };
/// ```
///
/// ```compile_fail
/// use rt_explain::report::ProvenanceSummary;
/// let _forged = ProvenanceSummary::NotVerifiedNoSources {
///     codes: Vec::new(),
///     without_sources: 0,
///     verified: 0,
///     total: 0,
/// };
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProvenanceSummary {
    /// The Explainer verified every trace.
    #[non_exhaustive]
    Verified {
        /// Number of traces, all verified.
        traces: u32,
    },
    /// At least one trace is not verified, or there is no trace.
    #[non_exhaustive]
    NotVerified {
        /// Codes of the verification Diagnostics, ascending and without
        /// repeats.
        codes: Vec<Code>,
        /// Number of traces the Explainer verified.
        verified: u32,
        /// Number of traces.
        total: u32,
    },
    /// At least one trace had no Source_Model supplied (Req 31.14). The
    /// summary is not verified even when other traces were.
    #[non_exhaustive]
    NotVerifiedNoSources {
        /// Codes of the verification Diagnostics, ascending and without
        /// repeats.
        codes: Vec<Code>,
        /// Number of traces that carry RT-PRV-0006.
        without_sources: u32,
        /// Number of traces the Explainer verified.
        verified: u32,
        /// Number of traces.
        total: u32,
    },
}

impl ProvenanceSummary {
    /// Derives the summary from the Explainer's verifications.
    ///
    /// It is `Verified` only when there is at least one trace and the
    /// Explainer verified every one.
    #[must_use]
    pub fn from_verifications(verifications: &[Verification]) -> Self {
        let total = count(verifications.len());
        let verified = count(
            verifications
                .iter()
                .filter(|v| v.status() == VerificationStatus::Verified)
                .count(),
        );
        let without_sources = count(
            verifications
                .iter()
                .filter(|v| {
                    v.diagnostics()
                        .as_slice()
                        .iter()
                        .any(|d| d.code() == Code::ProvNotVerifiedNoSources)
                })
                .count(),
        );
        let ids: BTreeSet<&'static str> = verifications
            .iter()
            .flat_map(|v| v.diagnostics().as_slice())
            .map(|d| d.code().id())
            .collect();
        let codes = ids.into_iter().filter_map(Code::from_id).collect();
        if without_sources > 0 {
            return Self::NotVerifiedNoSources {
                codes,
                without_sources,
                verified,
                total,
            };
        }
        if total > 0 && verified == total {
            return Self::Verified { traces: total };
        }
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

/// What the caller states in the report header (design §8.5). The format
/// identifier and version are the constants of the writer. The provenance
/// status is not here: [`ReportDoc::new`] derives it from the Explainer's
/// verifications.
///
/// ```compile_fail
/// use rt_explain::report::{ProvenanceSummary, ReportApplication, ReportHeader, Sha256Hex};
/// let h = Sha256Hex::new(&"a".repeat(64)).unwrap();
/// let _header = ReportHeader {
///     application: ReportApplication::PrizeCheck,
///     statements: Vec::new(),
///     rulebook_hash: h,
///     input_hashes: Vec::new(),
///     provenance: ProvenanceSummary::from_verifications(&[]),
///     synthetic_data: None,
/// };
/// ```
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
    /// A difference names a kind that is not in the table of kinds.
    UnknownDifferenceKind {
        /// The kind name given.
        kind: String,
    },
    /// The Verification was not produced for this trace.
    VerificationNotForTrace {
        /// Index of the trace.
        trace: usize,
    },
    /// An entry's evidence does not match the Verification at its trace index.
    EvidenceNotFromVerification {
        /// Index of the trace the evidence names.
        trace: usize,
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
            Self::UnknownDifferenceKind { kind } => {
                write!(
                    f,
                    "difference kind {} is not supported",
                    escape_controls(kind)
                )
            }
            Self::VerificationNotForTrace { trace } => {
                write!(f, "the verification was not produced for trace {trace}")
            }
            Self::EvidenceNotFromVerification { trace } => {
                write!(
                    f,
                    "the evidence of trace {trace} does not match its verification"
                )
            }
        }
    }
}

impl std::error::Error for ReportError {}

/// One trace with its Explainer output and its index in the results file.
///
/// The fields are private. [`TraceRef::new`] refuses a Verification that was
/// not produced for the trace.
///
/// ```compile_fail
/// use rt_explain::report::TraceRef;
/// use rt_explain::verify_trace;
/// use rt_srcmodel::SourceModels;
/// use rt_trace::DecisionTrace;
/// fn forge(trace: &DecisionTrace) {
///     let v = verify_trace(trace, &SourceModels::new());
///     let _ = TraceRef { index: 0, trace, verification: &v };
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct TraceRef<'a> {
    index: usize,
    trace: &'a DecisionTrace,
    verification: &'a Verification,
}

impl<'a> TraceRef<'a> {
    /// Pairs the trace at `index` of the results file with the Explainer's
    /// output for it.
    ///
    /// # Errors
    /// [`ReportError::VerificationNotForTrace`] when `verification` does not
    /// hold one verdict per span reference of `trace`, with the same rule,
    /// revision and recorded span.
    pub fn new(
        index: usize,
        trace: &'a DecisionTrace,
        verification: &'a Verification,
    ) -> Result<Self, ReportError> {
        if !verification.is_for(trace) {
            return Err(ReportError::VerificationNotForTrace { trace: index });
        }
        Ok(Self {
            index,
            trace,
            verification,
        })
    }

    /// Zero-based index of the Question in the results file.
    #[must_use]
    pub fn index(&self) -> usize {
        self.index
    }
}

/// What every entry carries: the trace reference, the Explainer's verdict, the
/// rules and the sources (Req 31.14).
///
/// The fields are private and the only constructor takes a [`TraceRef`], so
/// `verified` is true only when the Explainer's status is verified, and the
/// source text is the text [`verify_trace`](crate::verify_trace) resolved.
///
/// ```compile_fail
/// use rt_explain::report::Evidence;
/// let _forged = Evidence {
///     trace: 0,
///     verified: true,
///     rules: Vec::new(),
///     sources: Vec::new(),
/// };
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    pub(crate) trace: usize,
    pub(crate) verified: bool,
    pub(crate) rules: Vec<(RuleId, RevisionNumber)>,
    pub(crate) sources: Vec<SpanVerdict>,
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

    /// Index of the trace in the results file.
    #[must_use]
    pub fn trace(&self) -> usize {
        self.trace
    }

    /// True only when the Explainer's status is verified.
    #[must_use]
    pub fn verified(&self) -> bool {
        self.verified
    }

    /// Applied rules: identifier and Revision_Number.
    #[must_use]
    pub fn rules(&self) -> &[(RuleId, RevisionNumber)] {
        &self.rules
    }

    /// Every span with the text the Explainer resolved.
    #[must_use]
    pub fn sources(&self) -> &[SpanVerdict] {
        &self.sources
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
///
/// The fields are private. The only constructor is [`EligibilityEntry::from_trace`],
/// so the evidence of an entry always comes from a [`TraceRef`].
///
/// ```compile_fail
/// use rt_explain::report::{EligibilityEntry, Evidence, Truth};
/// fn forge(evidence: Evidence) {
///     let _ = EligibilityEntry {
///         participant: "p1".to_owned(),
///         target: "eligible".to_owned(),
///         result: Truth::True,
///         evidence,
///     };
/// }
/// ```
///
/// ```compile_fail
/// use rt_explain::report::{EligibilityEntry, Truth};
/// fn flip(entry: &mut EligibilityEntry) {
///     entry.result = Truth::True;
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EligibilityEntry {
    /// Participant identifier.
    pub(crate) participant: String,
    /// Eligibility Target.
    pub(crate) target: String,
    /// The Outcome.
    pub(crate) result: Truth,
    /// Trace reference, verdict, rules and sources.
    pub(crate) evidence: Evidence,
}

impl EligibilityEntry {
    /// Participant identifier.
    #[must_use]
    pub fn participant(&self) -> &str {
        &self.participant
    }

    /// Eligibility Target.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The Outcome.
    #[must_use]
    pub fn result(&self) -> &Truth {
        &self.result
    }

    /// Trace reference, verdict, rules and sources.
    #[must_use]
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }

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
    pub(crate) participant: String,
    /// Score or derived Target.
    pub(crate) target: String,
    /// The Outcome.
    pub(crate) result: Score,
    /// Trace reference, verdict, rules and sources.
    pub(crate) evidence: Evidence,
}

impl ScoreEntry {
    /// Participant identifier.
    #[must_use]
    pub fn participant(&self) -> &str {
        &self.participant
    }

    /// Score or derived Target.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The Outcome.
    #[must_use]
    pub fn result(&self) -> &Score {
        &self.result
    }

    /// Trace reference, verdict, rules and sources.
    #[must_use]
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }

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
    pub(crate) target: String,
    /// The Outcome.
    pub(crate) result: StandingsResult,
    /// The comparisons of adjacent entries, in trace order. Written only in a
    /// Bracket_Check report.
    pub(crate) comparisons: Vec<Comparison>,
    /// Trace reference, verdict, rules and sources.
    pub(crate) evidence: Evidence,
}

impl StandingsTable {
    /// The ranking Target.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The Outcome.
    #[must_use]
    pub fn result(&self) -> &StandingsResult {
        &self.result
    }

    /// The comparisons of adjacent entries, in trace order.
    #[must_use]
    pub fn comparisons(&self) -> &[Comparison] {
        &self.comparisons
    }

    /// Trace reference, verdict, rules and sources.
    #[must_use]
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }

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
    pub(crate) participant: String,
    /// The eligibility Target that is the predicate.
    pub(crate) target: String,
    /// The Outcome.
    pub(crate) result: Truth,
    /// The inputs the trace read, in trace order.
    pub(crate) inputs_read: Vec<InputRead>,
    /// Trace reference, verdict, rules and sources.
    pub(crate) evidence: Evidence,
}

impl QualificationEntry {
    /// Participant identifier.
    #[must_use]
    pub fn participant(&self) -> &str {
        &self.participant
    }

    /// The eligibility Target that is the predicate.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The Outcome.
    #[must_use]
    pub fn result(&self) -> &Truth {
        &self.result
    }

    /// The inputs the trace read, in trace order.
    #[must_use]
    pub fn inputs_read(&self) -> &[InputRead] {
        &self.inputs_read
    }

    /// Trace reference, verdict, rules and sources.
    #[must_use]
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }

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

/// The values of one published difference, copied from the engine's
/// `Difference`. The kind and its catalogue code are not here: they come from
/// [`PublishedDifference::new`] and the table of kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DifferenceFacts {
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
    /// Whether the computed number has a non-terminating expansion.
    pub non_terminating: bool,
    /// The participants of the tie the kind refers to, empty for
    /// `value_differs`.
    pub group: Vec<String>,
}

/// One published value that differs from the computed one. The catalogue code
/// follows the kind through the table of kinds, and a caller cannot choose it.
///
/// ```compile_fail
/// use rt_explain::report::PublishedDifference;
/// fn forge(d: &PublishedDifference) {
///     let _kind = d.kind;
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedDifference {
    pub(crate) participant: String,
    pub(crate) target: String,
    pub(crate) published: String,
    pub(crate) computed: Option<String>,
    pub(crate) trace: usize,
    pub(crate) entry: Option<u32>,
    pub(crate) kind: &'static DifferenceKindInfo,
    pub(crate) non_terminating: bool,
    pub(crate) group: Vec<String>,
}

impl PublishedDifference {
    /// Builds a difference of the kind named `kind`.
    ///
    /// # Errors
    /// [`ReportError::UnknownDifferenceKind`] when the name is not in the
    /// table of kinds.
    pub fn new(kind: &str, facts: DifferenceFacts) -> Result<Self, ReportError> {
        let kind = difference_kind(kind).ok_or_else(|| ReportError::UnknownDifferenceKind {
            kind: kind.to_owned(),
        })?;
        Ok(Self {
            participant: facts.participant,
            target: facts.target,
            published: facts.published,
            computed: facts.computed,
            trace: facts.trace,
            entry: facts.entry,
            kind,
            non_terminating: facts.non_terminating,
            group: facts.group,
        })
    }

    /// Participant identifier.
    #[must_use]
    pub fn participant(&self) -> &str {
        &self.participant
    }

    /// Target.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The published value, canonical text.
    #[must_use]
    pub fn published(&self) -> &str {
        &self.published
    }

    /// The computed value, canonical text, or `None` when Indeterminate.
    #[must_use]
    pub fn computed(&self) -> Option<&str> {
        self.computed.as_deref()
    }

    /// Index of the trace that produced the computed value.
    #[must_use]
    pub fn trace(&self) -> usize {
        self.trace
    }

    /// Sequence number of the trace entry, when the trace has one.
    #[must_use]
    pub fn entry(&self) -> Option<u32> {
        self.entry
    }

    /// The kind with its catalogue code and description.
    #[must_use]
    pub fn kind(&self) -> &'static DifferenceKindInfo {
        self.kind
    }

    /// Whether the computed number has a non-terminating expansion.
    #[must_use]
    pub fn non_terminating(&self) -> bool {
        self.non_terminating
    }

    /// The participants of the tie the kind refers to.
    #[must_use]
    pub fn group(&self) -> &[String] {
        &self.group
    }
}

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

impl ReportSection {
    fn evidences(&self) -> Vec<&Evidence> {
        match self {
            Self::Eligibility(es) => es.iter().map(|e| &e.evidence).collect(),
            Self::Scores(es) => es.iter().map(|e| &e.evidence).collect(),
            Self::Standings(ts) => ts.iter().map(|t| &t.evidence).collect(),
            Self::Qualification(es) => es.iter().map(|e| &e.evidence).collect(),
            Self::Published(_) | Self::Diagnostics(_) => Vec::new(),
        }
    }
}

/// A report: a header, the provenance summary and sections in the fixed order
/// of the writers.
///
/// The provenance summary is derived in [`ReportDoc::new`] from the same
/// verifications that every entry's evidence is checked against, so header
/// and entries cannot disagree. There is no parser: a report is built from
/// verifications and traces and never read back.
///
/// ```compile_fail
/// use rt_explain::report::ReportDoc;
/// fn needs_from_str<T: std::str::FromStr>() {}
/// needs_from_str::<ReportDoc>();
/// ```
///
/// ```compile_fail
/// use rt_explain::report::ReportDoc;
/// fn needs_try_from_bytes<'a, T: TryFrom<&'a [u8]>>() {}
/// needs_try_from_bytes::<ReportDoc>();
/// ```
///
/// ```compile_fail
/// use rt_explain::report::ReportDoc;
/// fn needs_default<T: Default>() {}
/// needs_default::<ReportDoc>();
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportDoc {
    header: ReportHeader,
    provenance: ProvenanceSummary,
    sections: Vec<ReportSection>,
}

impl ReportDoc {
    /// Assembles a report. Sections are kept in the fixed order of the
    /// writers whatever order they are given in. `verifications` are the
    /// Explainer's outputs indexed like the results file.
    ///
    /// # Errors
    /// [`ReportError`] when a section is not in the application's field set,
    /// occurs twice, when a Prize_Check table carries comparisons, or when an
    /// entry's evidence does not match the verification at its trace index.
    pub fn new(
        header: ReportHeader,
        verifications: &[Verification],
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
            if let ReportSection::Standings(tables) = s {
                if header.application == ReportApplication::PrizeCheck
                    && tables.iter().any(|t| !t.comparisons.is_empty())
                {
                    return Err(ReportError::ComparisonsNotAllowed);
                }
            }
            for e in s.evidences() {
                let matches = verifications.get(e.trace).is_some_and(|v| {
                    e.sources == v.spans()
                        && e.verified == (v.status() == VerificationStatus::Verified)
                });
                if !matches {
                    return Err(ReportError::EvidenceNotFromVerification { trace: e.trace });
                }
            }
        }
        Ok(Self {
            header,
            provenance: ProvenanceSummary::from_verifications(verifications),
            sections,
        })
    }

    /// The header as the caller stated it.
    #[must_use]
    pub fn header(&self) -> &ReportHeader {
        &self.header
    }

    /// The provenance status, derived from the Explainer's verifications.
    #[must_use]
    pub fn provenance(&self) -> &ProvenanceSummary {
        &self.provenance
    }

    /// The sections in writer order.
    #[must_use]
    pub fn sections(&self) -> &[ReportSection] {
        &self.sections
    }
}
