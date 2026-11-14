//! Pure status derivation over a Rulebook and a verified ledger (design §3.4.4, §3.8.2).

use crate::record::{ApprovalLedger, BindingPair, LedgerRecord, RecordKind};
use rt_foundation::RuleId;
use rt_ir::{rule_content_hash, ApprovalStatus, RuleKey, RuleRevision, Rulebook};
use std::collections::{BTreeMap, BTreeSet};

/// The binding of a revision: sorted, de-duplicated (document, declared hash) pairs for the
/// documents its spans reference. `None` when a span names an undeclared document or one
/// declared with two different hashes.
fn current_binding(rb: &Rulebook, rev: &RuleRevision) -> Option<Vec<BindingPair>> {
    let mut pairs: BTreeSet<BindingPair> = BTreeSet::new();
    for span in &rev.content.spans {
        let mut found: Option<&rt_hash::Sha256Hex> = None;
        for doc in rb.documents.iter().filter(|d| d.id == span.document) {
            match found {
                Some(prev) if prev != &doc.sha256 => return None,
                _ => found = Some(&doc.sha256),
            }
        }
        pairs.insert(BindingPair {
            document: span.document.clone(),
            sha256: found?.clone(),
        });
    }
    Some(pairs.into_iter().collect())
}

/// True when `record` is an approval that matches the current revision in rule, revision,
/// recomputed content hash, IR_Version and every binding pair.
fn approval_matches(rb: &Rulebook, rev: &RuleRevision, record: &LedgerRecord) -> bool {
    if record.kind() != RecordKind::Approval
        || record.rule() != &rev.key.id
        || record.revision() != rev.key.revision
        || record.ir_version() != rb.ir_version.as_str()
    {
        return false;
    }
    let Ok(hash) = rule_content_hash(&rb.ir_version, rev, &rb.documents) else {
        return false;
    };
    if record.content_hash() != &hash {
        return false;
    }
    match current_binding(rb, rev) {
        Some(binding) => record.binding().pairs() == binding.as_slice(),
        None => false,
    }
}

/// Derives the status of every revision of the Rulebook (Req 26.10): `Rejected` if a rejection
/// record names the key, else `Approved` if some approval record matches, else `Proposed`.
/// The ledger must have been verified by [`parse_ledger`](crate::parse_ledger).
#[must_use]
pub fn derive_statuses(
    rb: &Rulebook,
    ledger: &ApprovalLedger,
) -> BTreeMap<RuleKey, ApprovalStatus> {
    rb.revisions
        .iter()
        .map(|rev| {
            let rejected = ledger.records().iter().any(|r| {
                r.kind() == RecordKind::Rejection
                    && r.rule() == &rev.key.id
                    && r.revision() == rev.key.revision
            });
            let status = if rejected {
                ApprovalStatus::Rejected
            } else if ledger
                .records()
                .iter()
                .any(|r| approval_matches(rb, rev, r))
            {
                ApprovalStatus::Approved
            } else {
                ApprovalStatus::Proposed
            };
            (rev.key.clone(), status)
        })
        .collect()
}

/// The Effective revision of each rule: its highest-numbered Approved revision. A rule with no
/// Approved revision is absent. A higher Proposed revision never causes a fallback to a lower
/// one to be hidden or invented: that case is reported by [`unresolved`] (Req 25.11).
#[must_use]
pub fn effective_revisions(
    rb: &Rulebook,
    statuses: &BTreeMap<RuleKey, ApprovalStatus>,
) -> BTreeMap<RuleId, RuleKey> {
    let mut out: BTreeMap<RuleId, RuleKey> = BTreeMap::new();
    for rev in &rb.revisions {
        if statuses.get(&rev.key) == Some(&ApprovalStatus::Approved) {
            let better = out
                .get(&rev.key.id)
                .is_none_or(|cur| cur.revision < rev.key.revision);
            if better {
                out.insert(rev.key.id.clone(), rev.key.clone());
            }
        }
    }
    out
}

/// The unresolved revisions: Proposed, with no higher revision of the same rule that is
/// Approved or Rejected. Ascending key order.
#[must_use]
pub fn unresolved(rb: &Rulebook, statuses: &BTreeMap<RuleKey, ApprovalStatus>) -> Vec<RuleKey> {
    let mut keys: Vec<RuleKey> = rb
        .revisions
        .iter()
        .filter(|rev| statuses.get(&rev.key) == Some(&ApprovalStatus::Proposed))
        .filter(|rev| {
            !rb.revisions.iter().any(|o| {
                o.key.id == rev.key.id
                    && o.key.revision > rev.key.revision
                    && matches!(
                        statuses.get(&o.key),
                        Some(ApprovalStatus::Approved | ApprovalStatus::Rejected)
                    )
            })
        })
        .map(|rev| rev.key.clone())
        .collect();
    keys.sort();
    keys
}
