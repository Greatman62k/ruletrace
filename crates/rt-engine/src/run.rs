//! Evaluation of the rules a question depends on, for one participant.
//!
//! One [`Run`] builds one Decision_Trace. Every node of a rule is evaluated exactly once, in
//! post-order with operands left to right and no short-circuit, so the order of the trace
//! entries is fixed. Graph walks use explicit stacks, so the depth of a hostile artifact cannot
//! exhaust the call stack.

use crate::index::{Index, Prec};
use crate::support::{conflict, input_location, rule_location, rule_ref, trace_spans};
use crate::value::{convert, faults_of, show, unknowns_of, Eval, Refusal, Unk, Val, Value};
use rt_compiled::{
    CType, CollectionIdx, CompiledBody, CompiledRule, CompiledRulebook, CompiledTerm, FieldIdx,
    InputIdx, NodeIdx, NodeKind, RuleIdx, SetIdx, TargetIdx,
};
use rt_data::{Participant, ParticipantId};
use rt_foundation::{escape_controls, Code, Diagnostic, Location};
use rt_ir::{AggFn, BinaryOp, CmpOp, Rounding};
use rt_numeric as num;
use rt_numeric::{ExactNumber, NumericError};
use rt_trace::{
    DecisionEntry, DecisionOp, DecisionValue, Entry, NotApplied, NotAppliedReason, ReadEntry,
    Reason, StepEntry, StepFault, StepOp, StepResult, TraceValue, Tri,
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

/// A rule together with its index.
#[derive(Clone, Copy)]
pub(crate) struct Cx<'r> {
    pub(crate) idx: RuleIdx,
    pub(crate) rule: &'r CompiledRule,
}

type Vals = BTreeMap<NodeIdx, Val>;

pub(crate) struct Run<'a> {
    pub(crate) rulebook: &'a CompiledRulebook,
    index: &'a Index,
    participant_id: &'a ParticipantId,
    participant: &'a Participant,
    pub(crate) entries: Vec<Entry>,
    pub(crate) used: BTreeSet<RuleIdx>,
    pub(crate) done: BTreeMap<RuleIdx, Val>,
    pub(crate) not_applied: Vec<NotApplied>,
}

fn children(kind: &NodeKind) -> Vec<NodeIdx> {
    match kind {
        NodeKind::Literal(_)
        | NodeKind::Field(_)
        | NodeKind::Ref(_)
        | NodeKind::RankReference(_)
        | NodeKind::Agg { .. } => Vec::new(),
        NodeKind::Binary { left, right, .. } | NodeKind::Cmp { left, right, .. } => {
            vec![*left, *right]
        }
        NodeKind::And(args) | NodeKind::Or(args) => args.clone(),
        NodeKind::Not(arg) | NodeKind::Round { arg, .. } => vec![*arg],
        NodeKind::In { value, .. } => vec![*value],
    }
}

/// The root nodes of a rule body, in the order in which they are evaluated.
fn roots(body: &CompiledBody) -> Vec<NodeIdx> {
    match body {
        CompiledBody::Eligibility { expr } | CompiledBody::Derived { expr } => vec![*expr],
        CompiledBody::WeightedScore { terms, .. } => {
            terms.iter().flat_map(|t| [t.weight, t.value]).collect()
        }
        CompiledBody::Ranking {
            key, tie_breakers, ..
        } => std::iter::once(*key)
            .chain(tie_breakers.iter().map(|t| t.expr))
            .collect(),
        CompiledBody::Default { .. } => Vec::new(),
    }
}

/// The nodes reachable from `roots`, children before parents, operands left to right, each once.
fn reachable(rule: &CompiledRule, roots: &[NodeIdx]) -> Vec<NodeIdx> {
    let mut order = Vec::new();
    let mut seen: BTreeSet<NodeIdx> = BTreeSet::new();
    for &root in roots {
        if !seen.insert(root) {
            continue;
        }
        let first = rule
            .nodes
            .get(root.at())
            .map(|n| children(&n.kind))
            .unwrap_or_default();
        let mut stack: Vec<(NodeIdx, Vec<NodeIdx>, usize)> = vec![(root, first, 0)];
        while let Some(top) = stack.last_mut() {
            let next = top.1.get(top.2).copied();
            top.2 = top.2.saturating_add(1);
            match next {
                Some(child) => {
                    if seen.insert(child) {
                        let kids = rule
                            .nodes
                            .get(child.at())
                            .map(|n| children(&n.kind))
                            .unwrap_or_default();
                        stack.push((child, kids, 0));
                    }
                }
                None => {
                    order.push(top.0);
                    stack.pop();
                }
            }
        }
    }
    order
}

/// The rules a rule reads through `ref` nodes: the first-listed rule when a precedence applies,
/// else every rule of the Target.
fn dependencies(rulebook: &CompiledRulebook, index: &Index, rule: &CompiledRule) -> Vec<RuleIdx> {
    let mut out = Vec::new();
    for node in reachable(rule, &roots(&rule.body)) {
        let Some(NodeKind::Ref(t)) = rule.nodes.get(node.at()).map(|n| &n.kind) else {
            continue;
        };
        if let Some(name) = rulebook.target(*t) {
            out.extend(index.sources(name.as_str()));
        }
    }
    out
}

/// The rules to evaluate for `start`, dependencies first. A dependency cycle (which the Compiler
/// never produces) is cut here and reported when the cut rule is read.
pub(crate) fn plan(rulebook: &CompiledRulebook, index: &Index, start: RuleIdx) -> Vec<RuleIdx> {
    let mut order = Vec::new();
    let mut seen: BTreeSet<RuleIdx> = BTreeSet::new();
    let deps_of = |r: RuleIdx| {
        rulebook
            .rule(r)
            .map(|rule| dependencies(rulebook, index, rule))
            .unwrap_or_default()
    };
    seen.insert(start);
    let mut stack: Vec<(RuleIdx, Vec<RuleIdx>, usize)> = vec![(start, deps_of(start), 0)];
    while let Some(top) = stack.last_mut() {
        let next = top.1.get(top.2).copied();
        top.2 = top.2.saturating_add(1);
        match next {
            Some(dep) => {
                if seen.insert(dep) {
                    stack.push((dep, deps_of(dep), 0));
                }
            }
            None => {
                order.push(top.0);
                stack.pop();
            }
        }
    }
    order
}

/// The rules that `prec` lists after `first`, as considered and not applied.
pub(crate) fn overridden(
    rulebook: &CompiledRulebook,
    prec: &Prec,
    rules: &[RuleIdx],
    first: RuleIdx,
) -> Vec<NotApplied> {
    prec.order
        .iter()
        .filter(|idx| **idx != first && rules.contains(idx))
        .filter_map(|idx| rulebook.rule(*idx))
        .map(|rule| NotApplied {
            rule: rule.id.clone(),
            revision: rule.revision,
            spans: trace_spans(&rule.spans),
            reason: NotAppliedReason::Precedence {
                id: prec.id.clone(),
            },
        })
        .collect()
}

fn step_fault(error: NumericError) -> StepFault {
    match error {
        NumericError::Overflow => StepFault::Overflow,
        NumericError::DivisionByZero => StepFault::DivisionByZero,
        NumericError::InvalidLiteral(_) | NumericError::InvalidPlaces => StepFault::Invalid,
    }
}

fn lift(result: Result<ExactNumber, NumericError>) -> StepResult {
    match result {
        Ok(n) => StepResult::Value(TraceValue::Number(n)),
        Err(e) => StepResult::Fault(step_fault(e)),
    }
}

fn holds(op: CmpOp, ordering: Ordering) -> bool {
    match op {
        CmpOp::Eq => ordering == Ordering::Equal,
        CmpOp::Ne => ordering != Ordering::Equal,
        CmpOp::Lt => ordering == Ordering::Less,
        CmpOp::Le => ordering != Ordering::Greater,
        CmpOp::Gt => ordering == Ordering::Greater,
        CmpOp::Ge => ordering != Ordering::Less,
    }
}

/// Recomputes a numeric step exactly as `rt_trace::replay` will.
fn compute(op: StepOp, nums: &[ExactNumber]) -> Option<StepResult> {
    let bool_of = |v: bool| StepResult::Value(TraceValue::Bool(v));
    Some(match (op, nums) {
        (StepOp::Add, [a, b]) => lift(num::add(a, b)),
        (StepOp::Sub, [a, b]) => lift(num::sub(a, b)),
        (StepOp::Mul, [a, b]) => lift(num::mul(a, b)),
        (StepOp::Div, [a, b]) => lift(num::div(a, b)),
        (StepOp::Round { places, mode }, [a]) => lift(num::round(a, places, mode)),
        (StepOp::Sum, all) => lift(num::sum(all)),
        (StepOp::Min, all) => num::min(all).map_or(StepResult::Undefined, |n| {
            StepResult::Value(TraceValue::Number(n))
        }),
        (StepOp::Max, all) => num::max(all).map_or(StepResult::Undefined, |n| {
            StepResult::Value(TraceValue::Number(n))
        }),
        (StepOp::Lt, [a, b]) => bool_of(num::cmp(a, b) == Ordering::Less),
        (StepOp::Le, [a, b]) => bool_of(num::cmp(a, b) != Ordering::Greater),
        (StepOp::Gt, [a, b]) => bool_of(num::cmp(a, b) == Ordering::Greater),
        (StepOp::Ge, [a, b]) => bool_of(num::cmp(a, b) != Ordering::Less),
        (StepOp::Eq, [a, b]) => bool_of(num::cmp(a, b) == Ordering::Equal),
        (StepOp::Ne, [a, b]) => bool_of(num::cmp(a, b) != Ordering::Equal),
        _ => return None,
    })
}

impl<'a> Run<'a> {
    pub(crate) fn new(
        rulebook: &'a CompiledRulebook,
        index: &'a Index,
        participant_id: &'a ParticipantId,
        participant: &'a Participant,
    ) -> Self {
        Self {
            rulebook,
            index,
            participant_id,
            participant,
            entries: Vec::new(),
            used: BTreeSet::new(),
            done: BTreeMap::new(),
            not_applied: Vec::new(),
        }
    }

    fn push(&mut self, entry: Entry) -> u32 {
        self.entries.push(entry);
        u32::try_from(self.entries.len()).unwrap_or(u32::MAX)
    }

    /// Evaluates `start` and everything it reads, dependencies first.
    pub(crate) fn run(&mut self, start: RuleIdx) {
        self.run_plan(start, true);
    }

    /// Evaluates everything the ranking rule `start` reads, but not the ranking rule itself.
    pub(crate) fn prepare(&mut self, start: RuleIdx) {
        self.run_plan(start, false);
    }

    fn run_plan(&mut self, start: RuleIdx, with_start: bool) {
        for idx in plan(self.rulebook, self.index, start) {
            if self.done.contains_key(&idx) {
                continue;
            }
            if idx == start && !with_start {
                self.used.insert(idx);
                continue;
            }
            let Some(rule) = self.rulebook.rule(idx) else {
                continue;
            };
            let cx = Cx { idx, rule };
            self.used.insert(idx);
            let mut val = self.rule_value(cx);
            self.ensure(cx, &mut val);
            self.done.insert(idx, val);
        }
    }

    fn rule_value(&mut self, cx: Cx<'_>) -> Val {
        match &cx.rule.body {
            CompiledBody::Eligibility { expr } | CompiledBody::Derived { expr } => {
                let vals = self.eval_nodes(cx, &[*expr]);
                match vals.get(expr) {
                    Some(v) => v.clone(),
                    None => self.internal(cx, "the root node is missing"),
                }
            }
            CompiledBody::WeightedScore { terms, rounding } => self.weighted(cx, terms, *rounding),
            CompiledBody::Ranking { .. } => self.unsupported(
                cx,
                "a ranking has no value; rankings are answered by rank questions",
            ),
            CompiledBody::Default { .. } => self.unsupported(
                cx,
                "a default supplies an input and has no value of its own",
            ),
        }
    }

    fn eval_nodes(&mut self, cx: Cx<'_>, root_nodes: &[NodeIdx]) -> Vals {
        let mut vals = Vals::new();
        for idx in reachable(cx.rule, root_nodes) {
            let val = match cx.rule.nodes.get(idx.at()) {
                Some(node) => self.node(cx, &node.kind, &mut vals),
                None => self.internal(cx, "a node index is out of range"),
            };
            vals.insert(idx, val);
        }
        vals
    }

    // ----- diagnostics and faults -----

    fn diag(cx: Cx<'_>, code: Code, message: String, location: Location) -> Diagnostic {
        Diagnostic::new(code, format!("rule {}: {message}", cx.rule.id), location)
            .with_rules(vec![rule_ref(cx.rule)])
    }

    fn fault(cx: Cx<'_>, code: Code, message: String) -> Val {
        Val {
            eval: Eval::Fault(vec![Self::diag(cx, code, message, rule_location(cx.rule))]),
            seq: None,
        }
    }

    fn internal(&mut self, cx: Cx<'_>, what: &str) -> Val {
        Self::fault(
            cx,
            Code::ValUnsupportedConstruct,
            format!("the Compiled_Rulebook cannot be evaluated: {what}"),
        )
    }

    fn unsupported(&mut self, cx: Cx<'_>, what: &str) -> Val {
        Self::fault(
            cx,
            Code::ValUnsupportedConstruct,
            format!("not supported by the evaluator: {what}"),
        )
    }

    // ----- trace entries -----

    /// Gives a value a trace entry if it has none, so that a decision can refer to it.
    fn ensure(&mut self, cx: Cx<'_>, val: &mut Val) {
        if val.seq.is_some() {
            return;
        }
        let value = match &val.eval {
            Eval::Known(v) => DecisionValue::Known(v.trace(self.rulebook)),
            Eval::Unknown(_) | Eval::Fault(_) => DecisionValue::Unknown,
        };
        let seq = self.push(Entry::Decision(DecisionEntry {
            rule: cx.rule.id.clone(),
            op: DecisionOp::Recorded,
            value,
            from: Vec::new(),
        }));
        val.seq = Some(seq);
    }

    /// A result that replay keeps as recorded: a value of a kind replay does not derive, or an
    /// Unknown or fault.
    fn recorded(&mut self, cx: Cx<'_>, eval: Eval) -> Val {
        let mut val = Val { eval, seq: None };
        self.ensure(cx, &mut val);
        val
    }

    fn get(&mut self, cx: Cx<'_>, vals: &Vals, idx: NodeIdx) -> Val {
        match vals.get(&idx) {
            Some(v) => v.clone(),
            None => self.internal(cx, "an operand is missing"),
        }
    }

    fn seq_of(&mut self, cx: Cx<'_>, vals: &mut Vals, idx: NodeIdx) -> Option<u32> {
        let mut val = self.get(cx, vals, idx);
        self.ensure(cx, &mut val);
        let seq = val.seq;
        vals.insert(idx, val);
        seq
    }

    // ----- nodes -----

    fn node(&mut self, cx: Cx<'_>, kind: &NodeKind, vals: &mut Vals) -> Val {
        match kind {
            NodeKind::Literal(l) => Val::known(Value::from_literal(l)),
            NodeKind::Field(input) => self.read(cx, *input),
            NodeKind::Ref(t) => self.reference(cx, *t),
            NodeKind::RankReference(_) => {
                self.unsupported(cx, "rank_reference needs the ranking stage")
            }
            NodeKind::Binary { op, left, right } => {
                let (a, b) = (self.get(cx, vals, *left), self.get(cx, vals, *right));
                let step = match op {
                    BinaryOp::Add => StepOp::Add,
                    BinaryOp::Sub => StepOp::Sub,
                    BinaryOp::Mul => StepOp::Mul,
                    BinaryOp::Div => StepOp::Div,
                };
                self.arith(cx, step, vec![a, b], None)
            }
            NodeKind::Cmp { op, left, right } => {
                let (a, b) = (self.get(cx, vals, *left), self.get(cx, vals, *right));
                self.compare(cx, *op, a, b)
            }
            NodeKind::And(args) => self.logic(cx, true, args, vals),
            NodeKind::Or(args) => self.logic(cx, false, args, vals),
            NodeKind::Not(arg) => self.negate(cx, *arg, vals),
            NodeKind::In { value, set } => {
                let v = self.get(cx, vals, *value);
                self.member(cx, v, *set)
            }
            NodeKind::Agg {
                func,
                collection,
                field,
            } => self.aggregate(cx, *func, *collection, *field),
            NodeKind::Round { arg, rounding } => {
                let a = self.get(cx, vals, *arg);
                let step = StepOp::Round {
                    places: rounding.places.get(),
                    mode: rounding.mode,
                };
                self.arith(cx, step, vec![a], None)
            }
        }
    }

    fn missing(&mut self, cx: Cx<'_>, field: &str) -> Val {
        Val {
            eval: Eval::Unknown(vec![Unk {
                reason: Reason::MissingInput {
                    participant: self.participant_id.as_str().to_owned(),
                    field: field.to_owned(),
                },
                rule: cx.idx,
            }]),
            seq: None,
        }
    }

    fn data_fault(
        &self,
        cx: Cx<'_>,
        refusal: &Refusal,
        path: String,
        what: &str,
        value: &str,
    ) -> Val {
        let (code, detail) = match refusal {
            Refusal::Type(d) => (Code::ExeInputTypeMismatch, d),
            Refusal::Numeric(d) => (Code::ExeNumericValueInvalid, d),
        };
        let shown: String = escape_controls(value).chars().take(80).collect();
        let message = format!(
            "{what} of participant '{}' has the value \"{shown}\": {detail}",
            escape_controls(self.participant_id.as_str())
        );
        Val {
            eval: Eval::Fault(vec![Self::diag(cx, code, message, input_location(&path))]),
            seq: None,
        }
    }

    fn data_text(value: &rt_data::DataValue) -> String {
        match value {
            rt_data::DataValue::Bool(b) => b.to_string(),
            rt_data::DataValue::Text(t) => t.clone(),
        }
    }

    /// Reads an input of the participant: the supplied value, else the declared default, else
    /// Unknown. A value that is present is never replaced by a default (Req 11.5).
    fn read(&mut self, cx: Cx<'_>, input: InputIdx) -> Val {
        let rulebook = self.rulebook;
        let Some(decl) = rulebook.inputs().get(input.at()) else {
            return self.internal(cx, "an input index is out of range");
        };
        let name = decl.name.as_str();
        let pid = self.participant_id.as_str().to_owned();
        match self.participant.field(name) {
            Some(data) => match convert(rulebook, decl.ty, data) {
                Ok(value) => {
                    let seq = self.push(Entry::Read(ReadEntry {
                        name: name.to_owned(),
                        participant: Some(pid),
                        value: value.trace(rulebook),
                        rule: cx.rule.id.clone(),
                        default_spans: Vec::new(),
                    }));
                    Val {
                        eval: Eval::Known(value),
                        seq: Some(seq),
                    }
                }
                Err(refusal) => {
                    let path = format!(
                        "participants.{}.fields.{}",
                        escape_controls(&pid),
                        escape_controls(name)
                    );
                    self.data_fault(
                        cx,
                        &refusal,
                        path,
                        &format!("input '{}'", escape_controls(name)),
                        &Self::data_text(data),
                    )
                }
            },
            None => self.default_or_missing(cx, decl.ty, name),
        }
    }

    fn default_or_missing(&mut self, cx: Cx<'_>, ty: CType, name: &str) -> Val {
        let rulebook = self.rulebook;
        let mut defaults: Vec<(RuleIdx, &CompiledRule)> = self
            .index
            .defaults
            .get(name)
            .map(|list| {
                list.iter()
                    .filter_map(|i| rulebook.rule(*i).map(|r| (*i, r)))
                    .collect()
            })
            .unwrap_or_default();
        let literal_of = |r: &CompiledRule| match &r.body {
            CompiledBody::Default { value } => Some(value.clone()),
            _ => None,
        };
        if let Some((prec, first)) = self.index.first_listed(
            name,
            &defaults.iter().map(|(i, _)| *i).collect::<Vec<RuleIdx>>(),
        ) {
            let all: Vec<RuleIdx> = defaults.iter().map(|(i, _)| *i).collect();
            self.note_overridden(prec, &all, first);
            defaults.retain(|(i, _)| *i == first);
        }
        let Some((_, first)) = defaults.first() else {
            return self.missing(cx, name);
        };
        let Some(literal) = literal_of(first) else {
            return self.internal(cx, "a default rule has no value");
        };
        let consistent = defaults
            .iter()
            .all(|(_, r)| literal_of(r).as_ref() == Some(&literal));
        if !consistent {
            let rules: Vec<&CompiledRule> = defaults.iter().map(|(_, r)| *r).collect();
            return Val {
                eval: Eval::Fault(vec![conflict(
                    &rules,
                    &format!("the defaults for input '{}' differ", escape_controls(name)),
                )]),
                seq: None,
            };
        }
        if literal.ty() != ty {
            return self.internal(cx, "a default does not have the type of its input");
        }
        let value = Value::from_literal(&literal);
        let mut spans = Vec::new();
        for (idx, r) in &defaults {
            self.used.insert(*idx);
            spans.extend(trace_spans(&r.spans));
        }
        let seq = self.push(Entry::Read(ReadEntry {
            name: name.to_owned(),
            participant: Some(self.participant_id.as_str().to_owned()),
            value: value.trace(rulebook),
            rule: cx.rule.id.clone(),
            default_spans: spans,
        }));
        Val {
            eval: Eval::Known(value),
            seq: Some(seq),
        }
    }

    /// The value of another rule's Target (Req 12.5, 12.10). With a precedence declaration only
    /// the first-listed rule's result is used and the others are recorded as not applied. Without
    /// one, equal Known results are compatible and unequal ones are an execution conflict.
    fn reference(&mut self, cx: Cx<'_>, target: TargetIdx) -> Val {
        let Some(name) = self.rulebook.target(target) else {
            return self.internal(cx, "a target index is out of range");
        };
        let found = self.index.value.get(name.as_str()).map(Vec::as_slice);
        match found {
            Some(rules) if !rules.is_empty() => self.resolve(cx, name.as_str(), rules),
            _ => self.unsupported(cx, "a reference to a target that has no value rule"),
        }
    }

    /// The single result of the value rules `rules` of the Target `name`.
    pub(crate) fn resolve(&mut self, cx: Cx<'_>, name: &str, rules: &[RuleIdx]) -> Val {
        let index = self.index;
        if let Some((prec, first)) = index.first_listed(name, rules) {
            self.note_overridden(prec, rules, first);
            return self.done_of(cx, first, name);
        }
        match rules {
            [one] => self.done_of(cx, *one, name),
            many => self.combine(name, many),
        }
    }

    fn done_of(&mut self, cx: Cx<'_>, rule: RuleIdx, name: &str) -> Val {
        match self.done.get(&rule) {
            Some(v) => v.clone(),
            None => Self::fault(
                cx,
                Code::CmpDependencyCycle,
                format!("target '{}' depends on itself", escape_controls(name)),
            ),
        }
    }

    /// Records every rule of `rules` that `prec` lists after `first` as considered and not
    /// applied, with the declaration as the reason (Req 12.10, 13.4). Each rule once.
    pub(crate) fn note_overridden(&mut self, prec: &Prec, rules: &[RuleIdx], first: RuleIdx) {
        for n in overridden(self.rulebook, prec, rules, first) {
            if !self.not_applied.iter().any(|x| x.rule == n.rule) {
                self.not_applied.push(n);
            }
        }
    }

    /// The common result of several rules for one Target and subject when no precedence applies.
    /// Unequal Known results are one `RT-DGN-0004` and no result is selected.
    pub(crate) fn combine(&mut self, name: &str, rules: &[RuleIdx]) -> Val {
        let rulebook = self.rulebook;
        let found: Vec<&CompiledRule> = rules.iter().filter_map(|i| rulebook.rule(*i)).collect();
        let mut vals: Vec<Val> = Vec::with_capacity(rules.len());
        for r in rules {
            match self.done.get(r) {
                Some(v) => vals.push(v.clone()),
                None => {
                    let location = found.first().map_or_else(
                        || Location::None(Code::CmpDependencyCycle),
                        |r| rule_location(r),
                    );
                    let d = Diagnostic::new(
                        Code::CmpDependencyCycle,
                        format!("target '{}' depends on itself", escape_controls(name)),
                        location,
                    );
                    return Val {
                        eval: Eval::Fault(vec![d]),
                        seq: None,
                    };
                }
            }
        }
        let mut diagnostics = faults_of(&vals).unwrap_or_default();
        let known: Vec<&Value> = vals
            .iter()
            .filter_map(|v| match &v.eval {
                Eval::Known(x) => Some(x),
                _ => None,
            })
            .collect();
        let first_known = known.first().copied();
        if known.iter().any(|v| Some(*v) != first_known) {
            diagnostics.push(conflict(
                &found,
                &format!(
                    "target '{}' has several rules and no precedence applies, and their results differ",
                    escape_controls(name)
                ),
            ));
        }
        if !diagnostics.is_empty() {
            return Val {
                eval: Eval::Fault(diagnostics),
                seq: None,
            };
        }
        if let Some(u) = unknowns_of(&vals) {
            return Val {
                eval: Eval::Unknown(u),
                seq: None,
            };
        }
        vals.into_iter().next().unwrap_or(Val {
            eval: Eval::Unknown(Vec::new()),
            seq: None,
        })
    }

    /// Evaluates one criterion expression of the ranking rule `start` for this participant.
    pub(crate) fn criterion(&mut self, start: RuleIdx, root: NodeIdx) -> Val {
        let Some(rule) = self.rulebook.rule(start) else {
            return Val {
                eval: Eval::Unknown(Vec::new()),
                seq: None,
            };
        };
        let cx = Cx { idx: start, rule };
        let vals = self.eval_nodes(cx, &[root]);
        match vals.get(&root) {
            Some(v) => v.clone(),
            None => self.internal(cx, "a criterion node is missing"),
        }
    }

    /// Gives the result of a rule a trace entry, so that a trace shows it.
    pub(crate) fn record(&mut self, idx: RuleIdx) {
        let Some(rule) = self.rulebook.rule(idx) else {
            return;
        };
        if let Some(mut val) = self.done.get(&idx).cloned() {
            self.ensure(Cx { idx, rule }, &mut val);
            self.done.insert(idx, val);
        }
    }

    /// Emits one numeric step. Faults of operands win, then Unknowns; otherwise the exact
    /// result is computed, recorded and checked by replay.
    fn arith(&mut self, cx: Cx<'_>, op: StepOp, operands: Vec<Val>, field: Option<String>) -> Val {
        if let Some(d) = faults_of(&operands) {
            return self.recorded(cx, Eval::Fault(d));
        }
        if let Some(u) = unknowns_of(&operands) {
            return self.recorded(cx, Eval::Unknown(u));
        }
        let mut nums = Vec::with_capacity(operands.len());
        for o in &operands {
            match &o.eval {
                Eval::Known(Value::Number(n)) => nums.push(n.clone()),
                _ => return self.internal(cx, "a numeric operation has a non-numeric operand"),
            }
        }
        let Some(result) = compute(op, &nums) else {
            return self.internal(cx, "a numeric operation has the wrong number of operands");
        };
        let operands: Vec<TraceValue> = nums.into_iter().map(TraceValue::Number).collect();
        self.step(cx, op, operands, result, field)
    }

    fn step(
        &mut self,
        cx: Cx<'_>,
        op: StepOp,
        operands: Vec<TraceValue>,
        result: StepResult,
        field: Option<String>,
    ) -> Val {
        let seq = self.push(Entry::Step(StepEntry {
            op,
            operands: operands.clone(),
            result: result.clone(),
            rule: cx.rule.id.clone(),
            field: field.clone(),
        }));
        let eval = match result {
            StepResult::Value(TraceValue::Number(n)) => Eval::Known(Value::Number(n)),
            StepResult::Value(TraceValue::Bool(b)) => Eval::Known(Value::Bool(b)),
            StepResult::Value(_) => {
                return self.internal(cx, "a step produced a value of an unexpected type")
            }
            StepResult::Undefined => Eval::Unknown(vec![Unk {
                reason: Reason::EmptyAggregate {
                    field: field.unwrap_or_default(),
                },
                rule: cx.idx,
            }]),
            StepResult::Fault(f) => {
                let shown: Vec<String> = operands.iter().map(show).collect();
                let message = format!(
                    "{} of {}: {}",
                    op.name(),
                    shown.join(", "),
                    match f {
                        StepFault::DivisionByZero => "division by zero",
                        StepFault::Overflow =>
                            "the exact result is outside the Exact_Number limits",
                        StepFault::Invalid => "invalid numeric operand or rounding places",
                    }
                );
                Eval::Fault(vec![Self::diag(
                    cx,
                    f.code(),
                    message,
                    rule_location(cx.rule),
                )])
            }
        };
        Val {
            eval,
            seq: Some(seq),
        }
    }

    fn compare(&mut self, cx: Cx<'_>, op: CmpOp, a: Val, b: Val) -> Val {
        if let (Eval::Known(Value::Number(_)), Eval::Known(Value::Number(_))) = (&a.eval, &b.eval) {
            let step = match op {
                CmpOp::Eq => StepOp::Eq,
                CmpOp::Ne => StepOp::Ne,
                CmpOp::Lt => StepOp::Lt,
                CmpOp::Le => StepOp::Le,
                CmpOp::Gt => StepOp::Gt,
                CmpOp::Ge => StepOp::Ge,
            };
            return self.arith(cx, step, vec![a, b], None);
        }
        let operands = [a, b];
        if let Some(d) = faults_of(&operands) {
            return self.recorded(cx, Eval::Fault(d));
        }
        if let Some(u) = unknowns_of(&operands) {
            return self.recorded(cx, Eval::Unknown(u));
        }
        let [a, b] = operands;
        let result = match (&a.eval, &b.eval) {
            (Eval::Known(Value::Date(x)), Eval::Known(Value::Date(y))) => holds(op, x.cmp(y)),
            (Eval::Known(x), Eval::Known(y)) => match op {
                CmpOp::Eq => x == y,
                CmpOp::Ne => x != y,
                _ => return self.internal(cx, "an ordering comparison of unordered values"),
            },
            _ => return self.internal(cx, "a comparison has no operands"),
        };
        self.recorded(cx, Eval::Known(Value::Bool(result)))
    }

    fn member(&mut self, cx: Cx<'_>, value: Val, set: SetIdx) -> Val {
        let operands = [value];
        if let Some(d) = faults_of(&operands) {
            return self.recorded(cx, Eval::Fault(d));
        }
        if let Some(u) = unknowns_of(&operands) {
            return self.recorded(cx, Eval::Unknown(u));
        }
        let Some(decl) = self.rulebook.sets().get(set.at()) else {
            return self.internal(cx, "a set index is out of range");
        };
        let [value] = operands;
        let Eval::Known(v) = &value.eval else {
            return self.internal(cx, "a membership test has no operand");
        };
        let found = decl.members.iter().any(|m| &Value::from_literal(m) == v);
        self.recorded(cx, Eval::Known(Value::Bool(found)))
    }

    /// Kleene AND or OR. Every operand is evaluated; a fault in any operand is a fault.
    fn logic(&mut self, cx: Cx<'_>, is_and: bool, args: &[NodeIdx], vals: &mut Vals) -> Val {
        let mut operands = Vec::with_capacity(args.len());
        for a in args {
            operands.push(self.get(cx, vals, *a));
        }
        if let Some(d) = faults_of(&operands) {
            return self.recorded(cx, Eval::Fault(d));
        }
        let mut tris = Vec::with_capacity(operands.len());
        for o in &operands {
            match &o.eval {
                Eval::Known(Value::Bool(b)) => tris.push(Tri::from_bool(*b)),
                Eval::Unknown(_) => tris.push(Tri::Unknown),
                _ => return self.internal(cx, "a Boolean operation has a non-Boolean operand"),
            }
        }
        let Some((first, rest)) = tris.split_first() else {
            return self.internal(cx, "a Boolean operation has no operands");
        };
        let folded = rest.iter().fold(*first, |acc, next| {
            if is_and {
                acc.and(*next)
            } else {
                acc.or(*next)
            }
        });
        let mut from = Vec::with_capacity(args.len());
        for a in args {
            if let Some(s) = self.seq_of(cx, vals, *a) {
                from.push(s);
            }
        }
        self.decision(
            cx,
            if is_and {
                DecisionOp::And
            } else {
                DecisionOp::Or
            },
            folded,
            from,
            &operands,
        )
    }

    fn negate(&mut self, cx: Cx<'_>, arg: NodeIdx, vals: &mut Vals) -> Val {
        let operand = self.get(cx, vals, arg);
        if let Some(d) = faults_of(std::slice::from_ref(&operand)) {
            return self.recorded(cx, Eval::Fault(d));
        }
        let tri = match &operand.eval {
            Eval::Known(Value::Bool(b)) => Tri::from_bool(*b),
            Eval::Unknown(_) => Tri::Unknown,
            _ => return self.internal(cx, "a negation has a non-Boolean operand"),
        };
        let from: Vec<u32> = self.seq_of(cx, vals, arg).into_iter().collect();
        self.decision(cx, DecisionOp::Not, !tri, from, &[operand])
    }

    fn decision(
        &mut self,
        cx: Cx<'_>,
        op: DecisionOp,
        result: Tri,
        from: Vec<u32>,
        operands: &[Val],
    ) -> Val {
        let (value, eval) = match result.to_bool() {
            Some(b) => (
                DecisionValue::Known(TraceValue::Bool(b)),
                Eval::Known(Value::Bool(b)),
            ),
            None => (
                DecisionValue::Unknown,
                Eval::Unknown(unknowns_of(operands).unwrap_or_default()),
            ),
        };
        let seq = self.push(Entry::Decision(DecisionEntry {
            rule: cx.rule.id.clone(),
            op,
            value,
            from,
        }));
        Val {
            eval,
            seq: Some(seq),
        }
    }

    fn aggregate(
        &mut self,
        cx: Cx<'_>,
        func: AggFn,
        collection: CollectionIdx,
        field: Option<FieldIdx>,
    ) -> Val {
        let rulebook = self.rulebook;
        let Some(decl) = rulebook.collections().get(collection.at()) else {
            return self.internal(cx, "a collection index is out of range");
        };
        let cname = decl.name.as_str();
        // A collection the data does not supply is missing information, not an empty one.
        let Some(elements) = self.participant.collection(cname) else {
            return self.missing(cx, cname);
        };
        if func == AggFn::Count {
            let operands: Vec<TraceValue> = (0..elements.len())
                .map(|i| TraceValue::Text(format!("{cname}[{i}]")))
                .collect();
            let result = lift(num::count(operands.len()));
            return self.step(cx, StepOp::Count, operands, result, Some(cname.to_owned()));
        }
        let Some(fdecl) = field.and_then(|f| decl.fields.get(f.at())) else {
            return self.internal(cx, "an aggregate has no element field");
        };
        let fname = fdecl.name.as_str();
        let mut operands = Vec::with_capacity(elements.len());
        for (i, element) in elements.iter().enumerate() {
            let val = match element.get(fname) {
                None => self.missing(cx, &format!("{cname}.{fname}")),
                Some(data) => match convert(rulebook, fdecl.ty, data) {
                    Ok(v) => Val::known(v),
                    Err(refusal) => {
                        let path = format!(
                            "participants.{}.collections.{}[{i}].{}",
                            escape_controls(self.participant_id.as_str()),
                            escape_controls(cname),
                            escape_controls(fname)
                        );
                        self.data_fault(
                            cx,
                            &refusal,
                            path,
                            &format!(
                                "field '{}' of '{}[{i}]'",
                                escape_controls(fname),
                                escape_controls(cname)
                            ),
                            &Self::data_text(data),
                        )
                    }
                },
            };
            operands.push(val);
        }
        let step = match func {
            AggFn::Sum => StepOp::Sum,
            AggFn::Min => StepOp::Min,
            AggFn::Max => StepOp::Max,
            AggFn::Count => StepOp::Count,
        };
        self.arith(cx, step, operands, Some(cname.to_owned()))
    }

    /// `sum(weight * value)` with the declared rounding of the sum, if any.
    fn weighted(&mut self, cx: Cx<'_>, terms: &[CompiledTerm], rounding: Option<Rounding>) -> Val {
        let roots: Vec<NodeIdx> = terms.iter().flat_map(|t| [t.weight, t.value]).collect();
        let vals = self.eval_nodes(cx, &roots);
        let mut products = Vec::with_capacity(terms.len());
        for t in terms {
            let weight = self.get(cx, &vals, t.weight);
            let value = self.get(cx, &vals, t.value);
            products.push(self.arith(cx, StepOp::Mul, vec![weight, value], None));
        }
        let sum = self.arith(cx, StepOp::Sum, products, None);
        match rounding {
            Some(r) => self.arith(
                cx,
                StepOp::Round {
                    places: r.places.get(),
                    mode: r.mode,
                },
                vec![sum],
                None,
            ),
            None => sum,
        }
    }
}
