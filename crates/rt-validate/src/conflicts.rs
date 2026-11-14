//! Pass 9: conflict detection and precedence validation (design §3.6.1, Req 6.7, 6.8, 6.16 to
//! 6.21, 6.23).
//!
//! Only Effective revisions take part. No Diagnostic or result field designates a winner: a
//! conflict names every rule in ascending identifier order. A Precedence_Declaration that lists
//! every rule of a `DuplicateTarget`, `DuplicateRankKey` or `ContradictoryDefault` conflict, and
//! is itself valid, turns the Error into the Warning `RT-VAL-0010`. `InputShadowing` is never
//! resolved. A declaration that is part of a cycle or lists a rule that is not Effective gives
//! `RT-VAL-0012` and resolves nothing.
//!
//! Choices the design leaves open:
//!
//! * A declaration whose order lists one rule twice is not a total order. It is a cycle of one
//!   rule and gives `RT-VAL-0012`.
//! * Cycles are searched over the pairwise "listed before" edges of all declarations together,
//!   whatever their Target.
//! * A declaration resolves a conflict only when its Target equals the Target of the conflict.
//!   Extra listed rules are allowed when they are Effective.
//! * Default rules on one input field with all literals equal are `DuplicateTarget`, otherwise
//!   `ContradictoryDefault` (Req 6.18).

use crate::ctx::{Decls, Site};
use crate::{graph, ConflictClass};
use rt_foundation::{Code, Diagnostic, DiagnosticState, Location, RuleId, RuleRef};
use rt_ir::{DefaultValue, Name, Precedence, RuleBody, RuleContent, RuleKey, Rulebook};
use std::collections::{BTreeMap, BTreeSet};

/// Every class that pass 9 checks, ascending.
pub(crate) const CHECKED: [ConflictClass; 4] = [
    ConflictClass::ContradictoryDefault,
    ConflictClass::DuplicateRankKey,
    ConflictClass::DuplicateTarget,
    ConflictClass::InputShadowing,
];

struct Member<'a> {
    id: &'a RuleId,
    key: &'a RuleKey,
    content: &'a RuleContent,
}

struct Conflict<'a> {
    class: ConflictClass,
    target: &'a Name,
    /// Ascending rule identifier order.
    members: Vec<&'a Member<'a>>,
}

fn class_code(class: ConflictClass) -> Code {
    match class {
        ConflictClass::ContradictoryDefault => Code::ValConflictContradictoryDefault,
        ConflictClass::DuplicateRankKey => Code::ValConflictDuplicateRankKey,
        ConflictClass::DuplicateTarget => Code::ValConflictDuplicateTarget,
        ConflictClass::InputShadowing => Code::ValConflictInputShadowing,
    }
}

fn quoted<'a>(ids: impl IntoIterator<Item = &'a RuleId>) -> String {
    let v: Vec<String> = ids.into_iter().map(|i| format!("'{i}'")).collect();
    v.join(", ")
}

fn rule_ref(m: &Member<'_>) -> RuleRef {
    RuleRef {
        rule: m.id.clone(),
        revision: Some(m.key.revision),
    }
}

/// Runs pass 9 over the Effective revisions of `rb` and returns the checked classes.
pub(crate) fn check(
    rb: &Rulebook,
    effective: &BTreeMap<RuleId, RuleKey>,
    decls: &Decls<'_>,
    out: &mut DiagnosticState,
) -> Vec<ConflictClass> {
    let members: Vec<Member<'_>> = effective
        .iter()
        .filter_map(|(id, key)| {
            rb.revision(key).map(|rev| Member {
                id,
                key: &rev.key,
                content: &rev.content,
            })
        })
        .collect();
    let sites: BTreeMap<&RuleId, Site> = members
        .iter()
        .map(|m| {
            (
                m.id,
                Site::new(m.id, Some(m.key.revision), &m.content.spans, decls),
            )
        })
        .collect();

    // InputShadowing: one Error per rule, never resolved by precedence.
    for m in &members {
        let value_or_rank = matches!(
            m.content.body,
            RuleBody::Eligibility { .. }
                | RuleBody::Derived { .. }
                | RuleBody::WeightedScore { .. }
                | RuleBody::Ranking { .. }
        );
        if value_or_rank && decls.inputs.contains_key(&m.content.target) {
            if let Some(site) = sites.get(m.id) {
                out.push(site.diag(
                    Code::ValConflictInputShadowing,
                    format!(
                        "conflict InputShadowing: rule '{}' declares the Target '{}', which is also a declared input field",
                        m.id, m.content.target
                    ),
                ));
            }
        }
    }

    let conflicts = find_conflicts(&members);
    let valid = precedence(rb, effective, decls, out);
    for c in &conflicts {
        let ids: Vec<&RuleId> = c.members.iter().map(|m| m.id).collect();
        let resolver = valid
            .iter()
            .find(|p| p.decl.target == *c.target && ids.iter().all(|id| p.listed.contains(*id)));
        let related_rules: Vec<RuleRef> = c.members.iter().map(|m| rule_ref(m)).collect();
        if let Some(p) = resolver {
            out.push(
                Diagnostic::new(
                    Code::ValConflictResolved,
                    format!(
                        "conflict {} on Target '{}' among rules {} is resolved by precedence declaration '{}'",
                        c.class.name(),
                        c.target,
                        quoted(ids.iter().copied()),
                        p.decl.id
                    ),
                    p.primary.clone(),
                )
                .with_rules(related_rules),
            );
            continue;
        }
        let code = class_code(c.class);
        let mut locs = c
            .members
            .iter()
            .map(|m| sites.get(m.id).and_then(|s| s.primary().cloned()));
        let primary = locs.next().flatten().unwrap_or(Location::None(code));
        let related: Vec<Location> = locs.flatten().collect();
        out.push(
            Diagnostic::new(
                code,
                format!(
                    "conflict {} on Target '{}' among rules {}: no precedence declaration resolves it and no rule is selected",
                    c.class.name(),
                    c.target,
                    quoted(ids.iter().copied())
                ),
                primary,
            )
            .with_related(related)
            .with_rules(related_rules),
        );
    }
    CHECKED.to_vec()
}

fn find_conflicts<'a>(members: &'a [Member<'a>]) -> Vec<Conflict<'a>> {
    let mut values: BTreeMap<&Name, Vec<&Member<'_>>> = BTreeMap::new();
    let mut ranks: BTreeMap<&Name, Vec<&Member<'_>>> = BTreeMap::new();
    let mut defaults: BTreeMap<&Name, Vec<&Member<'_>>> = BTreeMap::new();
    for m in members {
        let slot = match m.content.body {
            RuleBody::Eligibility { .. }
            | RuleBody::Derived { .. }
            | RuleBody::WeightedScore { .. } => &mut values,
            RuleBody::Ranking { .. } => &mut ranks,
            RuleBody::Default { .. } => &mut defaults,
            RuleBody::Unrecognized(_) => continue,
        };
        slot.entry(&m.content.target).or_default().push(m);
    }
    let mut found: Vec<Conflict<'a>> = Vec::new();
    for (target, group) in values.into_iter().filter(|(_, g)| g.len() > 1) {
        found.push(Conflict {
            class: ConflictClass::DuplicateTarget,
            target,
            members: group,
        });
    }
    for (target, group) in ranks.into_iter().filter(|(_, g)| g.len() > 1) {
        found.push(Conflict {
            class: ConflictClass::DuplicateRankKey,
            target,
            members: group,
        });
    }
    for (target, group) in defaults.into_iter().filter(|(_, g)| g.len() > 1) {
        let literals: Vec<Option<&DefaultValue>> = group
            .iter()
            .map(|m| match &m.content.body {
                RuleBody::Default { value } => Some(value),
                _ => None,
            })
            .collect();
        let first = literals.first().copied().flatten().map(|d| &d.literal);
        let all_equal = literals
            .iter()
            .all(|d| d.map(|d| &d.literal) == first && first.is_some());
        found.push(Conflict {
            class: if all_equal {
                ConflictClass::DuplicateTarget
            } else {
                ConflictClass::ContradictoryDefault
            },
            target,
            members: group,
        });
    }
    found
}

/// A valid declaration, with what it lists.
struct Valid<'a> {
    decl: &'a Precedence,
    listed: BTreeSet<&'a RuleId>,
    primary: Location,
}

/// Validates the Precedence_Declarations, emits `RT-VAL-0012` per offending one and returns the
/// valid ones in ascending declaration identifier order.
fn precedence<'a>(
    rb: &'a Rulebook,
    effective: &BTreeMap<RuleId, RuleKey>,
    decls: &Decls<'_>,
    out: &mut DiagnosticState,
) -> Vec<Valid<'a>> {
    // Nodes: every listed rule. Edges: "listed before", over all declarations.
    let ids: Vec<&RuleId> = rb
        .precedence
        .iter()
        .flat_map(|p| p.order.iter())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let index: BTreeMap<&RuleId, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut edges: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); ids.len()];
    for p in &rb.precedence {
        for (i, a) in p.order.iter().enumerate() {
            for b in p.order.iter().skip(i + 1) {
                if let (Some(&x), Some(&y)) = (index.get(a), index.get(b)) {
                    if let Some(set) = edges.get_mut(x) {
                        set.insert(y);
                    }
                }
            }
        }
    }
    let adj: Vec<Vec<usize>> = edges.into_iter().map(|s| s.into_iter().collect()).collect();
    let mut comp_of: Vec<Option<usize>> = vec![None; ids.len()];
    for (c, comp) in graph::tarjan(&adj)
        .iter()
        .enumerate()
        .filter(|(_, comp)| graph::is_cycle(comp, &adj))
    {
        for &n in comp {
            if let Some(slot) = comp_of.get_mut(n) {
                *slot = Some(c);
            }
        }
    }
    let cyclic_pair = |a: &RuleId, b: &RuleId| -> bool {
        let ca = index
            .get(a)
            .and_then(|i| comp_of.get(*i))
            .copied()
            .flatten();
        let cb = index
            .get(b)
            .and_then(|i| comp_of.get(*i))
            .copied()
            .flatten();
        ca.is_some() && ca == cb
    };

    let mut sorted: Vec<&Precedence> = rb.precedence.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    let mut valid = Vec::new();
    for p in sorted {
        let mut offending: BTreeSet<&RuleId> = p
            .order
            .iter()
            .filter(|id| !effective.contains_key(*id))
            .collect();
        for (i, a) in p.order.iter().enumerate() {
            for b in p.order.iter().skip(i + 1) {
                if cyclic_pair(a, b) {
                    offending.insert(a);
                    offending.insert(b);
                }
            }
        }
        let site = Site::new(&p.id, None, &p.spans, decls);
        let primary = site
            .primary()
            .cloned()
            .unwrap_or(Location::None(Code::ValPrecedenceInvalid));
        if offending.is_empty() {
            valid.push(Valid {
                decl: p,
                listed: p.order.iter().collect(),
                primary,
            });
            continue;
        }
        out.push(
            Diagnostic::new(
                Code::ValPrecedenceInvalid,
                format!(
                    "precedence declaration '{}' is invalid: rules {} are in a precedence cycle or are not Effective",
                    p.id,
                    quoted(offending.iter().copied())
                ),
                primary,
            )
            .with_rules(
                offending
                    .iter()
                    .map(|id| RuleRef {
                        rule: (*id).clone(),
                        revision: None,
                    })
                    .collect(),
            ),
        );
    }
    valid
}
