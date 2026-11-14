//! Span resolution and the derived verification status (design §3.5).

use rt_foundation::{
    escape_controls, Code, Diagnostic, DiagnosticState, DocumentId, Location, RevisionNumber,
    RuleId, RuleRef, SourceSpan,
};
use rt_srcmodel::{SourceModel, SourceModels, SpanFailure};
use rt_trace::{DecisionTrace, Entry, TraceSpan};
use std::collections::BTreeMap;

/// Whether a trace is verified. Always derived from a [`Verification`], never stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationStatus {
    /// At least one span, every span resolved, and no blocking verification diagnostic.
    Verified,
    /// Anything else.
    NotVerified,
}

/// What became of one recorded span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpanResolution {
    /// The span resolved. The text is the exact bytes of the Source_Model between start and end.
    Resolved(String),
    /// The span did not resolve. The code is the catalogue code of the failure. No text exists.
    Unresolved(Code),
}

/// One span of the trace with its resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpanVerdict {
    /// The rule that references the span.
    pub rule: RuleId,
    /// Its revision, when the trace records one for this reference.
    pub revision: Option<RevisionNumber>,
    /// The span as recorded in the trace.
    pub span: TraceSpan,
    /// The result.
    pub resolution: SpanResolution,
}

impl SpanVerdict {
    /// The exact source text, only when the span resolved.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match &self.resolution {
            SpanResolution::Resolved(t) => Some(t),
            SpanResolution::Unresolved(_) => None,
        }
    }
}

/// The result of verifying one trace: a verdict per span (in the order of
/// `DecisionTrace::spans`) and the verification diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verification {
    spans: Vec<SpanVerdict>,
    diagnostics: DiagnosticState,
}

impl Verification {
    /// Verdicts in the order of `DecisionTrace::spans`.
    #[must_use]
    pub fn spans(&self) -> &[SpanVerdict] {
        &self.spans
    }

    /// The verification diagnostics, separate from the trace's own diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticState {
        &self.diagnostics
    }

    /// Derived from the verdicts and the diagnostics only (Req 13.17).
    #[must_use]
    pub fn status(&self) -> VerificationStatus {
        let all = self
            .spans
            .iter()
            .all(|v| matches!(v.resolution, SpanResolution::Resolved(_)));
        if !self.spans.is_empty() && all && !self.diagnostics.has_errors() {
            VerificationStatus::Verified
        } else {
            VerificationStatus::NotVerified
        }
    }
}

/// Every span reference of the trace in the order of `DecisionTrace::spans`, with its rule.
fn references(trace: &DecisionTrace) -> Vec<(&RuleId, Option<RevisionNumber>, &TraceSpan)> {
    let mut out = Vec::new();
    for r in trace.rules() {
        out.extend(r.spans.iter().map(|s| (&r.rule, Some(r.revision), s)));
    }
    for r in trace.not_applied() {
        out.extend(r.spans.iter().map(|s| (&r.rule, Some(r.revision), s)));
    }
    for entry in trace.entries() {
        if let Entry::Read(r) = entry {
            out.extend(r.default_spans.iter().map(|s| (&r.rule, None, s)));
        }
    }
    out
}

/// True when the model passes a fresh structural check (hash, length and element spans).
fn model_is_valid(model: &SourceModel) -> bool {
    SourceModel::new(
        model.document().clone(),
        model.kind(),
        model.content().to_owned(),
        model.elements().to_vec(),
    )
    .is_ok_and(|fresh| fresh.sha256() == model.sha256())
}

/// Verifies every span of `trace` against `models`. All spans are checked (no stop at the first
/// failure), and each failed span yields exactly one Diagnostic, the first matching row of
/// Req 13.16 in the order `RT-PRV-0006`, `0001`, `0002`, `0003`, `0004`, `0005`.
#[must_use]
pub fn verify_trace(trace: &DecisionTrace, models: &SourceModels) -> Verification {
    let refs = references(trace);
    let mut diagnostics = DiagnosticState::empty();
    let mut spans = Vec::with_capacity(refs.len());
    let mut validity: BTreeMap<&DocumentId, bool> = BTreeMap::new();

    if models.is_empty() && !refs.is_empty() {
        diagnostics.push(Diagnostic::new(
            Code::ProvNotVerifiedNoSources,
            "no Source_Model was supplied, so the provenance of this trace is not verified"
                .to_owned(),
            Location::None(Code::ProvNotVerifiedNoSources),
        ));
    }
    for (rule, revision, span) in refs {
        let resolution = if models.is_empty() {
            SpanResolution::Unresolved(Code::ProvNotVerifiedNoSources)
        } else {
            match resolve(models, span, &mut validity) {
                Ok(text) => SpanResolution::Resolved(text),
                Err(d) => {
                    let code = d.code();
                    diagnostics.push((*d).with_rules(vec![RuleRef {
                        rule: rule.clone(),
                        revision,
                    }]));
                    SpanResolution::Unresolved(code)
                }
            }
        };
        spans.push(SpanVerdict {
            rule: rule.clone(),
            revision,
            span: span.clone(),
            resolution,
        });
    }
    if spans.is_empty() {
        diagnostics.push(Diagnostic::new(
            Code::ProvTraceNoSourceSpans,
            "the trace has no source span, so it is not verified".to_owned(),
            Location::None(Code::ProvTraceNoSourceSpans),
        ));
    }
    Verification { spans, diagnostics }
}

/// The recorded span as a Location when the model can verify it, else the document.
fn recorded_location(model: Option<&SourceModel>, span: &TraceSpan) -> Location {
    model
        .and_then(|m| {
            SourceSpan::new(m, span.start, span.end, span.line, span.column)
                .ok()
                .map(Location::Source)
        })
        .unwrap_or_else(|| Location::Document(span.document.clone()))
}

fn describe(span: &TraceSpan) -> String {
    format!(
        "{} bytes {}..{} (line {}, column {})",
        escape_controls(span.document.as_str()),
        span.start,
        span.end,
        span.line,
        span.column
    )
}

fn resolve<'a>(
    models: &'a SourceModels,
    span: &'a TraceSpan,
    validity: &mut BTreeMap<&'a DocumentId, bool>,
) -> Result<String, Box<Diagnostic>> {
    let Some(model) = models.get(&span.document) else {
        return Err(Box::new(Diagnostic::new(
            Code::ProvDocumentMissing,
            format!("no Source_Model for the span {}", describe(span)),
            recorded_location(None, span),
        )));
    };
    if model.sha256().as_str() != span.sha256.as_str() {
        return Err(Box::new(Diagnostic::new(
            Code::ProvHashMismatch,
            format!(
                "recorded hash {} differs from the Source_Model hash {} for the span {}",
                span.sha256.as_str(),
                model.sha256().as_str(),
                describe(span)
            ),
            recorded_location(Some(model), span),
        )));
    }
    let text = match model.text(span.start, span.end) {
        Ok(t) => t,
        Err(failure) => {
            let what = match failure {
                SpanFailure::NotOnBoundary => "an offset is not on a UTF-8 scalar boundary of",
                SpanFailure::OutOfRange | SpanFailure::HashMismatch => {
                    "the span is empty, reversed or beyond the end of"
                }
            };
            let code = match failure {
                SpanFailure::NotOnBoundary => Code::ProvSpanNotOnBoundary,
                SpanFailure::OutOfRange | SpanFailure::HashMismatch => Code::ProvSpanOutOfRange,
            };
            return Err(Box::new(Diagnostic::new(
                code,
                format!("{what} the document for the span {}", describe(span)),
                recorded_location(Some(model), span),
            )));
        }
    };
    let valid = *validity
        .entry(&span.document)
        .or_insert_with(|| model_is_valid(model));
    if !valid {
        return Err(Box::new(Diagnostic::new(
            Code::ProvSourceModelInvalid,
            format!(
                "the Source_Model of {} is invalid",
                escape_controls(span.document.as_str())
            ),
            Location::Document(span.document.clone()),
        )));
    }
    Ok(text.to_owned())
}
