//! Adoption of a Pending_Proposal: prospective revision, hash and new
//! Rulebook bytes (design §3.8.2).

use rt_foundation::{
    Code, Diagnostic, InputId, Location, RevisionNumber, MAX_CONTAINER_DEPTH,
    MAX_PROPOSAL_BATCH_BYTES, MAX_RULEBOOK_BYTES,
};
use rt_hash::Sha256Hex;
use rt_ir::{
    draft_hash, parse_draft, parse_rulebook, print_pretty, rule_content_hash, AiOrigin, Origin,
    RuleKey, RuleRevision, Rulebook,
};
use rt_proposals::{pending_proposals, Batch, PendingProposal};

/// What a Reviewer must name to adopt one Pending_Proposal now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prospective {
    /// The Revision_Number the adoption would assign.
    pub revision: RevisionNumber,
    /// The Rule_Content_Hash of the adopted revision.
    pub content_hash: Sha256Hex,
    /// The revision as it would enter the Rulebook, with its Origin.
    pub adopted: RuleRevision,
}

/// The result of adopting: the new Rulebook and its exact bytes.
pub(crate) struct Adopted {
    pub rulebook: Rulebook,
    pub bytes: Vec<u8>,
}

fn store_invalid(why: String, rule_at: &Location) -> Diagnostic {
    Diagnostic::new(Code::ReviewProposalStoreInvalid, why, rule_at.clone())
}

/// Computes the prospective revision and Rule_Content_Hash of a
/// Pending_Proposal against the current Rulebook. `rule_at` is where the
/// caller read the rule name, and is the location of every refusal.
///
/// # Errors
/// `RT-APR-0210` when the next number exceeds 4,294,967,295, and
/// `RT-APR-0213` when the stored draft does not decode or its recomputed
/// Draft hash differs from the stored one.
// Diagnostic is the error value of this crate's checks.
#[allow(clippy::result_large_err)]
pub fn prospective_adoption(
    rulebook: &Rulebook,
    pending: &PendingProposal<'_>,
    rule_at: &Location,
) -> Result<Prospective, Diagnostic> {
    let proposal = pending.proposal;
    let Some((draft, stored_hash)) = &proposal.rule else {
        return Err(store_invalid(
            format!("proposal {} carries no rule draft", proposal.proposal_id),
            rule_at,
        ));
    };
    let highest = rulebook
        .revisions
        .iter()
        .filter(|r| r.key.id == draft.id)
        .map(|r| r.key.revision.get())
        .max();
    let revision = match highest {
        None => Some(1),
        Some(n) => n.checked_add(1),
    }
    .and_then(|n| RevisionNumber::new(n).ok())
    .ok_or_else(|| {
        Diagnostic::new(
            Code::ReviewRevisionExhausted,
            format!(
                "rule '{}' has no next revision number after 4294967295",
                draft.id
            ),
            rule_at.clone(),
        )
    })?;
    let batch = pending.batch;
    let origin = Origin::AiProposal(AiOrigin {
        proposer: proposal.proposer.clone(),
        provider: batch.provider.clone(),
        model: batch.model.clone(),
        adapter_version: batch.adapter_version.clone(),
        request_hash: batch.request_id.clone(),
        response_hash: batch.response_sha256.clone(),
        proposal_id: proposal.proposal_id.as_str().to_owned(),
        ai_identities: batch.ai_identities.clone(),
    });
    let input = InputId::new("proposal-store")
        .map_err(|_| store_invalid("the proposal input name is invalid".to_owned(), rule_at))?;
    let parsed = parse_draft(
        draft.json.as_bytes(),
        &input,
        revision,
        origin,
        MAX_PROPOSAL_BATCH_BYTES,
        MAX_CONTAINER_DEPTH,
    )
    .map_err(|rejected| {
        let first = rejected
            .diagnostics()
            .as_slice()
            .first()
            .map_or_else(|| "unknown".to_owned(), |d| d.message().to_owned());
        store_invalid(
            format!(
                "the stored draft of proposal {} does not decode: {first}",
                proposal.proposal_id
            ),
            rule_at,
        )
    })?;
    let adopted = parsed.into_parts().0;
    if adopted.key.id != draft.id {
        return Err(store_invalid(
            format!(
                "the draft of proposal {} names rule '{}', not '{}'",
                proposal.proposal_id, adopted.key.id, draft.id
            ),
            rule_at,
        ));
    }
    let unbound = |why: &dyn std::fmt::Debug| {
        Diagnostic::new(
            Code::ReviewBlockingDiagnostic,
            format!(
                "the draft of rule '{}' has no approval binding: {why:?}",
                draft.id
            ),
            rule_at.clone(),
        )
    };
    let recomputed = draft_hash(
        &rulebook.ir_version,
        &draft.id,
        &adopted.content,
        &rulebook.documents,
    )
    .map_err(|e| unbound(&e))?;
    if &recomputed != stored_hash {
        return Err(store_invalid(
            format!(
                "the stored Draft hash {stored_hash} of proposal {} differs from the recomputed {recomputed}",
                proposal.proposal_id
            ),
            rule_at,
        ));
    }
    let content_hash = rule_content_hash(&rulebook.ir_version, &adopted, &rulebook.documents)
        .map_err(|e| unbound(&e))?;
    Ok(Prospective {
        revision,
        content_hash,
        adopted,
    })
}

fn not_found(key: &RuleKey, note: &str, rule_at: &Location) -> Diagnostic {
    Diagnostic::new(
        Code::ReviewTargetNotFound,
        format!("rule '{}' has no revision {}{note}", key.id, key.revision),
        rule_at.clone(),
    )
    .with_rules(vec![rt_foundation::RuleRef {
        rule: key.id.clone(),
        revision: Some(key.revision),
    }])
}

/// Resolves `key` among the Pending_Proposals and adopts it into a copy of
/// the Rulebook. `None` when the Rulebook already holds the revision.
// Diagnostic is the error value of this crate's checks.
#[allow(clippy::result_large_err)]
pub(crate) fn adopt(
    rb: &Rulebook,
    key: &RuleKey,
    named_hash: &Sha256Hex,
    batches: &[Batch],
    rule_at: &Location,
) -> Result<Option<Adopted>, Diagnostic> {
    if rb.revision(key).is_some() {
        return Ok(None);
    }
    let candidates: Vec<PendingProposal<'_>> = pending_proposals(batches, rb)
        .into_iter()
        .filter(|p| {
            p.proposal
                .rule
                .as_ref()
                .is_some_and(|(d, _)| d.id == key.id)
        })
        .collect();
    if candidates.is_empty() {
        return Err(not_found(key, "", rule_at));
    }
    let mut found: Vec<Prospective> = Vec::new();
    let mut first_error: Option<Diagnostic> = None;
    for candidate in &candidates {
        match prospective_adoption(rb, candidate, rule_at) {
            Ok(p) => found.push(p),
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    if found.is_empty() {
        return Err(first_error.unwrap_or_else(|| not_found(key, "", rule_at)));
    }
    let at_revision: Vec<&Prospective> = found
        .iter()
        .filter(|p| p.revision == key.revision)
        .collect();
    let chosen = at_revision
        .iter()
        .find(|p| &p.content_hash == named_hash)
        .or_else(|| at_revision.first())
        .copied();
    let Some(chosen) = chosen else {
        let next = found.first().map_or(0, |p| p.revision.get());
        return Err(not_found(
            key,
            &format!("; the next revision to adopt is {next}"),
            rule_at,
        ));
    };
    let mut next = rb.clone();
    next.revisions.push(chosen.adopted.clone());
    let text = print_pretty(&next).map_err(|_| {
        store_invalid(
            "the adopted Rulebook exceeds a count limit".to_owned(),
            rule_at,
        )
    })?;
    let bytes = text.into_bytes();
    let input = InputId::new("rulebook")
        .map_err(|_| store_invalid("the rulebook input name is invalid".to_owned(), rule_at))?;
    let reparsed = parse_rulebook(&bytes, &input, MAX_RULEBOOK_BYTES, MAX_CONTAINER_DEPTH)
        .map_err(|_| {
            store_invalid(
                "the adopted Rulebook does not parse back".to_owned(),
                rule_at,
            )
        })?
        .into_parts()
        .0;
    Ok(Some(Adopted {
        rulebook: reparsed,
        bytes,
    }))
}
