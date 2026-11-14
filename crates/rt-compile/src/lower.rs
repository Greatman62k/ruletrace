//! Lowering of Effective revisions to the compiled form (design §3.6.2 step 4, §4.1.6).
//!
//! Names become indices, declared types become resolved types, spans are copied in order, and
//! Origin and approval data are dropped. A construct that cannot be lowered (an unresolved
//! name, an unrecognized construct, a missing type) is a [`LowerError`]. The Validator has
//! already reported every such defect, so this is a defensive path.

use rt_compiled::{
    CType, CollectionIdx, CompiledBody, CompiledCollection, CompiledDocument, CompiledEnum,
    CompiledField, CompiledInput, CompiledLiteral, CompiledNode, CompiledParts, CompiledPrecedence,
    CompiledRule, CompiledSet, CompiledSpan, CompiledTerm, CompiledTieBreaker, EnumIdx, FieldIdx,
    InputIdx, MemberIdx, NodeIdx, NodeKind, RuleIdx, SetIdx, TargetIdx,
};
use rt_foundation::{DocumentId, RuleId};
use rt_hash::Sha256Hex;
use rt_ir::{
    Expr, ExprKind, Literal, Name, Rounding, RuleBody, RuleKey, RuleRevision, Rulebook, Span, Type,
};
use std::collections::{BTreeMap, BTreeSet};

/// A construct that cannot be lowered, with the rule it was found in, when there is one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LowerError {
    pub rule: Option<RuleId>,
    pub message: String,
}

pub(crate) type Lowered<T> = Result<T, LowerError>;

pub(crate) fn err<T>(rule: Option<&RuleId>, message: impl Into<String>) -> Lowered<T> {
    Err(LowerError {
        rule: rule.cloned(),
        message: message.into(),
    })
}

fn idx<T>(value: usize, make: impl Fn(u32) -> T, what: &str) -> Lowered<T> {
    match u32::try_from(value) {
        Ok(v) => Ok(make(v)),
        Err(_) => err(None, format!("too many {what}")),
    }
}

/// The fields of one collection by name.
type Fields<'a> = BTreeMap<&'a Name, (FieldIdx, CType)>;

/// The declarations of the Rulebook as index tables, plus the Targets of the compiled rules.
struct Tables<'a> {
    rb: &'a Rulebook,
    enums: BTreeMap<&'a Name, EnumIdx>,
    inputs: BTreeMap<&'a Name, (InputIdx, CType)>,
    sets: BTreeMap<&'a Name, SetIdx>,
    collections: BTreeMap<&'a Name, (CollectionIdx, Fields<'a>)>,
    targets: BTreeMap<&'a Name, TargetIdx>,
}

impl<'a> Tables<'a> {
    fn ty(&self, t: &Type) -> Lowered<CType> {
        match t {
            Type::Number => Ok(CType::Number),
            Type::Boolean => Ok(CType::Boolean),
            Type::Text => Ok(CType::Text),
            Type::Date => Ok(CType::Date),
            Type::Ranking => Ok(CType::Ranking),
            Type::Enum(name) => match self.enums.get(name) {
                Some(e) => Ok(CType::Enum(*e)),
                None => err(None, format!("enum '{name}' is not declared")),
            },
            Type::Error => err(None, "an expression has the internal error type"),
        }
    }

    fn literal(&self, lit: &Literal) -> Lowered<CompiledLiteral> {
        Ok(match lit {
            Literal::Number(n) => CompiledLiteral::Number(n.clone()),
            Literal::Boolean(b) => CompiledLiteral::Boolean(*b),
            Literal::Text(t) => CompiledLiteral::Text(t.clone()),
            Literal::Date(d) => CompiledLiteral::Date(*d),
            Literal::Enum { enum_name, member } => {
                let Some(e) = self.enums.get(enum_name) else {
                    return err(None, format!("enum '{enum_name}' is not declared"));
                };
                let position = self
                    .rb
                    .types
                    .enums
                    .iter()
                    .find(|d| &d.name == enum_name)
                    .and_then(|d| d.members.iter().position(|m| m == member));
                let Some(position) = position else {
                    return err(None, format!("'{enum_name}' has no member '{member}'"));
                };
                CompiledLiteral::Enum {
                    enumeration: *e,
                    member: idx(position, MemberIdx::new, "enum members")?,
                }
            }
        })
    }
}

/// The node arena of one rule.
struct Arena<'t, 'a> {
    t: &'t Tables<'a>,
    rule: &'t RuleId,
    nodes: Vec<CompiledNode>,
}

impl Arena<'_, '_> {
    fn push(&mut self, ty: CType, kind: NodeKind) -> Lowered<NodeIdx> {
        let at = idx(self.nodes.len(), NodeIdx::new, "nodes")?;
        self.nodes.push(CompiledNode { ty, kind });
        Ok(at)
    }

    fn declared(&self, e: &Expr) -> Lowered<CType> {
        match &e.ty {
            Some(t) => self.t.ty(t),
            None => err(Some(self.rule), "an expression has no declared type"),
        }
    }

    fn expr(&mut self, e: &Expr) -> Lowered<NodeIdx> {
        match &e.kind {
            ExprKind::Literal(lit) => {
                let lit = self.t.literal(lit)?;
                self.push(lit.ty(), NodeKind::Literal(lit))
            }
            ExprKind::Field { name } => match self.t.inputs.get(name) {
                Some((i, ty)) => self.push(*ty, NodeKind::Field(*i)),
                None => err(Some(self.rule), format!("input '{name}' is not declared")),
            },
            ExprKind::Ref { target } => {
                let ty = self.declared(e)?;
                let t = self.target(target)?;
                self.push(ty, NodeKind::Ref(t))
            }
            ExprKind::RankReference { target } => {
                let t = self.target(target)?;
                self.push(CType::Number, NodeKind::RankReference(t))
            }
            ExprKind::Binary { op, left, right } => {
                let (left, right) = (self.expr(left)?, self.expr(right)?);
                self.push(
                    CType::Number,
                    NodeKind::Binary {
                        op: *op,
                        left,
                        right,
                    },
                )
            }
            ExprKind::Cmp { op, left, right } => {
                let (left, right) = (self.expr(left)?, self.expr(right)?);
                self.push(
                    CType::Boolean,
                    NodeKind::Cmp {
                        op: *op,
                        left,
                        right,
                    },
                )
            }
            ExprKind::And(args) => {
                let args = self.exprs(args)?;
                self.push(CType::Boolean, NodeKind::And(args))
            }
            ExprKind::Or(args) => {
                let args = self.exprs(args)?;
                self.push(CType::Boolean, NodeKind::Or(args))
            }
            ExprKind::Not(arg) => {
                let arg = self.expr(arg)?;
                self.push(CType::Boolean, NodeKind::Not(arg))
            }
            ExprKind::In { value, set } => {
                let value = self.expr(value)?;
                match self.t.sets.get(set) {
                    Some(s) => self.push(CType::Boolean, NodeKind::In { value, set: *s }),
                    None => err(Some(self.rule), format!("set '{set}' is not declared")),
                }
            }
            ExprKind::Agg {
                func,
                collection,
                field,
            } => {
                let Some((c, fields)) = self.t.collections.get(collection) else {
                    return err(
                        Some(self.rule),
                        format!("collection '{collection}' is not declared"),
                    );
                };
                let field = match field {
                    None => None,
                    Some(f) => match fields.get(f) {
                        Some((fi, _)) => Some(*fi),
                        None => {
                            return err(
                                Some(self.rule),
                                format!("collection '{collection}' has no field '{f}'"),
                            )
                        }
                    },
                };
                self.push(
                    CType::Number,
                    NodeKind::Agg {
                        func: *func,
                        collection: *c,
                        field,
                    },
                )
            }
            ExprKind::Round { arg, places, mode } => {
                let arg = self.expr(arg)?;
                self.push(
                    CType::Number,
                    NodeKind::Round {
                        arg,
                        rounding: Rounding {
                            mode: *mode,
                            places: *places,
                        },
                    },
                )
            }
            ExprKind::Opaque(o) => err(
                Some(self.rule),
                format!(
                    "unrecognized construct '{}' cannot compile",
                    o.name.as_str()
                ),
            ),
        }
    }

    fn exprs(&mut self, args: &[Expr]) -> Lowered<Vec<NodeIdx>> {
        args.iter().map(|a| self.expr(a)).collect()
    }

    fn target(&self, name: &Name) -> Lowered<TargetIdx> {
        match self.t.targets.get(name) {
            Some(t) => Ok(*t),
            None => err(
                Some(self.rule),
                format!("no compiled rule defines the Target '{name}'"),
            ),
        }
    }
}

/// The declared hash of a document, which must be declared with one hash only.
fn doc_hash(rb: &Rulebook, id: &DocumentId) -> Lowered<Sha256Hex> {
    let mut found: Option<&Sha256Hex> = None;
    for d in rb.documents.iter().filter(|d| &d.id == id) {
        match found {
            Some(prev) if prev != &d.sha256 => {
                return err(None, format!("document '{id}' is declared with two hashes"))
            }
            _ => found = Some(&d.sha256),
        }
    }
    match found {
        Some(h) => Ok(h.clone()),
        None => err(None, format!("document '{id}' is not declared")),
    }
}

fn spans(rb: &Rulebook, spans: &[Span]) -> Lowered<Vec<CompiledSpan>> {
    spans
        .iter()
        .map(|s| {
            Ok(CompiledSpan {
                document: s.document.clone(),
                sha256: doc_hash(rb, &s.document)?,
                start: s.start,
                end: s.end,
                line: s.line,
                column: s.column,
            })
        })
        .collect()
}

fn body(a: &mut Arena<'_, '_>, b: &RuleBody) -> Lowered<CompiledBody> {
    Ok(match b {
        RuleBody::Eligibility { expr } => CompiledBody::Eligibility {
            expr: a.expr(expr)?,
        },
        RuleBody::Derived { expr } => CompiledBody::Derived {
            expr: a.expr(expr)?,
        },
        RuleBody::WeightedScore { terms, rounding } => {
            let mut out = Vec::with_capacity(terms.len());
            for t in terms {
                let weight = a.expr(&t.weight)?;
                let value = a.expr(&t.value)?;
                out.push(CompiledTerm { weight, value });
            }
            CompiledBody::WeightedScore {
                terms: out,
                rounding: *rounding,
            }
        }
        RuleBody::Ranking {
            key,
            direction,
            tie_breakers,
            ties,
        } => {
            let key = a.expr(key)?;
            let mut tbs = Vec::with_capacity(tie_breakers.len());
            for t in tie_breakers {
                tbs.push(CompiledTieBreaker {
                    expr: a.expr(&t.expr)?,
                    direction: t.direction,
                });
            }
            CompiledBody::Ranking {
                key,
                direction: *direction,
                tie_breakers: tbs,
                ties: *ties,
            }
        }
        RuleBody::Default { value } => CompiledBody::Default {
            value: a.t.literal(&value.literal)?,
        },
        RuleBody::Unrecognized(o) => {
            return err(
                Some(a.rule),
                format!(
                    "unrecognized rule kind '{}' cannot compile",
                    o.name.as_str()
                ),
            )
        }
    })
}

fn schema<'a>(
    rb: &'a Rulebook,
    targets: &BTreeMap<&'a Name, TargetIdx>,
) -> Lowered<(Tables<'a>, Schema)> {
    let mut t = Tables {
        rb,
        enums: BTreeMap::new(),
        inputs: BTreeMap::new(),
        sets: BTreeMap::new(),
        collections: BTreeMap::new(),
        targets: targets.clone(),
    };
    // Enums first: every other declaration may name one. The first declaration of a name is the
    // one that resolves; a repeated name is rejected by the Compiled_Rulebook checks.
    for (i, e) in rb.types.enums.iter().enumerate() {
        t.enums
            .entry(&e.name)
            .or_insert(idx(i, EnumIdx::new, "enums")?);
    }
    let mut s = Schema::default();
    for e in &rb.types.enums {
        s.enums.push(CompiledEnum {
            name: e.name.clone(),
            members: e.members.clone(),
        });
    }
    for (i, d) in rb.types.inputs.iter().enumerate() {
        let ty = t.ty(&d.ty)?;
        t.inputs
            .entry(&d.name)
            .or_insert((idx(i, InputIdx::new, "inputs")?, ty));
        s.inputs.push(CompiledInput {
            name: d.name.clone(),
            ty,
        });
    }
    for (i, d) in rb.types.collections.iter().enumerate() {
        let mut fields = BTreeMap::new();
        let mut out = Vec::new();
        for (j, f) in d.fields.iter().enumerate() {
            let ty = t.ty(&f.ty)?;
            fields
                .entry(&f.name)
                .or_insert((idx(j, FieldIdx::new, "fields")?, ty));
            out.push(CompiledField {
                name: f.name.clone(),
                ty,
            });
        }
        t.collections
            .entry(&d.name)
            .or_insert((idx(i, CollectionIdx::new, "collections")?, fields));
        s.collections.push(CompiledCollection {
            name: d.name.clone(),
            fields: out,
        });
    }
    for (i, d) in rb.types.sets.iter().enumerate() {
        t.sets
            .entry(&d.name)
            .or_insert(idx(i, SetIdx::new, "sets")?);
        let members = d
            .members
            .iter()
            .map(|m| t.literal(m))
            .collect::<Lowered<Vec<_>>>()?;
        s.sets.push(CompiledSet {
            name: d.name.clone(),
            element_type: t.ty(&d.element_type)?,
            members,
        });
    }
    Ok((t, s))
}

#[derive(Default)]
struct Schema {
    inputs: Vec<CompiledInput>,
    collections: Vec<CompiledCollection>,
    enums: Vec<CompiledEnum>,
    sets: Vec<CompiledSet>,
}

/// [`CompiledParts`] before the order and the hash are known.
pub(crate) struct PartsWithoutOrder {
    pub documents: Vec<CompiledDocument>,
    pub inputs: Vec<CompiledInput>,
    pub collections: Vec<CompiledCollection>,
    pub enums: Vec<CompiledEnum>,
    pub sets: Vec<CompiledSet>,
    pub rules: Vec<CompiledRule>,
    pub precedence: Vec<CompiledPrecedence>,
}

impl PartsWithoutOrder {
    pub(crate) fn into_parts(
        self,
        compiler_version: String,
        ir_version: rt_ir::IrVersion,
        rulebook_hash: Sha256Hex,
        order: Vec<RuleIdx>,
    ) -> CompiledParts {
        CompiledParts {
            compiler_version,
            ir_version,
            rulebook_hash,
            documents: self.documents,
            inputs: self.inputs,
            collections: self.collections,
            enums: self.enums,
            sets: self.sets,
            order,
            rules: self.rules,
            precedence: self.precedence,
        }
    }
}

/// Lowers the Effective revisions (ascending by rule identifier) and the Precedence_Declarations.
pub(crate) fn lower(
    rb: &Rulebook,
    effective: &BTreeMap<RuleId, RuleKey>,
) -> Lowered<PartsWithoutOrder> {
    let revisions: Vec<&RuleRevision> = effective.values().filter_map(|k| rb.revision(k)).collect();
    if revisions.len() != effective.len() {
        return err(None, "an Effective revision is missing from the Rulebook");
    }
    let target_names: BTreeSet<&Name> = revisions.iter().map(|r| &r.content.target).collect();
    let mut targets: BTreeMap<&Name, TargetIdx> = BTreeMap::new();
    for (i, n) in target_names.into_iter().enumerate() {
        targets.insert(n, idx(i, TargetIdx::new, "targets")?);
    }
    let (tables, s) = schema(rb, &targets)?;

    let mut rules = Vec::with_capacity(revisions.len());
    for rev in &revisions {
        let c = &rev.content;
        let ty = match &c.ty {
            Some(t) => tables.ty(t)?,
            None => return err(Some(&rev.key.id), "the rule has no declared type"),
        };
        let mut arena = Arena {
            t: &tables,
            rule: &rev.key.id,
            nodes: Vec::new(),
        };
        let b = body(&mut arena, &c.body)?;
        rules.push(CompiledRule {
            id: rev.key.id.clone(),
            revision: rev.key.revision,
            target: c.target.clone(),
            ty,
            spans: spans(rb, &c.spans)?,
            nodes: arena.nodes,
            body: b,
        });
    }

    let mut decls: Vec<&rt_ir::Precedence> = rb.precedence.iter().collect();
    decls.sort_by(|a, b| a.id.cmp(&b.id));
    let mut precedence = Vec::with_capacity(decls.len());
    for p in decls {
        let Some(target) = tables.targets.get(&p.target) else {
            return err(
                None,
                format!(
                    "precedence '{}' orders the Target '{}', which no compiled rule defines",
                    p.id, p.target
                ),
            );
        };
        let mut order = Vec::with_capacity(p.order.len());
        for id in &p.order {
            match rules.binary_search_by(|r| r.id.cmp(id)) {
                Ok(i) => order.push(idx(i, RuleIdx::new, "rules")?),
                Err(_) => {
                    return err(
                        None,
                        format!("precedence '{}' lists '{id}', which is not compiled", p.id),
                    )
                }
            }
        }
        precedence.push(CompiledPrecedence {
            id: p.id.clone(),
            target: *target,
            order,
            spans: spans(rb, &p.spans)?,
        });
    }

    // Referenced documents, ascending by identifier, with their declared hashes.
    let mut docs: BTreeMap<DocumentId, Sha256Hex> = BTreeMap::new();
    for sp in rules
        .iter()
        .flat_map(|r| r.spans.iter())
        .chain(precedence.iter().flat_map(|p| p.spans.iter()))
    {
        docs.insert(sp.document.clone(), sp.sha256.clone());
    }
    let documents = docs
        .into_iter()
        .map(|(id, sha256)| CompiledDocument { id, sha256 })
        .collect();

    Ok(PartsWithoutOrder {
        documents,
        inputs: s.inputs,
        collections: s.collections,
        enums: s.enums,
        sets: s.sets,
        rules,
        precedence,
    })
}
