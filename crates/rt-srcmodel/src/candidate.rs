//! The Candidate_Clause value.

use crate::SourceModel;
use rt_foundation::{SourceSpan, SpanError, SpanSource};

/// A clause the Extractor offers for rule authoring. It is not a Rule and has
/// no approval state (Req 2.2). The span was verified against the model, and
/// the text is the exact span bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateClause {
    span: SourceSpan,
    label: Option<String>,
    text: String,
}

impl CandidateClause {
    /// Builds a clause for the byte range `start..end` of `model`. Line and
    /// column come from the model, and the text is copied from it.
    ///
    /// # Errors
    /// A [`SpanError`] when the range is empty, outside the document or not on
    /// character boundaries.
    pub fn new(
        model: &SourceModel,
        start: u32,
        end: u32,
        label: Option<String>,
    ) -> Result<Self, SpanError> {
        let (line, column) = model.line_column(start).ok_or(SpanError::OutOfBounds)?;
        let span = SourceSpan::new(model, start, end, line, column)?;
        let text = model
            .span_text(&span)
            .ok_or(SpanError::OutOfBounds)?
            .to_owned();
        Ok(Self { span, label, text })
    }

    /// The verified span.
    #[must_use]
    pub fn span(&self) -> &SourceSpan {
        &self.span
    }

    /// The numbering label, when the clause has one.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// The exact source text of the span.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}
