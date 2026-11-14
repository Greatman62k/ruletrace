//! Evaluation order (design §3.6.2 step 5, Req 9.8, 9.9).
//!
//! Kahn's algorithm over the dependencies between rules. The ready set is a
//! `BTreeSet<RuleId>`, so among rules that are ready together the lowest identifier (bytewise)
//! comes first, whatever the input order. A rule that remains once the ready set is empty lies on
//! or behind a cycle; each cycle is one Diagnostic `RT-CMP-0001`.

use rt_compiled::{CompiledBody, CompiledRule, NodeKind, RuleIdx};
use rt_foundation::{Code, Diagnostic, Location, RuleId, RuleRef};
use std::collections::{BTreeMap, BTreeSet};

/// For each rule, the rules it depends on. A `ref` depends on every non-ranking rule with that
/// Target, and a `rank_reference` on every ranking rule with that Target.
fn dependencies(rules: &[CompiledRule], targets: &[rt_ir::Name]) -> Vec<BTreeSet<usize>> {
    let mut value: BTreeMap<&rt_ir::Name, Vec<usize>> = BTreeMap::new();
    let mut ranking: BTreeMap<&rt_ir::Name, Vec<usize>> = BTreeMap::new();
    for (i, r) in rules.iter().enumerate() {
        match r.body {
            CompiledBody::Ranking { .. } => ranking.entry(&r.target).or_default().push(i),
            CompiledBody::Default { .. } => {}
            _ => value.entry(&r.target).or_default().push(i),
        }
    }
    rules
        .iter()
        .map(|r| {
            let mut deps = BTreeSet::new();
            for node in &r.nodes {
                let (table, t) = match &node.kind {
                    NodeKind::Ref(t) => (&value, t),
                    NodeKind::RankReference(t) => (&ranking, t),
                    _ => continue,
                };
                if let Some(name) = targets.get(t.at()) {
                    deps.extend(table.get(name).into_iter().flatten().copied());
                }
            }
            deps
        })
        .collect()
}

/// The first span of a rule as a Location. The Compiler holds no Source_Model, so a span cannot
/// be verified here and the Location names the document (Req 6.24); none is invented.
fn first_span(rule: &CompiledRule) -> Location {
    rule.spans
        .first()
        .map_or(Location::None(Code::CmpDependencyCycle), |s| {
            Location::Document(s.document.clone())
        })
}

/// The order of `rules` as indices, or one Diagnostic per cycle.
pub(crate) fn evaluation_order(
    rules: &[CompiledRule],
    targets: &[rt_ir::Name],
) -> Result<Vec<RuleIdx>, Vec<Diagnostic>> {
    let deps = dependencies(rules, targets);
    // dependents[d] lists the rules that wait for d.
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); rules.len()];
    let mut waiting: Vec<usize> = Vec::with_capacity(rules.len());
    for (i, d) in deps.iter().enumerate() {
        waiting.push(d.len());
        for &j in d {
            if let Some(list) = dependents.get_mut(j) {
                list.push(i);
            }
        }
    }
    let id_of = |i: usize| rules.get(i).map(|r| &r.id);
    let mut ready: BTreeSet<&RuleId> = BTreeSet::new();
    let by_id: BTreeMap<&RuleId, usize> =
        rules.iter().enumerate().map(|(i, r)| (&r.id, i)).collect();
    for (i, n) in waiting.iter().enumerate() {
        if *n == 0 {
            ready.extend(id_of(i));
        }
    }
    let mut order: Vec<usize> = Vec::with_capacity(rules.len());
    while let Some(id) = ready.pop_first() {
        let Some(&i) = by_id.get(id) else { continue };
        order.push(i);
        for &d in dependents.get(i).into_iter().flatten() {
            if let Some(n) = waiting.get_mut(d) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    ready.extend(id_of(d));
                }
            }
        }
    }
    if order.len() == rules.len() {
        return order
            .into_iter()
            .map(|i| {
                RuleIdx::from_usize(i).ok_or_else(|| {
                    vec![Diagnostic::new(
                        Code::CmpArtifactInvalid,
                        "too many rules".to_owned(),
                        Location::None(Code::CmpArtifactInvalid),
                    )]
                })
            })
            .collect();
    }
    Err(cycles(rules, &deps, &waiting))
}

/// One Diagnostic per cycle among the rules that never became ready.
fn cycles(rules: &[CompiledRule], deps: &[BTreeSet<usize>], waiting: &[usize]) -> Vec<Diagnostic> {
    let remaining: BTreeSet<usize> = waiting
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(i, _)| i)
        .collect();
    // reach[u]: the remaining rules reachable from u over one or more dependency edges.
    let reach = |start: usize| -> BTreeSet<usize> {
        let mut seen = BTreeSet::new();
        let mut stack: Vec<usize> = vec![start];
        while let Some(u) = stack.pop() {
            for &v in deps.get(u).into_iter().flatten() {
                if remaining.contains(&v) && seen.insert(v) {
                    stack.push(v);
                }
            }
        }
        seen
    };
    let reaches: BTreeMap<usize, BTreeSet<usize>> =
        remaining.iter().map(|&u| (u, reach(u))).collect();
    let mut done: BTreeSet<usize> = BTreeSet::new();
    let mut out = Vec::new();
    for &u in &remaining {
        let Some(ru) = reaches.get(&u) else { continue };
        if done.contains(&u) || !ru.contains(&u) {
            continue;
        }
        // The cycle through u: the rules that reach u and are reached from it. Rule indices
        // ascend with rule identifiers, so the set is already in ascending identifier order.
        let members: Vec<usize> = ru
            .iter()
            .copied()
            .filter(|v| reaches.get(v).is_some_and(|r| r.contains(&u)))
            .collect();
        done.extend(members.iter().copied());
        let in_cycle: Vec<&CompiledRule> = members.iter().filter_map(|i| rules.get(*i)).collect();
        let Some(first) = in_cycle.first() else {
            continue;
        };
        let names: Vec<String> = in_cycle.iter().map(|r| format!("'{}'", r.id)).collect();
        out.push(
            Diagnostic::new(
                Code::CmpDependencyCycle,
                format!("dependency cycle among rules: {}", names.join(", ")),
                first_span(first),
            )
            .with_related(in_cycle.iter().skip(1).map(|r| first_span(r)).collect())
            .with_rules(
                in_cycle
                    .iter()
                    .map(|r| RuleRef {
                        rule: r.id.clone(),
                        revision: Some(r.revision),
                    })
                    .collect(),
            ),
        );
    }
    out
}
