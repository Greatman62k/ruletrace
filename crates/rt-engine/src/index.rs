//! Lookup tables over a Compiled_Rulebook, built once per evaluation.

use rt_compiled::{CompiledBody, CompiledRulebook, RuleIdx};
use std::collections::BTreeMap;

/// A Precedence_Declaration for one Target.
#[derive(Debug)]
pub(crate) struct Prec {
    /// The declaration identifier.
    pub(crate) id: String,
    /// The rules in order of precedence.
    pub(crate) order: Vec<RuleIdx>,
}

/// Rules grouped by the name they define. Lists are in ascending rule
/// identifier order because the Compiled_Rulebook keeps its rules in that
/// order.
#[derive(Debug, Default)]
pub(crate) struct Index {
    /// Rules that give a value to a Target (eligibility, derived, weighted
    /// score), by Target.
    pub(crate) value: BTreeMap<String, Vec<RuleIdx>>,
    /// Ranking rules, by Target.
    pub(crate) ranking: BTreeMap<String, Vec<RuleIdx>>,
    /// Default rules, by the input they supply.
    pub(crate) defaults: BTreeMap<String, Vec<RuleIdx>>,
    /// Precedence declarations, by Target name.
    pub(crate) precedence: BTreeMap<String, Prec>,
}

impl Index {
    pub(crate) fn new(rulebook: &CompiledRulebook) -> Self {
        let mut index = Self::default();
        for (i, rule) in rulebook.rules().iter().enumerate() {
            let Some(idx) = RuleIdx::from_usize(i) else {
                continue;
            };
            let table = match rule.body {
                CompiledBody::Eligibility { .. }
                | CompiledBody::Derived { .. }
                | CompiledBody::WeightedScore { .. } => &mut index.value,
                CompiledBody::Ranking { .. } => &mut index.ranking,
                CompiledBody::Default { .. } => &mut index.defaults,
            };
            table
                .entry(rule.target.as_str().to_owned())
                .or_default()
                .push(idx);
        }
        for d in rulebook.precedence() {
            if let Some(target) = rulebook.target(d.target) {
                index.precedence.insert(
                    target.as_str().to_owned(),
                    Prec {
                        id: d.id.as_str().to_owned(),
                        order: d.order.clone(),
                    },
                );
            }
        }
        index
    }

    /// The first-listed rule of `rules` under the precedence of `target`, if
    /// one is declared.
    pub(crate) fn first_listed(&self, target: &str, rules: &[RuleIdx]) -> Option<(&Prec, RuleIdx)> {
        let prec = self.precedence.get(target)?;
        let first = prec.order.iter().copied().find(|r| rules.contains(r))?;
        Some((prec, first))
    }

    /// The value rules whose results a read of `target` needs: the first-listed
    /// rule when a precedence applies, else every rule of the Target.
    pub(crate) fn sources(&self, target: &str) -> Vec<RuleIdx> {
        let Some(rules) = self.value.get(target) else {
            return Vec::new();
        };
        match self.first_listed(target, rules) {
            Some((_, first)) => vec![first],
            None => rules.clone(),
        }
    }
}
