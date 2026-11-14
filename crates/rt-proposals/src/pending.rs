//! Pending_Proposal derivation (design §3.7.5, decision D-8).

use crate::model::{Batch, Proposal, ProposalKind};
use rt_ir::{Origin, Rulebook};
use std::collections::BTreeSet;

/// A `rule_suggestion` that no Origin of the Rulebook references.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingProposal<'a> {
    /// The batch that holds it.
    pub batch: &'a Batch,
    /// Its index in the batch.
    pub index: usize,
    /// The proposal.
    pub proposal: &'a Proposal,
}

/// Derives the Pending_Proposals from the stored batches and a Rulebook.
/// Nothing is stored about adoption: a proposal is adopted when an AI
/// Origin of the Rulebook names its identifier. The result is ordered by
/// request identifier, then index, whatever the order of `batches`.
#[must_use]
pub fn pending_proposals<'a>(
    batches: &'a [Batch],
    rulebook: &Rulebook,
) -> Vec<PendingProposal<'a>> {
    let adopted: BTreeSet<&str> = rulebook
        .revisions
        .iter()
        .filter_map(|r| match &r.origin {
            Origin::AiProposal(origin) => Some(origin.proposal_id.as_str()),
            _ => None,
        })
        .collect();
    let mut ordered: Vec<&Batch> = batches.iter().collect();
    ordered.sort_by(|a, b| a.request_id.cmp(&b.request_id));
    let mut pending = Vec::new();
    for batch in ordered {
        for (index, proposal) in batch.proposals.iter().enumerate() {
            if proposal.kind == ProposalKind::RuleSuggestion
                && !adopted.contains(proposal.proposal_id.as_str())
            {
                pending.push(PendingProposal {
                    batch,
                    index,
                    proposal,
                });
            }
        }
    }
    pending
}
