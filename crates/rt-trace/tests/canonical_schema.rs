//! Conformance of the canonical Decision_Trace schema (Req 13.2 to 13.6, 13.10,
//! 13.11) to the committed golden files and to the examples of design §3.6.3.
//!
//! The traces are built by hand, so the golden files do not depend on the
//! engine. The design examples are read from the design document at compile
//! time and must equal the golden files after canonical re-printing.

#![allow(clippy::expect_used)]

use rt_foundation::{
    Code, Diagnostic, DiagnosticState, DocumentId, InputId, Location, RevisionNumber, RuleId,
    RuleRef,
};
use rt_json::{parse, write_compact, write_pretty, ParseLimits};
use rt_numeric::parse_literal;
use rt_trace::{
    replay, DecisionEntry, DecisionOp, DecisionTrace, DecisionValue, DocumentHash, Entry,
    MissingInput, NotApplied, NotAppliedReason, Outcome, QuestionKind, ReadEntry, Reason,
    StepEntry, StepOp, StepResult, TraceParts, TraceQuestion, TraceRule, TraceSpan, TraceValue,
};

const DECIDED: &str = include_str!("golden/trace_decided.json");
const INDETERMINATE: &str = include_str!("golden/trace_indeterminate.json");
const DESIGN: &str = include_str!("../../../.kiro/specs/ruletrace-compiler/design.md");

fn num(text: &str) -> TraceValue {
    TraceValue::Number(parse_literal(text).expect("valid test input"))
}

fn rule_id(s: &str) -> RuleId {
    RuleId::new(s).expect("valid test input")
}

fn span(start: u32, end: u32, line: u32) -> TraceSpan {
    TraceSpan {
        document: DocumentId::new("rules.md").expect("valid test input"),
        sha256: DocumentHash::new(&"ab".repeat(32)).expect("valid test input"),
        start,
        end,
        line,
        column: 1,
    }
}

fn revision() -> RevisionNumber {
    RevisionNumber::new(1).expect("valid test input")
}

fn read(name: &str, participant: &str, value: TraceValue, rule: &str) -> Entry {
    Entry::Read(ReadEntry {
        name: name.to_owned(),
        participant: Some(participant.to_owned()),
        value,
        rule: rule_id(rule),
        default_spans: Vec::new(),
    })
}

fn decided_trace() -> DecisionTrace {
    let entries = vec![
        read("judges", "p1", num("8"), "score.total"),
        read("weight", "p1", num("3"), "score.total"),
        Entry::Step(StepEntry {
            op: StepOp::Mul,
            operands: vec![num("8"), num("3")],
            result: StepResult::Value(num("24")),
            rule: rule_id("score.total"),
            field: None,
        }),
        Entry::Decision(DecisionEntry {
            rule: rule_id("score.total"),
            op: DecisionOp::Pass,
            value: DecisionValue::Known(num("24")),
            from: vec![3],
        }),
    ];
    DecisionTrace::new(TraceParts {
        question: TraceQuestion {
            kind: QuestionKind::Value,
            target: "score.total".to_owned(),
            participant: Some("p1".to_owned()),
        },
        outcome: Outcome::Decided(num("24")),
        rules: vec![TraceRule {
            rule: rule_id("score.total"),
            revision: revision(),
            spans: vec![span(120, 180, 4)],
        }],
        entries,
        not_applied: vec![NotApplied {
            rule: rule_id("score.alt"),
            revision: revision(),
            spans: vec![span(200, 260, 6)],
            reason: NotAppliedReason::Precedence {
                id: "prec.score".to_owned(),
            },
        }],
        missing: Vec::new(),
        diagnostics: DiagnosticState::empty(),
    })
    .expect("valid test input")
}

fn indeterminate_trace() -> DecisionTrace {
    let diagnostic = Diagnostic::new(
        Code::ExeMissingValue,
        "rule elig.age: required input 'age' of participant 'p3' is a Missing_Value".to_owned(),
        Location::Document(DocumentId::new("rules.md").expect("valid test input")),
    )
    .with_rules(vec![RuleRef {
        rule: rule_id("elig.age"),
        revision: Some(revision()),
    }]);
    DecisionTrace::new(TraceParts {
        question: TraceQuestion {
            kind: QuestionKind::Value,
            target: "elig.age".to_owned(),
            participant: Some("p3".to_owned()),
        },
        outcome: Outcome::indeterminate(vec![Reason::MissingInput {
            participant: "p3".to_owned(),
            field: "age".to_owned(),
        }]),
        rules: vec![TraceRule {
            rule: rule_id("elig.age"),
            revision: revision(),
            spans: vec![span(120, 180, 4)],
        }],
        entries: vec![Entry::Decision(DecisionEntry {
            rule: rule_id("elig.age"),
            op: DecisionOp::Recorded,
            value: DecisionValue::Unknown,
            from: Vec::new(),
        })],
        not_applied: Vec::new(),
        missing: vec![MissingInput {
            participant: "p3".to_owned(),
            field: "age".to_owned(),
        }],
        diagnostics: {
            let mut state = DiagnosticState::empty();
            state.push(diagnostic);
            state
        },
    })
    .expect("valid test input")
}

fn canonical(text: &str) -> (String, String) {
    let input = InputId::new("example.json").expect("valid test input");
    let value =
        parse(&input, text.as_bytes(), ParseLimits::new(1 << 20, 64)).expect("valid test input");
    (write_pretty(&value), write_compact(&value))
}

/// The fenced block that follows the line containing `marker`.
fn design_example(marker: &str) -> String {
    let mut lines = DESIGN.lines().skip_while(|l| !l.contains(marker));
    assert!(lines.next().is_some(), "marker {marker:?} not in design.md");
    let mut lines = lines.skip_while(|l| !l.starts_with("```"));
    assert!(lines.next().is_some(), "no fenced block after {marker:?}");
    let body: Vec<&str> = lines.take_while(|l| !l.starts_with("```")).collect();
    body.join("\n")
}

#[test]
fn decided_trace_matches_golden() {
    let trace = decided_trace();
    assert_eq!(trace.to_canonical_pretty(), DECIDED);
    assert_eq!(trace.to_canonical_compact(), canonical(DECIDED).1);
}

#[test]
fn indeterminate_trace_matches_golden() {
    let trace = indeterminate_trace();
    assert_eq!(trace.to_canonical_pretty(), INDETERMINATE);
    assert_eq!(trace.to_canonical_compact(), canonical(INDETERMINATE).1);
}

#[test]
fn golden_traces_replay_to_their_recorded_outcome() {
    for trace in [decided_trace(), indeterminate_trace()] {
        assert_eq!(replay(&trace).as_ref(), Ok(trace.outcome()));
    }
}

#[test]
fn design_examples_equal_the_golden_files() {
    for (marker, golden) in [
        ("Canonical example, decided:", DECIDED),
        ("Canonical example, indeterminate:", INDETERMINATE),
    ] {
        assert_eq!(canonical(&design_example(marker)), canonical(golden));
        assert_eq!(canonical(golden).0, golden);
    }
}
