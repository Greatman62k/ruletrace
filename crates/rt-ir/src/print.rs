//! The canonical Rule_IR printers (design §4.1.1, §4.1.2).
//!
//! Both forms use the single key order of design §4.1.2 and the string escaping
//! of `rt-json`. Rules are written sorted by (identifier bytes, revision) and
//! documents by identifier bytes, so two equal Rulebooks give byte-identical
//! output whatever order they were built in.
//!
//! The printers build a private output tree ([`Out`]) and write it. The tree is
//! private because it must carry number lexemes of opaque subtrees verbatim,
//! which `rt-json` builders cannot express. [`write`] follows the layout of
//! `rt_json::write_compact` and `rt_json::write_pretty`; a test checks the two
//! against each other.

use crate::expr::{Expr, ExprKind, Literal, OpaqueConstruct, Rounding};
use crate::rule::{
    AiOrigin, DefaultValue, Marker, Origin, RuleBody, RuleRevision, Span, TieBreaker, WeightedTerm,
};
use crate::rulebook::{DocumentDecl, Precedence, Rulebook, FORMAT_NAME};
use crate::types::TypeDecls;
use rt_foundation::{
    Code, Diagnostic, InputId, Location, MAX_REVISIONS_PER_RULEBOOK, MAX_RULES_PER_RULEBOOK,
    MAX_SPANS_PER_RULE,
};
use rt_json::{Kind, ParseLimits, Position, Value};
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// A node of the output tree. Object members keep the order given, which is the
/// key order.
#[derive(Clone, Debug)]
pub(crate) enum Out {
    Null,
    Bool(bool),
    Int(u32),
    /// A number lexeme written as it was read (opaque subtrees only).
    Lexeme(String),
    Str(String),
    Arr(Vec<Out>),
    Obj(Vec<(String, Out)>),
}

/// Deepest container nesting accepted when an opaque subtree is read back.
const OPAQUE_DEPTH: u32 = 1024;

pub(crate) fn s(text: &str) -> Out {
    Out::Str(text.to_owned())
}

pub(crate) fn obj(members: Vec<(&str, Out)>) -> Out {
    Out::Obj(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
}

/// Converts a parsed value into an output node, keeping number lexemes
/// verbatim.
pub(crate) fn value_to_out(value: &Value<Position>) -> Out {
    match value.kind() {
        Kind::Null => Out::Null,
        Kind::Bool(b) => Out::Bool(*b),
        Kind::Integer(n) => Out::Lexeme(n.to_string()),
        Kind::Number(text) => Out::Lexeme(text.clone()),
        Kind::String(text) => Out::Str(text.clone()),
        Kind::Array(items) => Out::Arr(items.iter().map(value_to_out).collect()),
        Kind::Object(members) => Out::Obj(
            members
                .iter()
                .map(|m| (m.key.clone(), value_to_out(&m.value)))
                .collect(),
        ),
    }
}

/// The members of the opaque subtree, in carried order. The raw text was
/// written by the canonical writer, so it always parses; an unreadable value
/// gives no members.
fn splice(opaque: &OpaqueConstruct) -> Vec<(String, Out)> {
    let Ok(input) = InputId::new("opaque") else {
        return Vec::new();
    };
    let limits = ParseLimits::new(u64::from(u32::MAX), OPAQUE_DEPTH);
    match rt_json::parse(&input, opaque.raw().as_bytes(), limits) {
        Ok(value) => match value_to_out(&value) {
            Out::Obj(members) => members,
            _ => Vec::new(),
        },
        Err(_) => Vec::new(),
    }
}

// ----- Writer -----

fn push_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                // Writing to a String cannot fail.
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn newline(out: &mut String, pretty: bool, level: usize) {
    if pretty {
        out.push('\n');
        for _ in 0..level {
            out.push_str("  ");
        }
    }
}

/// Writes a node: compact form, or pretty form with two-space indentation.
pub(crate) fn write(value: &Out, pretty: bool) -> String {
    let mut out = String::new();
    write_node(&mut out, value, pretty, 0);
    if pretty {
        out.push('\n');
    }
    out
}

fn write_node(out: &mut String, value: &Out, pretty: bool, level: usize) {
    match value {
        Out::Null => out.push_str("null"),
        Out::Bool(true) => out.push_str("true"),
        Out::Bool(false) => out.push_str("false"),
        Out::Int(n) => {
            let _ = write!(out, "{n}");
        }
        Out::Lexeme(text) => out.push_str(text),
        Out::Str(text) => push_string(out, text),
        Out::Arr(items) if items.is_empty() => out.push_str("[]"),
        Out::Obj(members) if members.is_empty() => out.push_str("{}"),
        Out::Arr(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                newline(out, pretty, level + 1);
                write_node(out, item, pretty, level + 1);
            }
            newline(out, pretty, level);
            out.push(']');
        }
        Out::Obj(members) => {
            out.push('{');
            for (index, (key, member)) in members.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                newline(out, pretty, level + 1);
                push_string(out, key);
                out.push(':');
                if pretty {
                    out.push(' ');
                }
                write_node(out, member, pretty, level + 1);
            }
            newline(out, pretty, level);
            out.push('}');
        }
    }
}

// ----- Rulebook to output tree -----

pub(crate) fn opt_str(text: Option<&str>) -> Out {
    text.map_or(Out::Null, s)
}

fn literal_type_text(literal: &Literal) -> String {
    match literal {
        Literal::Number(_) => "number".to_owned(),
        Literal::Boolean(_) => "boolean".to_owned(),
        Literal::Text(_) => "text".to_owned(),
        Literal::Date(_) => "date".to_owned(),
        Literal::Enum { enum_name, .. } => format!("enum:{enum_name}"),
    }
}

fn literal_node_name(literal: &Literal) -> &'static str {
    match literal {
        Literal::Number(_) => "num",
        Literal::Boolean(_) => "bool",
        Literal::Text(_) => "text",
        Literal::Date(_) => "date",
        Literal::Enum { .. } => "enum",
    }
}

/// The fields of a literal after `node` and `t`.
fn literal_fields(literal: &Literal) -> Vec<(String, Out)> {
    let one = |name: &str, value: Out| vec![(name.to_owned(), value)];
    match literal {
        Literal::Number(n) => one("value", Out::Str(rt_numeric::to_canonical(n))),
        Literal::Boolean(b) => one("value", Out::Bool(*b)),
        Literal::Text(t) => one("value", Out::Str(t.clone())),
        Literal::Date(d) => one("value", Out::Str(d.to_string())),
        Literal::Enum { enum_name, member } => vec![
            ("enum".to_owned(), s(enum_name.as_str())),
            ("member".to_owned(), s(member.as_str())),
        ],
    }
}

/// A literal in a position that has no `t` of its own (set member, default):
/// the type is derived.
fn literal_out(literal: &Literal) -> Out {
    let mut members = vec![
        ("node".to_owned(), s(literal_node_name(literal))),
        ("t".to_owned(), Out::Str(literal_type_text(literal))),
    ];
    members.extend(literal_fields(literal));
    Out::Obj(members)
}

fn rounding_fields(r: &Rounding) -> Vec<(&'static str, Out)> {
    vec![
        ("mode", s(r.mode.name())),
        ("places", Out::Int(r.places.get())),
    ]
}

fn expr_out(e: &Expr) -> Out {
    if let ExprKind::Opaque(opaque) = &e.kind {
        let mut members = vec![("node".to_owned(), s(opaque.name.as_str()))];
        if let Some(t) = &e.ty {
            members.push(("t".to_owned(), Out::Str(t.text())));
        }
        members.extend(splice(opaque));
        return Out::Obj(members);
    }
    let node = match &e.kind {
        ExprKind::Literal(l) => literal_node_name(l),
        ExprKind::Field { .. } => "field",
        ExprKind::Ref { .. } => "ref",
        ExprKind::Binary { .. } => "binary",
        ExprKind::Cmp { .. } => "cmp",
        ExprKind::And(_) => "and",
        ExprKind::Or(_) => "or",
        ExprKind::Not(_) => "not",
        ExprKind::In { .. } => "in",
        ExprKind::Agg { .. } => "agg",
        ExprKind::Round { .. } => "round",
        ExprKind::RankReference { .. } => "rank_reference",
        ExprKind::Opaque(_) => "",
    };
    let mut members: Vec<(String, Out)> = vec![("node".to_owned(), s(node))];
    if let Some(t) = &e.ty {
        members.push(("t".to_owned(), Out::Str(t.text())));
    }
    let mut add = |key: &str, value: Out| members.push((key.to_owned(), value));
    match &e.kind {
        ExprKind::Literal(l) => {
            for (k, v) in literal_fields(l) {
                add(&k, v);
            }
        }
        ExprKind::Field { name } => add("name", s(name.as_str())),
        ExprKind::Ref { target } | ExprKind::RankReference { target } => {
            add("target", s(target.as_str()));
        }
        ExprKind::Binary { op, left, right } => {
            add("op", s(op.name()));
            add("left", expr_out(left));
            add("right", expr_out(right));
        }
        ExprKind::Cmp { op, left, right } => {
            add("op", s(op.name()));
            add("left", expr_out(left));
            add("right", expr_out(right));
        }
        ExprKind::And(args) | ExprKind::Or(args) => {
            add("args", Out::Arr(args.iter().map(expr_out).collect()));
        }
        ExprKind::Not(arg) => add("arg", expr_out(arg)),
        ExprKind::In { value, set } => {
            add("value", expr_out(value));
            add("set", s(set.as_str()));
        }
        ExprKind::Agg {
            func,
            collection,
            field,
        } => {
            add("fn", s(func.name()));
            add("collection", s(collection.as_str()));
            add("field", opt_str(field.as_ref().map(|f| f.as_str())));
        }
        ExprKind::Round { arg, places, mode } => {
            add("arg", expr_out(arg));
            add("places", Out::Int(places.get()));
            add("mode", s(mode.name()));
        }
        ExprKind::Opaque(_) => {}
    }
    Out::Obj(members)
}

fn span_out(span: &Span) -> Out {
    obj(vec![
        ("document", s(span.document.as_str())),
        ("start", Out::Int(span.start)),
        ("end", Out::Int(span.end)),
        ("line", Out::Int(span.line)),
        ("column", Out::Int(span.column)),
    ])
}

pub(crate) fn marker_out(marker: &Marker) -> Out {
    let texts = |items: &[String]| Out::Arr(items.iter().map(|t| s(t)).collect());
    match marker {
        Marker::Ambiguous { interpretations } => obj(vec![
            ("kind", s("ambiguous")),
            ("interpretations", texts(interpretations)),
        ]),
        Marker::Incomplete { missing } => {
            obj(vec![("kind", s("incomplete")), ("missing", texts(missing))])
        }
    }
}

fn ai_origin_out(ai: &AiOrigin) -> Out {
    obj(vec![
        ("kind", s("ai_proposal")),
        ("proposer", s(ai.proposer.as_str())),
        ("provider", s(&ai.provider)),
        ("model", s(&ai.model)),
        ("adapter_version", s(&ai.adapter_version)),
        ("request_hash", s(ai.request_hash.as_str())),
        ("response_hash", s(ai.response_hash.as_str())),
        ("proposal_id", s(&ai.proposal_id)),
        (
            "ai_identities",
            Out::Arr(ai.ai_identities.iter().map(|i| s(i.as_str())).collect()),
        ),
    ])
}

fn origin_out(origin: &Origin) -> Out {
    match origin {
        Origin::Manual { author } => obj(vec![
            ("kind", s("manual")),
            ("author", opt_str(author.as_ref().map(|a| a.as_str()))),
        ]),
        Origin::AiProposal(ai) => ai_origin_out(ai),
    }
}

fn term_out(term: &WeightedTerm) -> Out {
    obj(vec![
        ("weight", expr_out(&term.weight)),
        ("value", expr_out(&term.value)),
    ])
}

fn tie_breaker_out(tb: &TieBreaker) -> Out {
    obj(vec![
        ("expr", expr_out(&tb.expr)),
        ("direction", s(tb.direction.name())),
    ])
}

fn default_out(value: &DefaultValue) -> Out {
    literal_out(&value.literal)
}

fn revision_out(rev: &RuleRevision) -> Out {
    let c = &rev.content;
    let mut members: Vec<(String, Out)> = vec![
        ("id".to_owned(), s(rev.key.id.as_str())),
        ("revision".to_owned(), Out::Int(rev.key.revision.get())),
        ("origin".to_owned(), origin_out(&rev.origin)),
        ("kind".to_owned(), s(c.body.kind_name())),
        ("target".to_owned(), s(c.target.as_str())),
    ];
    if let Some(t) = &c.ty {
        members.push(("type".to_owned(), Out::Str(t.text())));
    }
    members.extend(body_members(&c.body));
    members.push((
        "markers".to_owned(),
        Out::Arr(c.markers.iter().map(marker_out).collect()),
    ));
    members.push((
        "spans".to_owned(),
        Out::Arr(c.spans.iter().map(span_out).collect()),
    ));
    Out::Obj(members)
}

/// The kind-specific members that follow `type` in a rule, in canonical order.
pub(crate) fn body_members(body: &RuleBody) -> Vec<(String, Out)> {
    let mut members: Vec<(String, Out)> = Vec::new();
    let mut add = |key: &str, value: Out| members.push((key.to_owned(), value));
    match body {
        RuleBody::Eligibility { expr } | RuleBody::Derived { expr } => add("expr", expr_out(expr)),
        RuleBody::WeightedScore { terms, rounding } => {
            add("terms", Out::Arr(terms.iter().map(term_out).collect()));
            add(
                "rounding",
                rounding
                    .as_ref()
                    .map_or(Out::Null, |r| obj(rounding_fields(r))),
            );
        }
        RuleBody::Ranking {
            key,
            direction,
            tie_breakers,
            ties,
        } => {
            add("key", expr_out(key));
            add("direction", s(direction.name()));
            add(
                "tie_breakers",
                Out::Arr(tie_breakers.iter().map(tie_breaker_out).collect()),
            );
            let ties = match ties {
                crate::rule::Ties::Group => "group",
            };
            add("ties", s(ties));
        }
        RuleBody::Default { value } => add("value", default_out(value)),
        RuleBody::Unrecognized(opaque) => {
            for (k, v) in splice(opaque) {
                members.push((k, v));
            }
        }
    }
    members
}

fn document_out(doc: &DocumentDecl) -> Out {
    obj(vec![
        ("id", s(doc.id.as_str())),
        ("sha256", s(doc.sha256.as_str())),
    ])
}

fn types_out(types: &TypeDecls) -> Out {
    let inputs = types
        .inputs
        .iter()
        .map(|i| {
            obj(vec![
                ("name", s(i.name.as_str())),
                ("type", Out::Str(i.ty.text())),
            ])
        })
        .collect();
    let enums = types
        .enums
        .iter()
        .map(|e| {
            obj(vec![
                ("name", s(e.name.as_str())),
                (
                    "members",
                    Out::Arr(e.members.iter().map(|m| s(m.as_str())).collect()),
                ),
            ])
        })
        .collect();
    let sets = types
        .sets
        .iter()
        .map(|d| {
            obj(vec![
                ("name", s(d.name.as_str())),
                ("element_type", Out::Str(d.element_type.text())),
                (
                    "members",
                    Out::Arr(d.members.iter().map(literal_out).collect()),
                ),
            ])
        })
        .collect();
    let collections = types
        .collections
        .iter()
        .map(|c| {
            let fields = c
                .fields
                .iter()
                .map(|f| {
                    obj(vec![
                        ("name", s(f.name.as_str())),
                        ("type", Out::Str(f.ty.text())),
                    ])
                })
                .collect();
            obj(vec![
                ("name", s(c.name.as_str())),
                ("fields", Out::Arr(fields)),
            ])
        })
        .collect();
    obj(vec![
        ("inputs", Out::Arr(inputs)),
        ("enums", Out::Arr(enums)),
        ("sets", Out::Arr(sets)),
        ("collections", Out::Arr(collections)),
    ])
}

fn precedence_out(p: &Precedence) -> Out {
    obj(vec![
        ("id", s(p.id.as_str())),
        ("target", s(p.target.as_str())),
        (
            "order",
            Out::Arr(p.order.iter().map(|r| s(r.as_str())).collect()),
        ),
        ("spans", Out::Arr(p.spans.iter().map(span_out).collect())),
    ])
}

fn limit_diagnostic(what: &str, count: usize, limit: u32) -> Diagnostic {
    Diagnostic::new(
        Code::LimitCountExceeded,
        format!("the Rulebook has {count} {what}, above the limit of {limit}"),
        Location::None(Code::LimitCountExceeded),
    )
}

/// Checks the count limits of design §4.1.5 on an in-memory Rulebook, so that
/// the printer never writes a document its own parser would refuse.
fn check_limits(rulebook: &Rulebook) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    let ids: BTreeSet<&str> = rulebook
        .revisions
        .iter()
        .map(|r| r.key.id.as_str())
        .collect();
    if ids.len() > MAX_RULES_PER_RULEBOOK as usize {
        found.push(limit_diagnostic("rules", ids.len(), MAX_RULES_PER_RULEBOOK));
    }
    if rulebook.revisions.len() > MAX_REVISIONS_PER_RULEBOOK as usize {
        found.push(limit_diagnostic(
            "rule revisions",
            rulebook.revisions.len(),
            MAX_REVISIONS_PER_RULEBOOK,
        ));
    }
    for rev in &rulebook.revisions {
        if rev.content.spans.len() > MAX_SPANS_PER_RULE as usize {
            found.push(limit_diagnostic(
                &format!(
                    "spans in rule '{}' revision {}",
                    rev.key.id, rev.key.revision
                ),
                rev.content.spans.len(),
                MAX_SPANS_PER_RULE,
            ));
        }
    }
    for p in &rulebook.precedence {
        if p.spans.len() > MAX_SPANS_PER_RULE as usize {
            found.push(limit_diagnostic(
                &format!("spans in precedence declaration '{}'", p.id),
                p.spans.len(),
                MAX_SPANS_PER_RULE,
            ));
        }
    }
    found.sort();
    found
}

fn rulebook_out(rulebook: &Rulebook) -> Out {
    let mut revisions: Vec<&RuleRevision> = rulebook.revisions.iter().collect();
    revisions.sort_by(|a, b| a.key.cmp(&b.key));
    let mut documents: Vec<&DocumentDecl> = rulebook.documents.iter().collect();
    documents.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.sha256.cmp(&b.sha256)));
    obj(vec![
        ("format", s(FORMAT_NAME)),
        ("ir_version", s(rulebook.ir_version.as_str())),
        ("name", s(&rulebook.name)),
        ("version", s(&rulebook.version)),
        (
            "documents",
            Out::Arr(documents.into_iter().map(document_out).collect()),
        ),
        ("types", types_out(&rulebook.types)),
        (
            "rules",
            Out::Arr(revisions.into_iter().map(revision_out).collect()),
        ),
        (
            "precedence",
            Out::Arr(rulebook.precedence.iter().map(precedence_out).collect()),
        ),
    ])
}

/// Prints a Rulebook in the pretty form: two-space indent, one member or
/// element per line and a trailing line feed. This is the form of a Rulebook
/// file. The same Rulebook always gives the same bytes, and
/// [`parse_rulebook`](crate::parse_rulebook) accepts them.
///
/// # Errors
/// `RT-LIM-0004` for each count limit of design §4.1.5 that the Rulebook
/// exceeds (rules, revisions, spans per rule). Nothing is printed then, because
/// the parser would refuse it.
pub fn print_pretty(rulebook: &Rulebook) -> Result<String, Vec<Diagnostic>> {
    let found = check_limits(rulebook);
    if found.is_empty() {
        Ok(write(&rulebook_out(rulebook), true))
    } else {
        Err(found)
    }
}

/// Prints a Rulebook in the compact form: no whitespace and no trailing line
/// feed.
///
/// # Errors
/// The same `RT-LIM-0004` diagnostics as [`print_pretty`].
pub fn print_compact(rulebook: &Rulebook) -> Result<String, Vec<Diagnostic>> {
    let found = check_limits(rulebook);
    if found.is_empty() {
        Ok(write(&rulebook_out(rulebook), false))
    } else {
        Err(found)
    }
}
