//! Pass 10 (approval gate) and pass 11 (candidates) (design §3.6.1, Req 2.5,
//! 5, 6.13 to 6.15, 25, 26).
//!
//! Statuses come from `rt_ledger::derive_statuses` only. This module never
//! stores a status.
//!
//! * `RT-APR-0001` (Error): one per unresolved revision, at the first span of
//!   the rule.
//! * `RT-APR-0002` (Error): no Effective revision exists.
//! * `RT-APR-0003` (Warning): one per ledger record that matches no current
//!   revision or, for an approval, does not match its revision in rule,
//!   revision, content hash, IR_Version and binding. Such a record never counts
//!   as an approval, because `derive_statuses` requires a full match.
//! * `RT-APR-0004` (Error): a Rejected revision whose current content hash
//!   differs from the one in its rejection record. The revision stays Rejected.
//! * `RT-APR-0006` (Warning): one per Rejected revision.
//! * `RT-ING-0006` (Warning): one per Candidate_Clause that no rule references.
//!
//! Choices the design leaves open:
//!
//! * A Rule references a Candidate_Clause when a span of any revision of the
//!   Rule, whatever its status, is in the same document and shares at least one
//!   byte with the clause span.
//! * The stale-record and unreferenced-clause Diagnostics point at the ledger
//!   record (`Location::Input`, input `ledger`, the file line of the record)
//!   and at the clause span. The stale record Diagnostic also lists the first
//!   span of its revision as a related location when the revision exists.
//! * `RT-APR-0004` is not combined with `RT-APR-0003`: a rejection record is
//!   stale only when it names no revision of the Rulebook.

use crate::ctx::{Decls, Site};
use rt_foundation::{Code, Diagnostic, DiagnosticState, InputId, InputLocation, Location, RuleRef};
use rt_ir::{rule_content_hash, ApprovalStatus, RuleKey, RuleRevision, Rulebook};
use rt_ledger::{
    derive_statuses, effective_revisions, unresolved, ApprovalLedger, BindingPair, LedgerRecord,
    RecordKind,
};
use rt_srcmodel::CandidateClause;
use std::collections::{BTreeMap, BTreeSet};

/// The sorted binding of a revision, or `None` when a span names an undeclared
/// document or one declared with two different hashes. Mirrors the derivation
/// of `rt-ledger`.
fn current_binding(rb: &Rulebook, rev: &RuleRevision) -> Option<Vec<BindingPair>> {
    let mut pairs: BTreeSet<BindingPair> = BTreeSet::new();
    for span in &rev.content.spans {
        let mut hashes = rb
            .documents
            .iter()
            .filter(|d| d.id == span.document)
            .map(|d| &d.sha256);
        let first = hashes.next()?;
        if hashes.any(|h| h != first) {
            return None;
        }
        pairs.insert(BindingPair {
            document: span.document.clone(),
            sha256: first.clone(),
        });
    }
    Some(pairs.into_iter().collect())
}

/// True when the approval `record` matches `rev` in every bound field (Req
/// 26.6).
fn approval_matches(rb: &Rulebook, rev: &RuleRevision, record: &LedgerRecord) -> bool {
    if record.ir_version() != rb.ir_version.as_str() {
        return false;
    }
    let Ok(hash) = rule_content_hash(&rb.ir_version, rev, &rb.documents) else {
        return false;
    };
    record.content_hash() == &hash
        && current_binding(rb, rev).is_some_and(|b| record.binding().pairs() == b.as_slice())
}

/// The location of a ledger record: the file line (the header is line 1).
fn record_location(record: &LedgerRecord) -> Location {
    let input = InputId::new("ledger").ok();
    match input {
        Some(input) => Location::Input(InputLocation {
            input,
            line: record.seq().checked_add(1),
            column: None,
            field: None,
        }),
        None => Location::None(Code::ApprovalRecordStale),
    }
}

fn record_ref(record: &LedgerRecord) -> RuleRef {
    RuleRef {
        rule: record.rule().clone(),
        revision: Some(record.revision()),
    }
}

/// Pass 10: the approval gate. `decls` must have been built without reported
/// documents.
pub(crate) fn gate(
    rb: &Rulebook,
    ledger: &ApprovalLedger,
    decls: &Decls<'_>,
    out: &mut DiagnosticState,
) {
    let statuses = derive_statuses(rb, ledger);
    let effective = effective_revisions(rb, &statuses);
    let site_of = |rev: &RuleRevision| {
        Site::new(
            &rev.key.id,
            Some(rev.key.revision),
            &rev.content.spans,
            decls,
        )
    };

    // Unresolved revisions: Proposed with no higher decided revision (Req
    // 25.13, 25.21).
    for key in unresolved(rb, &statuses) {
        if let Some(rev) = rb.revision(&key) {
            out.push(site_of(rev).diag(
                Code::ApprovalUnresolved,
                format!(
                    "revision {} of rule '{}' is Proposed and has no Approval_Record or rejection; approve or reject it with `ruletrace review`",
                    key.revision, key.id
                ),
            ));
        }
    }

    if effective.is_empty() {
        out.push(Diagnostic::new(
            Code::ApprovalNoExecutableRules,
            "the Rulebook has no Effective_Revision: no revision is Approved by a matching Approval_Record"
                .to_owned(),
            Location::None(Code::ApprovalNoExecutableRules),
        ));
    }

    // Rejected revisions: a Warning, and an Error when the content changed
    // since the rejection.
    let rejections: BTreeMap<RuleKey, &LedgerRecord> = ledger
        .records()
        .iter()
        .filter(|r| r.kind() == RecordKind::Rejection)
        .map(|r| {
            (
                RuleKey {
                    id: r.rule().clone(),
                    revision: r.revision(),
                },
                r,
            )
        })
        .collect();
    for rev in &rb.revisions {
        if statuses.get(&rev.key) != Some(&ApprovalStatus::Rejected) {
            continue;
        }
        let site = site_of(rev);
        out.push(site.diag(
            Code::ApprovalRevisionRejected,
            format!(
                "revision {} of rule '{}' is Rejected and excluded from compilation",
                rev.key.revision, rev.key.id
            ),
        ));
        let recorded = rejections.get(&rev.key).map(|r| r.content_hash());
        let current = rule_content_hash(&rb.ir_version, rev, &rb.documents).ok();
        if let (Some(recorded), Some(current)) = (recorded, current) {
            if recorded != &current {
                out.push(site.diag(
                    Code::ApprovalRejectedRevisionModified,
                    format!(
                        "revision {} of rule '{}' is Rejected but its content hash {} differs from {} in its rejection record",
                        rev.key.revision, rev.key.id, current, recorded
                    ),
                ));
            }
        }
    }

    // Stale records: one Warning per record, and never an approval.
    for record in ledger.records() {
        let key = RuleKey {
            id: record.rule().clone(),
            revision: record.revision(),
        };
        let rev = rb.revision(&key);
        let stale = match (record.kind(), rev) {
            (_, None) => true,
            (RecordKind::Approval, Some(rev)) => !approval_matches(rb, rev, record),
            (RecordKind::Rejection, Some(_)) => false,
        };
        if !stale {
            continue;
        }
        let why = if rev.is_none() {
            "matches no revision of the Rulebook"
        } else {
            "does not match the current content, IR_Version or document binding of its revision"
        };
        let related: Vec<Location> = rev
            .and_then(|r| site_of(r).primary().cloned())
            .into_iter()
            .collect();
        out.push(
            Diagnostic::new(
                Code::ApprovalRecordStale,
                format!(
                    "ledger record {} ({}) for revision {} of rule '{}' with content hash {} {}; it is not counted as an approval",
                    record.seq(),
                    record.kind().name(),
                    record.revision(),
                    record.rule(),
                    record.content_hash(),
                    why
                ),
                record_location(record),
            )
            .with_related(related)
            .with_rules(vec![record_ref(record)]),
        );
    }
}

/// Pass 11: a Warning for each Candidate_Clause that no rule references (Req
/// 2.5).
pub(crate) fn candidates(rb: &Rulebook, clauses: &[CandidateClause], out: &mut DiagnosticState) {
    for clause in clauses {
        let span = clause.span();
        let (cs, ce) = (span.start().get(), span.end().get());
        let referenced = rb.revisions.iter().any(|rev| {
            rev.content
                .spans
                .iter()
                .any(|s| &s.document == span.doc() && s.start < ce && cs < s.end)
        });
        if referenced {
            continue;
        }
        out.push(Diagnostic::new(
            Code::CandidateUnreferenced,
            format!(
                "the Candidate_Clause at bytes {cs}..{ce} of document '{}' is referenced by no rule",
                span.doc()
            ),
            Location::Source(span.clone()),
        ));
    }
}
