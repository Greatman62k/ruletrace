//! Locations, spans and diagnostics shared by the evaluator.

use rt_compiled::{CompiledRule, CompiledSpan};
use rt_foundation::{
    Code, Diagnostic, DocumentId, InputId, InputLocation, Location, RuleRef, SourceSpan, SpanSource,
};
use rt_trace::{DocumentHash, TraceSpan};

/// A span as the Compiled_Rulebook retains it. The Validator checked it against
/// the Source_Model before compilation, and the engine has no source text, so
/// this view claims what the artifact records. The Explainer verifies the span
/// again against the Source_Model.
struct Claimed<'a>(&'a CompiledSpan);

impl SpanSource for Claimed<'_> {
    fn document_id(&self) -> &DocumentId {
        &self.0.document
    }
    fn len_bytes(&self) -> u64 {
        u64::from(self.0.end)
    }
    fn is_char_boundary(&self, _offset: u32) -> bool {
        true
    }
    fn line_column(&self, offset: u32) -> Option<(u32, u32)> {
        (offset == self.0.start).then_some((self.0.line, self.0.column))
    }
}

/// The location of a retained span. If the artifact's span is inconsistent the
/// diagnostic points at the whole document, so no position is ever invented.
pub(crate) fn claimed_location(span: &CompiledSpan) -> Location {
    match SourceSpan::new(&Claimed(span), span.start, span.end, span.line, span.column) {
        Ok(s) => Location::Source(s),
        Err(_) => Location::Document(span.document.clone()),
    }
}

/// The primary location of a diagnostic about a rule: its first Source_Span.
pub(crate) fn rule_location(rule: &CompiledRule) -> Location {
    rule.spans.first().map_or_else(
        || Location::None(Code::ValUnsupportedConstruct),
        claimed_location,
    )
}

pub(crate) fn rule_ref(rule: &CompiledRule) -> RuleRef {
    RuleRef {
        rule: rule.id.clone(),
        revision: Some(rule.revision),
    }
}

/// The spans of a rule as trace spans (position and document hash, no text).
pub(crate) fn trace_spans(spans: &[CompiledSpan]) -> Vec<TraceSpan> {
    spans
        .iter()
        .filter_map(|s| {
            Some(TraceSpan {
                document: s.document.clone(),
                sha256: DocumentHash::new(s.sha256.as_str())?,
                start: s.start,
                end: s.end,
                line: s.line,
                column: s.column,
            })
        })
        .collect()
}

/// A position in a data input that the engine received as typed values. The
/// file name belongs to the caller, so the location names the kind of input and
/// the path inside it.
pub(crate) fn input_location(path: &str) -> Location {
    named_input("competition-data", path)
}

pub(crate) fn named_input(input: &str, path: &str) -> Location {
    match InputId::new(input) {
        Ok(id) => Location::Input(InputLocation {
            input: id,
            line: None,
            column: None,
            field: Some(path.to_owned()),
        }),
        Err(_) => Location::None(Code::DataMalformed),
    }
}

/// `RT-DGN-0004` for rules that give incompatible results for one Target and
/// subject. `rules` is in ascending identifier order. The primary location is
/// the first span of the first rule; every other span of those rules is related
/// (Req 12.11).
pub(crate) fn conflict(rules: &[&CompiledRule], what: &str) -> Diagnostic {
    let primary = rules.first().map_or_else(
        || Location::None(Code::ExecutionConflictDetected),
        |r| rule_location(r),
    );
    let related: Vec<Location> = rules
        .iter()
        .enumerate()
        .flat_map(|(i, r)| {
            r.spans
                .iter()
                .skip(usize::from(i == 0))
                .map(claimed_location)
        })
        .collect();
    let ids: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();
    Diagnostic::new(
        Code::ExecutionConflictDetected,
        format!("{what}; no result is selected (rules: {})", ids.join(", ")),
        primary,
    )
    .with_related(related)
    .with_rules(rules.iter().map(|r| rule_ref(r)).collect())
}
