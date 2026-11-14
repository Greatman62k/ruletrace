//! Comparison of the computed Outcomes with a Published_Result (Req 13.12,
//! 13.20, 8.26).
//!
//! Equality is exact: numbers are equal when `rt_numeric::cmp` says so,
//! Booleans when they are the same Boolean. There is no tolerance and nothing
//! is rounded. Differences are listed in participant then Target order. The two
//! extra difference kinds for published ties belong to task 9.2.

use rt_data::{
    CompetitionData, ParticipantId, PublishedResult, PublishedValue, Question, TargetName,
};
use rt_numeric::{cmp, count, to_canonical, ExactNumber};
use rt_trace::{DecisionTrace, Entry, Outcome, TraceValue};
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// What the engine computed for one published entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Computed {
    /// A definite value (a Boolean, a number, or a Rank_Position as a number).
    Value(TraceValue),
    /// The Outcome is Indeterminate, so it cannot equal any published value.
    Indeterminate,
}

/// One published value that does not equal the computed one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    /// The participant.
    pub participant: ParticipantId,
    /// The Target.
    pub target: TargetName,
    /// The published value.
    pub published: PublishedValue,
    /// The computed value.
    pub computed: Computed,
    /// Index of the Question (and of its Outcome) that gave the computed value.
    pub question: usize,
    /// Sequence number, in the Decision_Trace of that Question, of the entry
    /// that produced the computed value: the final decision of a value
    /// Question, the position entry of a rank Question. `None` when the
    /// Question has no trace.
    pub entry: Option<u32>,
    /// Whether the computed number has a non-terminating decimal expansion (Req
    /// 8.26).
    pub non_terminating: bool,
    /// The statement of the difference, with the non-terminating note where it
    /// applies.
    pub message: String,
}

/// The result of comparing with a Published_Result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comparison {
    /// The published entries that a Question answers, in participant then
    /// Target order.
    pub compared: Vec<(ParticipantId, TargetName)>,
    /// The published entries that no Question answers. They are not compared.
    pub unused: Vec<(ParticipantId, TargetName)>,
    /// The published values that differ, in participant then Target order.
    pub differences: Vec<Difference>,
}

impl Comparison {
    /// Whether every compared published value equals the computed one. Derived
    /// from the differences only.
    #[must_use]
    pub fn is_equal(&self) -> bool {
        self.differences.is_empty()
    }
}

struct Found {
    question: usize,
    computed: Computed,
    entry: Option<u32>,
}

fn seq_of(index: usize) -> Option<u32> {
    u32::try_from(index.saturating_add(1)).ok()
}

/// Whether the canonical text of `n` is a fraction whose reduced denominator
/// has a prime factor other than 2 and 5, so that its decimal expansion does
/// not terminate.
fn non_terminating(n: &ExactNumber) -> bool {
    let text = to_canonical(n);
    let Some((_, denominator)) = text.split_once('/') else {
        return false;
    };
    let Ok(mut d) = denominator.parse::<u128>() else {
        // A denominator beyond u128 is not terminating unless it is a power of
        // 2 and 5, which the Exact_Number bounds do not allow; state the
        // fraction as non-terminating.
        return true;
    };
    for p in [2_u128, 5] {
        while d != 0 && d % p == 0 {
            d /= p;
        }
    }
    d != 1
}

fn shown(value: &Computed) -> String {
    match value {
        Computed::Value(TraceValue::Number(n)) => to_canonical(n),
        Computed::Value(TraceValue::Bool(b)) => b.to_string(),
        Computed::Value(_) => "a value of another type".to_owned(),
        Computed::Indeterminate => "Indeterminate".to_owned(),
    }
}

fn shown_published(value: &PublishedValue) -> String {
    match value {
        PublishedValue::Bool(b) => b.to_string(),
        PublishedValue::Number(n) => to_canonical(n.value()),
    }
}

fn equal(published: &PublishedValue, computed: &Computed) -> bool {
    match (published, computed) {
        (PublishedValue::Bool(p), Computed::Value(TraceValue::Bool(c))) => p == c,
        (PublishedValue::Number(p), Computed::Value(TraceValue::Number(c))) => {
            cmp(p.value(), c) == Ordering::Equal
        }
        _ => false,
    }
}

/// What each Question computed, by (participant, Target). The first Question
/// that answers a pair wins, so a repeated Question cannot change the
/// comparison.
fn computed_values(
    data: &CompetitionData,
    questions: &[Question],
    outcomes: &[Outcome],
    traces: &[DecisionTrace],
    trace_of: &[Option<usize>],
) -> BTreeMap<(ParticipantId, TargetName), Found> {
    let mut out: BTreeMap<(ParticipantId, TargetName), Found> = BTreeMap::new();
    for (qi, question) in questions.iter().enumerate() {
        let Some(outcome) = outcomes.get(qi) else {
            continue;
        };
        let trace = trace_of
            .get(qi)
            .copied()
            .flatten()
            .and_then(|t| traces.get(t));
        let last = trace
            .filter(|t| !t.entries().is_empty())
            .and_then(|t| seq_of(t.entries().len().saturating_sub(1)));
        match question {
            Question::Value {
                target,
                participant,
            } => {
                let computed = match outcome {
                    Outcome::Decided(v) => Computed::Value(v.clone()),
                    Outcome::Indeterminate(_) => Computed::Indeterminate,
                };
                out.entry((participant.clone(), target.clone()))
                    .or_insert(Found {
                        question: qi,
                        computed,
                        entry: last,
                    });
            }
            Question::Rank {
                target,
                participants,
            } => {
                let ids: Vec<&ParticipantId> = match participants {
                    Some(set) => set.iter().collect(),
                    None => data.participants().keys().collect(),
                };
                for id in ids {
                    let place = trace.and_then(|t| {
                        t.entries().iter().enumerate().find_map(|(i, e)| match e {
                            Entry::Place(p) if p.members().iter().any(|m| m == id.as_str()) => {
                                Some((i, p.position()))
                            }
                            _ => None,
                        })
                    });
                    let found = match place.and_then(|(i, pos)| {
                        let n = count(usize::try_from(pos).ok()?).ok()?;
                        Some((seq_of(i), n))
                    }) {
                        Some((entry, n)) => Found {
                            question: qi,
                            computed: Computed::Value(TraceValue::Number(n)),
                            entry,
                        },
                        None => Found {
                            question: qi,
                            computed: Computed::Indeterminate,
                            entry: last,
                        },
                    };
                    out.entry((id.clone(), target.clone())).or_insert(found);
                }
            }
        }
    }
    out
}

/// Compares `published` with the computed results.
pub(crate) fn compare(
    published: &PublishedResult,
    data: &CompetitionData,
    questions: &[Question],
    outcomes: &[Outcome],
    traces: &[DecisionTrace],
    trace_of: &[Option<usize>],
) -> Comparison {
    let computed = computed_values(data, questions, outcomes, traces, trace_of);
    let mut result = Comparison::default();
    for ((participant, target), value) in published.entries() {
        let key = (participant.clone(), target.clone());
        let Some(found) = computed.get(&key) else {
            result.unused.push(key);
            continue;
        };
        result.compared.push(key);
        if equal(value, &found.computed) {
            continue;
        }
        let fraction = match (value, &found.computed) {
            (PublishedValue::Number(_), Computed::Value(TraceValue::Number(n))) => {
                non_terminating(n)
            }
            _ => false,
        };
        let mut message = format!(
            "participant '{}', target '{}': published {} differs from computed {}",
            rt_foundation::escape_controls(participant.as_str()),
            rt_foundation::escape_controls(target.as_str()),
            shown_published(value),
            shown(&found.computed)
        );
        if fraction {
            message.push_str(
                "; the computed value is non-terminating and the Rule declares no rounding, or a rounding that yields another value",
            );
        }
        result.differences.push(Difference {
            participant: participant.clone(),
            target: target.clone(),
            published: value.clone(),
            computed: found.computed.clone(),
            question: found.question,
            entry: found.entry,
            non_terminating: fraction,
            message,
        });
    }
    result
}
