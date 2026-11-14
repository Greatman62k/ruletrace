//! The structural invariants of a Compiled_Rulebook.
//!
//! The Compiler builds a rulebook that satisfies these by construction, and the
//! parser of the artifact runs the same checks, so an engine never meets a
//! dangling index, a cyclic arena or an ill-typed node.

use crate::model::{
    CType, CompiledBody, CompiledLiteral, CompiledNode, CompiledParts, CompiledPrecedence,
    CompiledRule, CompiledSpan, NodeKind,
};
use crate::{NodeIdx, RuleIdx};
use rt_ir::{AggFn, CmpOp};
use std::collections::BTreeSet;
use std::fmt;

/// Which invariant failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompiledErrorKind {
    /// A Compiled_Rulebook has no rule (Req 9.13).
    NoRules,
    /// Identifiers are not strictly ascending, or a name is repeated.
    NotCanonicalOrder,
    /// An index points outside its table, or a node refers to a later node.
    BadIndex,
    /// A type does not match what the node, field or declaration requires.
    TypeMismatch,
    /// A Source_Span is empty, reversed, has a zero line or column, or is
    /// missing.
    BadSpan,
    /// A span or document entry does not agree with the document table.
    DocumentMismatch,
    /// The evaluation order is not a permutation of the rules.
    BadOrder,
    /// A node has the wrong number of operands.
    BadArity,
    /// A Precedence_Declaration lists a rule of another Target or repeats a
    /// rule.
    BadPrecedence,
}

impl CompiledErrorKind {
    fn text(self) -> &'static str {
        match self {
            CompiledErrorKind::NoRules => "a Compiled_Rulebook needs at least one rule",
            CompiledErrorKind::NotCanonicalOrder => "not in canonical order or repeated",
            CompiledErrorKind::BadIndex => "index out of range or not an earlier node",
            CompiledErrorKind::TypeMismatch => "type does not fit",
            CompiledErrorKind::BadSpan => "invalid or missing Source_Span",
            CompiledErrorKind::DocumentMismatch => "document table does not agree",
            CompiledErrorKind::BadOrder => "evaluation order is not a permutation of the rules",
            CompiledErrorKind::BadArity => "wrong number of operands",
            CompiledErrorKind::BadPrecedence => "invalid precedence order",
        }
    }
}

/// A violated invariant and where it is, as a path such as `rules[2].nodes[5]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledError {
    /// The path of the offending element.
    pub at: String,
    /// The invariant.
    pub kind: CompiledErrorKind,
}

impl fmt::Display for CompiledError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.at, self.kind.text())
    }
}

impl std::error::Error for CompiledError {}

type Check = Result<(), CompiledError>;

fn fail(at: impl Into<String>, kind: CompiledErrorKind) -> Check {
    Err(CompiledError {
        at: at.into(),
        kind,
    })
}

fn ensure(ok: bool, at: impl FnOnce() -> String, kind: CompiledErrorKind) -> Check {
    if ok {
        Ok(())
    } else {
        fail(at(), kind)
    }
}

pub(crate) fn check(p: &CompiledParts) -> Check {
    ensure(
        !p.rules.is_empty(),
        || "rules".to_owned(),
        CompiledErrorKind::NoRules,
    )?;
    schema(p)?;
    documents(p)?;
    let targets = crate::targets_of(&p.rules);
    ensure(
        p.rules.windows(2).all(|w| match w {
            [a, b] => a.id < b.id,
            _ => true,
        }),
        || "rules".to_owned(),
        CompiledErrorKind::NotCanonicalOrder,
    )?;
    for (i, rule) in p.rules.iter().enumerate() {
        self::rule(p, targets.len(), i, rule)?;
    }
    order(p)?;
    precedence(p, &targets)
}

fn unique<'a>(names: impl Iterator<Item = &'a str>, at: &str) -> Check {
    let mut seen = BTreeSet::new();
    for name in names {
        ensure(
            seen.insert(name),
            || at.to_owned(),
            CompiledErrorKind::NotCanonicalOrder,
        )?;
    }
    Ok(())
}

fn ty_in_range(p: &CompiledParts, ty: CType, at: &str) -> Check {
    match ty {
        CType::Enum(e) => ensure(
            e.at() < p.enums.len(),
            || at.to_owned(),
            CompiledErrorKind::BadIndex,
        ),
        _ => Ok(()),
    }
}

fn literal(p: &CompiledParts, lit: &CompiledLiteral, at: &str) -> Check {
    if let CompiledLiteral::Enum {
        enumeration,
        member,
    } = lit
    {
        let found = p
            .enums
            .get(enumeration.at())
            .is_some_and(|e| member.at() < e.members.len());
        ensure(found, || at.to_owned(), CompiledErrorKind::BadIndex)?;
    }
    Ok(())
}

fn schema(p: &CompiledParts) -> Check {
    unique(p.inputs.iter().map(|i| i.name.as_str()), "inputs")?;
    unique(p.collections.iter().map(|c| c.name.as_str()), "collections")?;
    unique(p.enums.iter().map(|e| e.name.as_str()), "enums")?;
    unique(p.sets.iter().map(|s| s.name.as_str()), "sets")?;
    for (i, input) in p.inputs.iter().enumerate() {
        let at = format!("inputs[{i}].type");
        ty_in_range(p, input.ty, &at)?;
        ensure(
            input.ty != CType::Ranking,
            || at.clone(),
            CompiledErrorKind::TypeMismatch,
        )?;
    }
    for (i, c) in p.collections.iter().enumerate() {
        unique(
            c.fields.iter().map(|f| f.name.as_str()),
            &format!("collections[{i}].fields"),
        )?;
        for (j, f) in c.fields.iter().enumerate() {
            let at = format!("collections[{i}].fields[{j}].type");
            ty_in_range(p, f.ty, &at)?;
            ensure(
                f.ty != CType::Ranking,
                || at.clone(),
                CompiledErrorKind::TypeMismatch,
            )?;
        }
    }
    for (i, e) in p.enums.iter().enumerate() {
        unique(
            e.members.iter().map(|m| m.as_str()),
            &format!("enums[{i}].members"),
        )?;
    }
    for (i, s) in p.sets.iter().enumerate() {
        let at = format!("sets[{i}]");
        ty_in_range(p, s.element_type, &at)?;
        for (j, m) in s.members.iter().enumerate() {
            let at = format!("sets[{i}].members[{j}]");
            literal(p, m, &at)?;
            ensure(
                m.ty() == s.element_type,
                || at.clone(),
                CompiledErrorKind::TypeMismatch,
            )?;
        }
    }
    Ok(())
}

fn documents(p: &CompiledParts) -> Check {
    ensure(
        p.documents.windows(2).all(|w| match w {
            [a, b] => a.id < b.id,
            _ => true,
        }),
        || "documents".to_owned(),
        CompiledErrorKind::NotCanonicalOrder,
    )?;
    let mut referenced = BTreeSet::new();
    let all = p
        .rules
        .iter()
        .enumerate()
        .flat_map(|(i, r)| {
            r.spans
                .iter()
                .enumerate()
                .map(move |(j, s)| (format!("rules[{i}].spans[{j}]"), s))
        })
        .chain(p.precedence.iter().enumerate().flat_map(|(i, d)| {
            d.spans
                .iter()
                .enumerate()
                .map(move |(j, s)| (format!("precedence[{i}].spans[{j}]"), s))
        }));
    for (at, span) in all {
        span_ok(span, &at)?;
        let declared = p
            .documents
            .iter()
            .any(|d| d.id == span.document && d.sha256 == span.sha256);
        ensure(declared, || at.clone(), CompiledErrorKind::DocumentMismatch)?;
        referenced.insert(&span.document);
    }
    for (i, d) in p.documents.iter().enumerate() {
        ensure(
            referenced.contains(&d.id),
            || format!("documents[{i}]"),
            CompiledErrorKind::DocumentMismatch,
        )?;
    }
    Ok(())
}

fn span_ok(s: &CompiledSpan, at: &str) -> Check {
    ensure(
        s.start < s.end && s.line >= 1 && s.column >= 1,
        || at.to_owned(),
        CompiledErrorKind::BadSpan,
    )
}

fn node_ty(nodes: &[CompiledNode], idx: NodeIdx) -> Option<&CType> {
    nodes.get(idx.at()).map(|n| &n.ty)
}

fn rule(p: &CompiledParts, n_targets: usize, i: usize, r: &CompiledRule) -> Check {
    let base = format!("rules[{i}]");
    ty_in_range(p, r.ty, &format!("{base}.type"))?;
    ensure(
        !r.spans.is_empty(),
        || format!("{base}.spans"),
        CompiledErrorKind::BadSpan,
    )?;
    for (j, node) in r.nodes.iter().enumerate() {
        let at = format!("{base}.nodes[{j}]");
        ty_in_range(p, node.ty, &at)?;
        let own = NodeIdx::from_usize(j);
        node_ok(p, n_targets, r, own, node, &at)?;
    }
    body(p, r, &base)
}

/// A child must be an earlier node. This makes the arena acyclic by
/// construction.
fn child(r: &CompiledRule, own: NodeIdx, c: NodeIdx, at: &str) -> Result<CType, CompiledError> {
    let earlier = c < own;
    match (earlier, node_ty(&r.nodes, c)) {
        (true, Some(ty)) => Ok(*ty),
        _ => Err(CompiledError {
            at: at.to_owned(),
            kind: CompiledErrorKind::BadIndex,
        }),
    }
}

fn node_ok(
    p: &CompiledParts,
    n_targets: usize,
    r: &CompiledRule,
    own: Option<NodeIdx>,
    node: &CompiledNode,
    at: &str,
) -> Check {
    let Some(own) = own else {
        return fail(at, CompiledErrorKind::BadIndex);
    };
    let ty_is = |want: CType| {
        ensure(
            node.ty == want,
            || at.to_owned(),
            CompiledErrorKind::TypeMismatch,
        )
    };
    let operand = |c: NodeIdx, want: CType| -> Check {
        let got = child(r, own, c, at)?;
        ensure(
            got == want,
            || at.to_owned(),
            CompiledErrorKind::TypeMismatch,
        )
    };
    match &node.kind {
        NodeKind::Literal(lit) => {
            literal(p, lit, at)?;
            ty_is(lit.ty())
        }
        NodeKind::Field(input) => {
            let declared = p.inputs.get(input.at()).map(|d| d.ty);
            ensure(
                declared.is_some(),
                || at.to_owned(),
                CompiledErrorKind::BadIndex,
            )?;
            ensure(
                declared == Some(node.ty),
                || at.to_owned(),
                CompiledErrorKind::TypeMismatch,
            )
        }
        NodeKind::Ref(t) => ensure(
            t.at() < n_targets,
            || at.to_owned(),
            CompiledErrorKind::BadIndex,
        ),
        NodeKind::RankReference(t) => {
            ensure(
                t.at() < n_targets,
                || at.to_owned(),
                CompiledErrorKind::BadIndex,
            )?;
            ty_is(CType::Number)
        }
        NodeKind::Binary { left, right, .. } => {
            operand(*left, CType::Number)?;
            operand(*right, CType::Number)?;
            ty_is(CType::Number)
        }
        NodeKind::Cmp { op, left, right } => {
            let l = child(r, own, *left, at)?;
            let rt = child(r, own, *right, at)?;
            ensure(l == rt, || at.to_owned(), CompiledErrorKind::TypeMismatch)?;
            let ordered = matches!(op, CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge);
            ensure(
                !ordered || matches!(l, CType::Number | CType::Date),
                || at.to_owned(),
                CompiledErrorKind::TypeMismatch,
            )?;
            ty_is(CType::Boolean)
        }
        NodeKind::And(args) | NodeKind::Or(args) => {
            ensure(
                (2..=64).contains(&args.len()),
                || at.to_owned(),
                CompiledErrorKind::BadArity,
            )?;
            for a in args {
                operand(*a, CType::Boolean)?;
            }
            ty_is(CType::Boolean)
        }
        NodeKind::Not(arg) => {
            operand(*arg, CType::Boolean)?;
            ty_is(CType::Boolean)
        }
        NodeKind::In { value, set } => {
            let v = child(r, own, *value, at)?;
            let element = p.sets.get(set.at()).map(|s| s.element_type);
            ensure(
                element.is_some(),
                || at.to_owned(),
                CompiledErrorKind::BadIndex,
            )?;
            ensure(
                element == Some(v),
                || at.to_owned(),
                CompiledErrorKind::TypeMismatch,
            )?;
            ty_is(CType::Boolean)
        }
        NodeKind::Agg {
            func,
            collection,
            field,
        } => {
            let c = p.collections.get(collection.at());
            ensure(c.is_some(), || at.to_owned(), CompiledErrorKind::BadIndex)?;
            match (func, field) {
                (AggFn::Count, None) => {}
                (AggFn::Count, Some(_)) | (_, None) => {
                    return fail(at, CompiledErrorKind::BadArity);
                }
                (_, Some(f)) => {
                    let fty = c.and_then(|c| c.fields.get(f.at())).map(|f| f.ty);
                    ensure(fty.is_some(), || at.to_owned(), CompiledErrorKind::BadIndex)?;
                    ensure(
                        fty == Some(CType::Number),
                        || at.to_owned(),
                        CompiledErrorKind::TypeMismatch,
                    )?;
                }
            }
            ty_is(CType::Number)
        }
        NodeKind::Round { arg, .. } => {
            operand(*arg, CType::Number)?;
            ty_is(CType::Number)
        }
    }
}

fn body(p: &CompiledParts, r: &CompiledRule, base: &str) -> Check {
    let root = |idx: NodeIdx, want: Option<CType>, name: &str| -> Check {
        let at = format!("{base}.{name}");
        let got = node_ty(&r.nodes, idx).copied();
        ensure(got.is_some(), || at.clone(), CompiledErrorKind::BadIndex)?;
        match want {
            Some(w) => ensure(
                got == Some(w),
                || at.clone(),
                CompiledErrorKind::TypeMismatch,
            ),
            None => Ok(()),
        }
    };
    let rule_ty = |want: CType| {
        ensure(
            r.ty == want,
            || format!("{base}.type"),
            CompiledErrorKind::TypeMismatch,
        )
    };
    match &r.body {
        CompiledBody::Eligibility { expr } => {
            rule_ty(CType::Boolean)?;
            root(*expr, Some(CType::Boolean), "expr")
        }
        CompiledBody::Derived { expr } => {
            rule_ty(CType::Number)?;
            root(*expr, Some(CType::Number), "expr")
        }
        CompiledBody::WeightedScore { terms, .. } => {
            rule_ty(CType::Number)?;
            for (k, t) in terms.iter().enumerate() {
                root(t.weight, Some(CType::Number), &format!("terms[{k}].weight"))?;
                root(t.value, Some(CType::Number), &format!("terms[{k}].value"))?;
            }
            Ok(())
        }
        CompiledBody::Ranking {
            key, tie_breakers, ..
        } => {
            rule_ty(CType::Ranking)?;
            root(*key, None, "key")?;
            for (k, t) in tie_breakers.iter().enumerate() {
                root(t.expr, None, &format!("tie_breakers[{k}].expr"))?;
            }
            Ok(())
        }
        CompiledBody::Default { value } => {
            let at = format!("{base}.value");
            literal(p, value, &at)?;
            ensure(
                value.ty() == r.ty,
                || at.clone(),
                CompiledErrorKind::TypeMismatch,
            )
        }
    }
}

fn order(p: &CompiledParts) -> Check {
    let seen: BTreeSet<RuleIdx> = p.order.iter().copied().collect();
    let ok = p.order.len() == p.rules.len()
        && seen.len() == p.rules.len()
        && p.order.iter().all(|i| i.at() < p.rules.len());
    ensure(ok, || "order".to_owned(), CompiledErrorKind::BadOrder)
}

fn precedence(p: &CompiledParts, targets: &[rt_ir::Name]) -> Check {
    ensure(
        p.precedence.windows(2).all(|w| match w {
            [a, b] => a.id < b.id,
            _ => true,
        }),
        || "precedence".to_owned(),
        CompiledErrorKind::NotCanonicalOrder,
    )?;
    for (i, d) in p.precedence.iter().enumerate() {
        precedence_one(p, targets, i, d)?;
    }
    Ok(())
}

fn precedence_one(
    p: &CompiledParts,
    targets: &[rt_ir::Name],
    i: usize,
    d: &CompiledPrecedence,
) -> Check {
    let at = format!("precedence[{i}]");
    let target = targets.get(d.target.at());
    ensure(
        target.is_some(),
        || format!("{at}.target"),
        CompiledErrorKind::BadIndex,
    )?;
    let mut seen = BTreeSet::new();
    for (k, idx) in d.order.iter().enumerate() {
        let rule = p.rules.get(idx.at());
        ensure(
            rule.is_some(),
            || format!("{at}.order[{k}]"),
            CompiledErrorKind::BadIndex,
        )?;
        ensure(
            rule.map(|r| &r.target) == target && seen.insert(*idx),
            || format!("{at}.order[{k}]"),
            CompiledErrorKind::BadPrecedence,
        )?;
    }
    Ok(())
}
