//! Locations attached to diagnostics.
//!
//! A [`SourceSpan`] has private fields and one constructor, which verifies the span against a
//! [`SpanSource`] (implemented by the Source_Model). A span from untrusted input is therefore plain
//! data elsewhere and never becomes a `Location::Source` without that check (Req 7.14, 7.15).

use crate::ids::{DocumentId, InputId};
use crate::Code;
use std::fmt;

/// Byte offset into a Source_Document (documents are limited to 16 MiB by default).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(u32);

impl ByteOffset {
    /// Builds an offset.
    #[must_use]
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    /// The numeric value.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

/// The view of a verified Source_Model that span construction needs.
pub trait SpanSource {
    /// Identifier of the document.
    fn document_id(&self) -> &DocumentId;
    /// Length of the document in bytes.
    fn len_bytes(&self) -> u64;
    /// True when `offset` is on a UTF-8 character boundary of the document.
    fn is_char_boundary(&self, offset: u32) -> bool;
    /// 1-based `(line, column)` of the byte offset, or `None` when it is outside the document.
    fn line_column(&self, offset: u32) -> Option<(u32, u32)>;
}

/// Why a span was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanError {
    /// `start` is not less than `end`.
    EmptyOrReversed,
    /// `end` is greater than the document length.
    OutOfBounds,
    /// An offset is not on a UTF-8 character boundary.
    NotOnBoundary,
    /// The stated line and column do not match the start offset.
    LineColumnMismatch,
}

impl fmt::Display for SpanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            SpanError::EmptyOrReversed => "span start is not less than its end",
            SpanError::OutOfBounds => "span end is beyond the document length",
            SpanError::NotOnBoundary => "span offset is not on a UTF-8 character boundary",
            SpanError::LineColumnMismatch => "span line and column do not match the start offset",
        };
        f.write_str(text)
    }
}

impl std::error::Error for SpanError {}

/// A verified span of a source document.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceSpan {
    doc: DocumentId,
    start: ByteOffset,
    end: ByteOffset,
    line: u32,
    column: u32,
}

impl SourceSpan {
    /// Builds a span after checking it against the document: `start < end <= length`, both
    /// offsets on character boundaries, and `line`/`column` equal to the 1-based position of
    /// `start`.
    ///
    /// # Errors
    /// Returns the first failed condition as a [`SpanError`].
    pub fn new(
        source: &impl SpanSource,
        start: u32,
        end: u32,
        line: u32,
        column: u32,
    ) -> Result<Self, SpanError> {
        if start >= end {
            return Err(SpanError::EmptyOrReversed);
        }
        if u64::from(end) > source.len_bytes() {
            return Err(SpanError::OutOfBounds);
        }
        if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            return Err(SpanError::NotOnBoundary);
        }
        if source.line_column(start) != Some((line, column)) {
            return Err(SpanError::LineColumnMismatch);
        }
        Ok(Self {
            doc: source.document_id().clone(),
            start: ByteOffset(start),
            end: ByteOffset(end),
            line,
            column,
        })
    }

    /// The document.
    #[must_use]
    pub fn doc(&self) -> &DocumentId {
        &self.doc
    }

    /// Start offset (inclusive).
    #[must_use]
    pub fn start(&self) -> ByteOffset {
        self.start
    }

    /// End offset (exclusive).
    #[must_use]
    pub fn end(&self) -> ByteOffset {
        self.end
    }

    /// 1-based line of the start offset.
    #[must_use]
    pub fn line(&self) -> u32 {
        self.line
    }

    /// 1-based column of the start offset.
    #[must_use]
    pub fn column(&self) -> u32 {
        self.column
    }
}

/// A position in an input file (Rule_IR, ledger, data, configuration).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputLocation {
    /// The input file.
    pub input: InputId,
    /// 1-based line, when known.
    pub line: Option<u32>,
    /// 1-based column, when known.
    pub column: Option<u32>,
    /// Field path, when known.
    pub field: Option<String>,
}

/// A position on the command line.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CliLocation {
    /// An argument by position and optional option name.
    Argument {
        /// Zero-based position.
        position: u32,
        /// The option name, when the argument is an option.
        name: Option<String>,
    },
    /// A command or subcommand name.
    Command(String),
}

/// Where a diagnostic points. Variants are ordered from most to least specific.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Location {
    /// A verified span of a source document.
    Source(SourceSpan),
    /// A whole document.
    Document(DocumentId),
    /// A position in an input file.
    Input(InputLocation),
    /// A position on the command line.
    Cli(CliLocation),
    /// No location exists. The reason is a catalogue code.
    None(Code),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "ab\ncé" : bytes a=0 b=1 \n=2 c=3 é=4..6, length 6.
    struct Doc(DocumentId);

    impl SpanSource for Doc {
        fn document_id(&self) -> &DocumentId {
            &self.0
        }
        fn len_bytes(&self) -> u64 {
            6
        }
        fn is_char_boundary(&self, offset: u32) -> bool {
            offset != 5
        }
        fn line_column(&self, offset: u32) -> Option<(u32, u32)> {
            match offset {
                0..=2 => Some((1, offset + 1)),
                3..=6 => Some((2, offset - 2)),
                _ => None,
            }
        }
    }

    fn doc() -> Doc {
        Doc(DocumentId::new("rules.md").unwrap())
    }

    #[test]
    fn builds_verified_span() {
        let s = SourceSpan::new(&doc(), 3, 6, 2, 1).unwrap();
        assert_eq!(
            (s.start().get(), s.end().get(), s.line(), s.column()),
            (3, 6, 2, 1)
        );
        assert_eq!(s.doc().as_str(), "rules.md");
        assert!(matches!(Location::Source(s), Location::Source(_)));
    }

    #[test]
    fn refuses_bad_spans() {
        let d = doc();
        assert_eq!(
            SourceSpan::new(&d, 3, 3, 2, 1),
            Err(SpanError::EmptyOrReversed)
        );
        assert_eq!(
            SourceSpan::new(&d, 4, 3, 2, 2),
            Err(SpanError::EmptyOrReversed)
        );
        assert_eq!(SourceSpan::new(&d, 0, 7, 1, 1), Err(SpanError::OutOfBounds));
        assert_eq!(
            SourceSpan::new(&d, 3, 5, 2, 1),
            Err(SpanError::NotOnBoundary)
        );
        assert_eq!(
            SourceSpan::new(&d, 3, 6, 2, 2),
            Err(SpanError::LineColumnMismatch)
        );
    }
}
