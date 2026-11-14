//! Pure span and hash verification helpers used by the Explainer and review display
//! (design §3.5, steps 3 to 5 of the resolution algorithm).

use crate::SourceModel;
use rt_foundation::{Code, SourceSpan};
use rt_hash::Sha256Hex;

/// Why a recorded span does not resolve against a model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanFailure {
    /// The recorded document hash differs from the model hash (`RT-PRV-0002`).
    HashMismatch,
    /// `start >= end` or `end` beyond the document (`RT-PRV-0003`).
    OutOfRange,
    /// An offset is not on a UTF-8 scalar boundary (`RT-PRV-0004`).
    NotOnBoundary,
}

impl SpanFailure {
    /// The catalogue code of the failure.
    #[must_use]
    pub fn code(self) -> Code {
        match self {
            SpanFailure::HashMismatch => Code::ProvHashMismatch,
            SpanFailure::OutOfRange => Code::ProvSpanOutOfRange,
            SpanFailure::NotOnBoundary => Code::ProvSpanNotOnBoundary,
        }
    }
}

impl SourceModel {
    /// Checks a recorded document hash against the model hash.
    ///
    /// # Errors
    /// [`SpanFailure::HashMismatch`] when they differ.
    pub fn verify_hash(&self, recorded: &Sha256Hex) -> Result<(), SpanFailure> {
        if self.sha256() == recorded {
            Ok(())
        } else {
            Err(SpanFailure::HashMismatch)
        }
    }

    /// The exact text of the byte range `start..end`.
    ///
    /// # Errors
    /// [`SpanFailure::OutOfRange`] when `start >= end` or `end` is beyond the document, then
    /// [`SpanFailure::NotOnBoundary`] when an offset splits a character.
    pub fn text(&self, start: u32, end: u32) -> Result<&str, SpanFailure> {
        if start >= end || u64::from(end) > self.byte_len() {
            return Err(SpanFailure::OutOfRange);
        }
        self.content()
            .get(start as usize..end as usize)
            .ok_or(SpanFailure::NotOnBoundary)
    }

    /// Resolves a recorded span: hash first, then range, then boundaries. No text is produced
    /// unless every check passes.
    ///
    /// # Errors
    /// The first failed check, in the order of the Explainer's algorithm.
    pub fn resolve(
        &self,
        recorded_hash: &Sha256Hex,
        start: u32,
        end: u32,
    ) -> Result<&str, SpanFailure> {
        self.verify_hash(recorded_hash)?;
        self.text(start, end)
    }

    /// The exact text of a verified [`SourceSpan`], or `None` when the span names another
    /// document or does not lie in this one.
    #[must_use]
    pub fn span_text(&self, span: &SourceSpan) -> Option<&str> {
        if span.doc() != self.document() {
            return None;
        }
        self.text(span.start().get(), span.end().get()).ok()
    }
}
