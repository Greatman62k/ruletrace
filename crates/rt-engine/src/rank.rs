//! Ranking questions (Req 12, design §3.6.3 "Ranking").
//!
//! One [`Run`] per participant evaluates the criteria of the ranking rule. A
//! criterion is evaluated for a participant only when the participant is still
//! tied with another one after the preceding criteria (Req 12.1), and the
//! result is cached for the comparisons recorded afterwards. Participants are
//! handled by their position in the ascending identifier order, so nothing
//! depends on input order. Ties are never broken by identifier: equal
//! participants stay together and their identifiers are listed in ascending
//! order for serialization only.

use crate::index::Index;
use crate::support::{conflict, rule_location, rule_ref, trace_spans};
use crate::value::{normalise_unks, Eval, Unk, Value};
use crate::{finish, invalid, refused, state_of, Answer};
use rt_compiled::Direction;
use rt_compiled::{CompiledBody, CompiledRule, CompiledRulebook, NodeIdx, RuleIdx};
use rt_data::{CompetitionData, ParticipantId, TargetName};
use rt_foundation::{escape_controls, Code, Diagnostic};
use rt_numeric as num;
use rt_trace::{
    CompareCriterion, CompareEntry, CriterionResult, DecisionEntry, DecisionOp, DecisionValue,
    Entry, MissingInput, NotApplied, Outcome, Place, RankDirection, Reason, TraceParts,
    TraceQuestion, TraceRule, TraceValue,
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

/// The value of one criterion for one participant.
enum Slot {
    Known(Value, TraceValue),
    Lacking(Lack),
}

/// Why a criterion has no value for a participant.
struct Lack {
    unknowns: Vec<Unk>,
    faults: Vec<Diagnostic>,
}

/// A set of participants (positions in the ascending identifier order) in the
/// final order.
enum Block {
    /// Still tied after the criteria applied so far.
    Active(Vec<usize>),
    /// Equal on every criterion: one position, a Tie_Group when it has several
    /// members.
    Final(Vec<usize>),
    /// Cannot be placed because a value that a comparison needs is missing.
    Indeterminate(Vec<usize>),
}

struct Ranker<'a> {
    rulebook: &'a CompiledRulebook,
    idx: RuleIdx,
    rule: &'a CompiledRule,
    criteria: Vec<(NodeIdx, Direction)>,
    pids: Vec<&'a ParticipantId>,
    runs: Vec<crate::run::Run<'a>>,
    slots: BTreeMap<(usize, usize), Slot>,
}

fn orderable(value: &Value) -> bool {
    matches!(value, Value::Number(_) | Value::Date(_))
}

/// `Less` when `a` is ahead of `b` under `direction`.
fn ahead(direction: Direction, a: &Value, b: &Value) -> Ordering {
    let natural = match (a, b) {
        (Value::Number(x), Value::Number(y)) => num::cmp(x, y),
        (Value::Date(x), Value::Date(y)) => x.cmp(y),
        _ => Ordering::Equal,
    };
    match direction {
        Direction::Asc => natural,
        Direction::Desc => natural.reverse(),
    }
}

fn rank_direction(direction: Direction) -> RankDirection {
    match direction {
        Direction::Asc => RankDirection::Asc,
        Direction::Desc => RankDirection::Desc,
    }
}

impl<'a> Ranker<'a> {
    /// The cached value of criterion `c` for participant `p`, evaluating it on
    /// first use.
    fn slot(&mut self, c: usize, p: usize) -> Option<&Slot> {
        if !self.slots.contains_key(&(c, p)) {
            let node = self.criteria.get(c)?.0;
            let run = self.runs.get_mut(p)?;
            let val = run.criterion(self.idx, node);
            let slot = match val.eval {
                Eval::Known(v) if orderable(&v) => {
                    let shown = v.trace(self.rulebook);
                    Slot::Known(v, shown)
                }
                Eval::Known(_) => Slot::Lacking(Lack {
                    unknowns: Vec::new(),
                    faults: vec![Diagnostic::new(
                        Code::ValUnsupportedConstruct,
                        format!(
                            "rule {}: ranking criterion {c} has a value that cannot be ordered; only numbers and dates can be ranked",
                            self.rule.id
                        ),
                        rule_location(self.rule),
                    )
                    .with_rules(vec![rule_ref(self.rule)])],
                }),
                Eval::Unknown(u) => Slot::Lacking(Lack {
                    unknowns: normalise_unks(u),
                    faults: Vec::new(),
                }),
                Eval::Fault(f) => Slot::Lacking(Lack {
                    unknowns: Vec::new(),
                    faults: f,
                }),
            };
            self.slots.insert((c, p), slot);
        }
        self.slots.get(&(c, p))
    }

    fn known(&self, c: usize, p: usize) -> Option<&Value> {
        match self.slots.get(&(c, p)) {
            Some(Slot::Known(v, _)) => Some(v),
            _ => None,
        }
    }

    fn shown(&self, c: usize, p: usize) -> Option<&TraceValue> {
        match self.slots.get(&(c, p)) {
            Some(Slot::Known(_, t)) => Some(t),
            _ => None,
        }
    }

    fn direction(&self, c: usize) -> Direction {
        self.criteria.get(c).map_or(Direction::Asc, |k| k.1)
    }

    /// Evaluates criterion `c` for all of `members`; returns those that lack a
    /// value.
    fn lacking(&mut self, c: usize, members: &[usize]) -> Vec<usize> {
        let mut out = Vec::new();
        for p in members {
            if !matches!(self.slot(c, *p), Some(Slot::Known(..))) {
                out.push(*p);
            }
        }
        out
    }

    /// Splits `members`, all with a value for criterion `c`, into groups equal
    /// on `c`, ahead first. Within a group the members stay in ascending order
    /// and carry no ranking meaning.
    fn split(&self, c: usize, members: &[usize]) -> Vec<Vec<usize>> {
        let direction = self.direction(c);
        let mut sorted: Vec<usize> = members.to_vec();
        sorted.sort_by(|a, b| match (self.known(c, *a), self.known(c, *b)) {
            (Some(x), Some(y)) => ahead(direction, x, y).then(a.cmp(b)),
            _ => a.cmp(b),
        });
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for p in sorted {
            let same = groups.last().and_then(|g| g.first()).is_some_and(|first| {
                matches!((self.known(c, *first), self.known(c, p)),
                    (Some(x), Some(y)) if ahead(direction, x, y) == Ordering::Equal)
            });
            match groups.last_mut() {
                Some(g) if same => g.push(p),
                _ => groups.push(vec![p]),
            }
        }
        groups
    }

    /// The blocks in final order, and the participants that lack a needed
    /// value, by criterion.
    fn order(&mut self) -> (Vec<Block>, Vec<(usize, usize)>) {
        let everyone: Vec<usize> = (0..self.pids.len()).collect();
        let mut missing: Vec<(usize, usize)> = Vec::new();
        if everyone.is_empty() {
            return (Vec::new(), missing);
        }
        let lacking = self.lacking(0, &everyone);
        if !lacking.is_empty() {
            missing.extend(lacking.into_iter().map(|p| (0, p)));
            return (vec![Block::Indeterminate(everyone)], missing);
        }
        let mut blocks: Vec<Block> = self
            .split(0, &everyone)
            .into_iter()
            .map(Block::Active)
            .collect();
        for c in 1..self.criteria.len() {
            let mut next = Vec::with_capacity(blocks.len());
            for block in blocks {
                match block {
                    Block::Active(group) if group.len() > 1 => {
                        let lacking = self.lacking(c, &group);
                        if lacking.is_empty() {
                            next.extend(self.split(c, &group).into_iter().map(Block::Active));
                        } else {
                            missing.extend(lacking.into_iter().map(|p| (c, p)));
                            next.push(Block::Indeterminate(group));
                        }
                    }
                    other => next.push(other),
                }
            }
            blocks = next;
        }
        let blocks = blocks
            .into_iter()
            .map(|b| match b {
                Block::Active(g) => Block::Final(g),
                other => other,
            })
            .collect();
        (blocks, missing)
    }

    fn pid(&self, p: usize) -> String {
        self.pids
            .get(p)
            .map_or_else(String::new, |id| id.as_str().to_owned())
    }

    /// The comparison of two participants adjacent in the final order.
    fn compare(&self, a: usize, b: usize) -> CompareEntry {
        let mut criteria = Vec::new();
        let mut first_ahead = false;
        for c in 0..self.criteria.len() {
            let (Some(x), Some(y)) = (self.known(c, a), self.known(c, b)) else {
                break;
            };
            let (Some(left), Some(right)) = (self.shown(c, a), self.shown(c, b)) else {
                break;
            };
            let direction = self.direction(c);
            let result = match ahead(direction, x, y) {
                Ordering::Less => CriterionResult::First,
                Ordering::Greater => CriterionResult::Second,
                Ordering::Equal => CriterionResult::Equal,
            };
            criteria.push(CompareCriterion {
                index: u32::try_from(c).unwrap_or(u32::MAX),
                rule: self.rule.id.clone(),
                direction: rank_direction(direction),
                left: left.clone(),
                right: right.clone(),
                result,
            });
            if result != CriterionResult::Equal {
                first_ahead = result == CriterionResult::First;
                break;
            }
        }
        CompareEntry {
            between: (self.pid(a), self.pid(b)),
            criteria,
            first_ahead,
        }
    }

    /// The diagnostics for the participants that lack a value, in the order of
    /// discovery.
    fn diagnostics(&self, missing: &[(usize, usize)], target: &str) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for (c, p) in missing {
            let Some(Slot::Lacking(lack)) = self.slots.get(&(*c, *p)) else {
                continue;
            };
            let name = if *c == 0 {
                "the ranking key".to_owned()
            } else {
                format!("tie-breaker {c}")
            };
            let mut detail = Vec::new();
            for u in &lack.unknowns {
                match &u.reason {
                    Reason::MissingInput { field, .. } => {
                        detail.push(format!("missing input '{}'", escape_controls(field)));
                    }
                    Reason::EmptyAggregate { field } => {
                        detail.push(format!(
                            "minimum or maximum over the empty collection '{}'",
                            escape_controls(field)
                        ));
                        out.push(
                            Diagnostic::new(
                                Code::AggregateEmptyCollection,
                                format!(
                                    "rule {}: minimum or maximum over the empty collection '{}' is required for the Outcome",
                                    self.rule.id,
                                    escape_controls(field)
                                ),
                                rule_location(self.rule),
                            )
                            .with_rules(vec![rule_ref(self.rule)]),
                        );
                    }
                    Reason::Diagnosed(_) => {}
                }
            }
            if !lack.faults.is_empty() {
                detail.push("a fault in the evaluation".to_owned());
            }
            out.push(
                Diagnostic::new(
                    Code::ExeRankMissingValue,
                    format!(
                        "rule {}: participant '{}' has no value for {name} of ranking Target '{}' ({})",
                        self.rule.id,
                        escape_controls(&self.pid(*p)),
                        escape_controls(target),
                        detail.join(", ")
                    ),
                    rule_location(self.rule),
                )
                .with_rules(vec![rule_ref(self.rule)]),
            );
            out.extend(lack.faults.iter().cloned());
        }
        out
    }

    fn reasons(&self, missing: &[(usize, usize)]) -> Vec<Reason> {
        let mut out = vec![Reason::Diagnosed(Code::ExeRankMissingValue)];
        for (c, p) in missing {
            let Some(Slot::Lacking(lack)) = self.slots.get(&(*c, *p)) else {
                continue;
            };
            out.extend(lack.unknowns.iter().map(|u| u.reason.clone()));
            out.extend(lack.faults.iter().map(|d| Reason::Diagnosed(d.code())));
        }
        out
    }
}

/// Appends the entries of one participant's run, moving the references of its
/// decisions behind the entries already present.
fn merge(out: &mut Vec<Entry>, entries: Vec<Entry>) {
    let offset = u32::try_from(out.len()).unwrap_or(u32::MAX);
    for entry in entries {
        out.push(match entry {
            Entry::Decision(mut d) => {
                d.from = d.from.iter().map(|n| n.saturating_add(offset)).collect();
                Entry::Decision(d)
            }
            other => other,
        });
    }
}

/// Answers a `rank` Question.
pub(crate) fn answer(
    rulebook: &CompiledRulebook,
    index: &Index,
    data: &CompetitionData,
    position: usize,
    target: &TargetName,
    participants: Option<&BTreeSet<ParticipantId>>,
    tq: TraceQuestion,
) -> Answer {
    let mut problems = Vec::new();
    let rules = index.ranking.get(target.as_str());
    if rules.is_none() {
        problems.push(format!(
            "'{}' is not a ranking Target of the Compiled_Rulebook",
            escape_controls(target.as_str())
        ));
    }
    for p in participants.into_iter().flatten() {
        if data.participant(p).is_none() {
            problems.push(format!(
                "participant '{}' is not in the Competition_Data",
                escape_controls(p.as_str())
            ));
        }
    }
    let Some(rules) = rules.filter(|r| !r.is_empty() && problems.is_empty()) else {
        return invalid(position, tq, &problems);
    };
    let mut overridden: Option<(&crate::index::Prec, RuleIdx)> = None;
    let idx = match (index.first_listed(target.as_str(), rules), rules.as_slice()) {
        (Some((prec, first)), _) => {
            overridden = Some((prec, first));
            first
        }
        (None, [one]) => *one,
        (None, many) => {
            let found: Vec<&CompiledRule> = many.iter().filter_map(|i| rulebook.rule(*i)).collect();
            let d = conflict(
                &found,
                &format!(
                    "ranking Target '{}' has several rules and no precedence applies",
                    escape_controls(target.as_str())
                ),
            );
            return refused(tq, many, rulebook, vec![d], Code::ExecutionConflictDetected);
        }
    };
    let Some(rule) = rulebook.rule(idx) else {
        return invalid(
            position,
            tq,
            &["the ranking rule does not exist".to_owned()],
        );
    };
    let CompiledBody::Ranking {
        key,
        direction,
        tie_breakers,
        ..
    } = &rule.body
    else {
        return invalid(
            position,
            tq,
            &["the rule of a ranking Target is not a ranking".to_owned()],
        );
    };
    let mut criteria = vec![(*key, *direction)];
    criteria.extend(tie_breakers.iter().map(|t| (t.expr, t.direction)));

    let pids: Vec<&ParticipantId> = match participants {
        Some(set) => set.iter().collect(),
        None => data.participants().keys().collect(),
    };
    let mut runs = Vec::with_capacity(pids.len());
    for pid in pids.iter().copied() {
        let Some(part) = data.participant(pid) else {
            return invalid(
                position,
                tq,
                &["a participant is not in the data".to_owned()],
            );
        };
        let mut run = crate::run::Run::new(rulebook, index, pid, part);
        run.prepare(idx);
        runs.push(run);
    }
    let mut ranker = Ranker {
        rulebook,
        idx,
        rule,
        criteria,
        pids,
        runs,
        slots: BTreeMap::new(),
    };
    let (blocks, missing) = ranker.order();

    // Positions: 1 plus the number of participants strictly ahead, which for a
    // block is the number of participants in the blocks before it (Req 12.9).
    let mut places: Vec<Place> = Vec::new();
    let mut entries_after: Vec<Entry> = Vec::new();
    let mut ahead_count: usize = 0;
    let mut previous: Option<usize> = None;
    let mut any_indeterminate = false;
    for block in &blocks {
        match block {
            Block::Final(group) | Block::Active(group) => {
                for p in group {
                    if let Some(before) = previous {
                        entries_after.push(Entry::Compare(ranker.compare(before, *p)));
                    }
                    previous = Some(*p);
                }
                let at = u32::try_from(ahead_count.saturating_add(1)).unwrap_or(u32::MAX);
                if let Some(place) = Place::new(at, group.iter().map(|p| ranker.pid(*p)).collect())
                {
                    entries_after.push(Entry::Place(place.clone()));
                    places.push(place);
                }
                ahead_count = ahead_count.saturating_add(group.len());
            }
            Block::Indeterminate(group) => {
                any_indeterminate = true;
                previous = None;
                ahead_count = ahead_count.saturating_add(group.len());
            }
        }
    }

    let diagnostics = ranker.diagnostics(&missing, target.as_str());
    let outcome = if any_indeterminate {
        Outcome::indeterminate(ranker.reasons(&missing))
    } else {
        Outcome::Decided(TraceValue::Standings(places))
    };
    let value = match &outcome {
        Outcome::Decided(v) => DecisionValue::Known(v.clone()),
        Outcome::Indeterminate(_) => DecisionValue::Unknown,
    };

    let mut used: BTreeSet<RuleIdx> = BTreeSet::from([idx]);
    let mut not_applied: Vec<NotApplied> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    let mut missing_inputs: Vec<MissingInput> = Vec::new();
    if let Some((prec, first)) = overridden {
        not_applied.extend(crate::run::overridden(rulebook, prec, rules, first));
    }
    for run in ranker.runs {
        used.extend(run.used.iter().copied());
        for n in run.not_applied {
            if !not_applied.iter().any(|x| x.rule == n.rule) {
                not_applied.push(n);
            }
        }
        merge(&mut entries, run.entries);
    }
    for (c, p) in &missing {
        if let Some(Slot::Lacking(lack)) = ranker.slots.get(&(*c, *p)) {
            for u in &lack.unknowns {
                if let Reason::MissingInput { participant, field } = &u.reason {
                    missing_inputs.push(MissingInput {
                        participant: participant.clone(),
                        field: field.clone(),
                    });
                }
            }
        }
    }
    entries.extend(entries_after);
    entries.push(Entry::Decision(DecisionEntry {
        rule: rule.id.clone(),
        op: DecisionOp::Recorded,
        value,
        from: Vec::new(),
    }));
    let trace_rules = used
        .iter()
        .filter_map(|i| rulebook.rule(*i))
        .map(|r| TraceRule {
            rule: r.id.clone(),
            revision: r.revision,
            spans: trace_spans(&r.spans),
        })
        .collect();
    let parts = TraceParts {
        question: tq,
        outcome: outcome.clone(),
        rules: trace_rules,
        entries,
        not_applied,
        missing: missing_inputs,
        diagnostics: state_of(&diagnostics),
    };
    finish(parts, outcome, diagnostics)
}
