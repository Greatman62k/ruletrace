//! Canonical serialization of Outcomes and Decision_Traces (design §3.6.3).
//!
//! The key order of each object is fixed by the code below. Numbers are written
//! as the canonical exact text of `rt-numeric`. The writers of `rt-json` give
//! compact and pretty forms. No timestamp, path or source text is written.

use crate::outcome::{normalise, Outcome, Place, RankPlace, Reason, TraceValue};
use crate::trace::{
    CompareEntry, DecisionEntry, DecisionTrace, DecisionValue, Entry, MissingInput, NotApplied,
    NotAppliedReason, QuestionKind, ReadEntry, StepEntry, StepOp, StepResult, TraceQuestion,
    TraceRule, TraceSpan,
};
use rt_foundation::{CliLocation, Diagnostic, Location, RuleRef, Severity};
use rt_json::Value;

type Json = Value<()>;

/// Builds an object. Every call site uses distinct literal keys, so the
/// duplicate-key error cannot occur; the unit tests would show a `null` if it
/// did.
fn obj(members: Vec<(&str, Json)>) -> Json {
    Value::object(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
    .unwrap_or_else(|_| Value::null())
}

fn text(s: &str) -> Json {
    Value::string(s)
}

fn opt_text(s: Option<&str>) -> Json {
    s.map_or_else(Value::null, Value::string)
}

fn opt_int(n: Option<u32>) -> Json {
    n.map_or_else(Value::null, Value::integer)
}

fn strings(items: &[String]) -> Json {
    Value::array(items.iter().map(|s| text(s)).collect())
}

fn rank(r: &RankPlace) -> Json {
    obj(vec![
        ("type", text("rank")),
        ("position", Value::integer(r.position())),
        ("group", strings(r.group())),
    ])
}

fn value(v: &TraceValue) -> Json {
    match v {
        TraceValue::Number(n) => obj(vec![
            ("type", text("number")),
            ("value", text(&rt_numeric::to_canonical(n))),
        ]),
        TraceValue::Bool(b) => obj(vec![("type", text("bool")), ("value", Value::boolean(*b))]),
        TraceValue::Text(s) => obj(vec![("type", text("text")), ("value", text(s))]),
        TraceValue::Date(s) => obj(vec![("type", text("date")), ("value", text(s))]),
        TraceValue::Enum(s) => obj(vec![("type", text("enum")), ("value", text(s))]),
        TraceValue::Rank(r) => rank(r),
        TraceValue::Standings(places) => obj(vec![
            ("type", text("standings")),
            ("places", Value::array(places.iter().map(place).collect())),
        ]),
    }
}

fn place(p: &Place) -> Json {
    obj(vec![
        ("position", Value::integer(p.position())),
        ("members", strings(p.members())),
    ])
}

fn compare(seq: u32, c: &CompareEntry) -> Json {
    let criteria = c
        .criteria
        .iter()
        .map(|k| {
            obj(vec![
                ("index", Value::integer(k.index)),
                ("rule", text(k.rule.as_str())),
                ("direction", text(k.direction.name())),
                ("left", value(&k.left)),
                ("right", value(&k.right)),
                ("result", text(k.result.name())),
            ])
        })
        .collect();
    obj(vec![
        ("seq", Value::integer(seq)),
        ("kind", text("compare")),
        (
            "between",
            Value::array(vec![text(&c.between.0), text(&c.between.1)]),
        ),
        ("criteria", Value::array(criteria)),
        (
            "result",
            text(if c.first_ahead { "first_ahead" } else { "tied" }),
        ),
    ])
}

fn place_entry(seq: u32, p: &Place) -> Json {
    obj(vec![
        ("seq", Value::integer(seq)),
        (
            "kind",
            text(if p.is_tie_group() {
                "tie_group"
            } else {
                "place"
            }),
        ),
        ("position", Value::integer(p.position())),
        ("members", strings(p.members())),
    ])
}

fn reason(r: &Reason) -> Json {
    match r {
        Reason::MissingInput { participant, field } => obj(vec![
            ("kind", text("missing_input")),
            ("participant", text(participant)),
            ("field", text(field)),
        ]),
        Reason::EmptyAggregate { field } => obj(vec![
            ("kind", text("empty_aggregate")),
            ("field", text(field)),
        ]),
        Reason::Diagnosed(code) => obj(vec![
            ("kind", text("diagnostic")),
            ("code", text(code.id())),
        ]),
    }
}

/// The canonical JSON value of an Outcome.
pub(crate) fn outcome(o: &Outcome) -> Json {
    match o {
        Outcome::Decided(v) => obj(vec![("state", text("decided")), ("value", value(v))]),
        Outcome::Indeterminate(reasons) => obj(vec![
            ("state", text("indeterminate")),
            (
                "reasons",
                Value::array(normalise(reasons.clone()).iter().map(reason).collect()),
            ),
        ]),
    }
}

fn span(s: &TraceSpan) -> Json {
    obj(vec![
        ("document", text(s.document.as_str())),
        ("sha256", text(s.sha256.as_str())),
        ("start", Value::integer(s.start)),
        ("end", Value::integer(s.end)),
        ("line", Value::integer(s.line)),
        ("column", Value::integer(s.column)),
    ])
}

fn spans(items: &[TraceSpan]) -> Json {
    Value::array(items.iter().map(span).collect())
}

fn rule(r: &TraceRule) -> Json {
    obj(vec![
        ("rule", text(r.rule.as_str())),
        ("revision", Value::integer(r.revision.get())),
        ("spans", spans(&r.spans)),
    ])
}

fn question(q: &TraceQuestion) -> Json {
    let kind = match q.kind {
        QuestionKind::Value => "value",
        QuestionKind::Rank => "rank",
    };
    obj(vec![
        ("kind", text(kind)),
        ("target", text(&q.target)),
        ("participant", opt_text(q.participant.as_deref())),
    ])
}

fn step_result(r: &StepResult) -> Json {
    match r {
        StepResult::Value(v) => value(v),
        StepResult::Undefined => obj(vec![("type", text("undefined"))]),
        StepResult::Fault(f) => obj(vec![("type", text("fault")), ("code", text(f.code().id()))]),
    }
}

fn read(seq: u32, r: &ReadEntry) -> Json {
    obj(vec![
        ("seq", Value::integer(seq)),
        ("kind", text("read")),
        ("name", text(&r.name)),
        ("participant", opt_text(r.participant.as_deref())),
        ("value", value(&r.value)),
        ("rule", text(r.rule.as_str())),
        ("default_spans", spans(&r.default_spans)),
    ])
}

fn step(seq: u32, s: &StepEntry) -> Json {
    let mut members = vec![
        ("seq", Value::integer(seq)),
        ("kind", text("step")),
        ("op", text(s.op.name())),
    ];
    if let StepOp::Round { places, mode } = s.op {
        members.push(("places", Value::integer(places)));
        members.push(("mode", text(mode.name())));
    }
    members.push((
        "operands",
        Value::array(s.operands.iter().map(value).collect()),
    ));
    members.push(("result", step_result(&s.result)));
    members.push(("rule", text(s.rule.as_str())));
    members.push(("field", opt_text(s.field.as_deref())));
    obj(members)
}

fn decision(seq: u32, d: &DecisionEntry) -> Json {
    let v = match &d.value {
        DecisionValue::Known(v) => value(v),
        DecisionValue::Unknown => obj(vec![("type", text("unknown"))]),
    };
    obj(vec![
        ("seq", Value::integer(seq)),
        ("kind", text("decision")),
        ("rule", text(d.rule.as_str())),
        ("op", text(d.op.name())),
        ("value", v),
        (
            "from",
            Value::array(d.from.iter().map(|n| Value::integer(*n)).collect()),
        ),
    ])
}

fn not_applied(n: &NotApplied) -> Json {
    let reason = match &n.reason {
        NotAppliedReason::ConditionNotSatisfied { condition } => obj(vec![
            ("kind", text("condition_not_satisfied")),
            ("condition", text(condition)),
        ]),
        NotAppliedReason::MissingInput { input } => obj(vec![
            ("kind", text("missing_input")),
            ("input", text(input)),
        ]),
        NotAppliedReason::Precedence { id } => {
            obj(vec![("kind", text("precedence")), ("id", text(id))])
        }
    };
    obj(vec![
        ("rule", text(n.rule.as_str())),
        ("revision", Value::integer(n.revision.get())),
        ("spans", spans(&n.spans)),
        ("reason", reason),
    ])
}

fn missing(m: &MissingInput) -> Json {
    obj(vec![
        ("participant", text(&m.participant)),
        ("field", text(&m.field)),
    ])
}

fn location(l: &Location) -> Json {
    match l {
        Location::Source(s) => obj(vec![
            ("kind", text("source")),
            ("document", text(s.doc().as_str())),
            ("start", Value::integer(s.start().get())),
            ("end", Value::integer(s.end().get())),
            ("line", Value::integer(s.line())),
            ("column", Value::integer(s.column())),
        ]),
        Location::Document(d) => obj(vec![
            ("kind", text("document")),
            ("document", text(d.as_str())),
        ]),
        Location::Input(i) => obj(vec![
            ("kind", text("input")),
            ("input", text(i.input.as_str())),
            ("line", opt_int(i.line)),
            ("column", opt_int(i.column)),
            ("field", opt_text(i.field.as_deref())),
        ]),
        Location::Cli(CliLocation::Argument { position, name }) => obj(vec![
            ("kind", text("cli_argument")),
            ("position", Value::integer(*position)),
            ("name", opt_text(name.as_deref())),
        ]),
        Location::Cli(CliLocation::Command(c)) => {
            obj(vec![("kind", text("cli_command")), ("command", text(c))])
        }
        Location::None(code) => obj(vec![("kind", text("none")), ("reason", text(code.id()))]),
    }
}

fn rule_ref(r: &RuleRef) -> Json {
    obj(vec![
        ("rule", text(r.rule.as_str())),
        ("revision", opt_int(r.revision.map(|v| v.get()))),
    ])
}

fn diagnostic(d: &Diagnostic) -> Json {
    let severity = match d.severity() {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Ambiguity => "ambiguity",
        Severity::Unsupported => "unsupported",
    };
    obj(vec![
        ("code", text(d.code().id())),
        ("severity", text(severity)),
        ("message", text(d.message())),
        ("primary", location(d.primary())),
        (
            "related",
            Value::array(d.related().iter().map(location).collect()),
        ),
        (
            "rules",
            Value::array(d.rules().iter().map(rule_ref).collect()),
        ),
    ])
}

impl Outcome {
    /// The canonical RT-JSON value.
    #[must_use]
    pub fn to_value(&self) -> Value<()> {
        outcome(self)
    }
}

impl DecisionTrace {
    /// The canonical RT-JSON value, keys in the fixed order of design §3.6.3.
    #[must_use]
    pub fn to_value(&self) -> Value<()> {
        let entries = self
            .entries()
            .iter()
            .enumerate()
            .map(|(index, e)| {
                let seq = u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX);
                match e {
                    Entry::Read(r) => read(seq, r),
                    Entry::Step(s) => step(seq, s),
                    Entry::Decision(d) => decision(seq, d),
                    Entry::Compare(c) => compare(seq, c),
                    Entry::Place(p) => place_entry(seq, p),
                }
            })
            .collect();
        obj(vec![
            ("question", question(self.question())),
            ("outcome", outcome(self.outcome())),
            (
                "rules",
                Value::array(self.rules().iter().map(rule).collect()),
            ),
            ("entries", Value::array(entries)),
            (
                "not_applied",
                Value::array(self.not_applied().iter().map(not_applied).collect()),
            ),
            (
                "missing",
                Value::array(self.missing().iter().map(missing).collect()),
            ),
            (
                "diagnostics",
                Value::array(
                    self.diagnostics()
                        .as_slice()
                        .iter()
                        .map(diagnostic)
                        .collect(),
                ),
            ),
        ])
    }

    /// Compact canonical text, used for hashes and byte comparison.
    #[must_use]
    pub fn to_canonical_compact(&self) -> String {
        rt_json::write_compact(&self.to_value())
    }

    /// Pretty canonical text (two-space indent, trailing line feed).
    #[must_use]
    pub fn to_canonical_pretty(&self) -> String {
        rt_json::write_pretty(&self.to_value())
    }
}
