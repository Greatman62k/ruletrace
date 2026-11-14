//! Canonical serialization of a Compiled_Rulebook (design §4.1.6).
//!
//! RT-JSON with a fixed key order. Top level: `format`, `compiler_version`,
//! `ir_version`, `rulebook_hash`, `documents`, `inputs`, `collections`,
//! `enums`, `sets`, `order`, `rules`, `precedence`. A rule: `id`, `revision`,
//! `kind`, `target`, `type`, `spans`, `nodes`, then the root fields of its
//! kind. A node: `idx`, `node`, `t`, then its fields; children are indices. A
//! Target is written by name (it has no table of its own) and read back against
//! the distinct Targets of `rules`. Numbers are canonical exact text, so a
//! value without a finite decimal expansion is written as a fraction.

use crate::index::NodeIdx;
use crate::model::{
    CType, CompiledBody, CompiledLiteral, CompiledNode, CompiledPrecedence, CompiledRule,
    CompiledRulebook, CompiledSpan, NodeKind,
};
use rt_ir::Rounding;
use rt_json::Value;

/// The format name of the artifact.
pub const FORMAT_NAME: &str = "ruletrace-compiled";

type Json = Value<()>;

/// Builds an object. Every call site uses distinct literal keys, so the
/// duplicate-key error cannot occur; a test would show a `null` if it did.
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

fn int(n: u32) -> Json {
    Value::integer(n)
}

fn idx(n: NodeIdx) -> Json {
    int(n.get())
}

/// The type text: `number`, `boolean`, `text`, `date`, `ranking`, or
/// `enum:<index>`.
pub(crate) fn type_text(ty: CType) -> String {
    match ty {
        CType::Number => "number".to_owned(),
        CType::Boolean => "boolean".to_owned(),
        CType::Text => "text".to_owned(),
        CType::Date => "date".to_owned(),
        CType::Ranking => "ranking".to_owned(),
        CType::Enum(e) => format!("enum:{}", e.get()),
    }
}

fn literal_members(lit: &CompiledLiteral) -> Vec<(&'static str, Json)> {
    match lit {
        CompiledLiteral::Number(n) => vec![
            ("node", text("num")),
            ("value", text(&rt_numeric::to_canonical(n))),
        ],
        CompiledLiteral::Boolean(b) => vec![("node", text("bool")), ("value", Value::boolean(*b))],
        CompiledLiteral::Text(s) => vec![("node", text("text")), ("value", text(s))],
        CompiledLiteral::Date(d) => {
            vec![("node", text("date")), ("value", text(&d.to_string()))]
        }
        CompiledLiteral::Enum {
            enumeration,
            member,
        } => vec![
            ("node", text("enum")),
            ("enum", int(enumeration.get())),
            ("member", int(member.get())),
        ],
    }
}

fn literal(lit: &CompiledLiteral) -> Json {
    obj(literal_members(lit))
}

fn span(s: &CompiledSpan) -> Json {
    obj(vec![
        ("document", text(s.document.as_str())),
        ("sha256", text(s.sha256.as_str())),
        ("start", int(s.start)),
        ("end", int(s.end)),
        ("line", int(s.line)),
        ("column", int(s.column)),
    ])
}

fn spans(items: &[CompiledSpan]) -> Json {
    Value::array(items.iter().map(span).collect())
}

fn rounding(r: &Rounding) -> Json {
    obj(vec![
        ("mode", text(r.mode.name())),
        ("places", int(r.places.get())),
    ])
}

fn node(rb: &CompiledRulebook, index: usize, n: &CompiledNode) -> Json {
    let at = u32::try_from(index).unwrap_or(u32::MAX);
    let target = |t: crate::TargetIdx| rb.target(t).map_or("", |n| n.as_str()).to_owned();
    let mut m: Vec<(&str, Json)> = vec![("idx", int(at))];
    let ty = Some(("t", text(&type_text(n.ty))));
    match &n.kind {
        NodeKind::Literal(lit) => {
            let mut members = literal_members(lit).into_iter();
            if let Some(first) = members.next() {
                m.push(first);
            }
            m.extend(ty);
            m.extend(members);
        }
        NodeKind::Field(i) => {
            m.push(("node", text("field")));
            m.extend(ty);
            m.push(("input", int(i.get())));
        }
        NodeKind::Ref(t) => {
            m.push(("node", text("ref")));
            m.extend(ty);
            m.push(("target", text(&target(*t))));
        }
        NodeKind::Binary { op, left, right } => {
            m.push(("node", text("binary")));
            m.extend(ty);
            m.push(("op", text(op.name())));
            m.push(("left", idx(*left)));
            m.push(("right", idx(*right)));
        }
        NodeKind::Cmp { op, left, right } => {
            m.push(("node", text("cmp")));
            m.extend(ty);
            m.push(("op", text(op.name())));
            m.push(("left", idx(*left)));
            m.push(("right", idx(*right)));
        }
        NodeKind::And(args) | NodeKind::Or(args) => {
            let name = if matches!(n.kind, NodeKind::And(_)) {
                "and"
            } else {
                "or"
            };
            m.push(("node", text(name)));
            m.extend(ty);
            m.push(("args", Value::array(args.iter().map(|a| idx(*a)).collect())));
        }
        NodeKind::Not(arg) => {
            m.push(("node", text("not")));
            m.extend(ty);
            m.push(("arg", idx(*arg)));
        }
        NodeKind::In { value, set } => {
            m.push(("node", text("in")));
            m.extend(ty);
            m.push(("value", idx(*value)));
            m.push(("set", int(set.get())));
        }
        NodeKind::Agg {
            func,
            collection,
            field,
        } => {
            m.push(("node", text("agg")));
            m.extend(ty);
            m.push(("fn", text(func.name())));
            m.push(("collection", int(collection.get())));
            m.push(("field", field.map_or_else(Value::null, |f| int(f.get()))));
        }
        NodeKind::Round { arg, rounding } => {
            m.push(("node", text("round")));
            m.extend(ty);
            m.push(("arg", idx(*arg)));
            m.push(("places", int(rounding.places.get())));
            m.push(("mode", text(rounding.mode.name())));
        }
        NodeKind::RankReference(t) => {
            m.push(("node", text("rank_reference")));
            m.extend(ty);
            m.push(("target", text(&target(*t))));
        }
    }
    obj(m)
}

fn rule(rb: &CompiledRulebook, r: &CompiledRule) -> Json {
    let mut m: Vec<(&str, Json)> = vec![
        ("id", text(r.id.as_str())),
        ("revision", int(r.revision.get())),
        ("kind", text(r.body.kind_name())),
        ("target", text(r.target.as_str())),
        ("type", text(&type_text(r.ty))),
        ("spans", spans(&r.spans)),
        (
            "nodes",
            Value::array(
                r.nodes
                    .iter()
                    .enumerate()
                    .map(|(i, n)| node(rb, i, n))
                    .collect(),
            ),
        ),
    ];
    match &r.body {
        CompiledBody::Eligibility { expr } | CompiledBody::Derived { expr } => {
            m.push(("expr", idx(*expr)));
        }
        CompiledBody::WeightedScore { terms, rounding: r } => {
            let terms = terms
                .iter()
                .map(|t| obj(vec![("weight", idx(t.weight)), ("value", idx(t.value))]))
                .collect();
            m.push(("terms", Value::array(terms)));
            m.push(("rounding", r.as_ref().map_or_else(Value::null, rounding)));
        }
        CompiledBody::Ranking {
            key,
            direction,
            tie_breakers,
            ties,
        } => {
            m.push(("key", idx(*key)));
            m.push(("direction", text(direction.name())));
            let list = tie_breakers
                .iter()
                .map(|t| {
                    obj(vec![
                        ("expr", idx(t.expr)),
                        ("direction", text(t.direction.name())),
                    ])
                })
                .collect();
            m.push(("tie_breakers", Value::array(list)));
            let rt_ir::Ties::Group = ties;
            m.push(("ties", text("group")));
        }
        CompiledBody::Default { value } => m.push(("value", literal(value))),
    }
    obj(m)
}

fn precedence(rb: &CompiledRulebook, d: &CompiledPrecedence) -> Json {
    let id_of = |i: crate::RuleIdx| rb.rule(i).map_or("", |r| r.id.as_str()).to_owned();
    obj(vec![
        ("id", text(d.id.as_str())),
        (
            "target",
            text(rb.target(d.target).map_or("", |n| n.as_str())),
        ),
        (
            "order",
            Value::array(d.order.iter().map(|i| text(&id_of(*i))).collect()),
        ),
        ("spans", spans(&d.spans)),
    ])
}

impl CompiledRulebook {
    /// The canonical RT-JSON value, keys in the fixed order of design §4.1.6.
    #[must_use]
    pub fn to_value(&self) -> Value<()> {
        let documents = self
            .documents()
            .iter()
            .map(|d| {
                obj(vec![
                    ("id", text(d.id.as_str())),
                    ("sha256", text(d.sha256.as_str())),
                ])
            })
            .collect();
        let inputs = self
            .inputs()
            .iter()
            .map(|i| {
                obj(vec![
                    ("name", text(i.name.as_str())),
                    ("type", text(&type_text(i.ty))),
                ])
            })
            .collect();
        let collections = self
            .collections()
            .iter()
            .map(|c| {
                let fields = c
                    .fields
                    .iter()
                    .map(|f| {
                        obj(vec![
                            ("name", text(f.name.as_str())),
                            ("type", text(&type_text(f.ty))),
                        ])
                    })
                    .collect();
                obj(vec![
                    ("name", text(c.name.as_str())),
                    ("fields", Value::array(fields)),
                ])
            })
            .collect();
        let enums = self
            .enums()
            .iter()
            .map(|e| {
                let members = e.members.iter().map(|m| text(m.as_str())).collect();
                obj(vec![
                    ("name", text(e.name.as_str())),
                    ("members", Value::array(members)),
                ])
            })
            .collect();
        let sets = self
            .sets()
            .iter()
            .map(|s| {
                obj(vec![
                    ("name", text(s.name.as_str())),
                    ("element_type", text(&type_text(s.element_type))),
                    (
                        "members",
                        Value::array(s.members.iter().map(literal).collect()),
                    ),
                ])
            })
            .collect();
        let order = self
            .order()
            .iter()
            .map(|i| text(self.rule(*i).map_or("", |r| r.id.as_str())))
            .collect();
        obj(vec![
            ("format", text(FORMAT_NAME)),
            ("compiler_version", text(self.compiler_version())),
            ("ir_version", text(self.ir_version().as_str())),
            ("rulebook_hash", text(self.rulebook_hash().as_str())),
            ("documents", Value::array(documents)),
            ("inputs", Value::array(inputs)),
            ("collections", Value::array(collections)),
            ("enums", Value::array(enums)),
            ("sets", Value::array(sets)),
            ("order", Value::array(order)),
            (
                "rules",
                Value::array(self.rules().iter().map(|r| rule(self, r)).collect()),
            ),
            (
                "precedence",
                Value::array(
                    self.precedence()
                        .iter()
                        .map(|d| precedence(self, d))
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

    /// Pretty canonical text (two-space indent, trailing line feed), as stored
    /// in a file.
    #[must_use]
    pub fn to_canonical_pretty(&self) -> String {
        rt_json::write_pretty(&self.to_value())
    }
}
