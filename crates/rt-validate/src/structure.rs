//! Pass 1 (span structure) and the model-dependent span checks of pass 2.

use crate::ctx::{Decls, DocState, Site};
use rt_foundation::{Code, DiagnosticState, Location, SpanError, MAX_SPANS_PER_RULE};
use rt_ir::Span;

/// Pass 1, structure: at least one span, at most 64, every span names a declared document and
/// has `start < end`. Needs no Source_Model.
pub(crate) fn check_span_structure(
    spans: &[Span],
    site: &Site,
    decls: &Decls<'_>,
    out: &mut DiagnosticState,
) -> Vec<bool> {
    if spans.is_empty() {
        out.push(site.diag(
            Code::ValRuleNoSourceSpan,
            format!("rule '{}' has no Source_Span", site.rule.rule),
        ));
    }
    if spans.len() > MAX_SPANS_PER_RULE as usize {
        out.push(site.diag(
            Code::LimitCountExceeded,
            format!(
                "rule '{}' has {} spans, more than the limit of {MAX_SPANS_PER_RULE}",
                site.rule.rule,
                spans.len()
            ),
        ));
    }
    spans
        .iter()
        .enumerate()
        .map(|(i, span)| {
            let doc_loc = Location::Document(span.document.clone());
            if !decls.docs.contains_key(&span.document) {
                out.push(
                    rt_foundation::Diagnostic::new(
                        Code::ValSpanInvalid,
                        format!(
                            "span {i} of rule '{}' names document '{}', which the Rulebook does not declare",
                            site.rule.rule, span.document
                        ),
                        doc_loc,
                    )
                    .with_rules(vec![site.rule.clone()]),
                );
                false
            } else if span.start >= span.end {
                out.push(
                    rt_foundation::Diagnostic::new(
                        Code::ValSpanInvalid,
                        format!(
                            "span {i} of rule '{}' has start {} not less than end {}",
                            site.rule.rule, span.start, span.end
                        ),
                        doc_loc,
                    )
                    .with_rules(vec![site.rule.clone()]),
                );
                false
            } else {
                true
            }
        })
        .collect()
}

/// Pass 2, spans against the Source_Model: bounds (`RT-VAL-0019`), character boundaries
/// (`RT-VAL-0016`) and line and column (`RT-VAL-0022`). `structural_ok[i]` is false for a span
/// that pass 1 already reported, which is then skipped so one defect gives one Diagnostic.
pub(crate) fn check_span_sources(
    spans: &[Span],
    structural_ok: &[bool],
    site: &Site,
    decls: &Decls<'_>,
    out: &mut DiagnosticState,
) {
    for (i, span) in spans.iter().enumerate() {
        if !structural_ok.get(i).copied().unwrap_or(false) {
            continue;
        }
        let Some(DocState::Verified(model)) = decls.docs.get(&span.document) else {
            continue;
        };
        let Err(err) =
            rt_foundation::SourceSpan::new(*model, span.start, span.end, span.line, span.column)
        else {
            continue;
        };
        let (code, what) = match err {
            SpanError::OutOfBounds => (
                Code::ValSpanOutsideDocument,
                format!(
                    "ends at byte {}, beyond the {} bytes of the document",
                    span.end,
                    rt_foundation::SpanSource::len_bytes(*model)
                ),
            ),
            SpanError::NotOnBoundary => (
                Code::ValSpanInvalid,
                "has an offset that is not on a UTF-8 character boundary".to_owned(),
            ),
            SpanError::LineColumnMismatch => (
                Code::ValSpanLineColumnMismatch,
                format!(
                    "declares line {} column {}, which differ from the Source_Model",
                    span.line, span.column
                ),
            ),
            SpanError::EmptyOrReversed => (
                Code::ValSpanInvalid,
                "has start not less than end".to_owned(),
            ),
        };
        out.push(
            rt_foundation::Diagnostic::new(
                code,
                format!(
                    "span {i} of rule '{}' in document '{}' {what}",
                    site.rule.rule, span.document
                ),
                Location::Document(span.document.clone()),
            )
            .with_rules(vec![site.rule.clone()]),
        );
    }
}
