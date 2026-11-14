//! The Rulebook content hash (design §3.6.2, Req 9.5).
//!
//! The hash is the SHA-256 of the canonical compact Rule_IR of a normalized
//! copy of the Rulebook. The copy keeps the IR_Version, the type declarations,
//! the Effective revisions, the Precedence_Declarations and the referenced
//! documents. It drops every other revision, gives each revision the same
//! Origin, and gives the Rulebook a fixed name and version. The `line` and
//! `column` of a span are replaced by 1 because they are not part of the
//! Rule_Content. The ledger is never read, so the hash cannot depend on it.

use crate::lower::{err, Lowered};
use rt_foundation::{DocumentId, RuleId};
use rt_hash::{sha256, Sha256Hex};
use rt_ir::{
    print_compact, DocumentDecl, Origin, Precedence, RuleKey, RuleRevision, Rulebook, Span,
};
use std::collections::{BTreeMap, BTreeSet};

fn plain(span: &Span) -> Span {
    Span {
        line: 1,
        column: 1,
        ..span.clone()
    }
}

/// The hash of `rb` restricted to its Effective revisions.
pub(crate) fn rulebook_hash(
    rb: &Rulebook,
    effective: &BTreeMap<RuleId, RuleKey>,
) -> Lowered<Sha256Hex> {
    let revisions: Vec<RuleRevision> = rb
        .revisions
        .iter()
        .filter(|r| effective.get(&r.key.id) == Some(&r.key))
        .map(|r| {
            let mut copy = r.clone();
            copy.origin = Origin::Manual { author: None };
            copy.content.spans = r.content.spans.iter().map(plain).collect();
            copy
        })
        .collect();
    let precedence: Vec<Precedence> = rb
        .precedence
        .iter()
        .map(|p| Precedence {
            spans: p.spans.iter().map(plain).collect(),
            ..p.clone()
        })
        .collect();
    let referenced: BTreeSet<&DocumentId> = revisions
        .iter()
        .flat_map(|r| r.content.spans.iter())
        .chain(precedence.iter().flat_map(|p| p.spans.iter()))
        .map(|s| &s.document)
        .collect();
    let mut seen: BTreeSet<&DocumentId> = BTreeSet::new();
    let documents: Vec<DocumentDecl> = rb
        .documents
        .iter()
        .filter(|d| referenced.contains(&d.id) && seen.insert(&d.id))
        .cloned()
        .collect();
    let normalized = Rulebook {
        ir_version: rb.ir_version,
        name: String::new(),
        version: String::new(),
        documents,
        types: rb.types.clone(),
        revisions,
        precedence,
    };
    match print_compact(&normalized) {
        Ok(text) => Ok(sha256(text.as_bytes())),
        Err(_) => err(
            None,
            "the Rulebook exceeds a Rule_IR limit and cannot be hashed",
        ),
    }
}
