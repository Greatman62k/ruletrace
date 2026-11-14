//! Property 52: The Proof_Report is complete, ordered and replayable.
//!
//! Feature: ruletrace-compiler, Property 52: The Proof_Report is complete,
//! ordered and replayable
//!
//! **Validates: Requirements 33.14, 13.11**
//!
//! Traces are generated from integers; the expected texts are computed by this
//! file, not by the report builder. Removing a calculation step or changing its
//! result must fail the replay.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

use proptest::prelude::*;
use rt_explain::report::{
    build_proof_report, render_proof_human, render_proof_json, ApprovalFacts, ApprovalView,
    ProofInputs,
};
use rt_explain::{verify_trace, Verification};
use rt_foundation::{DiagnosticState, DocumentId, RevisionNumber, RuleId};
use rt_numeric::parse_literal;
use rt_srcmodel::{DocumentKind, Element, ElementKind, SourceModel, SourceModels};
use rt_trace::{
    replay, DecisionEntry, DecisionOp, DecisionTrace, DecisionValue, DocumentHash, Entry, Outcome,
    QuestionKind, ReadEntry, StepEntry, StepOp, StepResult, TraceParts, TraceQuestion, TraceRule,
    TraceSpan, TraceValue,
};

const CONTENT: &str = "Entrants must be 16.\nSecond line.";
const QUOTE: &str = "\"Entrants must be 16.\"";

fn models() -> (SourceModels, DocumentHash) {
    let model = SourceModel::new(
        DocumentId::new("rules.md").unwrap(),
        DocumentKind::Markdown,
        CONTENT.to_owned(),
        vec![Element {
            id: 0,
            kind: ElementKind::Paragraph,
            level: None,
            label: None,
            start: 0,
            end: u32::try_from(CONTENT.len()).unwrap(),
            line: 1,
            column: 1,
            parent: None,
        }],
    )
    .unwrap();
    let hash = DocumentHash::new(model.sha256().as_str()).unwrap();
    let mut m = SourceModels::new();
    m.insert(model).unwrap();
    (m, hash)
}

fn num(n: i64) -> TraceValue {
    TraceValue::Number(parse_literal(&n.to_string()).unwrap())
}

fn rule_id(k: usize) -> RuleId {
    RuleId::new(format!("score.r{k}")).unwrap()
}

/// `a + b`; `step_result` is what the recorded step claims, `with_step` drops
/// the step entirely.
fn trace(
    k: usize,
    rev: u32,
    (a, b): (i64, i64),
    step_result: i64,
    with_step: bool,
    hash: &DocumentHash,
) -> Result<DecisionTrace, rt_trace::TraceError> {
    let rule = rule_id(k);
    let read = |n: &str, v: i64| {
        Entry::Read(ReadEntry {
            name: n.to_owned(),
            participant: Some("p1".to_owned()),
            value: num(v),
            rule: rule.clone(),
            default_spans: Vec::new(),
        })
    };
    let mut entries = vec![read("a", a), read("b", b)];
    if with_step {
        entries.push(Entry::Step(StepEntry {
            op: StepOp::Add,
            operands: vec![num(a), num(b)],
            result: StepResult::Value(num(step_result)),
            rule: rule.clone(),
            field: None,
        }));
    }
    entries.push(Entry::Decision(DecisionEntry {
        rule: rule.clone(),
        op: DecisionOp::Pass,
        value: DecisionValue::Known(num(a + b)),
        from: vec![3],
    }));
    DecisionTrace::new(TraceParts {
        question: TraceQuestion {
            kind: QuestionKind::Value,
            target: "score.total".to_owned(),
            participant: Some("p1".to_owned()),
        },
        outcome: Outcome::Decided(num(a + b)),
        rules: vec![TraceRule {
            rule,
            revision: RevisionNumber::new(rev).unwrap(),
            spans: vec![TraceSpan {
                document: DocumentId::new("rules.md").unwrap(),
                sha256: hash.clone(),
                start: 0,
                end: 20,
                line: 1,
                column: 1,
            }],
        }],
        entries,
        not_applied: Vec::new(),
        missing: Vec::new(),
        diagnostics: DiagnosticState::empty(),
    })
}

#[derive(Debug)]
struct Case {
    rev: u32,
    reviewer: String,
    time: String,
    hash: String,
    operands: (i64, i64),
}

fn case() -> impl Strategy<Value = Case> {
    (
        1u32..9,
        "[A-Za-z]{3,10}",
        (2020u32..2040, 1u32..13, 1u32..28),
        "[0-9a-f]{8}",
        (0i64..1000, 0i64..1000),
    )
        .prop_map(|(rev, reviewer, (y, m, d), hash, operands)| Case {
            rev,
            reviewer,
            time: format!("{y}-{m:02}-{d:02}T09:00:00Z"),
            hash,
            operands,
        })
}

fn render(cases: &[Case]) -> (String, Vec<u8>, Vec<DecisionTrace>) {
    let (m, doc_hash) = models();
    let traces: Vec<DecisionTrace> = cases
        .iter()
        .enumerate()
        .map(|(k, c)| {
            trace(
                k,
                c.rev,
                c.operands,
                c.operands.0 + c.operands.1,
                true,
                &doc_hash,
            )
            .unwrap()
        })
        .collect();
    let vs: Vec<Verification> = traces.iter().map(|t| verify_trace(t, &m)).collect();
    let mut facts = ApprovalFacts::new();
    for (k, c) in cases.iter().enumerate() {
        facts.insert(
            rule_id(k),
            RevisionNumber::new(c.rev).unwrap(),
            ApprovalView {
                reviewer: c.reviewer.clone(),
                time: c.time.clone(),
                content_hash: c.hash.clone(),
            },
        );
    }
    let report = build_proof_report(&ProofInputs {
        traces: &traces,
        verifications: &vs,
        approvals: &facts,
        published: &[],
        rulebook_hash: "sha256:abc",
        synthetic_data: Some(true),
    })
    .unwrap();
    for d in &report.decisions {
        assert_eq!(replay(&d.trace), Ok(d.trace.outcome().clone()));
    }
    (
        render_proof_human(&report),
        render_proof_json(&report),
        traces,
    )
}

const LABELS: [&str; 11] = [
    "rules applied",
    "approval",
    "source",
    "inputs read",
    "calculation steps",
    "tie-breaking",
    "missing inputs",
    "not applied",
    "verification",
    "published",
    "",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn report_is_complete_ordered_and_replayable(cases in prop::collection::vec(case(), 1..4)) {
        let (human, json, _) = render(&cases);
        let sections: Vec<&str> = human.split("DECISION ").skip(1).collect();
        prop_assert_eq!(sections.len(), cases.len());
        for (k, (sec, c)) in sections.iter().zip(&cases).enumerate() {
            let mut at = 0;
            for l in &LABELS[..10] {
                let found = sec[at..].find(l);
                prop_assert!(found.is_some(), "decision {} lacks or misorders '{}'", k + 1, l);
                at += found.unwrap_or(0) + l.len();
            }
            let (a, b) = c.operands;
            let applied = format!("score.r{k} @ revision {}", c.rev);
            prop_assert!(sec.contains(&applied));
            let approval = format!(
                "reviewer \"{}\", time {}, content hash {}",
                c.reviewer, c.time, c.hash
            );
            prop_assert!(sec.contains(&approval));
            prop_assert!(sec.contains(QUOTE));
            let step = format!("add({a}, {b}) -> {}", a + b);
            prop_assert!(sec.contains(&step));
        }
        prop_assert!(!human.contains("provider"));
        // Byte-identical across runs, in both forms.
        let again = render(&cases);
        prop_assert_eq!((human, json), (again.0, again.1));
    }

    #[test]
    fn removing_or_altering_a_step_fails_the_replay(
        a in 0i64..1000, b in 0i64..1000, off in 1i64..50,
    ) {
        let (_, h) = models();
        let ok = trace(0, 1, (a, b), a + b, true, &h).unwrap();
        prop_assert_eq!(replay(&ok), Ok(Outcome::Decided(num(a + b))));
        for t in [
            trace(0, 1, (a, b), a + b + off, true, &h),
            trace(0, 1, (a, b), a + b, false, &h),
        ] {
            prop_assert!(t.is_err() || t.map(|t| replay(&t).is_err()).unwrap_or(true));
        }
    }
}
