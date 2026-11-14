//! Property 20: Missing values never become defaults (task 5.20).
//!
//! **Validates: Requirements 11.1, 11.2, 11.3, 11.5, 11.6, 13.5**
//!
//! Feature: ruletrace-compiler, Property 20: Missing values never become defaults.
//!
//! The Compiled_Rulebooks are built with `CompiledRulebook::from_parts` (there is no compiler
//! yet). The expectation is computed by an oracle that reads only the generated values: a field
//! is Missing exactly when the generator left it out, and `0` and `false` are present.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

use proptest::prelude::*;
use rt_compiled::{
    CType, CompiledBody, CompiledDocument, CompiledInput, CompiledLiteral, CompiledNode,
    CompiledParts, CompiledRule, CompiledRulebook, CompiledSpan, InputIdx, NodeIdx, NodeKind,
    RuleIdx,
};
use rt_data::{CompetitionData, DataValue, Participant, ParticipantId, Question, TargetName};
use rt_engine::{evaluate, Outcome, Reason};
use rt_foundation::{ByteOffset, Code, DocumentId, Location, RevisionNumber, RuleId};
use rt_hash::sha256;
use rt_ir::{BinaryOp, IrVersion, Name};
use rt_numeric::{parse_canonical, parse_literal};
use rt_trace::{DecisionTrace, Entry, MissingInput, TraceValue};
use std::collections::BTreeMap;

const A: u32 = 0;
const B: u32 = 1;
const C: u32 = 2;
const FLAG: u32 = 3;
const INPUT_NAMES: [&str; 4] = ["a", "b", "c", "flag"];
const PARTICIPANT: &str = "p1";

/// First span offset of each rule; the second span starts 40 bytes later.
const START_OTHER: u32 = 20;
const START_SUM: u32 = 120;
const START_OK: u32 = 220;
const START_DEFAULT: u32 = 320;

fn name(s: &str) -> Name {
    Name::new(s).unwrap()
}

fn doc_sha() -> rt_hash::Sha256Hex {
    sha256(b"the source document")
}

fn span(start: u32) -> CompiledSpan {
    CompiledSpan {
        document: DocumentId::new("rules.md").unwrap(),
        sha256: doc_sha(),
        start,
        end: start + 10,
        line: 3,
        column: 1,
    }
}

fn node(ty: CType, kind: NodeKind) -> CompiledNode {
    CompiledNode { ty, kind }
}

fn field_node(ty: CType, input: u32) -> CompiledNode {
    node(ty, NodeKind::Field(InputIdx::new(input)))
}

fn rule(
    id: &str,
    target: &str,
    ty: CType,
    start: u32,
    nodes: Vec<CompiledNode>,
    body: CompiledBody,
) -> CompiledRule {
    CompiledRule {
        id: RuleId::new(id).unwrap(),
        revision: RevisionNumber::new(1).unwrap(),
        target: name(target),
        ty,
        spans: vec![span(start), span(start + 40)],
        nodes,
        body,
    }
}

/// `other = c`, `sum = a + b`, `ok = flag`, and optionally the default `flag = true`.
fn rulebook(with_default: bool) -> CompiledRulebook {
    let mut rules = vec![
        rule(
            "calc.other",
            "other",
            CType::Number,
            START_OTHER,
            vec![field_node(CType::Number, C)],
            CompiledBody::Derived {
                expr: NodeIdx::new(0),
            },
        ),
        rule(
            "calc.sum",
            "sum",
            CType::Number,
            START_SUM,
            vec![
                field_node(CType::Number, A),
                field_node(CType::Number, B),
                node(
                    CType::Number,
                    NodeKind::Binary {
                        op: BinaryOp::Add,
                        left: NodeIdx::new(0),
                        right: NodeIdx::new(1),
                    },
                ),
            ],
            CompiledBody::Derived {
                expr: NodeIdx::new(2),
            },
        ),
        rule(
            "elig.ok",
            "ok",
            CType::Boolean,
            START_OK,
            vec![field_node(CType::Boolean, FLAG)],
            CompiledBody::Eligibility {
                expr: NodeIdx::new(0),
            },
        ),
    ];
    if with_default {
        rules.push(rule(
            "dflt.flag",
            "flag",
            CType::Boolean,
            START_DEFAULT,
            Vec::new(),
            CompiledBody::Default {
                value: CompiledLiteral::Boolean(true),
            },
        ));
    }
    rules.sort_by(|x, y| x.id.cmp(&y.id));
    let n = u32::try_from(rules.len()).unwrap();
    let types = [CType::Number, CType::Number, CType::Number, CType::Boolean];
    CompiledRulebook::from_parts(CompiledParts {
        compiler_version: "0.1.0".into(),
        ir_version: IrVersion::V1,
        rulebook_hash: sha256(b"rulebook"),
        documents: vec![CompiledDocument {
            id: DocumentId::new("rules.md").unwrap(),
            sha256: doc_sha(),
        }],
        inputs: INPUT_NAMES
            .iter()
            .zip(types)
            .map(|(n, ty)| CompiledInput { name: name(n), ty })
            .collect(),
        collections: Vec::new(),
        enums: Vec::new(),
        sets: Vec::new(),
        order: (0..n).map(RuleIdx::new).collect(),
        rules,
        precedence: Vec::new(),
    })
    .unwrap()
}

/// One generated participant: `None` is a Missing_Value. Numbers include 0, the flag includes false.
#[derive(Clone, Debug)]
struct Fields {
    a: Option<i32>,
    b: Option<i32>,
    c: Option<i32>,
    flag: Option<bool>,
}

fn fields() -> impl Strategy<Value = Fields> {
    let num = prop::option::of(prop_oneof![Just(0), -50..50i32]);
    (
        num.clone(),
        num.clone(),
        num,
        prop::option::of(any::<bool>()),
    )
        .prop_map(|(a, b, c, flag)| Fields { a, b, c, flag })
}

fn data_of(f: &Fields) -> CompetitionData {
    let mut m = BTreeMap::new();
    for (key, v) in [("a", f.a), ("b", f.b), ("c", f.c)] {
        if let Some(v) = v {
            m.insert(key.to_owned(), DataValue::Text(v.to_string()));
        }
    }
    if let Some(v) = f.flag {
        m.insert("flag".to_owned(), DataValue::Bool(v));
    }
    CompetitionData::new(BTreeMap::from([(
        ParticipantId::new(PARTICIPANT).unwrap(),
        Participant::new(m, BTreeMap::new()),
    )]))
}

fn ask(target: &str) -> Question {
    Question::Value {
        target: TargetName::new(target).unwrap(),
        participant: ParticipantId::new(PARTICIPANT).unwrap(),
    }
}

/// The Target, its rule's first span offset, and the fields it requires.
fn targets() -> [(&'static str, &'static str, u32, &'static [&'static str]); 3] {
    [
        ("sum", "calc.sum", START_SUM, &["a", "b"]),
        ("other", "calc.other", START_OTHER, &["c"]),
        ("ok", "elig.ok", START_OK, &["flag"]),
    ]
}

fn present(f: &Fields, field: &str) -> bool {
    match field {
        "a" => f.a.is_some(),
        "b" => f.b.is_some(),
        "c" => f.c.is_some(),
        _ => f.flag.is_some(),
    }
}

/// The expected Decided value of a Target when nothing it requires is missing.
fn decided(target: &str, f: &Fields, with_default: bool) -> TraceValue {
    match target {
        "sum" => {
            TraceValue::Number(parse_literal(&(f.a.unwrap() + f.b.unwrap()).to_string()).unwrap())
        }
        "other" => TraceValue::Number(parse_literal(&f.c.unwrap().to_string()).unwrap()),
        _ => TraceValue::Bool(f.flag.unwrap_or(with_default)),
    }
}

fn read_of<'t>(trace: &'t DecisionTrace, field: &str) -> Vec<&'t rt_trace::ReadEntry> {
    trace
        .entries()
        .iter()
        .filter_map(|e| match e {
            Entry::Read(r) if r.name == field => Some(r),
            _ => None,
        })
        .collect()
}

fn check(f: &Fields, with_default: bool) -> Result<(), TestCaseError> {
    let rb = rulebook(with_default);
    let data = data_of(f);
    for (target, rule_id, start, required) in targets() {
        let e = evaluate(&rb, &data, &[ask(target)], None);
        prop_assert_eq!(e.outcomes.len(), 1);
        prop_assert_eq!(e.traces.len(), 1);
        let (outcome, trace) = (&e.outcomes[0], &e.traces[0]);
        prop_assert_eq!(trace.outcome(), outcome);
        let replayed = rt_trace::replay(trace);
        prop_assert_eq!(replayed.as_ref(), Ok(outcome));

        // A declared default supplies only its own input, and only when it is absent.
        let defaulted = with_default && target == "ok" && f.flag.is_none();
        let missing: Vec<&str> = required
            .iter()
            .copied()
            .filter(|r| !present(f, r) && !defaulted)
            .collect();

        if missing.is_empty() {
            prop_assert_eq!(outcome, &Outcome::Decided(decided(target, f, with_default)));
            prop_assert!(e.diagnostics.as_slice().is_empty());
            prop_assert!(trace.missing().is_empty());
            if target == "ok" {
                let reads = read_of(trace, "flag");
                prop_assert_eq!(reads.len(), 1);
                let spans: Vec<u32> = reads[0].default_spans.iter().map(|s| s.start).collect();
                if defaulted {
                    // Req 11.2: the default's Source_Span is recorded in the trace.
                    prop_assert_eq!(spans, vec![START_DEFAULT, START_DEFAULT + 40]);
                } else {
                    // Req 11.5: a present value (including false) is never replaced.
                    prop_assert!(spans.is_empty());
                }
            }
        } else {
            let reasons: Vec<Reason> = missing
                .iter()
                .map(|m| Reason::MissingInput {
                    participant: PARTICIPANT.to_owned(),
                    field: (*m).to_owned(),
                })
                .collect();
            prop_assert_eq!(outcome, &Outcome::indeterminate(reasons));
            let mut expected: Vec<MissingInput> = missing
                .iter()
                .map(|m| MissingInput {
                    participant: PARTICIPANT.to_owned(),
                    field: (*m).to_owned(),
                })
                .collect();
            expected.sort();
            prop_assert_eq!(trace.missing(), expected.as_slice());
            // The rule and its first span are named (Req 11.1).
            prop_assert!(trace
                .rules()
                .iter()
                .any(|r| r.rule.as_str() == rule_id
                    && r.spans.first().map(|s| s.start) == Some(start)));
            let found: Vec<_> = e
                .diagnostics
                .as_slice()
                .iter()
                .filter(|d| d.code() == Code::ExeMissingValue)
                .collect();
            prop_assert_eq!(found.len(), missing.len());
            prop_assert_eq!(e.diagnostics.as_slice().len(), missing.len());
            for d in found {
                prop_assert_eq!(d.rules().first().map(|r| r.rule.as_str()), Some(rule_id));
                match d.primary() {
                    Location::Source(s) => prop_assert_eq!(s.start(), ByteOffset::new(start)),
                    other => prop_assert!(false, "primary location is {other:?}"),
                }
            }
        }

        // Unrelated fields never change the Outcome (Req 11.6): keep only the required ones.
        let only = Fields {
            a: f.a.filter(|_| required.contains(&"a")),
            b: f.b.filter(|_| required.contains(&"b")),
            c: f.c.filter(|_| required.contains(&"c")),
            flag: f.flag.filter(|_| required.contains(&"flag")),
        };
        let reduced = evaluate(&rb, &data_of(&only), &[ask(target)], None);
        prop_assert_eq!(&reduced.outcomes[0], outcome);
        prop_assert_eq!(&reduced.traces[0], trace);
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Feature: ruletrace-compiler, Property 20: Missing values never become defaults.
    #[test]
    fn missing_values_never_become_defaults(f in fields()) {
        check(&f, false)?;
        check(&f, true)?;
    }
}

#[test]
fn all_inputs_missing_are_indeterminate_and_zero_false_are_present() {
    let none = Fields {
        a: None,
        b: None,
        c: None,
        flag: None,
    };
    let rb = rulebook(false);
    let e = evaluate(
        &rb,
        &data_of(&none),
        &[ask("sum"), ask("other"), ask("ok")],
        None,
    );
    assert!(e
        .outcomes
        .iter()
        .all(|o| matches!(o, Outcome::Indeterminate(_))));
    let zeros = Fields {
        a: Some(0),
        b: Some(0),
        c: Some(0),
        flag: Some(false),
    };
    let e = evaluate(
        &rb,
        &data_of(&zeros),
        &[ask("sum"), ask("other"), ask("ok")],
        None,
    );
    let zero = TraceValue::Number(parse_canonical("0").unwrap());
    assert_eq!(e.outcomes[0], Outcome::Decided(zero.clone()));
    assert_eq!(e.outcomes[1], Outcome::Decided(zero));
    assert_eq!(e.outcomes[2], Outcome::Decided(TraceValue::Bool(false)));
}
