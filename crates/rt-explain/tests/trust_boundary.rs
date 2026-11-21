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

/// A rule id, a revision and spans, each a `start..end` range of `RULES`.
type RuleSpec<'a> = (&'a str, u32, &'a [(u32, u32)]);

fn multi(rules: &[RuleSpec<'_>]) -> DecisionTrace {
    let hash = DocumentHash::new(model(RULES).sha256().as_str()).unwrap();
    let rules = rules
        .iter()
        .map(|(id, rev, spans)| TraceRule {
            rule: RuleId::new(*id).unwrap(),
            revision: RevisionNumber::new(*rev).unwrap(),
            spans: spans
                .iter()
                .map(|&(start, end)| TraceSpan {
                    document: DocumentId::new("rules.md").unwrap(),
                    sha256: hash.clone(),
                    start,
                    end,
                    line: 1,
                    column: 1,
                })
                .collect(),
        })
        .collect();
    DecisionTrace::new(TraceParts {
        question: TraceQuestion {
            kind: QuestionKind::Value,
            target: "elig.age".to_owned(),
            participant: Some("p1".to_owned()),
        },
        outcome: Outcome::Decided(TraceValue::Bool(true)),
        rules,
        entries: Vec::new(),
        not_applied: Vec::new(),
        missing: Vec::new(),
        diagnostics: DiagnosticState::empty(),
    })
    .unwrap()
}

fn verified(t: &DecisionTrace) -> Verification {
    let v = verify_trace(t, &models(RULES));
    assert!(v.status() == rt_explain::VerificationStatus::Verified);
    v
}

#[test]
fn a_verification_with_fewer_spans_is_refused_for_a_longer_trace() {
    let one = multi(&[("r1", 1, &[(0, 20)])]);
    let two = multi(&[("r1", 1, &[(0, 20), (0, 10)])]);
    assert_eq!(
        TraceRef::new(4, &two, &verified(&one)).unwrap_err(),
        ReportError::VerificationNotForTrace { trace: 4 }
    );
}

#[test]
fn a_verification_with_more_spans_is_refused_for_a_shorter_trace() {
    let one = multi(&[("r1", 1, &[(0, 20)])]);
    let two = multi(&[("r1", 1, &[(0, 20), (0, 10)])]);
    assert_eq!(
        TraceRef::new(5, &one, &verified(&two)).unwrap_err(),
        ReportError::VerificationNotForTrace { trace: 5 }
    );
}

#[test]
fn a_verification_of_another_rule_is_refused() {
    let r1 = multi(&[("r1", 1, &[(0, 20)])]);
    let r2 = multi(&[("r2", 1, &[(0, 20)])]);
    assert_eq!(
        TraceRef::new(1, &r2, &verified(&r1)).unwrap_err(),
        ReportError::VerificationNotForTrace { trace: 1 }
    );
}

#[test]
fn a_verification_of_another_revision_is_refused() {
    let v1 = multi(&[("r1", 1, &[(0, 20)])]);
    let v2 = multi(&[("r1", 2, &[(0, 20)])]);
    assert_eq!(
        TraceRef::new(2, &v2, &verified(&v1)).unwrap_err(),
        ReportError::VerificationNotForTrace { trace: 2 }
    );
}

#[test]
fn a_verification_with_the_same_spans_in_another_order_is_refused() {
    let ab = multi(&[("r1", 1, &[(0, 20), (0, 10)])]);
    let ba = multi(&[("r1", 1, &[(0, 10), (0, 20)])]);
    assert_eq!(
        TraceRef::new(6, &ba, &verified(&ab)).unwrap_err(),
        ReportError::VerificationNotForTrace { trace: 6 }
    );
    assert!(TraceRef::new(6, &ab, &verified(&ab)).is_ok());
}

#[test]
fn an_entry_whose_sources_differ_from_the_verification_is_refused() {
    let wide = trace("p1", 0, 20);
    let narrow = trace("p1", 0, 10);
    let v_wide = verified(&wide);
    let v_narrow = verified(&narrow);
    let entries = eligibility(&[wide], &[v_wide]);
    assert_eq!(
        ReportDoc::new(
            header(),
            &[v_narrow],
            vec![ReportSection::Eligibility(entries)]
        ),
        Err(ReportError::EvidenceNotFromVerification { trace: 0 })
    );
}

#[test]
fn entries_paired_with_swapped_verifications_are_refused() {
    let t = [trace("p1", 0, 20), trace("p2", 0, 10)];
    let vs = [verified(&t[0]), verified(&t[1])];
    let entries = eligibility(&t, &vs);
    let swapped = [vs[1].clone(), vs[0].clone()];
    assert_eq!(
        ReportDoc::new(
            header(),
            &swapped,
            vec![ReportSection::Eligibility(entries)]
        ),
        Err(ReportError::EvidenceNotFromVerification { trace: 0 })
    );
}

#[test]
fn an_entry_at_a_valid_index_with_other_spans_and_the_same_flag_is_refused() {
    let t0 = trace("p1", 0, 20);
    let t1 = trace("p2", 0, 20);
    let other = trace("p2", 0, 10);
    let v0 = verified(&t0);
    let v1 = verified(&t1);
    let v_other = verified(&other);
    let entries = eligibility(&[t0, t1], &[v0.clone(), v1]);
    assert_eq!(
        ReportDoc::new(
            header(),
            &[v0, v_other],
            vec![ReportSection::Eligibility(entries)]
        ),
        Err(ReportError::EvidenceNotFromVerification { trace: 1 })
    );
}
