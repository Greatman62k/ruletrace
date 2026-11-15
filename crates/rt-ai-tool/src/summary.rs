//! The Run_Summary (design 3.7.7; Req 23.36, 23.37).
//!
//! The summary is plain text with three lists of clause identifiers. It
//! states `complete` only when no clause is unprocessed, and `incomplete`
//! in every other case.

use crate::plan::ClauseId;
use crate::redact::Redactor;
use rt_foundation::escape_controls;
use std::collections::BTreeSet;

/// The clauses of a run, by outcome.
///
/// Invariants held by construction: `processed` and `unprocessed` are
/// disjoint, their union is every requested clause, and `stored` is a
/// subset of `processed`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunSummary {
    processed: Vec<ClauseId>,
    stored: Vec<ClauseId>,
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
        let done: BTreeSet<&ClauseId> = processed.iter().collect();
        let kept: BTreeSet<&ClauseId> = stored.iter().filter(|c| done.contains(c)).collect();
        let rest: BTreeSet<&ClauseId> = requested.iter().filter(|c| !done.contains(c)).collect();
        Self {
            processed: sorted(done.into_iter().cloned()),
            stored: sorted(kept.into_iter().cloned()),
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

    /// Clauses of the failed request and of every request after it.
    #[must_use]
    pub fn unprocessed(&self) -> &[ClauseId] {
        &self.unprocessed
    }

    /// True only when no clause is unprocessed.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.unprocessed.is_empty()
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
        for (name, list) in [
            ("processed", &self.processed),
            ("stored", &self.stored),
            ("unprocessed", &self.unprocessed),
        ] {
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
