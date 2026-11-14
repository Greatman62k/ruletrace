//! The Rule_Content hash and the Draft hash (design §3.8.2, §4.1.7; Req 25.3 to 25.5).
//!
//! Both hash the canonical compact bytes of the content, in this key order: `ir_version`, `id`,
//! `revision` (Rule_Content only), `kind`, `target`, `type`, the kind-specific body, `markers`
//! and `spans`. A span is `document`, `sha256` (the hash the Rulebook declares for that
//! document), `start`, `end`. The Origin, approval data, span `line` and `column` and every
//! derived value are not part of the input, so changing only those never changes the hash.

use crate::print::{body_members, marker_out, s, write, Out};
use crate::rule::{RuleContent, RuleRevision, Span};
use crate::rulebook::{DocumentDecl, IrVersion};
use rt_foundation::{DocumentId, RevisionNumber, RuleId};
use rt_hash::{sha256, Sha256Hex};

/// Why a content hash cannot be computed. The Validator reports both cases as diagnostics; no
/// hash is made from a guessed document hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentHashError {
    /// A span names a document the Rulebook does not declare.
    UndeclaredDocument(DocumentId),
    /// The Rulebook declares the document twice with different hashes.
    ConflictingDocument(DocumentId),
}

fn declared_hash<'a>(
    documents: &'a [DocumentDecl],
    id: &DocumentId,
) -> Result<&'a Sha256Hex, ContentHashError> {
    let mut found: Option<&Sha256Hex> = None;
    for doc in documents.iter().filter(|d| &d.id == id) {
        match found {
            Some(prev) if prev != &doc.sha256 => {
                return Err(ContentHashError::ConflictingDocument(id.clone()));
            }
            _ => found = Some(&doc.sha256),
        }
    }
    found.ok_or_else(|| ContentHashError::UndeclaredDocument(id.clone()))
}

fn hashed_span(span: &Span, documents: &[DocumentDecl]) -> Result<Out, ContentHashError> {
    let sha = declared_hash(documents, &span.document)?;
    Ok(Out::Obj(vec![
        ("document".to_owned(), s(span.document.as_str())),
        ("sha256".to_owned(), s(sha.as_str())),
        ("start".to_owned(), Out::Int(span.start)),
        ("end".to_owned(), Out::Int(span.end)),
    ]))
}

fn hash_input(
    ir_version: &IrVersion,
    id: &RuleId,
    revision: Option<RevisionNumber>,
    content: &RuleContent,
    documents: &[DocumentDecl],
) -> Result<String, ContentHashError> {
    let mut members: Vec<(String, Out)> = vec![
        ("ir_version".to_owned(), s(ir_version.as_str())),
        ("id".to_owned(), s(id.as_str())),
    ];
    if let Some(revision) = revision {
        members.push(("revision".to_owned(), Out::Int(revision.get())));
    }
    members.push(("kind".to_owned(), s(content.body.kind_name())));
    members.push(("target".to_owned(), s(content.target.as_str())));
    if let Some(t) = &content.ty {
        members.push(("type".to_owned(), Out::Str(t.text())));
    }
    members.extend(body_members(&content.body));
    members.push((
        "markers".to_owned(),
        Out::Arr(content.markers.iter().map(marker_out).collect()),
    ));
    let spans = content
        .spans
        .iter()
        .map(|span| hashed_span(span, documents))
        .collect::<Result<Vec<_>, _>>()?;
    members.push(("spans".to_owned(), Out::Arr(spans)));
    Ok(write(&Out::Obj(members), false))
}

/// The Rule_Content_Hash of a revision: SHA-256 of the canonical compact Rule_Content including
/// `ir_version` and `revision`. Approvals bind this value (Req 25.3 to 25.5).
///
/// `documents` are the Rulebook's declared documents; each span contributes the declared hash of
/// its document.
///
/// # Errors
/// [`ContentHashError`] when a span's document is undeclared or declared with two hashes.
pub fn rule_content_hash(
    ir_version: &IrVersion,
    revision: &RuleRevision,
    documents: &[DocumentDecl],
) -> Result<Sha256Hex, ContentHashError> {
    let bytes = hash_input(
        ir_version,
        &revision.key.id,
        Some(revision.key.revision),
        &revision.content,
        documents,
    )?;
    Ok(sha256(bytes.as_bytes()))
}

/// The Draft hash of a proposal: the same hash as [`rule_content_hash`] over the content without
/// `revision`, because the Revision_Number is assigned at adoption (design §3.8.2).
///
/// # Errors
/// The same [`ContentHashError`] cases as [`rule_content_hash`].
pub fn draft_hash(
    ir_version: &IrVersion,
    id: &RuleId,
    content: &RuleContent,
    documents: &[DocumentDecl],
) -> Result<Sha256Hex, ContentHashError> {
    let bytes = hash_input(ir_version, id, None, content, documents)?;
    Ok(sha256(bytes.as_bytes()))
}
