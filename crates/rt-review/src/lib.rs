//! Review planning: `approve` and `reject` of a revision that already exists in
//! the Rulebook (design §3.8.2, §3.8.3 step 4; Req 5.8 to 5.12, 23.12 to 23.15,
//! 26.16 to 26.19, 26.24, 26.25; UD-23, UD-27).
//!
//! * [`plan_review`] is pure. It reads no file, clock or environment, and
//!   returns the exact new ledger bytes in a [`CommitPlan`]. The caller
//!   (`rt-cli`) owns the lock, the write and the rename.
//! * This crate holds the only function that builds a Ledger_Record.
//!   `rt-ledger` has none, and no `rt-ai-*` crate may depend on this crate
//!   (`check-graph`). The function is private; the public entry point is
//!   [`plan_review`].
//! * A refusal is exactly one Diagnostic and no bytes. The checks run in this
//!   order, and the first failing check decides: target (`RT-APR-0206`),
//!   reviewer (`0202`, then `0201`), time shape (`RT-CLI-0004`), reason
//!   (`0207`, reject only), transition (`0204`), stale hash (`0203`),
//!   Source_Models of the binding (`0209`), dry-run validation of the
//!   post-approval state (`0205`, approve only).
//! * A Pending_Proposal is adopted by the same call
//!   ([`plan_review_with_store`]). Its Revision_Number is `1 + max` of the
//!   rule in the Rulebook at that moment (`RT-APR-0210` on exhaustion), the
//!   stored draft is decoded and its Draft hash recomputed, and the plan then
//!   holds the new Rulebook bytes too. A rejection adopts the revision as
//!   Rejected, so no number is skipped.
//! * A decision on a revision already in the Rulebook carries ledger bytes
//!   only.
//! * A revision whose spans name an undeclared document, or one declared with
//!   two hashes, has no binding. It is refused with `RT-APR-0205` for both
//!   decisions.
//!
//! Reviewer names are compared by exact equality (UD-23). Names are not
//! authenticated (R-8).
#![forbid(unsafe_code)]

mod adopt;
mod line;
mod plan;

pub use adopt::{prospective_adoption, Prospective};
pub use plan::{
    plan_review, plan_review_with_store, CommitPlan, DecisionSites, ReviewAction, ReviewDecision,
};

#[cfg(test)]
mod tests;
