//! `plan_review`: the pure planning of one approve or reject decision.

use crate::line::{record_line, NewRecord};
use rt_foundation::{
    CliLocation, Code, Diagnostic, DiagnosticState, Location, Rejected, RevisionNumber, RuleId,
    RuleRef, MAX_CONTAINER_DEPTH, MAX_LEDGER_BYTES,
};
use rt_hash::Sha256Hex;
use rt_ir::{rule_content_hash, ApprovalStatus, RuleKey, RuleRevision, Rulebook};
use rt_json::ParseLimits;
use rt_ledger::{
    derive_statuses, parse_ledger, print_ledger, AiIdentitySet, ApprovalLedger, BindingPair,
    Reviewer, ZERO_HASH,
};
use rt_srcmodel::SourceModels;
use rt_validate::validate;
use std::collections::BTreeSet;

/// Longest rejection reason, in Unicode scalar values (design §3.8.1, Req 23.14).
const MAX_REASON_CHARS: usize = 2000;

/// What the Reviewer decides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewAction {
    /// Approve the revision.
    Approve,
    /// Reject the revision, with a reason of 1 to 2000 scalar values.
    Reject {
        /// The reason, stored verbatim.
        reason: String,
    },
}

/// One Review_Decision, as the command line states it. Nothing here is read from the
/// environment, and `time` is the caller-supplied text (Req 5.11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewDecision {
    /// The rule to decide.
    pub rule: RuleId,
    /// The revision to decide.
    pub revision: RevisionNumber,
    /// The Rule_Content_Hash the Reviewer saw.
    pub content_hash: Sha256Hex,
    /// The Reviewer name, validated here (UD-23).
    pub reviewer: String,
    /// The decision time, `YYYY-MM-DDTHH:MM:SSZ`, stored verbatim.
    pub time: String,
    /// Approve or reject.
    pub action: ReviewAction,
    /// AI identities recorded outside the Origin, for example in a Proposal_Store batch.
    pub batch_identities: AiIdentitySet,
}

/// The result of planning: the exact new bytes of the ledger. Nothing has been written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitPlan {
    ledger_bytes: Vec<u8>,
    ledger: ApprovalLedger,
    key: RuleKey,
    record_hash: Sha256Hex,
}

impl CommitPlan {
    /// The complete new ledger file: the old bytes, then exactly one more record line.
    #[must_use]
    pub fn ledger_bytes(&self) -> &[u8] {
        &self.ledger_bytes
    }
    /// The new ledger, parsed back and verified from [`CommitPlan::ledger_bytes`].
    #[must_use]
    pub fn ledger(&self) -> &ApprovalLedger {
        &self.ledger
    }
    /// The decided revision.
    #[must_use]
    pub fn key(&self) -> &RuleKey {
        &self.key
    }
    /// The `hash` of the appended record.
    #[must_use]
    pub fn record_hash(&self) -> &Sha256Hex {
        &self.record_hash
    }
}

fn arg(name: &str) -> Location {
    Location::Cli(CliLocation::Argument {
        position: 0,
        name: Some(name.to_owned()),
    })
}

fn refuse(d: Diagnostic) -> Rejected {
    let mut state = DiagnosticState::empty();
    state.push(d);
    match Rejected::new(state) {
        Ok(r) => r,
        // Every code used here is an Error, so this arm is not reached. The internal-error code
        // is an Error too, so the recursion ends at once.
        Err(_) => refuse(Diagnostic::new(
            Code::InternalError,
            "a review refusal carried a non-blocking code".to_owned(),
            Location::None(Code::InternalError),
        )),
    }
}

fn time_shape_ok(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

/// The binding of a revision: sorted, de-duplicated (document, declared hash) pairs.
fn binding_of(rb: &Rulebook, rev: &RuleRevision) -> Option<Vec<BindingPair>> {
    let mut pairs: BTreeSet<BindingPair> = BTreeSet::new();
    for span in &rev.content.spans {
        let mut found: Option<&Sha256Hex> = None;
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

fn rule_ref(key: &RuleKey) -> Vec<RuleRef> {
    vec![RuleRef {
        rule: key.id.clone(),
        revision: Some(key.revision),
    }]
}

/// Plans the approval or rejection of a revision that exists in the Rulebook (design §3.8.3
/// step 4). `ledger` must be a verified ledger. The Rulebook and the ledger are not changed.
///
/// # Errors
/// Exactly one Error Diagnostic of the REVIEW family (or `RT-CLI-0004` for a bad time shape),
/// and no bytes. See the crate documentation for the order of the checks.
pub fn plan_review(
    rulebook: &Rulebook,
    ledger: &ApprovalLedger,
    decision: &ReviewDecision,
    sources: &SourceModels,
) -> Result<CommitPlan, Rejected> {
    plan(rulebook, ledger, decision, sources).map_err(refuse)
}

#[allow(clippy::result_large_err)] // Diagnostic is the error value of this crate's checks.
fn plan(
    rb: &Rulebook,
    ledger: &ApprovalLedger,
    d: &ReviewDecision,
    sources: &SourceModels,
) -> Result<CommitPlan, Diagnostic> {
    // 1. Target (Req 5.10, design §3.8.3).
    let key = RuleKey {
        id: d.rule.clone(),
        revision: d.revision,
    };
    let rev = rb.revision(&key).ok_or_else(|| {
        Diagnostic::new(
            Code::ReviewTargetNotFound,
            format!("rule '{}' has no revision {}", d.rule, d.revision),
            arg("--rule"),
        )
        .with_rules(rule_ref(&key))
    })?;

    // 2. Reviewer (Req 23.15, UD-23).
    let reviewer = Reviewer::new(&d.reviewer)?;
    let known = AiIdentitySet::from_origin(&rev.origin).union(&d.batch_identities);
    reviewer.check_not_ai(&known)?;

    // 3. Time shape (Req 5.11). The time is stored verbatim and no clock is read.
    if !time_shape_ok(&d.time) {
        return Err(Diagnostic::new(
            Code::CliArgumentInvalid,
            "the time must have the shape YYYY-MM-DDTHH:MM:SSZ".to_owned(),
            arg("--time"),
        ));
    }

    // 4. Reason (Req 23.14).
    let reason = match &d.action {
        ReviewAction::Approve => None,
        ReviewAction::Reject { reason } => {
            if reason.is_empty() || reason.chars().nth(MAX_REASON_CHARS).is_some() {
                return Err(Diagnostic::new(
                    Code::ReviewReasonInvalid,
                    format!("the reason must have 1 to {MAX_REASON_CHARS} characters"),
                    arg("--reason"),
                )
                .with_rules(rule_ref(&key)));
            }
            Some(reason.as_str())
        }
    };

    // 5. Transition (Req 5.8, 26.25): only Proposed can change, and only once.
    if derive_statuses(rb, ledger).get(&key) != Some(&ApprovalStatus::Proposed) {
        return Err(Diagnostic::new(
            Code::ReviewInvalidTransition,
            format!(
                "revision {} of rule '{}' is already decided; Approved and Rejected are terminal",
                d.revision, d.rule
            ),
            arg("--revision"),
        )
        .with_rules(rule_ref(&key)));
    }

    // 6. Stale hash (Req 5.10, 26.24). A revision with no binding has no hash either.
    let binding = binding_of(rb, rev);
    let current = rule_content_hash(&rb.ir_version, rev, &rb.documents).ok();
    let (Some(binding), Some(current)) = (binding, current) else {
        return Err(Diagnostic::new(
            Code::ReviewBlockingDiagnostic,
            format!(
                "revision {} of rule '{}' names an undeclared document or one with two hashes, so it has no approval binding",
                d.revision, d.rule
            ),
            arg("--rule"),
        )
        .with_rules(rule_ref(&key)));
    };
    if current != d.content_hash {
        return Err(Diagnostic::new(
            Code::ReviewStaleHash,
            format!(
                "the content hash {} differs from the current hash {current}; list the revision again",
                d.content_hash
            ),
            arg("--content-hash"),
        )
        .with_rules(rule_ref(&key)));
    }

    // 7. Source_Models of every document of the binding (design §3.8.3 step 3).
    for pair in &binding {
        let ok = sources
            .get(&pair.document)
            .is_some_and(|m| m.sha256() == &pair.sha256);
        if !ok {
            return Err(Diagnostic::new(
                Code::ReviewSourceModelMissing,
                format!(
                    "no Source_Model with the declared hash was supplied for document '{}'",
                    pair.document
                ),
                Location::Document(pair.document.clone()),
            )
            .with_rules(rule_ref(&key)));
        }
    }

    // 8. The new record.
    let seq = u32::try_from(ledger.len())
        .ok()
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| write_failed("the ledger has no room for another record"))?;
    let prev = ledger
        .records()
        .last()
        .map_or(ZERO_HASH, |r| r.hash().as_str());
    let (line, record_hash) = record_line(&NewRecord {
        seq,
        rejection: reason.is_some(),
        rule: d.rule.as_str(),
        revision: d.revision.get(),
        content_hash: d.content_hash.as_str(),
        ir_version: rb.ir_version.as_str(),
        binding: &binding,
        reviewer: reviewer.as_str(),
        time: &d.time,
        reason,
        prev,
    })
    .ok_or_else(|| write_failed("the record could not be built"))?;
    let mut bytes = print_ledger(ledger).into_bytes();
    bytes.extend_from_slice(line.as_bytes());
    bytes.push(b'\n');

    // 9. The new bytes must verify as a ledger, with exactly one more record.
    let input = rt_foundation::InputId::new("ledger")
        .map_err(|_| write_failed("the ledger input name is invalid"))?;
    let new_ledger = parse_ledger(
        &input,
        &bytes,
        ParseLimits::new(MAX_LEDGER_BYTES, MAX_CONTAINER_DEPTH),
    )
    .map_err(|ds| {
        ds.into_iter()
            .next()
            .unwrap_or_else(|| write_failed("the new ledger did not verify"))
    })?;
    if new_ledger.len() != ledger.len() + 1 {
        return Err(write_failed(
            "the new ledger has an unexpected record count",
        ));
    }

    // 10. Dry-run validation of the post-approval state (Req 5.9, 26.16).
    if reason.is_none() {
        let result = validate(rb, &new_ledger, sources, None);
        let blocking: Vec<&Diagnostic> = result
            .state()
            .as_slice()
            .iter()
            .filter(|x| x.severity().is_blocking())
            .filter(|x| x.rules().iter().any(|r| r.rule == key.id))
            .filter(|x| x.code() != Code::ApprovalUnresolved)
            .collect();
        if let Some(first) = blocking.first() {
            let codes: Vec<&str> = blocking.iter().map(|x| x.code().id()).collect();
            return Err(Diagnostic::new(
                Code::ReviewBlockingDiagnostic,
                format!(
                    "rule '{}' has a blocking diagnostic ({}): {}",
                    d.rule,
                    codes.join(", "),
                    first.message()
                ),
                arg("--rule"),
            )
            .with_related(blocking.iter().map(|x| x.primary().clone()).collect())
            .with_rules(rule_ref(&key)));
        }
    }

    Ok(CommitPlan {
        ledger_bytes: bytes,
        ledger: new_ledger,
        key,
        record_hash,
    })
}

fn write_failed(why: &str) -> Diagnostic {
    Diagnostic::new(
        Code::ReviewWriteFailed,
        format!("review planning failed: {why}"),
        Location::None(Code::ReviewWriteFailed),
    )
}
