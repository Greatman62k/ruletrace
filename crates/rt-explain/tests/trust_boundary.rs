//! The trust boundary of the application report, written as an external
//! caller (task 9.4 follow-up; Req 31.14, 13.9).
//!
//! The review of 03869c5 built a `verified: true` entry, a `Verified` header
//! and invented source text from public fields. Those constructions no longer
//! compile; the `compile_fail` doctests on `Evidence`, the entries,
//! `ProvenanceSummary`, `ReportHeader`, `TraceRef`, `SpanVerdict`,
//! `PublishedDifference` and `ReportDoc` pin that. These tests pin the runtime
//! side: the only entry that says `verified` is one built from a successful
//! Verification, and header and entries are derived from the same ones.
//! Expected texts are computed here from the source constant, not by the
//! writers.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

use rt_explain::report::{
    render_human, render_json, EligibilityEntry, ProvenanceSummary, ReportApplication, ReportDoc,
    ReportError, ReportHeader, ReportSection, Sha256Hex, StatementId, TraceRef,
};
use rt_explain::{render_trace, verify_trace, SpanResolution, Verification};
use rt_foundation::{Code, DiagnosticState, DocumentId, RevisionNumber, RuleId};
use rt_srcmodel::{DocumentKind, Element, ElementKind, SourceModel, SourceModels};
use rt_trace::{
    DecisionTrace, DocumentHash, Outcome, QuestionKind, TraceParts, TraceQuestion, TraceRule,
    TraceSpan, TraceValue,
};

const RULES: &str = "Entrants must be 16 years old.";
const OTHER: &str = "A completely different document.";

fn model(content: &str) -> SourceModel {
    let len = u32::try_from(content.len()).unwrap();
    SourceModel::new(
        DocumentId::new("rules.md").unwrap(),
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

fn models(content: &str) -> SourceModels {
    let mut m = SourceModels::new();
    m.insert(model(content)).unwrap();
    m
}

/// A trace of rule `r1` whose only span covers `start..end` of `RULES`.
fn trace(who: &str, start: u32, end: u32) -> DecisionTrace {
    let hash = DocumentHash::new(model(RULES).sha256().as_str()).unwrap();
    DecisionTrace::new(TraceParts {
        question: TraceQuestion {
            kind: QuestionKind::Value,
            target: "elig.age".to_owned(),
            participant: Some(who.to_owned()),
        },
        outcome: Outcome::Decided(TraceValue::Bool(true)),
        rules: vec![TraceRule {
            rule: RuleId::new("r1").unwrap(),
            revision: RevisionNumber::new(1).unwrap(),
            spans: vec![TraceSpan {
                document: DocumentId::new("rules.md").unwrap(),
                sha256: hash,
                start,
                end,
                line: 1,
                column: 1,
            }],
        }],
        entries: Vec::new(),
        not_applied: Vec::new(),
        missing: Vec::new(),
        diagnostics: DiagnosticState::empty(),
    })
    .unwrap()
}

fn header() -> ReportHeader {
    let h = Sha256Hex::new(&"a".repeat(64)).unwrap();
    ReportHeader {
        application: ReportApplication::PrizeCheck,
        statements: vec![StatementId::new("ST-BOUNDARY")],
        rulebook_hash: h.clone(),
        input_hashes: vec![("data", h)],
        synthetic_data: Some(true),
    }
}

fn eligibility(traces: &[DecisionTrace], vs: &[Verification]) -> Vec<EligibilityEntry> {
    traces
        .iter()
        .zip(vs)
        .enumerate()
        .map(|(i, (t, v))| EligibilityEntry::from_trace(&TraceRef::new(i, t, v).unwrap()).unwrap())
        .collect()
}

fn document(traces: &[DecisionTrace], vs: &[Verification]) -> ReportDoc {
    ReportDoc::new(
        header(),
        vs,
        vec![ReportSection::Eligibility(eligibility(traces, vs))],
    )
    .unwrap()
}

#[test]
fn a_verified_entry_comes_only_from_a_successful_verification() {
    let t = [trace("p1", 0, 20)];
    let vs = [verify_trace(&t[0], &models(RULES))];
    let doc = document(&t, &vs);
    let ReportSection::Eligibility(entries) = &doc.sections()[0] else {
        panic!("eligibility section expected");
    };
    assert!(entries[0].evidence().verified());
    assert!(matches!(
        doc.provenance(),
        ProvenanceSummary::Verified { traces: 1, .. }
    ));
    let human = render_human(&doc);
    assert!(human.contains("provenance: verified (1 of 1 traces)\n"));
    assert!(human.contains("      trace #0  verified  rules r1@1\n"));
    let json = String::from_utf8(render_json(&doc)).unwrap();
    assert!(json.contains("\"verified\":true"));
}

#[test]
fn without_source_models_no_entry_and_no_header_says_verified() {
    let t = [trace("p1", 0, 20), trace("p2", 0, 20)];
    let vs: Vec<Verification> = t
        .iter()
        .map(|x| verify_trace(x, &SourceModels::new()))
        .collect();
    let doc = document(&t, &vs);
    let ReportSection::Eligibility(entries) = &doc.sections()[0] else {
        panic!("eligibility section expected");
    };
    for e in entries {
        assert!(!e.evidence().verified());
        for s in e.evidence().sources() {
            assert_eq!(s.text(), None);
            assert_eq!(
                s.resolution(),
                &SpanResolution::Unresolved(Code::ProvNotVerifiedNoSources)
            );
        }
    }
    assert!(matches!(
        doc.provenance(),
        ProvenanceSummary::NotVerifiedNoSources {
            without_sources: 2,
            total: 2,
            ..
        }
    ));
    let human = render_human(&doc);
    assert!(human.contains("provenance: not verified: no source models were supplied"));
    assert!(!human.contains("provenance: verified"));
    assert!(!human.contains("  verified  "));
    assert!(!human.contains("Entrants"));
    let json = String::from_utf8(render_json(&doc)).unwrap();
    assert!(!json.contains("\"verified\":true"));
    assert!(!json.contains("\"status\":\"verified\""));
}

#[test]
fn a_hash_mismatch_shows_neither_verified_nor_source_text() {
    let t = [trace("p1", 0, 20)];
    let vs = [verify_trace(&t[0], &models(OTHER))];
    let doc = document(&t, &vs);
    assert!(matches!(
        doc.provenance(),
        ProvenanceSummary::NotVerified {
            verified: 0,
            total: 1,
            ..
        }
    ));
    let ReportSection::Eligibility(entries) = &doc.sections()[0] else {
        panic!("eligibility section expected");
    };
    assert!(!entries[0].evidence().verified());
    assert_eq!(entries[0].evidence().sources()[0].text(), None);
    let human = render_human(&doc);
    assert!(!human.contains("provenance: verified"));
    assert!(!human.contains("Entrants"));
    assert!(!human.contains("completely different"));
    assert!(human.contains("unresolved: RT-PRV-0002"));
}

#[test]
fn report_source_text_is_exactly_what_the_render_path_returned() {
    let t = [trace("p1", 0, 20)];
    let m = models(RULES);
    let vs = [verify_trace(&t[0], &m)];
    let doc = document(&t, &vs);
    let ReportSection::Eligibility(entries) = &doc.sections()[0] else {
        panic!("eligibility section expected");
    };
    let in_report = entries[0].evidence().sources()[0].text().unwrap();
    // Independent expectation: the bytes of the constant, not the writer's.
    assert_eq!(in_report, &RULES[0..20]);
    assert_eq!(Some(in_report), vs[0].spans()[0].text());
    assert!(render_trace(&t[0], &vs[0]).contains(in_report));
    assert!(render_human(&doc).contains(&format!("\"{}\"", &RULES[0..20])));
    let json = String::from_utf8(render_json(&doc)).unwrap();
    assert!(json.contains(&format!("\"text\":\"{}\"", &RULES[0..20])));
}

#[test]
fn a_verification_of_another_trace_is_refused() {
    let a = trace("p1", 0, 20);
    let b = trace("p1", 0, 10);
    let va = verify_trace(&a, &models(RULES));
    assert!(va.status() == rt_explain::VerificationStatus::Verified);
    assert_eq!(
        TraceRef::new(3, &b, &va).unwrap_err(),
        ReportError::VerificationNotForTrace { trace: 3 }
    );
}

#[test]
fn a_verified_entry_beside_a_not_verified_header_is_refused() {
    let t = [trace("p1", 0, 20)];
    let verified = [verify_trace(&t[0], &models(RULES))];
    let unverified = [verify_trace(&t[0], &SourceModels::new())];
    let entries = eligibility(&t, &verified);
    assert_eq!(
        ReportDoc::new(
            header(),
            &unverified,
            vec![ReportSection::Eligibility(entries)]
        ),
        Err(ReportError::EvidenceNotFromVerification { trace: 0 })
    );
}

#[test]
fn a_not_verified_entry_beside_a_verified_header_is_refused() {
    let t = [trace("p1", 0, 20)];
    let verified = [verify_trace(&t[0], &models(RULES))];
    let unverified = [verify_trace(&t[0], &SourceModels::new())];
    let entries = eligibility(&t, &unverified);
    assert_eq!(
        ReportDoc::new(
            header(),
            &verified,
            vec![ReportSection::Eligibility(entries)]
        ),
        Err(ReportError::EvidenceNotFromVerification { trace: 0 })
    );
}

#[test]
fn an_entry_naming_a_trace_outside_the_verifications_is_refused() {
    let t = [trace("p1", 0, 20), trace("p2", 0, 20)];
    let vs: Vec<Verification> = t.iter().map(|x| verify_trace(x, &models(RULES))).collect();
    let entries = eligibility(&t, &vs);
    assert_eq!(
        ReportDoc::new(
            header(),
            &vs[..1],
            vec![ReportSection::Eligibility(entries)]
        ),
        Err(ReportError::EvidenceNotFromVerification { trace: 1 })
    );
}

#[test]
fn header_and_entries_agree_in_a_mixed_run() {
    let t = [trace("p1", 0, 20), trace("p2", 0, 20)];
    let vs = [
        verify_trace(&t[0], &models(RULES)),
        verify_trace(&t[1], &models(OTHER)),
    ];
    let doc = document(&t, &vs);
    assert!(matches!(
        doc.provenance(),
        ProvenanceSummary::NotVerified {
            verified: 1,
            total: 2,
            ..
        }
    ));
    let ReportSection::Eligibility(entries) = &doc.sections()[0] else {
        panic!("eligibility section expected");
    };
    let flags: Vec<bool> = entries.iter().map(|e| e.evidence().verified()).collect();
    assert_eq!(flags, [true, false]);
    let human = render_human(&doc);
    assert!(human.contains("provenance: not verified (1 of 2 traces verified; RT-PRV-0002)\n"));
    assert!(!human.contains("provenance: verified"));
}
