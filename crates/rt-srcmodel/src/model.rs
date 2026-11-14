//! The Source_Model value and its structural checks.

use rt_foundation::{DocumentId, SpanSource, MAX_SOURCE_DOCUMENT_BYTES};
use rt_hash::{sha256, Sha256Hex};
use std::fmt;

/// The format of the ingested document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DocumentKind {
    /// A Markdown document.
    Markdown,
    /// A plain text document.
    Text,
}

impl DocumentKind {
    /// The name used in the serialized form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentKind::Markdown => "markdown",
            DocumentKind::Text => "text",
        }
    }
}

/// The structural kind of an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ElementKind {
    /// An ATX heading (has a level).
    Heading,
    /// A paragraph.
    Paragraph,
    /// A list item.
    ListItem,
    /// A fenced code block.
    CodeBlock,
    /// A pipe table.
    Table,
    /// A header or body row of a table.
    TableRow,
}

impl ElementKind {
    /// The name used in the serialized form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ElementKind::Heading => "heading",
            ElementKind::Paragraph => "paragraph",
            ElementKind::ListItem => "list_item",
            ElementKind::CodeBlock => "code_block",
            ElementKind::Table => "table",
            ElementKind::TableRow => "table_row",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "heading" => ElementKind::Heading,
            "paragraph" => ElementKind::Paragraph,
            "list_item" => ElementKind::ListItem,
            "code_block" => ElementKind::CodeBlock,
            "table" => ElementKind::Table,
            "table_row" => ElementKind::TableRow,
            _ => return None,
        })
    }
}

/// One structural element of a document. The values are checked when the
/// element is part of a [`SourceModel`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    /// Identifier. Equal to the position of the element in the model.
    pub id: u32,
    /// Kind.
    pub kind: ElementKind,
    /// Heading level 1 to 6. `Some` for a heading and `None` for every other
    /// kind.
    pub level: Option<u8>,
    /// Leading numbering label, when present.
    pub label: Option<String>,
    /// Start byte offset (inclusive).
    pub start: u32,
    /// End byte offset (exclusive).
    pub end: u32,
    /// 1-based line of `start`.
    pub line: u32,
    /// 1-based column of `start`, in Unicode scalar values.
    pub column: u32,
    /// Identifier of the enclosing heading or list item. It is lower than `id`.
    pub parent: Option<u32>,
}

/// Why a Source_Model is invalid. Each case becomes `RT-PRV-0005`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelError {
    /// The content is longer than 16 MiB, which the Ingestor never produces.
    ContentTooLarge,
    /// The SHA-256 of the content differs from the recorded hash.
    HashMismatch,
    /// The recorded byte count differs from the content length.
    LengthMismatch,
    /// More elements than a 32-bit identifier can number.
    TooManyElements,
    /// An element identifier is not its position.
    IdNotSequential(u32),
    /// An element starts before the previous element.
    NotOrdered(u32),
    /// An element has `start >= end` or `end` beyond the content.
    SpanOutOfRange(u32),
    /// An element offset is not on a UTF-8 character boundary.
    SpanNotOnBoundary(u32),
    /// An element's line and column do not match its start offset.
    LineColumnMismatch(u32),
    /// An element's parent is not an earlier element.
    BadParent(u32),
    /// A heading has no level 1 to 6, or another kind has a level.
    BadLevel(u32),
}

impl ModelError {
    /// The element index the error concerns, when it concerns one.
    #[must_use]
    pub fn element(&self) -> Option<u32> {
        match self {
            ModelError::IdNotSequential(i)
            | ModelError::NotOrdered(i)
            | ModelError::SpanOutOfRange(i)
            | ModelError::SpanNotOnBoundary(i)
            | ModelError::LineColumnMismatch(i)
            | ModelError::BadParent(i)
            | ModelError::BadLevel(i) => Some(*i),
            _ => None,
        }
    }
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::ContentTooLarge => f.write_str("content is larger than 16 MiB"),
            ModelError::HashMismatch => {
                f.write_str("sha256 does not match the SHA-256 of the content")
            }
            ModelError::LengthMismatch => f.write_str("bytes does not match the content length"),
            ModelError::TooManyElements => f.write_str("too many elements"),
            ModelError::IdNotSequential(i) => {
                write!(f, "element {i} has an id that is not its position")
            }
            ModelError::NotOrdered(i) => {
                write!(f, "element {i} starts before the previous element")
            }
            ModelError::SpanOutOfRange(i) => {
                write!(
                    f,
                    "element {i} has start not below end, or end beyond the content"
                )
            }
            ModelError::SpanNotOnBoundary(i) => {
                write!(
                    f,
                    "element {i} has an offset that is not on a character boundary"
                )
            }
            ModelError::LineColumnMismatch(i) => {
                write!(
                    f,
                    "element {i} has a line and column that do not match its start"
                )
            }
            ModelError::BadParent(i) => write!(f, "element {i} has a parent that is not earlier"),
            ModelError::BadLevel(i) => write!(f, "element {i} has an invalid heading level"),
        }
    }
}

impl std::error::Error for ModelError {}

/// A verified Source_Model: the whole document text, its hash and its elements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceModel {
    document: DocumentId,
    kind: DocumentKind,
    sha256: Sha256Hex,
    content: String,
    elements: Vec<Element>,
    /// Byte offset of the start of each line. The first entry is 0.
    line_starts: Vec<u32>,
}

impl SourceModel {
    /// Builds a model from the document text, computing the hash and the byte
    /// length, and checks every element against the text.
    ///
    /// # Errors
    /// The first failed structural check as a [`ModelError`].
    pub fn new(
        document: DocumentId,
        kind: DocumentKind,
        content: String,
        elements: Vec<Element>,
    ) -> Result<Self, ModelError> {
        let hash = sha256(content.as_bytes());
        Self::from_claimed(document, kind, hash, content, elements)
    }

    /// Builds a model from a recorded hash, which must equal the hash of the
    /// content.
    pub(crate) fn from_claimed(
        document: DocumentId,
        kind: DocumentKind,
        claimed: Sha256Hex,
        content: String,
        elements: Vec<Element>,
    ) -> Result<Self, ModelError> {
        if u64::try_from(content.len()).map_or(true, |n| n > MAX_SOURCE_DOCUMENT_BYTES) {
            return Err(ModelError::ContentTooLarge);
        }
        if sha256(content.as_bytes()) != claimed {
            return Err(ModelError::HashMismatch);
        }
        let line_starts = line_starts(&content);
        let model = Self {
            document,
            kind,
            sha256: claimed,
            content,
            elements,
            line_starts,
        };
        model.check_elements()?;
        Ok(model)
    }

    fn check_elements(&self) -> Result<(), ModelError> {
        let len = self.len32();
        let mut previous_start = 0u32;
        for (index, e) in self.elements.iter().enumerate() {
            let position = u32::try_from(index).map_err(|_| ModelError::TooManyElements)?;
            if e.id != position {
                return Err(ModelError::IdNotSequential(position));
            }
            if e.start < previous_start {
                return Err(ModelError::NotOrdered(position));
            }
            previous_start = e.start;
            if e.start >= e.end || e.end > len {
                return Err(ModelError::SpanOutOfRange(position));
            }
            if !self.content.is_char_boundary(e.start as usize)
                || !self.content.is_char_boundary(e.end as usize)
            {
                return Err(ModelError::SpanNotOnBoundary(position));
            }
            if self.position_of(e.start) != Some((e.line, e.column)) {
                return Err(ModelError::LineColumnMismatch(position));
            }
            if e.parent.is_some_and(|p| p >= position) {
                return Err(ModelError::BadParent(position));
            }
            let level_ok = match e.kind {
                ElementKind::Heading => matches!(e.level, Some(1..=6)),
                _ => e.level.is_none(),
            };
            if !level_ok {
                return Err(ModelError::BadLevel(position));
            }
        }
        Ok(())
    }

    /// Content length as `u32`. The size check at construction keeps it within
    /// range.
    fn len32(&self) -> u32 {
        u32::try_from(self.content.len()).unwrap_or(u32::MAX)
    }

    fn position_of(&self, offset: u32) -> Option<(u32, u32)> {
        if offset > self.len32() || !self.content.is_char_boundary(offset as usize) {
            return None;
        }
        let lines = self.line_starts.partition_point(|&s| s <= offset);
        let start = *self.line_starts.get(lines.checked_sub(1)?)?;
        let column = self
            .content
            .get(start as usize..offset as usize)?
            .chars()
            .count();
        Some((
            u32::try_from(lines).ok()?,
            u32::try_from(column).ok()?.checked_add(1)?,
        ))
    }

    /// The document identifier.
    #[must_use]
    pub fn document(&self) -> &DocumentId {
        &self.document
    }

    /// The document format.
    #[must_use]
    pub fn kind(&self) -> DocumentKind {
        self.kind
    }

    /// The SHA-256 of the document bytes.
    #[must_use]
    pub fn sha256(&self) -> &Sha256Hex {
        &self.sha256
    }

    /// The whole document text.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    /// The document length in bytes.
    #[must_use]
    pub fn byte_len(&self) -> u64 {
        u64::from(self.len32())
    }

    /// The elements in ascending start order.
    #[must_use]
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    /// The element with the given identifier.
    #[must_use]
    pub fn element(&self, id: u32) -> Option<&Element> {
        self.elements.get(id as usize)
    }
}

/// Byte offset of the start of every line. LF, CRLF and a lone CR each end one
/// line.
fn line_starts(content: &str) -> Vec<u32> {
    let mut starts = vec![0u32];
    let mut bytes = content.as_bytes().iter().enumerate().peekable();
    while let Some((i, &b)) = bytes.next() {
        let next = match b {
            b'\n' => i + 1,
            b'\r' => {
                if matches!(bytes.peek(), Some((_, b'\n'))) {
                    bytes.next();
                    i + 2
                } else {
                    i + 1
                }
            }
            _ => continue,
        };
        if let Ok(next) = u32::try_from(next) {
            starts.push(next);
        }
    }
    starts
}

impl SpanSource for SourceModel {
    fn document_id(&self) -> &DocumentId {
        &self.document
    }

    fn len_bytes(&self) -> u64 {
        self.byte_len()
    }

    fn is_char_boundary(&self, offset: u32) -> bool {
        self.content.is_char_boundary(offset as usize)
    }

    fn line_column(&self, offset: u32) -> Option<(u32, u32)> {
        self.position_of(offset)
    }
}
