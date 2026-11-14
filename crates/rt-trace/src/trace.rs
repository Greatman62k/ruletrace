//! The Decision_Trace data model (design §3.6.3, Req 13).
//!
//! Entry sequence numbers are not stored: the n-th entry has `seq = n` (1-based), so the
//! numbering is always contiguous and a `from` reference can only point at an earlier entry.

use crate::outcome::{Outcome, Place, TraceValue};
use rt_foundation::{DiagnosticState, DocumentId, RevisionNumber, RuleId};
use rt_numeric::RoundingMode;
use std::fmt;

/// A SHA-256 digest as 64 lowercase hexadecimal digits.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentHash(String);

impl DocumentHash {
    /// Accepts exactly 64 characters of `0-9a-f`.
    #[must_use]
    pub fn new(text: &str) -> Option<Self> {
        let ok = text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        ok.then(|| Self(text.to_owned()))
    }

    /// The hexadecimal text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A source span as retained by the Compiled_Rulebook: position and document hash, no text.
/// It is plain data. The Explainer decides whether it resolves.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TraceSpan {
    /// Document identifier.
    pub document: DocumentId,
    /// Content hash of the document recorded in the Compiled_Rulebook.
    pub sha256: DocumentHash,
    /// Start byte offset (inclusive).
    pub start: u32,
    /// End byte offset (exclusive).
    pub end: u32,
    /// 1-based line of the start.
    pub line: u32,
    /// 1-based column of the start.
    pub column: u32,
}

/// A rule that applied, with every span the Compiled_Rulebook retains for it (Req 13.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceRule {
    /// Rule identifier.
    pub rule: RuleId,
    /// Revision.
    pub revision: RevisionNumber,
    /// Spans in the order the Compiled_Rulebook holds them.
    pub spans: Vec<TraceSpan>,
}

/// Why a considered rule did not apply: exactly one reason (Req 13.4, 13.21).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotAppliedReason {
    /// The named condition was not satisfied.
    ConditionNotSatisfied {
        /// Identifier or name of the condition.
        condition: String,
    },
    /// The named input was missing.
    MissingInput {
        /// Input name.
        input: String,
    },
    /// A Precedence_Declaration overrode the rule.
    Precedence {
        /// Identifier of the declaration.
        id: String,
    },
}

/// A rule that was considered and did not apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotApplied {
    /// Rule identifier.
    pub rule: RuleId,
    /// Revision.
    pub revision: RevisionNumber,
    /// All spans of the rule.
    pub spans: Vec<TraceSpan>,
    /// The single reason.
    pub reason: NotAppliedReason,
}

/// A Missing_Value that an Outcome depends on (Req 11.3).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MissingInput {
    /// Participant (record) identifier.
    pub participant: String,
    /// Field name.
    pub field: String,
}

/// Kind of question a trace answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionKind {
    /// The value of a Target for one participant.
    Value,
    /// The Rank_Position of one participant for a ranking Target.
    Rank,
}

/// The question a trace answers. The engine builds it from its own Question type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceQuestion {
    /// Kind.
    pub kind: QuestionKind,
    /// Target name.
    pub target: String,
    /// Participant identifier, when the question names one.
    pub participant: Option<String>,
}

/// An input value read (Req 13.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadEntry {
    /// Input name.
    pub name: String,
    /// Participant (record) identifier, if the input belongs to one.
    pub participant: Option<String>,
    /// The exact value.
    pub value: TraceValue,
    /// The rule that read it.
    pub rule: RuleId,
    /// Spans of the declared default when the value came from a default (Req 11.2), else empty.
    pub default_spans: Vec<TraceSpan>,
}

/// An operation that [`crate::replay`] re-derives through `rt-numeric`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepOp {
    /// `a + b`.
    Add,
    /// `a - b`.
    Sub,
    /// `a * b`.
    Mul,
    /// `a / b`.
    Div,
    /// `-a`.
    Neg,
    /// Sum of all operands in order; the empty sum is 0.
    Sum,
    /// Smallest operand; undefined when there are none.
    Min,
    /// Largest operand; undefined when there are none.
    Max,
    /// Number of operands (of any type); 0 when there are none.
    Count,
    /// One operand rounded to `places` with `mode`.
    Round {
        /// Places, 0 to 28.
        places: u32,
        /// Declared mode.
        mode: RoundingMode,
    },
    /// `a < b`.
    Lt,
    /// `a <= b`.
    Le,
    /// `a > b`.
    Gt,
    /// `a >= b`.
    Ge,
    /// `a == b`.
    Eq,
    /// `a != b`.
    Ne,
}

impl StepOp {
    /// The Rule_IR style name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            StepOp::Add => "add",
            StepOp::Sub => "sub",
            StepOp::Mul => "mul",
            StepOp::Div => "div",
            StepOp::Neg => "neg",
            StepOp::Sum => "sum",
            StepOp::Min => "min",
            StepOp::Max => "max",
            StepOp::Count => "count",
            StepOp::Round { .. } => "round",
            StepOp::Lt => "lt",
            StepOp::Le => "le",
            StepOp::Gt => "gt",
            StepOp::Ge => "ge",
            StepOp::Eq => "eq",
            StepOp::Ne => "ne",
        }
    }
}

/// A numeric failure recorded by a step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepFault {
    /// Division by zero (`RT-EXE-0002`).
    DivisionByZero,
    /// Overflow of the Exact_Number bounds (`RT-EXE-0003`).
    Overflow,
    /// Invalid numeric operand or rounding places (`RT-EXE-0005`).
    Invalid,
}

impl StepFault {
    /// The catalogue code of the fault.
    #[must_use]
    pub fn code(self) -> rt_foundation::Code {
        match self {
            StepFault::DivisionByZero => rt_foundation::Code::ExeDivisionByZero,
            StepFault::Overflow => rt_foundation::Code::ExeNumericOverflow,
            StepFault::Invalid => rt_foundation::Code::ExeNumericValueInvalid,
        }
    }
}

/// The recorded result of a step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepResult {
    /// A value.
    Value(TraceValue),
    /// `min` or `max` of an empty collection.
    Undefined,
    /// The operation failed.
    Fault(StepFault),
}

/// A calculation step (Req 13.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepEntry {
    /// Operation.
    pub op: StepOp,
    /// Operands in order.
    pub operands: Vec<TraceValue>,
    /// Recorded result.
    pub result: StepResult,
    /// The rule that performed it.
    pub rule: RuleId,
    /// Collection or field the step ranges over. Required when the result is `Undefined`.
    pub field: Option<String>,
}

/// How a decision follows from the entries it references.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionOp {
    /// One referenced entry, copied.
    Pass,
    /// Kleene AND of the referenced entries, folded left.
    And,
    /// Kleene OR of the referenced entries, folded left.
    Or,
    /// Kleene NOT of one referenced entry.
    Not,
    /// A value the engine computed from data that is not in the entries (for example a
    /// Rank_Position). Replay keeps it as recorded and does not re-derive it.
    Recorded,
}

impl DecisionOp {
    /// The serialized name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            DecisionOp::Pass => "pass",
            DecisionOp::And => "and",
            DecisionOp::Or => "or",
            DecisionOp::Not => "not",
            DecisionOp::Recorded => "recorded",
        }
    }
}

/// The value of a decision: known or Unknown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecisionValue {
    /// A known value.
    Known(TraceValue),
    /// Unknown because information is missing or a step failed.
    Unknown,
}

/// An intermediate decision (Req 13.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionEntry {
    /// The rule that produced it.
    pub rule: RuleId,
    /// How it follows from `from`.
    pub op: DecisionOp,
    /// Recorded value.
    pub value: DecisionValue,
    /// Sequence numbers (1-based) of earlier entries it is derived from.
    pub from: Vec<u32>,
}

/// The direction of a ranking criterion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RankDirection {
    /// Smaller values are ahead.
    Asc,
    /// Larger values are ahead.
    Desc,
}

impl RankDirection {
    /// The serialized name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            RankDirection::Asc => "asc",
            RankDirection::Desc => "desc",
        }
    }
}

/// How one criterion ordered the two participants of a [`CompareEntry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CriterionResult {
    /// The first participant is ahead.
    First,
    /// The second participant is ahead.
    Second,
    /// The values are equal, so the next criterion applies.
    Equal,
}

impl CriterionResult {
    /// The serialized name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            CriterionResult::First => "first",
            CriterionResult::Second => "second",
            CriterionResult::Equal => "equal",
        }
    }
}

/// One criterion applied to a pair, with both compared values (Req 12.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareCriterion {
    /// 0 for the ranking key, then 1, 2, ... for the tie-breakers in declared order.
    pub index: u32,
    /// The ranking rule that declares the criterion.
    pub rule: RuleId,
    /// The declared direction.
    pub direction: RankDirection,
    /// The value of the first participant.
    pub left: TraceValue,
    /// The value of the second participant.
    pub right: TraceValue,
    /// The recorded result, with the direction applied.
    pub result: CriterionResult,
}

/// A comparison of two participants adjacent in the final order (design §3.6.3, Ranking 6).
///
/// `criteria` lists the criteria in declared order up to and including the one that decided, or
/// all of them when the participants are tied. Replay re-derives every criterion result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareEntry {
    /// The two participants, the one that is ahead (or listed first when tied) first.
    pub between: (String, String),
    /// The criteria applied, in order.
    pub criteria: Vec<CompareCriterion>,
    /// `true` when the first participant is ahead, `false` when the two are tied.
    pub first_ahead: bool,
}

/// One trace entry, in evaluation order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// An input read.
    Read(ReadEntry),
    /// A calculation step.
    Step(StepEntry),
    /// An intermediate decision.
    Decision(DecisionEntry),
    /// A comparison of two adjacent participants of a ranking.
    Compare(CompareEntry),
    /// A position of a ranking: one participant, or one Tie_Group with its members.
    Place(Place),
}

/// The pieces of a trace before checking. Public so tests and tools can alter a trace and
/// rebuild it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceParts {
    /// The question.
    pub question: TraceQuestion,
    /// The recorded Outcome.
    pub outcome: Outcome,
    /// Rules that applied.
    pub rules: Vec<TraceRule>,
    /// Entries in evaluation order.
    pub entries: Vec<Entry>,
    /// Rules considered and not applied.
    pub not_applied: Vec<NotApplied>,
    /// Missing values the Outcome depends on.
    pub missing: Vec<MissingInput>,
    /// Diagnostics of this Outcome.
    pub diagnostics: DiagnosticState,
}

/// Why [`DecisionTrace::new`] refused its parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceError {
    /// More than `u32::MAX` entries.
    TooManyEntries,
    /// A decision refers to itself, a later entry or entry 0.
    BadReference {
        /// Sequence number of the decision.
        seq: u32,
        /// The offending reference.
        to: u32,
    },
    /// A read entry carries a value for an input listed as missing (Req 13.5).
    ReadOfMissingInput {
        /// Sequence number of the read.
        seq: u32,
    },
    /// An `Undefined` step names no collection.
    UndefinedWithoutField {
        /// Sequence number of the step.
        seq: u32,
    },
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TraceError::TooManyEntries => f.write_str("trace has more than u32::MAX entries"),
            TraceError::BadReference { seq, to } => {
                write!(
                    f,
                    "entry {seq} refers to entry {to}, which is not an earlier entry"
                )
            }
            TraceError::ReadOfMissingInput { seq } => {
                write!(
                    f,
                    "entry {seq} records a value for an input listed as missing"
                )
            }
            TraceError::UndefinedWithoutField { seq } => {
                write!(
                    f,
                    "entry {seq} has an undefined result and names no collection"
                )
            }
        }
    }
}

impl std::error::Error for TraceError {}

/// One Decision_Trace per Outcome (Req 13.1). Holds Locations and hashes, never source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionTrace {
    question: TraceQuestion,
    outcome: Outcome,
    rules: Vec<TraceRule>,
    entries: Vec<Entry>,
    not_applied: Vec<NotApplied>,
    missing: Vec<MissingInput>,
    diagnostics: DiagnosticState,
}

impl DecisionTrace {
    /// Checks the structure and puts the unordered parts in canonical order: rules and
    /// not-applied rules by (rule id, revision), missing inputs ascending without repeats,
    /// Outcome reasons canonical, diagnostics sorted. Entry order is evaluation order and is
    /// kept.
    ///
    /// # Errors
    /// [`TraceError`] for a forward `from` reference, a read of a missing input, an undefined
    /// step without a collection name, or too many entries.
    pub fn new(parts: TraceParts) -> Result<Self, TraceError> {
        let TraceParts {
            question,
            outcome,
            mut rules,
            entries,
            mut not_applied,
            mut missing,
            diagnostics,
        } = parts;
        if u32::try_from(entries.len()).is_err() {
            return Err(TraceError::TooManyEntries);
        }
        missing.sort();
        missing.dedup();
        for (index, entry) in entries.iter().enumerate() {
            let seq = u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX);
            match entry {
                Entry::Read(r) => {
                    let is_missing = missing.iter().any(|m| {
                        m.field == r.name && Some(&m.participant) == r.participant.as_ref()
                    });
                    if is_missing {
                        return Err(TraceError::ReadOfMissingInput { seq });
                    }
                }
                Entry::Step(s) => {
                    if s.result == StepResult::Undefined && s.field.is_none() {
                        return Err(TraceError::UndefinedWithoutField { seq });
                    }
                }
                Entry::Decision(d) => {
                    if let Some(&to) = d.from.iter().find(|&&to| to == 0 || to >= seq) {
                        return Err(TraceError::BadReference { seq, to });
                    }
                }
                Entry::Compare(_) | Entry::Place(_) => {}
            }
        }
        rules.sort_by(|a, b| (&a.rule, a.revision).cmp(&(&b.rule, b.revision)));
        not_applied.sort_by(|a, b| (&a.rule, a.revision).cmp(&(&b.rule, b.revision)));
        Ok(Self {
            question,
            outcome: outcome.normalised(),
            rules,
            entries,
            not_applied,
            missing,
            diagnostics: diagnostics.sorted(),
        })
    }

    /// Gives the parts back, for example to alter one and rebuild.
    #[must_use]
    pub fn into_parts(self) -> TraceParts {
        TraceParts {
            question: self.question,
            outcome: self.outcome,
            rules: self.rules,
            entries: self.entries,
            not_applied: self.not_applied,
            missing: self.missing,
            diagnostics: self.diagnostics,
        }
    }

    /// The question.
    #[must_use]
    pub fn question(&self) -> &TraceQuestion {
        &self.question
    }

    /// The recorded Outcome.
    #[must_use]
    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }

    /// Rules that applied, by (rule id, revision).
    #[must_use]
    pub fn rules(&self) -> &[TraceRule] {
        &self.rules
    }

    /// Entries in evaluation order; the n-th has sequence number n.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Rules considered and not applied, by (rule id, revision).
    #[must_use]
    pub fn not_applied(&self) -> &[NotApplied] {
        &self.not_applied
    }

    /// Missing values the Outcome depends on, ascending.
    #[must_use]
    pub fn missing(&self) -> &[MissingInput] {
        &self.missing
    }

    /// Diagnostics of this Outcome, in canonical order.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticState {
        &self.diagnostics
    }

    /// Every span of the trace: applied rules, then not-applied rules, then default spans, each
    /// in recorded order. The Explainer verifies exactly these.
    #[must_use]
    pub fn spans(&self) -> Vec<&TraceSpan> {
        let mut out: Vec<&TraceSpan> = self.rules.iter().flat_map(|r| &r.spans).collect();
        out.extend(self.not_applied.iter().flat_map(|r| &r.spans));
        for entry in &self.entries {
            if let Entry::Read(r) = entry {
                out.extend(&r.default_spans);
            }
        }
        out
    }
}
