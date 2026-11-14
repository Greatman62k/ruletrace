//! Values, the three-way intermediate result `Eval` and the conversion of data
//! values.

use rt_compiled::Date;
use rt_compiled::{CType, CompiledLiteral, CompiledRulebook, EnumIdx, MemberIdx, RuleIdx};
use rt_data::DataValue;
use rt_foundation::Diagnostic;
use rt_numeric::{parse_literal, to_canonical, ExactNumber};
use rt_trace::{Reason, TraceValue};

/// A known value of the rule language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    Number(ExactNumber),
    Bool(bool),
    Text(String),
    Date(Date),
    Enum(EnumIdx, MemberIdx),
}

impl Value {
    pub(crate) fn from_literal(literal: &CompiledLiteral) -> Self {
        match literal {
            CompiledLiteral::Number(n) => Value::Number(n.clone()),
            CompiledLiteral::Boolean(b) => Value::Bool(*b),
            CompiledLiteral::Text(t) => Value::Text(t.clone()),
            CompiledLiteral::Date(d) => Value::Date(*d),
            CompiledLiteral::Enum {
                enumeration,
                member,
            } => Value::Enum(*enumeration, *member),
        }
    }

    pub(crate) fn trace(&self, rulebook: &CompiledRulebook) -> TraceValue {
        match self {
            Value::Number(n) => TraceValue::Number(n.clone()),
            Value::Bool(b) => TraceValue::Bool(*b),
            Value::Text(t) => TraceValue::Text(t.clone()),
            Value::Date(d) => TraceValue::Date(d.to_string()),
            Value::Enum(e, m) => {
                let name = rulebook
                    .enums()
                    .get(e.at())
                    .and_then(|decl| decl.members.get(m.at()))
                    .map_or_else(|| format!("#{}", m.get()), |n| n.as_str().to_owned());
                TraceValue::Enum(name)
            }
        }
    }
}

/// The text of a trace value for a message: numbers in canonical form.
pub(crate) fn show(value: &TraceValue) -> String {
    match value {
        TraceValue::Number(n) => to_canonical(n),
        TraceValue::Bool(b) => b.to_string(),
        TraceValue::Text(t) | TraceValue::Date(t) | TraceValue::Enum(t) => format!("{t:?}"),
        TraceValue::Rank(r) => r.position().to_string(),
        TraceValue::Standings(places) => format!("standings of {} places", places.len()),
    }
}

/// Why a value is Unknown: the reason and the rule that needed the value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Unk {
    pub(crate) reason: Reason,
    pub(crate) rule: RuleIdx,
}

impl Unk {
    fn key(&self) -> (u8, String, String, RuleIdx) {
        match &self.reason {
            Reason::MissingInput { participant, field } => {
                (0, participant.clone(), field.clone(), self.rule)
            }
            Reason::EmptyAggregate { field } => (1, field.clone(), String::new(), self.rule),
            Reason::Diagnosed(code) => (2, code.id().to_owned(), String::new(), self.rule),
        }
    }
}

/// Sorted without repeats, so merging is independent of how the reasons arose.
pub(crate) fn normalise_unks(mut unks: Vec<Unk>) -> Vec<Unk> {
    unks.sort_by_key(Unk::key);
    unks.dedup();
    unks
}

/// `Eval = Known | Unknown | Fault` (design §3.6.3). A fault is outside the
/// three-valued logic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Eval {
    Known(Value),
    Unknown(Vec<Unk>),
    Fault(Vec<Diagnostic>),
}

/// An `Eval` and the sequence number of the trace entry that holds its value,
/// if one exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Val {
    pub(crate) eval: Eval,
    pub(crate) seq: Option<u32>,
}

impl Val {
    pub(crate) fn known(value: Value) -> Self {
        Self {
            eval: Eval::Known(value),
            seq: None,
        }
    }
}

/// Faults of the operands in operand order, or `None` when no operand is a
/// fault.
pub(crate) fn faults_of(operands: &[Val]) -> Option<Vec<Diagnostic>> {
    let mut out: Vec<Diagnostic> = Vec::new();
    let mut any = false;
    for o in operands {
        if let Eval::Fault(d) = &o.eval {
            any = true;
            for x in d {
                if !out.contains(x) {
                    out.push(x.clone());
                }
            }
        }
    }
    any.then_some(out)
}

/// The union of the reasons of the Unknown operands, or `None` when no operand
/// is Unknown.
pub(crate) fn unknowns_of(operands: &[Val]) -> Option<Vec<Unk>> {
    let mut out: Vec<Unk> = Vec::new();
    let mut any = false;
    for o in operands {
        if let Eval::Unknown(u) = &o.eval {
            any = true;
            out.extend(u.iter().cloned());
        }
    }
    any.then(|| normalise_unks(out))
}

/// Why a data value was refused.
pub(crate) enum Refusal {
    /// The value is not of the declared type (`RT-EXE-0004`).
    Type(String),
    /// A numeric value violates the literal rules (`RT-EXE-0005`).
    Numeric(String),
}

fn kind_of(value: &DataValue) -> &'static str {
    match value {
        DataValue::Bool(_) => "a boolean",
        DataValue::Text(_) => "text",
    }
}

fn expected(ty: CType) -> &'static str {
    match ty {
        CType::Number => "a number",
        CType::Boolean => "a boolean",
        CType::Text => "text",
        CType::Date => "a date (YYYY-MM-DD)",
        CType::Enum(_) => "a member of the declared enumeration",
        CType::Ranking => "a ranking",
    }
}

/// Applies the declared type to a data value (Req 10.6, 8.7). Nothing is
/// rounded or repaired.
pub(crate) fn convert(
    rulebook: &CompiledRulebook,
    ty: CType,
    value: &DataValue,
) -> Result<Value, Refusal> {
    let mismatch = || {
        Refusal::Type(format!(
            "expected {}, found {}",
            expected(ty),
            kind_of(value)
        ))
    };
    match (ty, value) {
        (CType::Number, DataValue::Text(t)) => parse_literal(t)
            .map(Value::Number)
            .map_err(|e| Refusal::Numeric(format!("{e}"))),
        (CType::Boolean, DataValue::Bool(b)) => Ok(Value::Bool(*b)),
        (CType::Text, DataValue::Text(t)) => Ok(Value::Text(t.clone())),
        (CType::Date, DataValue::Text(t)) => Date::parse(t)
            .map(Value::Date)
            .map_err(|e| Refusal::Type(format!("expected a date (YYYY-MM-DD): {e}"))),
        (CType::Enum(e), DataValue::Text(t)) => {
            let found = rulebook
                .enums()
                .get(e.at())
                .and_then(|decl| decl.members.iter().position(|m| m.as_str() == t));
            match found.and_then(|p| u32::try_from(p).ok()) {
                Some(p) => Ok(Value::Enum(e, MemberIdx::new(p))),
                None => Err(Refusal::Type(
                    "expected a member of the declared enumeration".to_owned(),
                )),
            }
        }
        _ => Err(mismatch()),
    }
}
