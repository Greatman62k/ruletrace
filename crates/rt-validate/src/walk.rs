//! Passes 1 (types present, depth), 3 (names), 4 (types), 5 (numeric declarations) and 6
//! (boundary) over the expressions of one rule revision, plus the dependencies for pass 8.
//!
//! A node whose name does not resolve, or that is outside the boundary, gets the internal type
//! `Type::Error`. Every check treats `Error` as compatible with anything, so one defect gives
//! one Diagnostic (design §3.6.1). The walk descends at most `MAX_EXPRESSION_DEPTH` levels, so a
//! library-built tree of any depth cannot overflow the stack.
//!
//! Pass 5 has nothing left to check here: a rounding mode or places that are missing, unknown or
//! outside 0 to 28 cannot be represented by `rt_ir::Rounding`, `Places` or the `round` node, and
//! the parser reports them as `RT-IR-0010` or `RT-IR-0002` (exit 2) before validation starts.

use crate::ctx::{Decls, Site};
use rt_foundation::{Code, DiagnosticState, MAX_EXPRESSION_DEPTH};
use rt_ir::{
    AggFn, CmpOp, DefaultValue, Expr, ExprKind, Literal, Name, OpaqueConstruct, RuleBody,
    RuleContent, Type,
};
use std::collections::{BTreeMap, BTreeSet};

/// A value Target and the type its rule computes.
#[derive(Clone, Debug)]
pub(crate) struct TargetInfo {
    pub rule: rt_foundation::RuleId,
    pub ty: Type,
}

/// Value Targets of the current revisions, by Target name.
pub(crate) type Targets = BTreeMap<Name, Vec<TargetInfo>>;

/// What a rule reads (Req 6.10): input fields, collections and the Targets it references.
#[derive(Clone, Debug, Default)]
pub(crate) struct Deps {
    pub fields: BTreeSet<Name>,
    pub collections: BTreeSet<Name>,
    pub refs: BTreeSet<Name>,
}

/// The type a rule kind computes, or `None` for a `default` (the input's type) or an unrecognized kind.
pub(crate) fn kind_type(body: &RuleBody) -> Option<Type> {
    match body {
        RuleBody::Eligibility { .. } => Some(Type::Boolean),
        RuleBody::Derived { .. } | RuleBody::WeightedScore { .. } => Some(Type::Number),
        RuleBody::Ranking { .. } => Some(Type::Ranking),
        RuleBody::Default { .. } | RuleBody::Unrecognized(_) => None,
    }
}

/// Capability names of well-formed construct names that lie outside the boundary (Req 14.5,
/// 14.13). The table is part of the validator, which reads no file. `rank_reference` is
/// unsupported until the user selects Rank_Reference under UD-32.
fn capability(name: &str) -> String {
    let known = match name {
        "rank_reference" => "rank reference",
        "slot_allocation" => "slot-based qualification",
        "seeding" => "seeding",
        "bracket_generation" => "bracket generation",
        "progression" => "tournament progression",
        "prize_allocation" => "prize allocation",
        _ => return format!("unrecognized construct '{name}'"),
    };
    known.to_owned()
}

fn is_ordered(t: &Type) -> bool {
    matches!(t, Type::Number | Type::Date)
}

/// Walks the expressions of one revision.
pub(crate) struct Walk<'a, 'b> {
    pub decls: &'b Decls<'a>,
    pub targets: &'b Targets,
    pub site: &'b Site,
    pub out: &'b mut DiagnosticState,
    /// False for a Rejected or superseded revision: only pass 1 findings are reported.
    pub full: bool,
    pub deps: Deps,
    depth_reported: bool,
}

impl<'a, 'b> Walk<'a, 'b> {
    pub(crate) fn new(
        decls: &'b Decls<'a>,
        targets: &'b Targets,
        site: &'b Site,
        out: &'b mut DiagnosticState,
        full: bool,
    ) -> Self {
        Self {
            decls,
            targets,
            site,
            out,
            full,
            deps: Deps::default(),
            depth_reported: false,
        }
    }

    /// A pass 3, 4 or 6 finding. Dropped for a structural-only revision.
    fn sem(&mut self, code: Code, message: String) {
        if self.full {
            self.out.push(self.site.diag(code, message));
        }
    }

    fn undeclared(&mut self, kind: &str, name: &str, path: &str) {
        self.sem(
            Code::ValUndeclaredReference,
            format!("undeclared {kind} '{name}' at {path}"),
        );
    }

    fn mismatch(&mut self, path: &str, expected: &str, actual: &Type) {
        if *actual != Type::Error {
            self.sem(
                Code::ValTypeMismatch,
                format!(
                    "type mismatch at {path}: expected {expected}, found {}",
                    actual.text()
                ),
            );
        }
    }

    fn lit_type(&mut self, lit: &Literal, path: &str) -> Type {
        match lit {
            Literal::Number(_) => Type::Number,
            Literal::Boolean(_) => Type::Boolean,
            Literal::Text(_) => Type::Text,
            Literal::Date(_) => Type::Date,
            Literal::Enum { enum_name, member } => match self.decls.enums.get(enum_name) {
                None => {
                    self.undeclared("enumeration", enum_name.as_str(), path);
                    Type::Error
                }
                Some(decl) if !decl.members.contains(member) => {
                    self.undeclared("enumeration member", &format!("{enum_name}.{member}"), path);
                    Type::Error
                }
                Some(_) => Type::Enum(enum_name.clone()),
            },
        }
    }

    /// Checks the expression and returns the type its form computes.
    pub(crate) fn expr(&mut self, e: &Expr, path: &str, depth: u32) -> Type {
        if depth > MAX_EXPRESSION_DEPTH {
            if !self.depth_reported {
                self.depth_reported = true;
                self.out.push(self.site.diag(
                    Code::LimitExpressionTooDeep,
                    format!(
                        "expression at {path} is nested deeper than {MAX_EXPRESSION_DEPTH} levels"
                    ),
                ));
            }
            return Type::Error;
        }
        let computed = self.node(e, path, depth);
        match &e.ty {
            None => self.out.push(self.site.diag(
                Code::ValMissingTypeDeclaration,
                format!("expression at {path} has no declared type"),
            )),
            Some(declared) if computed != Type::Error && *declared != computed => self.sem(
                Code::ValTypeMismatch,
                format!(
                    "type mismatch at {path}: declared type {}, but the node computes {}",
                    declared.text(),
                    computed.text()
                ),
            ),
            Some(_) => {}
        }
        computed
    }

    fn operand(
        &mut self,
        e: &Expr,
        path: &str,
        depth: u32,
        expected: &str,
        ok: fn(&Type) -> bool,
    ) -> Type {
        let t = self.expr(e, path, depth + 1);
        if t != Type::Error && !ok(&t) {
            self.mismatch(path, expected, &t);
        }
        t
    }

    fn node(&mut self, e: &Expr, path: &str, depth: u32) -> Type {
        match &e.kind {
            ExprKind::Literal(l) => self.lit_type(l, path),
            ExprKind::Field { name } => match self.decls.inputs.get(name) {
                Some(t) => {
                    self.deps.fields.insert(name.clone());
                    (*t).clone()
                }
                None => {
                    self.undeclared("field", name.as_str(), path);
                    Type::Error
                }
            },
            ExprKind::Ref { target } => match self.targets.get(target) {
                Some(infos) => {
                    self.deps.refs.insert(target.clone());
                    match infos.split_first() {
                        Some((first, rest)) if rest.iter().all(|i| i.ty == first.ty) => {
                            first.ty.clone()
                        }
                        _ => Type::Error,
                    }
                }
                None => {
                    self.undeclared("rule", target.as_str(), path);
                    Type::Error
                }
            },
            ExprKind::Binary { left, right, .. } => {
                let is_num: fn(&Type) -> bool = |t| *t == Type::Number;
                self.operand(left, &format!("{path}.left"), depth, "number", is_num);
                self.operand(right, &format!("{path}.right"), depth, "number", is_num);
                Type::Number
            }
            ExprKind::Cmp { op, left, right } => {
                let ordered = !matches!(op, CmpOp::Eq | CmpOp::Ne);
                let lp = format!("{path}.left");
                let rp = format!("{path}.right");
                let (lt, rt) = if ordered {
                    (
                        self.operand(left, &lp, depth, "number or date", is_ordered),
                        self.operand(right, &rp, depth, "number or date", is_ordered),
                    )
                } else {
                    (
                        self.expr(left, &lp, depth + 1),
                        self.expr(right, &rp, depth + 1),
                    )
                };
                let comparable = |t: &Type| *t != Type::Error && (!ordered || is_ordered(t));
                if comparable(&lt) && comparable(&rt) && lt != rt {
                    self.mismatch(&rp, &lt.text(), &rt);
                }
                Type::Boolean
            }
            ExprKind::And(args) | ExprKind::Or(args) => {
                if !(2..=64).contains(&args.len()) {
                    self.sem(
                        Code::ValUnsupportedConstruct,
                        format!(
                            "unsupported construct at {path}: and/or with {} arguments (2 to 64 are supported)",
                            args.len()
                        ),
                    );
                }
                for (i, a) in args.iter().enumerate() {
                    let is_bool: fn(&Type) -> bool = |t| *t == Type::Boolean;
                    self.operand(a, &format!("{path}.args[{i}]"), depth, "boolean", is_bool);
                }
                Type::Boolean
            }
            ExprKind::Not(arg) => {
                let is_bool: fn(&Type) -> bool = |t| *t == Type::Boolean;
                self.operand(arg, &format!("{path}.arg"), depth, "boolean", is_bool);
                Type::Boolean
            }
            ExprKind::In { value, set } => {
                let vt = self.expr(value, &format!("{path}.value"), depth + 1);
                match self.decls.sets.get(set) {
                    None => self.undeclared("set", set.as_str(), path),
                    Some(decl) => {
                        if vt != Type::Error && vt != decl.element_type {
                            let expected = decl.element_type.text();
                            self.mismatch(&format!("{path}.value"), &expected, &vt);
                        }
                    }
                }
                Type::Boolean
            }
            ExprKind::Agg {
                func,
                collection,
                field,
            } => {
                match self.decls.collections.get(collection) {
                    None => self.undeclared("collection", collection.as_str(), path),
                    Some(fields) => {
                        self.deps.collections.insert(collection.clone());
                        match field {
                            Some(f) => match fields.get(f) {
                                None => self.undeclared(
                                    "collection field",
                                    &format!("{collection}.{f}"),
                                    path,
                                ),
                                Some(t) if *func != AggFn::Count && **t != Type::Number => {
                                    self.mismatch(path, "number element field", t);
                                }
                                Some(_) => {}
                            },
                            None if *func != AggFn::Count => self.undeclared(
                                "collection field",
                                &format!("{collection}.(none) for '{}'", func.name()),
                                path,
                            ),
                            None => {}
                        }
                    }
                }
                Type::Number
            }
            ExprKind::Round { arg, .. } => {
                let is_num: fn(&Type) -> bool = |t| *t == Type::Number;
                self.operand(arg, &format!("{path}.arg"), depth, "number", is_num);
                Type::Number
            }
            ExprKind::RankReference { .. } => {
                self.boundary(&capability("rank_reference"), path);
                Type::Error
            }
            ExprKind::Opaque(o) => {
                self.boundary(&capability(o.name.as_str()), path);
                Type::Error
            }
        }
    }

    fn boundary(&mut self, capability: &str, path: &str) {
        self.sem(
            Code::ValUnsupportedConstruct,
            format!("unsupported construct at {path}: {capability}"),
        );
    }

    /// Checks a rule's declared type against the type its kind computes (pass 4) and that a
    /// declared type is present (pass 1).
    fn rule_type(&mut self, content: &RuleContent, expected: &Type) {
        match &content.ty {
            None => self.out.push(self.site.diag(
                Code::ValMissingTypeDeclaration,
                format!("rule '{}' has no declared type", self.site.rule.rule),
            )),
            Some(t) if t != expected => self.sem(
                Code::ValTypeMismatch,
                format!(
                    "type mismatch at rule type: expected {}, found {}",
                    expected.text(),
                    t.text()
                ),
            ),
            Some(_) => {}
        }
    }

    fn expect(&mut self, e: &Expr, path: &str, expected: &str, ok: fn(&Type) -> bool) {
        let t = self.expr(e, path, 1);
        if t != Type::Error && !ok(&t) {
            self.mismatch(path, expected, &t);
        }
    }

    fn default_value(&mut self, content: &RuleContent, d: &DefaultValue) {
        let lit = self.lit_type(&d.literal, "value");
        let target = &content.target;
        match self.decls.inputs.get(target) {
            None => {
                self.undeclared("field", target.as_str(), "target");
                if let (Some(rt), true) = (&content.ty, lit != Type::Error) {
                    if *rt != lit {
                        self.mismatch("rule type", &lit.text(), rt);
                    }
                }
            }
            Some(input) => {
                self.deps.fields.insert(target.clone());
                let input = (*input).clone();
                if lit != Type::Error && lit != input {
                    self.mismatch("value", &input.text(), &lit);
                }
                match &content.ty {
                    None => self.out.push(self.site.diag(
                        Code::ValMissingTypeDeclaration,
                        format!("rule '{}' has no declared type", self.site.rule.rule),
                    )),
                    Some(t) if *t != input => self.mismatch("rule type", &input.text(), t),
                    Some(_) => {}
                }
            }
        }
    }

    /// Checks one revision body and its rule type.
    pub(crate) fn body(&mut self, content: &RuleContent) {
        let is_bool: fn(&Type) -> bool = |t| *t == Type::Boolean;
        let is_num: fn(&Type) -> bool = |t| *t == Type::Number;
        match &content.body {
            RuleBody::Eligibility { expr } => {
                self.rule_type(content, &Type::Boolean);
                self.expect(expr, "expr", "boolean", is_bool);
            }
            RuleBody::Derived { expr } => {
                self.rule_type(content, &Type::Number);
                self.expect(expr, "expr", "number", is_num);
            }
            RuleBody::WeightedScore { terms, .. } => {
                self.rule_type(content, &Type::Number);
                for (i, t) in terms.iter().enumerate() {
                    self.expect(&t.weight, &format!("terms[{i}].weight"), "number", is_num);
                    self.expect(&t.value, &format!("terms[{i}].value"), "number", is_num);
                }
            }
            RuleBody::Ranking {
                key, tie_breakers, ..
            } => {
                self.rule_type(content, &Type::Ranking);
                self.expect(key, "key", "number or date", is_ordered);
                for (i, t) in tie_breakers.iter().enumerate() {
                    self.expect(
                        &t.expr,
                        &format!("tie_breakers[{i}].expr"),
                        "number or date",
                        is_ordered,
                    );
                }
            }
            RuleBody::Default { value } => self.default_value(content, value),
            RuleBody::Unrecognized(o) => self.unrecognized_kind(o, content),
        }
    }

    fn unrecognized_kind(&mut self, o: &OpaqueConstruct, content: &RuleContent) {
        if content.ty.is_none() {
            self.out.push(self.site.diag(
                Code::ValMissingTypeDeclaration,
                format!("rule '{}' has no declared type", self.site.rule.rule),
            ));
        }
        self.sem(
            Code::ValUnsupportedConstruct,
            format!(
                "unsupported rule kind '{}': {}",
                o.name.as_str(),
                capability(o.name.as_str())
            ),
        );
    }
}
