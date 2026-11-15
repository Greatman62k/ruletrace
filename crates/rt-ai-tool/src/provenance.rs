//! Provenance verification of parsed proposals (design 3.7.4 step 3, Req
//! 23.7, 23.8, 23.10).
//!
//! Every evidence span of every proposal is checked against the supplied
//! Source_Models and the clauses of the Provider_Request. Rule spans equal
//! the evidence spans by construction of the parser, so checking the
//! evidence covers both. Nothing is repaired, trimmed or re-anchored.

use crate::plan::ClauseId;
use crate::response::{EvidenceSite, ParsedEvidence, ParsedResponse};
use rt_foundation::{Code, Diagnostic, InputId, InputLocation, Location, SpanSource};
use rt_json::Position;
use rt_srcmodel::{SourceModel, SourceModels};

/// Verifies every evidence span of every proposal, advisory kinds included.
///
/// Each span yields at most one Diagnostic, the first failed check of
/// Req 23.7 in this order: unknown document, hash mismatch, range or
/// boundary failure, claimed line or column (design 3.7.4), outside every
/// clause of the request, quote mismatch. All failing spans are reported,
/// in ascending proposal index and then evidence index. An empty result
/// means every span verified.
///
/// `response_id` names the response in every Diagnostic location, and
/// `clauses` are those of the Provider_Request that produced the response.
#[must_use]
pub fn verify_provenance(
    response_id: &InputId,
    response: &ParsedResponse,
    models: &SourceModels,
    clauses: &[ClauseId],
) -> Vec<Diagnostic> {
    response
        .proposals
        .iter()
        .flat_map(|p| p.evidence.iter())
        .filter_map(|e| {
            check(e, models, clauses).map(|f| {
                Diagnostic::new(
                    f.code,
                    f.message,
                    place(response_id, &e.site, f.at, f.field),
                )
            })
        })
        .collect()
}

struct Failure {
    code: Code,
    message: String,
    at: Position,
    field: Option<&'static str>,
}

fn fail(code: Code, message: String, at: Position, field: Option<&'static str>) -> Failure {
    Failure {
        code,
        message,
        at,
        field,
    }
}

fn place(id: &InputId, site: &EvidenceSite, at: Position, field: Option<&'static str>) -> Location {
    let path = match field {
        Some(name) => format!("{}.{name}", site.path),
        None => site.path.clone(),
    };
    Location::Input(InputLocation {
        input: id.clone(),
        line: Some(at.line),
        column: Some(at.column),
        field: Some(path),
    })
}

fn check(e: &ParsedEvidence, models: &SourceModels, clauses: &[ClauseId]) -> Option<Failure> {
    let site = &e.site;
    let Some(model) = models.get(&e.document) else {
        return Some(fail(
            Code::AiProposalSpanUnresolved,
            format!(
                "document '{}' is not among the supplied sources",
                e.document
            ),
            site.document,
            Some("document"),
        ));
    };
    if &e.sha256 != model.sha256() {
        return Some(fail(
            Code::AiProposalHashMismatch,
            format!(
                "the stated hash of '{}' differs from the recorded hash",
                e.document
            ),
            site.sha256,
            Some("sha256"),
        ));
    }
    if let Some(f) = range(e, model) {
        return Some(f);
    }
    if let Some(f) = position(e, model) {
        return Some(f);
    }
    let inside = clauses
        .iter()
        .any(|c| c.document == e.document && c.start <= e.start && e.end <= c.end);
    if !inside {
        return Some(fail(
            Code::AiProposalSpanOutsideContext,
            format!(
                "span {}..{} of '{}' lies within no clause of the request",
                e.start, e.end, e.document
            ),
            site.item,
            None,
        ));
    }
    let bytes = usize::try_from(e.start)
        .ok()
        .zip(usize::try_from(e.end).ok())
        .and_then(|(s, t)| model.content().get(s..t));
    if bytes != Some(e.quote.as_str()) {
        return Some(fail(
            Code::AiProposalQuoteMismatch,
            format!(
                "the quote differs from the bytes {}..{} of '{}'",
                e.start, e.end, e.document
            ),
            site.quote,
            Some("quote"),
        ));
    }
    None
}

fn range(e: &ParsedEvidence, model: &SourceModel) -> Option<Failure> {
    let site = &e.site;
    let unresolved = |what: &str, at: Position, field: &'static str| {
        Some(fail(
            Code::AiProposalSpanUnresolved,
            format!(
                "span {}..{} of '{}' does not resolve: {what}",
                e.start, e.end, e.document
            ),
            at,
            Some(field),
        ))
    };
    if e.start >= e.end {
        return unresolved("start is not less than end", site.end, "end");
    }
    if u64::from(e.end) > model.byte_len() {
        return unresolved("end is beyond the document", site.end, "end");
    }
    if !model.is_char_boundary(e.start) {
        return unresolved("start is inside a character", site.start, "start");
    }
    if !model.is_char_boundary(e.end) {
        return unresolved("end is inside a character", site.end, "end");
    }
    None
}

fn position(e: &ParsedEvidence, model: &SourceModel) -> Option<Failure> {
    let site = &e.site;
    let Some((line, column)) = model.line_column(e.start) else {
        return Some(fail(
            Code::AiProposalSpanUnresolved,
            format!("start {} of '{}' has no position", e.start, e.document),
            site.start,
            Some("start"),
        ));
    };
    let (at, field) = if e.line != line {
        (site.line, "line")
    } else if e.column != column {
        (site.column, "column")
    } else {
        return None;
    };
    Some(fail(
        Code::AiProposalSpanUnresolved,
        format!(
            "the claimed line {} and column {} differ from {line} and {column}",
            e.line, e.column
        ),
        at,
        Some(field),
    ))
}
