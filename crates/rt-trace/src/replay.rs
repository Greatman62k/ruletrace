//! Replay: re-derive every step and decision of a trace and compare the Outcome
//! (Req 13.11).
//!
//! Steps are recomputed from `op` and `operands` through `rt-numeric`.
//! Decisions are recomputed from the entries they reference with Kleene logic.
//! The Outcome is that of the last decision entry. Equality is exact; there is
//! no tolerance.

use crate::outcome::{Outcome, Reason, TraceValue};
use crate::trace::{
    CompareEntry, CriterionResult, DecisionEntry, DecisionOp, DecisionTrace, DecisionValue, Entry,
    RankDirection, StepFault, StepOp, StepResult,
};
use crate::tri::Tri;
use rt_foundation::{Code, Severity};
use rt_numeric::{ExactNumber, NumericError};
use std::cmp::Ordering;

/// Why a trace does not replay to its recorded results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayError {
    /// A step has the wrong number of operands for its operation.
    BadArity {
        /// Sequence number of the step or decision.
        seq: u32,
    },
    /// An operand or referenced value has the wrong type for the operation.
    BadOperand {
        /// Sequence number of the step or decision.
        seq: u32,
    },
    /// A decision refers to an entry that does not exist.
    BadReference {
        /// Sequence number of the decision.
        seq: u32,
    },
    /// The recorded step result differs from the re-derived one.
    StepMismatch {
        /// Sequence number of the step.
        seq: u32,
        /// Result recorded in the trace.
        recorded: StepResult,
        /// Result re-derived from the operands.
        derived: StepResult,
    },
    /// A ranking comparison does not follow from its recorded values and
    /// directions.
    CompareMismatch {
        /// Sequence number of the comparison.
        seq: u32,
    },
    /// The recorded decision value differs from the re-derived one.
    DecisionMismatch {
        /// Sequence number of the decision.
        seq: u32,
        /// Value recorded in the trace.
        recorded: DecisionValue,
        /// Value re-derived from the referenced entries.
        derived: DecisionValue,
    },
    /// The recorded Outcome differs from the one the last decision gives.
    OutcomeMismatch {
        /// Outcome recorded in the trace.
        recorded: Outcome,
        /// Outcome re-derived.
        derived: Outcome,
    },
    /// The trace has no decision entry, so a Decided Outcome cannot be
    /// re-derived.
    NoDecision,
    /// An Indeterminate Outcome lists no reason.
    NoReason,
    /// An Indeterminate Outcome lists a reason that nothing in the trace
    /// supports.
    UnsupportedReason {
        /// The reason.
        reason: Reason,
    },
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::BadArity { seq } => write!(f, "entry {seq}: wrong number of operands"),
            ReplayError::BadOperand { seq } => write!(f, "entry {seq}: operand of the wrong type"),
            ReplayError::BadReference { seq } => write!(f, "entry {seq}: unknown referenced entry"),
            ReplayError::StepMismatch { seq, .. } => {
                write!(f, "entry {seq}: recorded step result differs from replay")
            }
            ReplayError::CompareMismatch { seq } => {
                write!(f, "entry {seq}: recorded comparison differs from replay")
            }
            ReplayError::DecisionMismatch { seq, .. } => {
                write!(f, "entry {seq}: recorded decision differs from replay")
            }
            ReplayError::OutcomeMismatch { .. } => {
                f.write_str("recorded Outcome differs from replay")
            }
            ReplayError::NoDecision => f.write_str("trace has no decision to derive the Outcome"),
            ReplayError::NoReason => f.write_str("Indeterminate Outcome lists no reason"),
            ReplayError::UnsupportedReason { .. } => {
                f.write_str("Indeterminate Outcome lists a reason the trace does not support")
            }
        }
    }
}

impl std::error::Error for ReplayError {}

fn fault_of(error: NumericError) -> StepFault {
    match error {
        NumericError::DivisionByZero => StepFault::DivisionByZero,
        NumericError::Overflow => StepFault::Overflow,
        NumericError::InvalidLiteral(_) | NumericError::InvalidPlaces => StepFault::Invalid,
    }
}

fn number(value: &TraceValue, seq: u32) -> Result<&ExactNumber, ReplayError> {
    match value {
        TraceValue::Number(n) => Ok(n),
        _ => Err(ReplayError::BadOperand { seq }),
    }
}

fn numbers(operands: &[TraceValue], seq: u32) -> Result<Vec<ExactNumber>, ReplayError> {
    operands.iter().map(|v| number(v, seq).cloned()).collect()
}

fn lift(result: Result<ExactNumber, NumericError>) -> StepResult {
    match result {
        Ok(n) => StepResult::Value(TraceValue::Number(n)),
        Err(e) => StepResult::Fault(fault_of(e)),
    }
}

fn compare(op: StepOp, a: Ordering) -> bool {
    match op {
        StepOp::Lt => a == Ordering::Less,
        StepOp::Le => a != Ordering::Greater,
        StepOp::Gt => a == Ordering::Greater,
        StepOp::Ge => a != Ordering::Less,
        StepOp::Eq => a == Ordering::Equal,
        _ => a != Ordering::Equal,
    }
}

/// Re-derives the result of one step from its operation and operands.
fn derive_step(seq: u32, op: StepOp, operands: &[TraceValue]) -> Result<StepResult, ReplayError> {
    let binary =
        |f: fn(&ExactNumber, &ExactNumber) -> Result<ExactNumber, NumericError>| match operands {
            [a, b] => Ok(lift(f(number(a, seq)?, number(b, seq)?))),
            _ => Err(ReplayError::BadArity { seq }),
        };
    match op {
        StepOp::Add => binary(rt_numeric::add),
        StepOp::Sub => binary(rt_numeric::sub),
        StepOp::Mul => binary(rt_numeric::mul),
        StepOp::Div => binary(rt_numeric::div),
        StepOp::Neg => match operands {
            [a] => Ok(StepResult::Value(TraceValue::Number(rt_numeric::neg(
                number(a, seq)?,
            )))),
            _ => Err(ReplayError::BadArity { seq }),
        },
        StepOp::Round { places, mode } => match operands {
            [a] => Ok(lift(rt_numeric::round(number(a, seq)?, places, mode))),
            _ => Err(ReplayError::BadArity { seq }),
        },
        StepOp::Sum => Ok(lift(rt_numeric::sum(&numbers(operands, seq)?))),
        StepOp::Min => Ok(rt_numeric::min(&numbers(operands, seq)?)
            .map_or(StepResult::Undefined, |n| {
                StepResult::Value(TraceValue::Number(n))
            })),
        StepOp::Max => Ok(rt_numeric::max(&numbers(operands, seq)?)
            .map_or(StepResult::Undefined, |n| {
                StepResult::Value(TraceValue::Number(n))
            })),
        StepOp::Count => Ok(lift(rt_numeric::count(operands.len()))),
        StepOp::Lt | StepOp::Le | StepOp::Gt | StepOp::Ge | StepOp::Eq | StepOp::Ne => {
            match operands {
                [a, b] => {
                    let ord = rt_numeric::cmp(number(a, seq)?, number(b, seq)?);
                    Ok(StepResult::Value(TraceValue::Bool(compare(op, ord))))
                }
                _ => Err(ReplayError::BadArity { seq }),
            }
        }
    }
}

fn lookup(values: &[DecisionValue], seq: u32) -> Option<&DecisionValue> {
    let index = usize::try_from(seq.checked_sub(1)?).ok()?;
    values.get(index)
}

fn tri_of(value: &DecisionValue, seq: u32) -> Result<Tri, ReplayError> {
    match value {
        DecisionValue::Unknown => Ok(Tri::Unknown),
        DecisionValue::Known(TraceValue::Bool(b)) => Ok(Tri::from_bool(*b)),
        DecisionValue::Known(_) => Err(ReplayError::BadOperand { seq }),
    }
}

fn of_tri(value: Tri) -> DecisionValue {
    match value.to_bool() {
        Some(b) => DecisionValue::Known(TraceValue::Bool(b)),
        None => DecisionValue::Unknown,
    }
}

/// Re-derives a decision from the values of the entries before it.
fn derive_decision(
    seq: u32,
    d: &DecisionEntry,
    values: &[DecisionValue],
) -> Result<DecisionValue, ReplayError> {
    let refs = d
        .from
        .iter()
        .map(|&to| lookup(values, to).ok_or(ReplayError::BadReference { seq }))
        .collect::<Result<Vec<_>, _>>()?;
    match (d.op, refs.as_slice()) {
        (DecisionOp::Recorded, _) => Ok(d.value.clone()),
        (DecisionOp::Pass, [one]) => Ok((*one).clone()),
        (DecisionOp::Not, [one]) => Ok(of_tri(!tri_of(one, seq)?)),
        (DecisionOp::And | DecisionOp::Or, [first, rest @ ..]) => {
            let mut acc = tri_of(first, seq)?;
            for next in rest {
                let next = tri_of(next, seq)?;
                acc = if d.op == DecisionOp::And {
                    acc.and(next)
                } else {
                    acc.or(next)
                };
            }
            Ok(of_tri(acc))
        }
        _ => Err(ReplayError::BadArity { seq }),
    }
}

fn step_value(result: &StepResult) -> DecisionValue {
    match result {
        StepResult::Value(v) => DecisionValue::Known(v.clone()),
        StepResult::Undefined | StepResult::Fault(_) => DecisionValue::Unknown,
    }
}

/// Whether the trace supports an Indeterminate reason.
fn supported(trace: &DecisionTrace, reason: &Reason) -> bool {
    match reason {
        Reason::MissingInput { participant, field } => trace
            .missing()
            .iter()
            .any(|m| &m.participant == participant && &m.field == field),
        Reason::EmptyAggregate { field } => trace.entries().iter().any(|e| {
            matches!(e, Entry::Step(s)
                if s.result == StepResult::Undefined && s.field.as_deref() == Some(field.as_str()))
        }),
        Reason::Diagnosed(code) => {
            trace
                .diagnostics()
                .as_slice()
                .iter()
                .any(|d| d.code() == *code && d.severity() != Severity::Warning)
                && !matches!(code, Code::ExeMissingValue | Code::AggregateEmptyCollection)
        }
    }
}

/// The order of two compared values: numbers by exact value, dates by their
/// canonical text.
fn order_of(left: &TraceValue, right: &TraceValue) -> Option<Ordering> {
    match (left, right) {
        (TraceValue::Number(a), TraceValue::Number(b)) => Some(rt_numeric::cmp(a, b)),
        (TraceValue::Date(a), TraceValue::Date(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

/// Re-derives each criterion of a comparison from its values and direction.
fn check_compare(seq: u32, c: &CompareEntry) -> Result<(), ReplayError> {
    let bad = ReplayError::CompareMismatch { seq };
    let mut last = None;
    let count = c.criteria.len();
    for (i, k) in c.criteria.iter().enumerate() {
        let ordering = order_of(&k.left, &k.right).ok_or(ReplayError::BadOperand { seq })?;
        let derived = match (ordering, k.direction) {
            (Ordering::Equal, _) => CriterionResult::Equal,
            (Ordering::Less, RankDirection::Asc) | (Ordering::Greater, RankDirection::Desc) => {
                CriterionResult::First
            }
            _ => CriterionResult::Second,
        };
        // Every criterion before the last must have been equal, or the
        // comparison would have stopped there.
        let is_last = i.saturating_add(1) == count;
        if derived != k.result || (!is_last && derived != CriterionResult::Equal) {
            return Err(bad);
        }
        last = Some(derived);
    }
    let expected = match last {
        Some(CriterionResult::First) => true,
        Some(CriterionResult::Equal) => false,
        _ => return Err(bad),
    };
    if expected == c.first_ahead {
        Ok(())
    } else {
        Err(bad)
    }
}

/// Replays a trace.
///
/// Every step is re-derived through `rt-numeric` and compared with its recorded
/// result; every decision is re-derived from the entries it references (Kleene
/// logic, left to right, no short-circuit) and compared; the Outcome of the
/// last decision entry is compared with the recorded Outcome. An Indeterminate
/// Outcome must list at least one reason and every reason must be supported by
/// the trace: a missing input in `missing`, an undefined step over the named
/// collection, or a blocking Diagnostic of that code.
///
/// # Errors
/// The first [`ReplayError`], in entry order, then the Outcome checks.
pub fn replay(trace: &DecisionTrace) -> Result<Outcome, ReplayError> {
    let mut values: Vec<DecisionValue> = Vec::with_capacity(trace.entries().len());
    let mut last_decision: Option<DecisionValue> = None;
    for (index, entry) in trace.entries().iter().enumerate() {
        let seq = u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX);
        let value = match entry {
            Entry::Read(r) => DecisionValue::Known(r.value.clone()),
            Entry::Step(s) => {
                let derived = derive_step(seq, s.op, &s.operands)?;
                if derived != s.result {
                    return Err(ReplayError::StepMismatch {
                        seq,
                        recorded: s.result.clone(),
                        derived,
                    });
                }
                step_value(&s.result)
            }
            Entry::Compare(c) => {
                check_compare(seq, c)?;
                DecisionValue::Unknown
            }
            Entry::Place(_) => DecisionValue::Unknown,
            Entry::Decision(d) => {
                let derived = derive_decision(seq, d, &values)?;
                if derived != d.value {
                    return Err(ReplayError::DecisionMismatch {
                        seq,
                        recorded: d.value.clone(),
                        derived,
                    });
                }
                last_decision = Some(d.value.clone());
                d.value.clone()
            }
        };
        values.push(value);
    }
    let recorded = trace.outcome();
    let derived = match (last_decision, recorded) {
        (Some(DecisionValue::Known(v)), _) => Outcome::Decided(v),
        (Some(DecisionValue::Unknown), Outcome::Indeterminate(r))
        | (None, Outcome::Indeterminate(r)) => Outcome::indeterminate(r.clone()),
        (Some(DecisionValue::Unknown), Outcome::Decided(_)) => {
            return Err(ReplayError::OutcomeMismatch {
                recorded: recorded.clone(),
                derived: Outcome::Indeterminate(Vec::new()),
            });
        }
        (None, Outcome::Decided(_)) => return Err(ReplayError::NoDecision),
    };
    if &derived != recorded {
        return Err(ReplayError::OutcomeMismatch {
            recorded: recorded.clone(),
            derived,
        });
    }
    if let Outcome::Indeterminate(reasons) = &derived {
        if reasons.is_empty() {
            return Err(ReplayError::NoReason);
        }
        if let Some(bad) = reasons.iter().find(|r| !supported(trace, r)) {
            return Err(ReplayError::UnsupportedReason {
                reason: bad.clone(),
            });
        }
    }
    Ok(derived)
}
