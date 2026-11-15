//! The Run_Summary (design 3.7.7; Req 23.36, 23.37).
//!
//! The summary is plain text with lists of clause identifiers. It states
//! `complete` only when no clause is unprocessed and no clause is stored
//! with unconfirmed durability, and `incomplete` in every other case.
//!
//! A batch whose directory sync failed after the link is whole in the
//! store but its durability is not confirmed. Its clauses are listed
//! under `stored, durability not confirmed`, a section that appears only
//! when it is not empty. Such a batch is neither processed nor
//! unprocessed.

use crate::plan::ClauseId;
use crate::redact::Redactor;
use rt_foundation::escape_controls;
use std::collections::BTreeSet;

/// The clauses of a run, by outcome.
///
/// Invariants held by construction: `processed`, `unconfirmed` and
/// `unprocessed` are pairwise disjoint, their union is every requested
/// clause, and `stored` is a subset of `processed`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunSummary {
    processed: Vec<ClauseId>,
    stored: Vec<ClauseId>,
    unconfirmed: Vec<ClauseId>,
    unprocessed: Vec<ClauseId>,
}

// Ascending start offset, then document bytes, then end (Req 23.36). The
// order of `ClauseId` itself puts the document first.
fn sorted(clauses: impl IntoIterator<Item = ClauseId>) -> Vec<ClauseId> {
    let mut all: Vec<ClauseId> = clauses.into_iter().collect();
    all.sort_by(|a, b| {
        (a.start, a.document.as_str().as_bytes(), a.end).cmp(&(
            b.start,
            b.document.as_str().as_bytes(),
            b.end,
        ))
    });
    all
}

impl RunSummary {
    /// A summary of the `requested` clauses. The unprocessed list is every
    /// requested clause that is not in `processed`. A clause of `stored`
    /// that is not in `processed` is dropped, so the invariants hold for
    /// any input.
    #[must_use]
    pub fn new(requested: &[ClauseId], processed: &[ClauseId], stored: &[ClauseId]) -> Self {
        Self::with_unconfirmed(requested, processed, stored, &[])
    }

    /// A summary that also lists the clauses of a batch that is whole in
    /// the store but whose directory sync failed. A clause of
    /// `unconfirmed` that is in `processed` or not in `requested` is
    /// dropped.
    #[must_use]
    pub fn with_unconfirmed(
        requested: &[ClauseId],
        processed: &[ClauseId],
        stored: &[ClauseId],
        unconfirmed: &[ClauseId],
    ) -> Self {
        let done: BTreeSet<&ClauseId> = processed.iter().collect();
        let asked: BTreeSet<&ClauseId> = requested.iter().collect();
        let kept: BTreeSet<&ClauseId> = stored.iter().filter(|c| done.contains(c)).collect();
        let open: BTreeSet<&ClauseId> = unconfirmed
            .iter()
            .filter(|c| asked.contains(c) && !done.contains(c))
            .collect();
        let rest: BTreeSet<&ClauseId> = requested
            .iter()
            .filter(|c| !done.contains(c) && !open.contains(c))
            .collect();
        Self {
            processed: sorted(done.into_iter().cloned()),
            stored: sorted(kept.into_iter().cloned()),
            unconfirmed: sorted(open.into_iter().cloned()),
            unprocessed: sorted(rest.into_iter().cloned()),
        }
    }

    /// Clauses of requests that finished without a failure, including a
    /// request whose batch was already stored.
    #[must_use]
    pub fn processed(&self) -> &[ClauseId] {
        &self.processed
    }

    /// Clauses of batches that this run wrote.
    #[must_use]
    pub fn stored(&self) -> &[ClauseId] {
        &self.stored
    }

    /// Clauses of the batch whose file is whole in the store but whose
    /// directory sync failed, so its durability is not confirmed.
    #[must_use]
    pub fn unconfirmed(&self) -> &[ClauseId] {
        &self.unconfirmed
    }

    /// Clauses of the failed request and of every request after it,
    /// except those of an unconfirmed batch.
    #[must_use]
    pub fn unprocessed(&self) -> &[ClauseId] {
        &self.unprocessed
    }

    /// True only when no clause is unprocessed and none is unconfirmed.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.unprocessed.is_empty() && self.unconfirmed.is_empty()
    }

    /// The summary text, written to standard output. Document identifiers
    /// come from the inputs, so the text passes through
    /// [`Redactor::redact_fragments`].
    #[must_use]
    pub fn render(&self, redactor: &Redactor) -> String {
        let state = if self.is_complete() {
            "complete"
        } else {
            "incomplete"
        };
        let mut text = format!("run summary: {state}\n");
        for (name, list, always) in [
            ("processed", &self.processed, true),
            ("stored", &self.stored, true),
            ("stored, durability not confirmed", &self.unconfirmed, false),
            ("unprocessed", &self.unprocessed, true),
        ] {
            if list.is_empty() && !always {
                continue;
            }
            text.push_str(&format!("{name}:\n"));
            if list.is_empty() {
                text.push_str("  (none)\n");
            }
            for c in list {
                text.push_str(&format!(
                    "  {} {} {}\n",
                    escape_controls(c.document.as_str()),
                    c.start,
                    c.end
                ));
            }
        }
        redactor.redact_fragments(&text)
    }
}
