//! The Rules_Engine: a pure function from a Compiled_Rulebook, Competition_Data
//! and Questions to Outcomes and Decision_Traces (design §3.6.3, Req 8, 10,
//! 11).
//!
//! * The engine takes in-memory typed values only. It reads no file,
//!   environment, clock, random source or source text, keeps no global state
//!   and uses only ordered collections, so equal inputs give equal outputs in
//!   any order and next to any other rulebook (Req 10.2 to 10.8).
//! * Numbers are `rt_numeric::ExactNumber`. Division, overflow and invalid
//!   numeric data are reported as Diagnostics (`RT-EXE-0002`, `0003`, `0005`);
//!   nothing wraps, saturates or rounds except where a rule declares rounding.
//! * A field is Missing only when absent or `null`. Zero, `false` and `""` are
//!   present. A declared default supplies a missing field and its spans are
//!   recorded in the trace (Req 11.2). Any other missing input makes the result
//!   Unknown, and Boolean logic is Kleene three-valued: every operand is
//!   evaluated left to right and nothing short-circuits (Req 11.4).
//! * `count` and `sum` of an empty collection are 0. `min` and `max` of an
//!   empty collection are Unknown, and only an Outcome that depends on them
//!   reports `RT-DGN-0005` (Req 8.29, 8.30). A collection the data does not
//!   supply is missing information.
//! * Every Outcome has one Decision_Trace that `rt_trace::replay` re-derives.
//!
//! * Ranking questions follow Req 12: every criterion has a declared direction,
//!   tie-breakers are applied in declared order to the participants still tied,
//!   equal participants form one Tie_Group, and
//!   `Rank_Position = 1 + participants strictly ahead`. A missing value that a
//!   comparison needs makes the whole set it belongs to Indeterminate
//!   (`RT-EXE-0006`).
//! * Several rules for one Target use only the first-listed rule of a
//!   precedence declaration and record the others as considered and not
//!   applied. Without a declaration, unequal Known results are one
//!   `RT-DGN-0004` and no result is selected (Req 12.5, 12.10, 12.11).
//! * A supplied Published_Result is compared by exact equality (no tolerance),
//!   and the differences are listed in participant then Target order (Req
//!   13.12).
#![forbid(unsafe_code)]

mod compare;
mod index;
mod rank;
mod run;
mod support;
mod value;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_published_ranks;
#[cfg(test)]
mod tests_rank;

use crate::index::Index;
use crate::run::{Cx, Run};
use crate::support::{named_input, rule_location, rule_ref, trace_spans};
use crate::value::{normalise_unks, Eval, Unk, Val};
use rt_compiled::{CompiledRulebook, RuleIdx};
use rt_data::{CompetitionData, PublishedResult, Question};
use rt_foundation::{escape_controls, Code, Diagnostic, DiagnosticState};
use rt_trace::{
    DecisionEntry, DecisionOp, DecisionValue, Entry, MissingInput, QuestionKind, TraceParts,
    TraceQuestion, TraceRule,
};
use std::collections::BTreeSet;

pub use crate::compare::{Comparison, Computed, Difference, DifferenceKind};
pub use rt_trace::{DecisionTrace, Outcome, Reason, TraceValue};

/// The result of one evaluation. `outcomes[i]` and `traces[i]` answer
/// `questions[i]`.
///
/// There is no stored validity flag: whether the evaluation had errors is
/// derived from `diagnostics`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evaluation {
    /// One Outcome per Question, in the order of the Questions.
    pub outcomes: Vec<Outcome>,
    /// One Decision_Trace per Outcome, in the same order.
    pub traces: Vec<DecisionTrace>,
    /// Every Diagnostic of the evaluation, without repeats, in canonical order.
    pub diagnostics: DiagnosticState,
    /// The comparison with the Published_Result, `None` when none was supplied.
    pub comparison: Option<Comparison>,
}

struct Answer {
    outcome: Outcome,
    trace: Option<DecisionTrace>,
    diagnostics: Vec<Diagnostic>,
}

/// Evaluates `questions` against `data` with the rules of `rulebook`.
///
/// This is a pure function of its arguments. A fault in one Question (division
/// by zero, overflow, a data value of the wrong type, a missing input) makes
/// only the Questions that depend on it Indeterminate; the others are unchanged
/// (Req 10.6, 11.6).
///
/// When `published` is supplied it is compared with the computed Outcomes by
/// exact equality and the result is in [`Evaluation::comparison`]. The
/// comparison never changes an Outcome.
#[must_use]
pub fn evaluate(
    rulebook: &CompiledRulebook,
    data: &CompetitionData,
    questions: &[Question],
    published: Option<&PublishedResult>,
) -> Evaluation {
    let index = Index::new(rulebook);
    let mut outcomes = Vec::with_capacity(questions.len());
    let mut traces = Vec::with_capacity(questions.len());
    let mut trace_of: Vec<Option<usize>> = Vec::with_capacity(questions.len());
    let mut all: BTreeSet<Diagnostic> = BTreeSet::new();
    for (i, question) in questions.iter().enumerate() {
        let answer = answer(rulebook, &index, data, i, question);
        outcomes.push(answer.outcome);
        // A trace is always buildable here: the parts have no entry that refers
        // forward.
        if let Some(trace) = answer.trace {
            trace_of.push(Some(traces.len()));
            traces.push(trace);
        } else {
            trace_of.push(None);
        }
        all.extend(answer.diagnostics);
    }
    let mut diagnostics = DiagnosticState::empty();
    for d in all {
        diagnostics.push(d);
    }
    let comparison =
        published.map(|p| compare::compare(p, data, questions, &outcomes, &traces, &trace_of));
    Evaluation {
        outcomes,
        traces,
        diagnostics: diagnostics.sorted(),
        comparison,
    }
}

fn trace_question(question: &Question) -> TraceQuestion {
    match question {
        Question::Value {
            target,
            participant,
        } => TraceQuestion {
            kind: QuestionKind::Value,
            target: target.as_str().to_owned(),
            participant: Some(participant.as_str().to_owned()),
        },
        Question::Rank { target, .. } => TraceQuestion {
            kind: QuestionKind::Rank,
            target: target.as_str().to_owned(),
            participant: None,
        },
    }
}

fn answer(
    rulebook: &CompiledRulebook,
    index: &Index,
    data: &CompetitionData,
    position: usize,
    question: &Question,
) -> Answer {
    let tq = trace_question(question);
    match question {
        Question::Value {
            target,
            participant,
        } => {
            let mut problems = Vec::new();
            let part = data.participant(participant);
            if part.is_none() {
                problems.push(format!(
                    "participant '{}' is not in the Competition_Data",
                    escape_controls(participant.as_str())
                ));
            }
            let rules = index.value.get(target.as_str()).map(Vec::as_slice);
            if rules.is_none() {
                problems.push(format!(
                    "'{}' is not a value Target of the Compiled_Rulebook",
                    escape_controls(target.as_str())
                ));
            }
            match (part, rules) {
                (Some(part), Some(rules)) if !rules.is_empty() => {
                    let run = Run::new(rulebook, index, participant, part);
                    value_answer(run, index, target.as_str(), rules, tq)
                }
                _ => invalid(position, tq, &problems),
            }
        }
        Question::Rank {
            target,
            participants,
        } => rank::answer(
            rulebook,
            index,
            data,
            position,
            target,
            participants.as_ref(),
            tq,
        ),
    }
}

/// Answers a value question over the rules of its Target (Req 12.5, 12.10,
/// 12.11).
fn value_answer(
    mut run: Run<'_>,
    index: &Index,
    name: &str,
    rules: &[RuleIdx],
    tq: TraceQuestion,
) -> Answer {
    if let Some((prec, first)) = index.first_listed(name, rules) {
        run.run(first);
        run.note_overridden(prec, rules, first);
        let result = run.done.get(&first).cloned();
        return conclude(run, first, result, tq);
    }
    for r in rules {
        run.run(*r);
    }
    match rules {
        [one] => {
            let result = run.done.get(one).cloned();
            conclude(run, *one, result, tq)
        }
        [first, ..] => {
            let result = run.combine(name, rules);
            let conflicted = matches!(&result.eval, Eval::Fault(found)
                if found.iter().any(|d| d.code() == Code::ExecutionConflictDetected));
            if conflicted {
                // Each incompatible rule and its result are recorded; none is
                // marked applied.
                for r in rules {
                    run.record(*r);
                }
            }
            conclude(run, *first, Some(result), tq)
        }
        [] => invalid(0, tq, &["the Target has no rule".to_owned()]),
    }
}

/// `RT-EXE-0010`: the Question names something the inputs do not have.
fn invalid(position: usize, question: TraceQuestion, problems: &[String]) -> Answer {
    let diagnostics: Vec<Diagnostic> = problems
        .iter()
        .map(|p| {
            Diagnostic::new(
                Code::QuestionInvalid,
                format!("question {}: {p}", position.saturating_add(1)),
                named_input("questions", &format!("questions[{position}]")),
            )
        })
        .collect();
    let outcome = Outcome::indeterminate(vec![Reason::Diagnosed(Code::QuestionInvalid)]);
    finish(
        TraceParts {
            question,
            outcome: outcome.clone(),
            rules: Vec::new(),
            entries: Vec::new(),
            not_applied: Vec::new(),
            missing: Vec::new(),
            diagnostics: state_of(&diagnostics),
        },
        outcome,
        diagnostics,
    )
}

/// An Indeterminate answer without evaluation: no rule result is selected or
/// computed.
fn refused(
    question: TraceQuestion,
    rules: &[RuleIdx],
    rulebook: &CompiledRulebook,
    diagnostics: Vec<Diagnostic>,
    code: Code,
) -> Answer {
    let outcome = Outcome::indeterminate(vec![Reason::Diagnosed(code)]);
    let rules = rules
        .iter()
        .filter_map(|i| rulebook.rule(*i))
        .map(|r| TraceRule {
            rule: r.id.clone(),
            revision: r.revision,
            spans: trace_spans(&r.spans),
        })
        .collect();
    finish(
        TraceParts {
            question,
            outcome: outcome.clone(),
            rules,
            entries: Vec::new(),
            not_applied: Vec::new(),
            missing: Vec::new(),
            diagnostics: state_of(&diagnostics),
        },
        outcome,
        diagnostics,
    )
}

fn state_of(diagnostics: &[Diagnostic]) -> DiagnosticState {
    let mut state = DiagnosticState::empty();
    for d in diagnostics {
        state.push(d.clone());
    }
    state
}

fn finish(parts: TraceParts, outcome: Outcome, diagnostics: Vec<Diagnostic>) -> Answer {
    Answer {
        outcome,
        trace: DecisionTrace::new(parts).ok(),
        diagnostics,
    }
}

/// The diagnostics for the Unknown reasons an Indeterminate Outcome depends on.
fn unknown_diagnostics(rulebook: &CompiledRulebook, unks: &[Unk]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for u in unks {
        let Some(rule) = rulebook.rule(u.rule) else {
            continue;
        };
        let (code, message) = match &u.reason {
            Reason::MissingInput { participant, field } => (
                Code::ExeMissingValue,
                format!(
                    "rule {}: required input '{}' of participant '{}' is a Missing_Value",
                    rule.id,
                    escape_controls(field),
                    escape_controls(participant)
                ),
            ),
            Reason::EmptyAggregate { field } => (
                Code::AggregateEmptyCollection,
                format!(
                    "rule {}: minimum or maximum over the empty collection '{}' is required for the Outcome",
                    rule.id,
                    escape_controls(field)
                ),
            ),
            Reason::Diagnosed(_) => continue,
        };
        out.push(
            Diagnostic::new(code, message, rule_location(rule)).with_rules(vec![rule_ref(rule)]),
        );
    }
    out
}

/// Turns the finished run of `root` into the Outcome and Decision_Trace of the
/// question.
fn conclude(
    mut run: Run<'_>,
    root: RuleIdx,
    result: Option<Val>,
    question: TraceQuestion,
) -> Answer {
    let rulebook = run.rulebook;
    let Some(rule) = rulebook.rule(root) else {
        return invalid(
            0,
            question,
            &["the rule to evaluate does not exist".to_owned()],
        );
    };
    let cx = Cx { idx: root, rule };
    let (outcome, diagnostics, missing, value, from) = match result {
        Some(val) => match val.eval {
            Eval::Known(v) => {
                let tv = v.trace(rulebook);
                (
                    Outcome::Decided(tv.clone()),
                    Vec::new(),
                    Vec::new(),
                    DecisionValue::Known(tv),
                    val.seq.into_iter().collect::<Vec<u32>>(),
                )
            }
            Eval::Unknown(unks) => {
                let unks = normalise_unks(unks);
                let reasons: Vec<Reason> = unks.iter().map(|u| u.reason.clone()).collect();
                let missing = reasons
                    .iter()
                    .filter_map(|r| match r {
                        Reason::MissingInput { participant, field } => Some(MissingInput {
                            participant: participant.clone(),
                            field: field.clone(),
                        }),
                        _ => None,
                    })
                    .collect();
                (
                    Outcome::indeterminate(reasons),
                    unknown_diagnostics(rulebook, &unks),
                    missing,
                    DecisionValue::Unknown,
                    Vec::new(),
                )
            }
            Eval::Fault(found) => {
                let mut found = found;
                found.sort();
                found.dedup();
                let reasons = found.iter().map(|d| Reason::Diagnosed(d.code())).collect();
                (
                    Outcome::indeterminate(reasons),
                    found,
                    Vec::new(),
                    DecisionValue::Unknown,
                    Vec::new(),
                )
            }
        },
        None => {
            let d = Diagnostic::new(
                Code::CmpDependencyCycle,
                format!("rule {} depends on itself", rule.id),
                rule_location(rule),
            );
            (
                Outcome::indeterminate(vec![Reason::Diagnosed(Code::CmpDependencyCycle)]),
                vec![d],
                Vec::new(),
                DecisionValue::Unknown,
                Vec::new(),
            )
        }
    };
    let op = if matches!(value, DecisionValue::Known(_)) {
        DecisionOp::Pass
    } else {
        DecisionOp::Recorded
    };
    run.entries.push(Entry::Decision(DecisionEntry {
        rule: cx.rule.id.clone(),
        op,
        value,
        from,
    }));
    let rules = run
        .used
        .iter()
        .filter_map(|i| rulebook.rule(*i))
        .map(|r| TraceRule {
            rule: r.id.clone(),
            revision: r.revision,
            spans: trace_spans(&r.spans),
        })
        .collect();
    let parts = TraceParts {
        question,
        outcome: outcome.clone(),
        rules,
        entries: run.entries,
        not_applied: run.not_applied,
        missing,
        diagnostics: state_of(&diagnostics),
    };
    finish(parts, outcome, diagnostics)
}

#[cfg(test)]
#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;
