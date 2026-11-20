//! Property 8: locations are never fabricated and are preserved.
//!
//! **Validates: Requirements 7.3, 7.14, 7.15, 7.21, 9.3, 13.2**
//!
//! Rulebooks of derived rules with one or two revisions (a first revision that
//! is rejected, then an approved one) are generated, each revision with 1 to 3
//! spans chosen among the clauses of a fixed document. The oracle is the
//! generated span list alone. Every `Source` location in the validation,
//! compilation and evaluation Diagnostics must equal one of those spans, the
//! rejected-revision Warning must carry the first span of its revision, the
//! compiled rule must hold the spans of its Effective revision in order with
//! the document hash, and every span of every trace must equal the span of the
//! compiled rule that carries it.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    missing_docs
)]

#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;
use proptest::collection::vec;
use proptest::prelude::*;
use proptest_floor::property_tests;
use rt_compile::compile;
use rt_data::{CompetitionData, Participant, ParticipantId, Question, TargetName};
use rt_engine::evaluate;
use rt_foundation::{Code, Diagnostic, DocumentId, InputId, Location};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, rule_content_hash, Rulebook};
use rt_json::ParseLimits;
use rt_ledger::{parse_ledger, HEADER_LINE, ZERO_HASH};
use rt_srcmodel::{DocumentKind, SourceModel, SourceModels};
use std::collections::BTreeMap;

const CLAUSES: usize = 4;

/// (start, end, line) of clause `k`; the column is always 1.
fn text_and_clauses() -> (String, Vec<(u32, u32, u32)>) {
    let mut text = String::from("# Rules\n\n");
    let mut out = Vec::new();
    for k in 0..CLAUSES {
        let start = text.len() as u32;
        text.push_str(&format!("{}. Clause number {k} stands here.", k + 1));
        out.push((start, text.len() as u32, 3 + k as u32));
        text.push('\n');
    }
    (text, out)
}

/// Per rule: whether it has a rejected first revision, and the clause indices
/// of each revision.
type Plan = Vec<(bool, Vec<Vec<usize>>)>;

fn plan() -> impl Strategy<Value = Plan> {
    let spans = vec(0..CLAUSES, 1..4);
    vec(
        (any::<bool>(), vec(spans, 2..3)).prop_map(|(two, mut revs)| {
            if !two {
                revs.truncate(1);
            }
            (two, revs)
        }),
        1..4,
    )
}

fn json_span((s, e, l): (u32, u32, u32)) -> String {
    format!(r#"{{"document":"rules.md","start":{s},"end":{e},"line":{l},"column":1}}"#)
}

fn book(plan: &Plan, clauses: &[(u32, u32, u32)], text: &str) -> Rulebook {
    let mut rules = Vec::new();
    for (i, (_, revs)) in plan.iter().enumerate() {
        for (j, idx) in revs.iter().enumerate() {
            let spans: Vec<String> = idx.iter().map(|&k| json_span(clauses[k])).collect();
            rules.push(format!(
                r#"{{"id":"d.r{i}","revision":{},"origin":{{"kind":"manual","author":null}},"kind":"derived","target":"t{i}","type":"number","expr":{{"node":"num","t":"number","value":"{}"}},"markers":[],"spans":[{}]}}"#,
                j + 1,
                j + 1,
                spans.join(",")
            ));
        }
    }
    let doc = format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{}"}}],"types":{{"inputs":[],"enums":[],"sets":[],"collections":[]}},"rules":[{}],"precedence":[]}}"#,
        sha256(text.as_bytes()),
        rules.join(",")
    );
    let input = InputId::new("rb.json").expect("input id");
    match parse_rulebook(doc.as_bytes(), &input, 1 << 22, 64) {
        Ok(s) => s.into_parts().0,
        Err(r) => panic!("rulebook rejected: {:?}", r.diagnostics().as_slice()),
    }
}

/// Rejects every first revision of a two-revision rule and approves every last
/// revision.
fn ledger(book: &Rulebook, plan: &Plan, text: &str) -> rt_ledger::ApprovalLedger {
    let doc = sha256(text.as_bytes());
    let mut out = format!("{HEADER_LINE}\n");
    let mut prev = ZERO_HASH.to_owned();
    let mut seq = 0;
    for (i, (two, revs)) in plan.iter().enumerate() {
        let mut steps = vec![(revs.len(), "approval", "null")];
        if *two {
            steps.insert(0, (1, "rejection", "\"no\""));
        }
        for (rev_no, kind, reason) in steps {
            let rev = book
                .revisions
                .iter()
                .find(|r| {
                    r.key.id.as_str() == format!("d.r{i}")
                        && r.key.revision.get() as usize == rev_no
                })
                .expect("revision");
            let hash = rule_content_hash(&book.ir_version, rev, &book.documents).expect("hash");
            seq += 1;
            let head = format!(
                r#"{{"seq":{seq},"kind":"{kind}","rule":"d.r{i}","revision":{rev_no},"content_hash":"{hash}","ir_version":"1","binding":[{{"document":"rules.md","sha256":"{doc}"}}],"reviewer":"A. Reviewer","time":"2026-11-14T09:00:00Z","reason":{reason},"prev":"{prev}""#
            );
            let h = sha256(format!("{head}}}").as_bytes()).as_str().to_owned();
            out.push_str(&format!("{head},\"hash\":\"{h}\"}}\n"));
            prev = h;
        }
    }
    let input = InputId::new("approvals.ledger").expect("input id");
    parse_ledger(&input, out.as_bytes(), ParseLimits::new(1 << 22, 64))
        .unwrap_or_else(|e| panic!("ledger rejected: {e:?}"))
}

type Key = (String, u32, u32, u32, u32);

fn key((s, e, l): (u32, u32, u32)) -> Key {
    ("rules.md".to_owned(), s, e, l, 1)
}

/// Every `Source` location of the diagnostics (primary and related) must be in
/// `allowed`.
fn check_sources(diags: &[Diagnostic], allowed: &[Key]) -> Result<(), TestCaseError> {
    for d in diags {
        for loc in std::iter::once(d.primary()).chain(d.related()) {
            if let Location::Source(s) = loc {
                let k = (
                    s.doc().as_str().to_owned(),
                    s.start().get(),
                    s.end().get(),
                    s.line(),
                    s.column(),
                );
                prop_assert!(allowed.contains(&k), "fabricated {k:?} in {:?}", d.code());
            }
        }
    }
    Ok(())
}

property_tests! {
    #![below_floor_config(ProptestConfig::with_cases(64))]

    /// **Validates: Requirements 7.3, 7.14, 7.15, 7.21, 9.3, 13.2**
    #[test]
    fn locations_come_from_the_rulebook_and_are_preserved(p in plan()) {
        let (text, clauses) = text_and_clauses();
        let rb = book(&p, &clauses, &text);
        let l = ledger(&rb, &p, &text);
        let mut models = SourceModels::new();
        models
            .insert(
                SourceModel::new(
                    DocumentId::new("rules.md").expect("document id"),
                    DocumentKind::Markdown,
                    text.clone(),
                    Vec::new(),
                )
                .expect("model"),
            )
            .expect("insert");
        let result = rt_validate::validate(&rb, &l, &models, None);

        let allowed: Vec<Key> = p
            .iter()
            .flat_map(|(_, revs)| revs.iter().flatten().map(|&k| key(clauses[k])))
            .collect();
        check_sources(result.state().as_slice(), &allowed)?;

        // One rejected-revision Warning per rejected first revision, at its
        // first span.
        let rejected: Vec<&Diagnostic> = result
            .state()
            .as_slice()
            .iter()
            .filter(|d| d.code() == Code::ApprovalRevisionRejected)
            .collect();
        prop_assert_eq!(rejected.len(), p.iter().filter(|(two, _)| *two).count());
        for (i, (_, revs)) in p.iter().enumerate().filter(|(_, (two, _))| *two) {
            let first = key(clauses[revs[0][0]]);
            let hit = rejected.iter().any(|d| matches!(d.primary(), Location::Source(s)
                if (s.doc().as_str().to_owned(), s.start().get(), s.end().get(), s.line(), s.column()) == first));
            prop_assert!(hit, "rule {i}: warning not at the first span of the revision");
        }

        let staged = compile(&result).expect("approved rulebook compiles");
        let (compiled, warnings) = staged.into_parts();
        check_sources(warnings.as_slice(), &allowed)?;

        // Compiled spans equal the spans of the Effective revision: same count,
        // order, hash.
        let doc_hash = sha256(text.as_bytes());
        prop_assert_eq!(compiled.rules().len(), p.len());
        for (i, (_, revs)) in p.iter().enumerate() {
            let rule = compiled
                .rules()
                .iter()
                .find(|r| r.id.as_str() == format!("d.r{i}"))
                .expect("compiled rule");
            let want: Vec<Key> = revs[revs.len() - 1].iter().map(|&k| key(clauses[k])).collect();
            let got: Vec<Key> = rule
                .spans
                .iter()
                .map(|s| (s.document.as_str().to_owned(), s.start, s.end, s.line, s.column))
                .collect();
            prop_assert_eq!(got, want);
            prop_assert!(rule.spans.iter().all(|s| s.sha256.as_str() == doc_hash.as_str()));
        }

        // Evaluation: traces carry exactly the spans of the compiled rules they
        // name.
        let by_rule: BTreeMap<String, Vec<Key>> = compiled
            .rules()
            .iter()
            .map(|r| {
                let spans = r
                    .spans
                    .iter()
                    .map(|s| (s.document.as_str().to_owned(), s.start, s.end, s.line, s.column))
                    .collect();
                (r.id.as_str().to_owned(), spans)
            })
            .collect();
        let participant = ParticipantId::new("p0").expect("participant");
        let data = CompetitionData::new(BTreeMap::from([(
            participant.clone(),
            Participant::new(BTreeMap::new(), BTreeMap::new()),
        )]));
        let mut questions: Vec<Question> = (0..p.len())
            .map(|i| Question::Value {
                target: TargetName::new(format!("t{i}")).expect("target"),
                participant: participant.clone(),
            })
            .collect();
        questions.push(Question::Value {
            target: TargetName::new("nosuch").expect("target"),
            participant,
        });
        let eval = evaluate(&compiled, &data, &questions, None);
        check_sources(eval.diagnostics.as_slice(), &allowed)?;
        let mut traced = 0;
        for trace in &eval.traces {
            for tr in trace.rules() {
                let want = by_rule.get(tr.rule.as_str()).expect("rule of the rulebook");
                let got: Vec<Key> = tr
                    .spans
                    .iter()
                    .map(|s| (s.document.as_str().to_owned(), s.start, s.end, s.line, s.column))
                    .collect();
                prop_assert_eq!(&got, want);
                prop_assert!(tr.spans.iter().all(|s| s.sha256.as_str() == doc_hash.as_str()));
                traced += 1;
            }
            for s in trace.spans() {
                let k = (s.document.as_str().to_owned(), s.start, s.end, s.line, s.column);
                prop_assert!(allowed.contains(&k));
            }
        }
        prop_assert!(traced >= p.len(), "every rule must appear in a trace");
    }
}
