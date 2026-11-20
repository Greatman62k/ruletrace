//! Property 26: Provenance verification.
//!
//! **Validates: Requirements 13.7, 13.8, 13.9, 13.13, 13.14, 13.15, 13.16,
//! 13.17, 13.19, 13.22, 19.5, 19.10**
//!
//! The expectation is computed by an independent oracle that reads only the
//! generated content strings and the recorded span, never the Explainer or the
//! Source_Model span logic. It applies the first matching row of the Req 13.16
//! table in order: document missing, hash mismatch, span out of range, span not
//! on a UTF-8 boundary.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;
use proptest::prelude::*;
use proptest_floor::property_tests;
use rt_explain::{render_trace, verify_trace, SpanResolution, VerificationStatus};
use rt_foundation::{
    Code, DiagnosticState, DocumentId, Location, RevisionNumber, RuleId, Severity,
};
use rt_numeric::parse_literal;
use rt_srcmodel::{DocumentKind, Element, ElementKind, SourceModel, SourceModels};
use rt_trace::{
    DecisionTrace, DocumentHash, Outcome, QuestionKind, TraceParts, TraceQuestion, TraceRule,
    TraceSpan, TraceValue,
};

const NAMES: [&str; 3] = ["a.md", "b.md", "c.md"];
/// Characters of the generated documents: ASCII, a two-byte letter, ESC, CR,
/// LF, TAB.
const ALPHABET: [char; 8] = ['a', '0', ' ', '\u{e9}', '\u{1b}', '\r', '\n', '\t'];
const MAX_OFFSET: u32 = 20;

fn doc(name: &str) -> DocumentId {
    DocumentId::new(name).unwrap()
}

fn model(name: &str, content: &str) -> SourceModel {
    let len = u32::try_from(content.len()).unwrap();
    SourceModel::new(
        doc(name),
        DocumentKind::Markdown,
        content.to_owned(),
        vec![Element {
            id: 0,
            kind: ElementKind::Paragraph,
            level: None,
            label: None,
            start: 0,
            end: len,
            line: 1,
            column: 1,
            parent: None,
        }],
    )
    .unwrap()
}

#[derive(Clone, Debug)]
struct SpanSpec {
    /// Index into `NAMES`, or `NAMES.len()` for a document no model can exist
    /// for.
    doc: usize,
    hash_ok: bool,
    start: u32,
    end: u32,
}

#[derive(Clone, Debug)]
struct Case {
    contents: Vec<String>,
    supplied: Vec<bool>,
    spans: Vec<SpanSpec>,
}

fn case_strategy() -> impl Strategy<Value = Case> {
    let content = prop::collection::vec(prop::sample::select(ALPHABET.to_vec()), 1..8)
        .prop_map(|v| v.into_iter().collect::<String>());
    let span = (0..=NAMES.len(), any::<bool>(), 0..MAX_OFFSET, 0..MAX_OFFSET).prop_map(
        |(doc, hash_ok, start, end)| SpanSpec {
            doc,
            hash_ok,
            start,
            end,
        },
    );
    (
        prop::collection::vec(content, NAMES.len()),
        prop::collection::vec(any::<bool>(), NAMES.len()),
        prop::collection::vec(span, 0..7),
    )
        .prop_map(|(contents, supplied, spans)| Case {
            contents,
            supplied,
            spans,
        })
}

fn doc_name(i: usize) -> &'static str {
    NAMES.get(i).copied().unwrap_or("unknown.md")
}

/// What the oracle expects for one span.
#[derive(Debug, PartialEq, Eq)]
enum Expect {
    Text(String),
    Fail(&'static str),
}

fn oracle(case: &Case, spec: &SpanSpec, any_model: bool) -> Expect {
    if !any_model {
        return Expect::Fail("RT-PRV-0006");
    }
    let supplied = spec.doc < NAMES.len() && case.supplied[spec.doc];
    if !supplied {
        return Expect::Fail("RT-PRV-0001");
    }
    if !spec.hash_ok {
        return Expect::Fail("RT-PRV-0002");
    }
    let content = &case.contents[spec.doc];
    if spec.start >= spec.end || spec.end as usize > content.len() {
        return Expect::Fail("RT-PRV-0003");
    }
    let (s, e) = (spec.start as usize, spec.end as usize);
    if !content.is_char_boundary(s) || !content.is_char_boundary(e) {
        return Expect::Fail("RT-PRV-0004");
    }
    Expect::Text(content[s..e].to_owned())
}

fn build(case: &Case) -> (DecisionTrace, SourceModels) {
    let mut models = SourceModels::new();
    for (i, name) in NAMES.iter().enumerate() {
        if case.supplied[i] {
            models.insert(model(name, &case.contents[i])).unwrap();
        }
    }
    let spans: Vec<TraceSpan> = case
        .spans
        .iter()
        .map(|s| {
            let real = if s.doc < NAMES.len() {
                model(NAMES[s.doc], &case.contents[s.doc])
                    .sha256()
                    .as_str()
                    .to_owned()
            } else {
                "11".repeat(32)
            };
            let hash = if s.hash_ok || s.doc >= NAMES.len() {
                real
            } else if real == "ab".repeat(32) {
                "cd".repeat(32)
            } else {
                "ab".repeat(32)
            };
            TraceSpan {
                document: doc(doc_name(s.doc)),
                sha256: DocumentHash::new(&hash).unwrap(),
                start: s.start,
                end: s.end,
                line: 1,
                column: 1,
            }
        })
        .collect();
    let mut rules = Vec::new();
    let mut chunks = spans.chunks(2);
    for id in ["elig.a", "elig.b", "elig.c", "elig.d"] {
        rules.push(TraceRule {
            rule: RuleId::new(id).unwrap(),
            revision: RevisionNumber::new(1).unwrap(),
            spans: chunks.next().map(<[TraceSpan]>::to_vec).unwrap_or_default(),
        });
        if spans.is_empty() {
            break;
        }
    }
    let trace = DecisionTrace::new(TraceParts {
        question: TraceQuestion {
            kind: QuestionKind::Value,
            target: "elig.a".to_owned(),
            participant: Some("p1".to_owned()),
        },
        outcome: Outcome::Decided(TraceValue::Number(parse_literal("1").unwrap())),
        rules,
        entries: Vec::new(),
        not_applied: Vec::new(),
        missing: Vec::new(),
        diagnostics: DiagnosticState::empty(),
    })
    .unwrap();
    (trace, models)
}

property_tests! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn provenance_verification(case in case_strategy()) {
        let (trace, models) = build(&case);
        let before = trace.clone();
        let v = verify_trace(&trace, &models);
        let any_model = !models.is_empty();

        // Req 13.19: verification leaves the trace, its outcome and spans
        // unchanged.
        prop_assert_eq!(&trace, &before);
        prop_assert_eq!(v.spans().len(), case.spans.len());

        // Spans are reported in the order of `DecisionTrace::spans`.
        let recorded = trace.spans();
        prop_assert_eq!(recorded.len(), v.spans().len());

        let mut expected_diags: Vec<String> = Vec::new();
        let mut resolved = 0usize;
        for (i, spec) in case.spans.iter().enumerate() {
            let verdict = &v.spans()[i];
            prop_assert_eq!(&verdict.span, recorded[i]);
            match oracle(&case, spec, any_model) {
                Expect::Text(text) => {
                    // Req 13.9, 13.15: text equals the model bytes, nothing
                    // else.
                    prop_assert_eq!(verdict.text(), Some(text.as_str()));
                    resolved += 1;
                }
                Expect::Fail(code) => {
                    // Req 13.16: exactly the first matching row, and no text.
                    prop_assert!(verdict.text().is_none());
                    match &verdict.resolution {
                        SpanResolution::Unresolved(c) => prop_assert_eq!(c.to_string(), code),
                        SpanResolution::Resolved(_) => prop_assert!(false, "resolved"),
                    }
                    if code != "RT-PRV-0006" {
                        expected_diags.push(format!("{code} {}", verdict.rule.as_str()));
                    }
                }
            }
        }

        // Req 13.7, 13.16: one Diagnostic per failing span, none for resolved
        // ones.
        let mut actual: Vec<String> = Vec::new();
        for d in v.diagnostics().as_slice() {
            if d.code() == Code::ProvNotVerifiedNoSources
                || d.code() == Code::ProvTraceNoSourceSpans
            {
                continue;
            }
            prop_assert_eq!(d.severity(), Severity::Error);
            prop_assert_eq!(d.rules().len(), 1);
            actual.push(format!("{} {}", d.code(), d.rules()[0].rule.as_str()));
        }
        expected_diags.sort();
        actual.sort();
        prop_assert_eq!(actual, expected_diags);

        // Req 13.13, 13.22: the two warning rows.
        let warnings: Vec<&_> = v
            .diagnostics()
            .as_slice()
            .iter()
            .filter(|d| d.severity() == Severity::Warning)
            .collect();
        if case.spans.is_empty() {
            prop_assert_eq!(warnings.len(), 1);
            prop_assert_eq!(warnings[0].code(), Code::ProvTraceNoSourceSpans);
            prop_assert!(matches!(warnings[0].primary(), Location::None(_)));
            prop_assert_eq!(v.diagnostics().as_slice().len(), 1);
        } else if !any_model {
            prop_assert_eq!(warnings.len(), 1);
            prop_assert_eq!(warnings[0].code(), Code::ProvNotVerifiedNoSources);
            prop_assert!(matches!(warnings[0].primary(), Location::None(_)));
            prop_assert_eq!(v.diagnostics().as_slice().len(), 1);
        } else {
            prop_assert!(warnings.is_empty());
        }

        // Req 13.17: verified iff at least one span and every span resolved.
        let verified = !case.spans.is_empty() && resolved == case.spans.len();
        prop_assert_eq!(v.status() == VerificationStatus::Verified, verified);

        // Req 13.9, 19.5, 19.10: text appears once per resolved span, with
        // controls escaped.
        let out = render_trace(&trace, &v);
        prop_assert_eq!(out.matches("  text: ").count(), resolved);
        prop_assert!(!out.chars().any(|c| c.is_control() && c != '\n'));
        for verdict in v.spans() {
            if let Some(text) = verdict.text() {
                prop_assert!(out.contains(&rt_foundation::escape_controls(text)));
            } else {
                prop_assert!(out.contains(verdict.span.sha256.as_str()));
            }
        }
        prop_assert_eq!(out.contains("provenance: verified"), verified);
        prop_assert_eq!(out, render_trace(&trace, &verify_trace(&trace, &models)));
    }
}
