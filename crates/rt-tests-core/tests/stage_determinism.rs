//! Property 2: Stage determinism.
//!
//! Feature: ruletrace-compiler, Property 2: Stage determinism
//!
//! The seven stage functions (ingest, extract, parse, validate, compile,
//! evaluate, verify) are each run on fixed typed inputs in a generated order,
//! on the main thread and on another thread, with a different rulebook's full
//! pipeline run in between. Every output, as canonical or Debug bytes, equals
//! the output of a reference run.
//!
//! **Validates: Requirements 1.7, 4.11, 6.12, 9.4, 10.2, 10.8, 24.6**

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    missing_docs
)]

use proptest::prelude::*;
use rt_data::{CompetitionData, Participant, ParticipantId, Question, TargetName};
use rt_foundation::{DocumentId, InputId};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, print_compact, rule_content_hash, Rulebook};
use rt_json::ParseLimits;
use rt_ledger::{parse_ledger, ApprovalLedger, HEADER_LINE, ZERO_HASH};
use rt_srcmodel::{print_candidates, SourceModel, SourceModels};
use rt_validate::ValidationResult;
use std::collections::BTreeMap;

const CLAUSE: &str = "1. A competitor is eligible if age is at least 16.";

fn text(extra: usize) -> String {
    let mut t = format!("# Rules\n\n{CLAUSE}\n");
    for i in 0..extra {
        t.push_str(&format!("{}. Extra clause number {i} applies.\n", i + 2));
    }
    t
}

fn rulebook_bytes(text: &str, n: usize) -> String {
    let start = text.find(CLAUSE).expect("clause");
    let span = format!(
        r#"{{"document":"rules.md","start":{start},"end":{},"line":3,"column":1}}"#,
        start + CLAUSE.len()
    );
    let rules: Vec<String> = (0..n)
        .map(|i| {
            format!(
                r#"{{"id":"d.r{i}","revision":1,"origin":{{"kind":"manual","author":null}},"kind":"derived","target":"t{i}","type":"number","expr":{{"node":"num","t":"number","value":"{}"}},"markers":[],"spans":[{span}]}}"#,
                i + 1
            )
        })
        .collect();
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{}"}}],"types":{{"inputs":[],"enums":[],"sets":[],"collections":[]}},"rules":[{}],"precedence":[]}}"#,
        sha256(text.as_bytes()),
        rules.join(",")
    )
}

fn ledger_text(book: &Rulebook, text: &str) -> Vec<u8> {
    let doc = sha256(text.as_bytes());
    let mut out = format!("{HEADER_LINE}\n");
    let mut prev = ZERO_HASH.to_owned();
    for (i, rev) in book.revisions.iter().enumerate() {
        let hash = rule_content_hash(&book.ir_version, rev, &book.documents).expect("hash");
        let head = format!(
            r#"{{"seq":{},"kind":"approval","rule":"{}","revision":1,"content_hash":"{hash}","ir_version":"1","binding":[{{"document":"rules.md","sha256":"{doc}"}}],"reviewer":"A. Reviewer","time":"2026-11-14T09:00:00Z","reason":null,"prev":"{prev}""#,
            i + 1,
            rev.key.id
        );
        let h = sha256(format!("{head}}}").as_bytes()).as_str().to_owned();
        out.push_str(&format!("{head},\"hash\":\"{h}\"}}\n"));
        prev = h;
    }
    out.into_bytes()
}

/// All typed inputs of the seven stages for one case.
struct Inputs {
    doc: DocumentId,
    text: String,
    model: SourceModel,
    rb_bytes: Vec<u8>,
    rulebook: Rulebook,
    ledger: ApprovalLedger,
    sources: SourceModels,
    validated: ValidationResult,
    compiled: rt_compiled::CompiledRulebook,
    data: CompetitionData,
    questions: Vec<Question>,
    trace: rt_trace::DecisionTrace,
}

fn st<T>(
    r: Result<rt_foundation::Staged<T>, rt_foundation::Rejected>,
    show: impl Fn(&T) -> String,
) -> String {
    match r {
        Ok(s) => format!("ok {} {:?}", show(s.value()), s.diagnostics()),
        Err(e) => format!("rejected {:?}", e.diagnostics()),
    }
}

impl Inputs {
    fn new(extra: usize, n: usize) -> Self {
        let text = text(extra);
        let doc = DocumentId::new("rules.md").expect("doc");
        let model = rt_source::ingest(text.as_bytes(), &doc, None, 1 << 20)
            .unwrap_or_else(|d| panic!("ingest: {d:?}"));
        let rb_bytes = rulebook_bytes(&text, n).into_bytes();
        let input = InputId::new("rb.json").expect("input id");
        let rulebook = match parse_rulebook(&rb_bytes, &input, 1 << 22, 64) {
            Ok(s) => s.into_parts().0,
            Err(r) => panic!("parse: {:?}", r.diagnostics()),
        };
        let ledger = parse_ledger(
            &InputId::new("a.ledger").expect("input id"),
            &ledger_text(&rulebook, &text),
            ParseLimits::new(1 << 22, 64),
        )
        .unwrap_or_else(|e| panic!("ledger: {e:?}"));
        let mut sources = SourceModels::new();
        sources.insert(model.clone()).expect("insert");
        let validated = rt_validate::validate(&rulebook, &ledger, &sources, None);
        assert!(validated.state().no_errors(), "{:?}", validated.state());
        let compiled = match rt_compile::compile(&validated) {
            Ok(s) => s.into_parts().0,
            Err(r) => panic!("compile: {:?}", r.diagnostics()),
        };
        let p = ParticipantId::new("p0").expect("participant");
        let data = CompetitionData::new(BTreeMap::from([(
            p.clone(),
            Participant::new(BTreeMap::new(), BTreeMap::new()),
        )]));
        let questions = vec![Question::Value {
            target: TargetName::new("t0").expect("target"),
            participant: p,
        }];
        let ev = rt_engine::evaluate(&compiled, &data, &questions, None);
        let trace = ev.traces.into_iter().next().expect("trace");
        Self {
            doc,
            text,
            model,
            rb_bytes,
            rulebook,
            ledger,
            sources,
            validated,
            compiled,
            data,
            questions,
            trace,
        }
    }

    /// Runs stage `k` (0 to 6) and returns its output as bytes.
    fn stage(&self, k: usize) -> String {
        let input = InputId::new("rb.json").expect("input id");
        match k {
            0 => match rt_source::ingest(self.text.as_bytes(), &self.doc, None, 1 << 20) {
                Ok(m) => m.print(),
                Err(d) => format!("{d:?}"),
            },
            1 => st(
                rt_source::extract(&self.model, rt_source::ExtractionMode::FirstVersion),
                |v: &Vec<_>| print_candidates(&self.doc, v),
            ),
            2 => match parse_rulebook(&self.rb_bytes, &input, 1 << 22, 64) {
                Ok(s) => format!("{:?}|{:?}", print_compact(s.value()), s.diagnostics()),
                Err(r) => format!("{:?}", r.diagnostics()),
            },
            3 => format!(
                "{:?}",
                rt_validate::validate(&self.rulebook, &self.ledger, &self.sources, None)
            ),
            4 => st(rt_compile::compile(&self.validated), |c| {
                c.to_canonical_compact()
            }),
            5 => {
                let e = rt_engine::evaluate(&self.compiled, &self.data, &self.questions, None);
                format!("{:?}", e)
            }
            _ => format!("{:?}", rt_explain::verify_trace(&self.trace, &self.sources)),
        }
    }
}

fn order() -> impl Strategy<Value = Vec<usize>> {
    Just((0..7).collect::<Vec<_>>()).prop_shuffle()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// Feature: ruletrace-compiler, Property 2: Stage determinism
    /// **Validates: Requirements 1.7, 4.11, 6.12, 9.4, 10.2, 10.8, 24.6**
    #[test]
    fn every_stage_output_is_independent_of_order_thread_and_other_rulebooks(
        extra in 0usize..3,
        n in 1usize..4,
        first in order(),
        second in order(),
    ) {
        let inputs = Inputs::new(extra, n);
        let other = Inputs::new(extra + 1, n + 1);
        let reference: Vec<String> = (0..7).map(|k| inputs.stage(k)).collect();

        for ord in [&first, &second] {
            for &k in ord {
                let _ = other.stage(k);
                prop_assert_eq!(&inputs.stage(k), &reference[k], "stage {}", k);
            }
        }
        // Another thread (the in-process stand-in for another process).
        let threaded: Vec<String> = std::thread::scope(|s| {
            s.spawn(|| first.iter().map(|&k| (k, inputs.stage(k))).collect::<Vec<_>>())
                .join()
                .expect("thread")
                .into_iter()
                .map(|(_, v)| v)
                .collect()
        });
        for (&k, out) in first.iter().zip(&threaded) {
            prop_assert_eq!(out, &reference[k], "stage {} on another thread", k);
        }
    }
}
