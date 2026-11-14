//! Source ingestion (design §3.5, Requirement 1): document bytes to a verified `SourceModel`.
//!
//! [`ingest`] is a pure function of its four arguments. It does no file, network, clock or
//! environment access. The checks run in a fixed order and the first failure decides the single
//! diagnostic: size limit, format, UTF-8, then structure. No `SourceModel` is produced for a
//! failed document.
#![forbid(unsafe_code)]

mod extract;
mod label;
mod structure;

pub use extract::{extract, ExtractionMode};

use rt_foundation::{Code, Diagnostic, DocumentId, Location, MAX_SOURCE_DOCUMENT_BYTES};
use rt_srcmodel::{DocumentKind, Element, SourceModel};

/// How many leading bytes are searched for an HTML marker.
const MAGIC_WINDOW: usize = 512;

/// Ingests a document.
///
/// - `bytes`: the raw document, including any BOM.
/// - `id`: the caller's opaque document identifier. Its extension selects the format.
/// - `format_hint`: overrides the extension. It never overrides the content check.
/// - `limit`: the effective size limit in bytes. A limit above 16 MiB is capped at 16 MiB.
///
/// # Errors
/// One diagnostic with the Document_Location of `id`:
/// - `RT-LIM-0001` when the document is larger than a configured `limit` below 16 MiB.
/// - `RT-ING-0003` when the document is larger than 16 MiB and the limit is not lower.
/// - `RT-ING-0002` (Unsupported) for an unsupported extension or a binary or HTML prefix.
/// - `RT-ING-0001` when the bytes are not UTF-8; the message gives the first invalid offset.
/// - `RT-ING-0004` when the structure found does not pass the `SourceModel` checks.
///   This indicates a defect in the Ingestor, not in the document.
pub fn ingest(
    bytes: &[u8],
    id: &DocumentId,
    format_hint: Option<DocumentKind>,
    limit: u64,
) -> Result<SourceModel, Box<Diagnostic>> {
    let fail = |code: Code, message: String| {
        Box::new(Diagnostic::new(
            code,
            message,
            Location::Document(id.clone()),
        ))
    };
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > limit.min(MAX_SOURCE_DOCUMENT_BYTES) {
        return Err(if limit < MAX_SOURCE_DOCUMENT_BYTES {
            fail(
                Code::LimitFileTooLarge,
                format!(
                    "document '{id}' is {size} bytes, above the effective limit of {limit} bytes"
                ),
            )
        } else {
            fail(
                Code::IngestDocumentTooLarge,
                format!(
                    "document '{id}' is {size} bytes, above the limit of \
                     {MAX_SOURCE_DOCUMENT_BYTES} bytes"
                ),
            )
        });
    }
    let kind = detect_format(bytes, id.as_str(), format_hint).map_err(|name| {
        fail(
            Code::IngestFormatUnsupported,
            unsupported_message(id, &name),
        )
    })?;
    let text = std::str::from_utf8(bytes).map_err(|e| {
        fail(
            Code::IngestNotUtf8,
            format!(
                "document '{id}' is not valid UTF-8: first invalid byte at offset {}",
                e.valid_up_to()
            ),
        )
    })?;
    let internal = |what: String| fail(Code::ExtractSourceModelInvalid, what);
    let drafts = structure::parse(text, kind == DocumentKind::Markdown)
        .map_err(|_| internal(format!("document '{id}' has an offset beyond 32 bits")))?;
    let elements = drafts
        .iter()
        .enumerate()
        .map(|(index, d)| {
            Some(Element {
                id: u32::try_from(index).ok()?,
                kind: d.kind,
                level: d.level,
                label: d.label(text).map(str::to_owned),
                start: u32::try_from(d.start).ok()?,
                end: u32::try_from(d.end).ok()?,
                line: d.line,
                column: d.column,
                parent: d.parent,
            })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| internal(format!("document '{id}' has an offset beyond 32 bits")))?;
    SourceModel::new(id.clone(), kind, text.to_owned(), elements)
        .map_err(|e| internal(format!("document '{id}' gave an invalid Source_Model: {e}")))
}

fn unsupported_message(id: &DocumentId, name: &str) -> String {
    format!("document '{id}' has an unsupported format: {name}; only plain text and Markdown are supported")
}

/// The document kind, or the name of the unsupported format.
fn detect_format(
    bytes: &[u8],
    id: &str,
    hint: Option<DocumentKind>,
) -> Result<DocumentKind, String> {
    let kind = match hint {
        Some(kind) => kind,
        None => kind_from_identifier(id)?,
    };
    match binary_or_html_prefix(bytes) {
        Some(name) => Err(name.to_owned()),
        None => Ok(kind),
    }
}

fn kind_from_identifier(id: &str) -> Result<DocumentKind, String> {
    let name = id.rsplit(['/', '\\']).next().unwrap_or(id);
    match name.rsplit_once('.') {
        None => Ok(DocumentKind::Text),
        Some((_, ext)) => match ext.to_ascii_lowercase().as_str() {
            "md" | "markdown" => Ok(DocumentKind::Markdown),
            "txt" | "text" => Ok(DocumentKind::Text),
            _ => Err(format!("extension '.{ext}'")),
        },
    }
}

fn binary_or_html_prefix(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"%PDF-") {
        return Some("PDF content");
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Some("ZIP container content");
    }
    if bytes.starts_with(b"\x89PNG") {
        return Some("PNG image content");
    }
    let window: Vec<u8> = bytes
        .iter()
        .take(MAGIC_WINDOW)
        .map(u8::to_ascii_lowercase)
        .collect();
    let contains = |needle: &[u8]| window.windows(needle.len()).any(|w| w == needle);
    (contains(b"<!doctype html") || contains(b"<html")).then_some("HTML content")
}

#[cfg(test)]
mod tests;
