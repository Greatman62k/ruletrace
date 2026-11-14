//! Tests for the Explainer (Req 13.7 to 13.9, 13.13 to 13.17, 13.19, 13.22,
//! 13.23).

use super::*;
use proptest::prelude::*;
use rt_foundation::{
    Code, DiagnosticState, DocumentId, Location, RevisionNumber, RuleId, Severity,
};
use rt_numeric::parse_literal;
use rt_srcmodel::{DocumentKind, Element, ElementKind, SourceModel, SourceModels};
use rt_trace::{
    DecisionTrace, DocumentHash, Outcome, QuestionKind, TraceParts, TraceQuestion, TraceRule,
    TraceSpan, TraceValue,
};

/// Content with an ESC byte inside the first span and a two-byte character at
/// bytes 11..13.
const CONTENT: &str = "# Title\n1. \u{e9}t\u{e9}\r\nend\u{1b}[0m";

fn doc(name: &str) -> DocumentId {
    DocumentId::new(name).unwrap()
}

fn model(name: &str) -> SourceModel {
    let len = u32::try_from(CONTENT.len()).unwrap();
    SourceModel::new(
        doc(name),
        DocumentKind::Markdown,
        CONTENT.to_owned(),
        vec![Element {
            id: 0,
            kind: ElementKind::Heading,
            level: Some(1),
            label: None,
            start: 0,
            end: 7,
            line: 1,
            column: 1,
            parent: None,
        }]
        .into_iter()
        .chain([Element {
            id: 1,
            kind: ElementKind::Paragraph,
            level: None,
            label: None,
            start: 18,
            end: len,
            line: 3,
            column: 1,
            parent: None,
        }])
        .collect(),
    )
    .unwrap()
}

fn models(names: &[&str]) -> SourceModels {
    let mut m = SourceModels::new();
    for n in names {
        m.insert(model(n)).unwrap();
    }
    m
}

fn span_of(name: &str, start: u32, end: u32) -> TraceSpan {
    let hash = model(name).sha256().as_str().to_owned();
    TraceSpan {
        document: doc(name),
        sha256: DocumentHash::new(&hash).unwrap(),
        start,
        end,
        line: 1,
        column: 1,
    }
}

fn trace_with(spans: Vec<TraceSpan>) -> DecisionTrace {
    DecisionTrace::new(TraceParts {
        question: TraceQuestion {
            kind: QuestionKind::Value,
            target: "elig.age".to_owned(),
            participant: Some("p1".to_owned()),
        },
        outcome: Outcome::Decided(TraceValue::Number(parse_literal("1").unwrap())),
        rules: vec![TraceRule {
            rule: RuleId::new("elig.age").unwrap(),
            revision: RevisionNumber::new(1).unwrap(),
            spans,
        }],
        entries: Vec::new(),
        not_applied: Vec::new(),
        missing: Vec::new(),
        diagnostics: DiagnosticState::empty(),
    })
    .unwrap()
}

fn codes(v: &Verification) -> Vec<Code> {
    v.diagnostics()
        .as_slice()
        .iter()
        .map(|d| d.code())
        .collect()
}

#[test]
fn all_spans_resolved_is_verified_with_exact_text() {
    let t = trace_with(vec![span_of("rules.md", 0, 7), span_of("rules.md", 8, 16)]);
    let v = verify_trace(&t, &models(&["rules.md"]));
    assert_eq!(v.status(), VerificationStatus::Verified);
    assert!(v.diagnostics().as_slice().is_empty());
    assert_eq!(v.spans()[0].text(), Some("# Title"));
    assert_eq!(v.spans()[1].text(), CONTENT.get(8..16));
}

#[test]
fn zero_spans_warns_and_is_not_verified() {
    let t = trace_with(Vec::new());
    let v = verify_trace(&t, &models(&["rules.md"]));
    assert_eq!(v.status(), VerificationStatus::NotVerified);
    assert_eq!(codes(&v), vec![Code::ProvTraceNoSourceSpans]);
    assert_eq!(v.diagnostics().as_slice()[0].severity(), Severity::Warning);
    assert!(matches!(
        v.diagnostics().as_slice()[0].primary(),
        Location::None(_)
    ));
    assert!(!render_trace(&t, &v).contains("text:"));
}

#[test]
fn no_models_gives_one_warning_and_no_text() {
    let t = trace_with(vec![span_of("rules.md", 0, 7), span_of("rules.md", 8, 16)]);
    let v = verify_trace(&t, &SourceModels::new());
    assert_eq!(v.status(), VerificationStatus::NotVerified);
    assert_eq!(codes(&v), vec![Code::ProvNotVerifiedNoSources]);
    let text = render_trace(&t, &v);
    assert!(!text.contains("text:"));
    assert!(text.contains("provenance: not verified"));
    assert!(text.contains(model("rules.md").sha256().as_str()));
}

#[test]
fn each_failure_row_has_its_code_and_no_text() {
    let mut wrong_hash = span_of("rules.md", 0, 7);
    wrong_hash.sha256 = DocumentHash::new(&"ab".repeat(32)).unwrap();
    let cases = [
        (span_of("other.md", 0, 7), Code::ProvDocumentMissing),
        (wrong_hash, Code::ProvHashMismatch),
        (span_of("rules.md", 7, 7), Code::ProvSpanOutOfRange),
        (span_of("rules.md", 0, 9999), Code::ProvSpanOutOfRange),
        (span_of("rules.md", 8, 12), Code::ProvSpanNotOnBoundary),
    ];
    for (span, code) in cases {
        let t = trace_with(vec![span]);
        let v = verify_trace(&t, &models(&["rules.md"]));
        assert_eq!(codes(&v), vec![code], "{code}");
        assert_eq!(v.status(), VerificationStatus::NotVerified);
        assert_eq!(v.diagnostics().as_slice()[0].severity(), Severity::Error);
        assert_eq!(
            v.diagnostics().as_slice()[0].rules()[0].rule.as_str(),
            "elig.age"
        );
        assert!(v.spans()[0].text().is_none());
        assert!(!render_trace(&t, &v).contains("text:"));
    }
}

#[test]
fn first_row_wins_when_several_hold() {
    // Wrong hash and out of range: hash mismatch is the earlier row.
    let mut s = span_of("rules.md", 0, 9999);
    s.sha256 = DocumentHash::new(&"cd".repeat(32)).unwrap();
    let v = verify_trace(&trace_with(vec![s]), &models(&["rules.md"]));
    assert_eq!(codes(&v), vec![Code::ProvHashMismatch]);
}

#[test]
fn mixed_trace_renders_text_only_for_the_resolved_span() {
    let t = trace_with(vec![
        span_of("rules.md", 0, 7),
        span_of("rules.md", 8, 9999),
    ]);
    let v = verify_trace(&t, &models(&["rules.md"]));
    assert_eq!(v.status(), VerificationStatus::NotVerified);
    assert_eq!(codes(&v), vec![Code::ProvSpanOutOfRange]);
    let text = render_trace(&t, &v);
    assert_eq!(text.matches("text:").count(), 1);
    assert!(text.contains("text: # Title"));
}

#[test]
fn every_failed_span_is_checked_and_trace_is_unchanged() {
    let t = trace_with(vec![
        span_of("other.md", 0, 7),
        span_of("rules.md", 7, 7),
        span_of("rules.md", 0, 7),
    ]);
    let before = t.clone();
    let v = verify_trace(&t, &models(&["rules.md"]));
    assert_eq!(
        codes(&v),
        vec![Code::ProvDocumentMissing, Code::ProvSpanOutOfRange]
    );
    assert_eq!(t, before);
    assert_eq!(t.outcome(), before.outcome());
    assert_eq!(t.spans().len(), v.spans().len());
}

#[test]
fn rendered_text_escapes_control_characters() {
    let len = u32::try_from(CONTENT.len()).unwrap();
    let t = trace_with(vec![span_of("rules.md", 18, len)]);
    let v = verify_trace(&t, &models(&["rules.md"]));
    assert_eq!(v.spans()[0].text(), CONTENT.get(18..));
    let text = render_trace(&t, &v);
    assert!(text.contains("text: end\\u{1b}[0m"));
    assert!(!text.contains('\u{1b}'));
}

#[test]
fn rendering_is_deterministic() {
    let t = trace_with(vec![span_of("rules.md", 0, 7)]);
    let m = models(&["rules.md"]);
    let a = render_trace(&t, &verify_trace(&t, &m));
    let b = render_trace(&t, &verify_trace(&t, &m));
    assert_eq!(a, b);
}

proptest! {
    /// Text is produced exactly when the span lies on boundaries inside the
    /// document, and then it equals the model bytes. Any other offset pair
    /// gives no text and is not verified.
    #[test]
    fn text_only_for_resolvable_spans(start in 0u32..40, end in 0u32..40) {
        let t = trace_with(vec![span_of("rules.md", start, end)]);
        let v = verify_trace(&t, &models(&["rules.md"]));
        let expected = if start < end { CONTENT.get(start as usize..end as usize) } else { None };
        prop_assert_eq!(v.spans()[0].text(), expected);
        prop_assert_eq!(v.status() == VerificationStatus::Verified, expected.is_some());
        prop_assert_eq!(v.diagnostics().as_slice().len(), usize::from(expected.is_none()));
    }
}
