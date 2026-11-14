//! Property 11: the compiled rule set equals the Effective set and compilation
//! is atomic.
//!
//! **Validates: Requirements 5.2, 9.1, 9.2, 9.10, 9.11, 9.12, 25.17, 25.18,
//! 7.6, 7.7**
//!
//! A Rulebook of derived rules with several revisions each is generated, with a
//! plan per revision (no record, approval, rejection). The ledger is written
//! with an independent hash chain. An oracle derives the Effective set and the
//! unresolved set from the plan alone: an unresolved revision gives
//! `RT-APR-0001` and no value, no Effective revision gives `RT-APR-0002` and no
//! value, otherwise the compiled rules are exactly the Effective revisions,
//! independent of Origin and of the order of the revision list.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use proptest::collection::vec;
use proptest::prelude::*;
use rt_compile::compile;
use rt_foundation::{Code, DocumentId, InputId};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, rule_content_hash, Identity, Origin, Rulebook};
use rt_json::ParseLimits;
use rt_ledger::{parse_ledger, ApprovalLedger, HEADER_LINE, ZERO_HASH};
use rt_srcmodel::{DocumentKind, SourceModel, SourceModels};
use rt_validate::validate;
use std::collections::BTreeSet;

const TEXT: &str = "# Spring Open Rules\n\n1. A competitor is eligible if age is at least 16.\n";
const CLAUSE: &str = "1. A competitor is eligible if age is at least 16.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    None,
    Approve,
    Reject,
}

fn plan() -> impl Strategy<Value = Vec<Vec<Decision>>> {
    let decision = prop_oneof![
        Just(Decision::None),
        Just(Decision::Approve),
        Just(Decision::Approve),
        Just(Decision::Reject),
    ];
    vec(vec(decision, 1..4), 1..4)
}

fn rule_id(i: usize) -> String {
    format!("r.rule{i}")
}

/// Every revision has its own constant, so no two revisions share content.
fn rulebook(plan: &[Vec<Decision>], author: Option<&str>) -> Rulebook {
    let start = TEXT.find(CLAUSE).expect("clause");
    let span = format!(
        r#"{{"document":"rules.md","start":{start},"end":{},"line":3,"column":1}}"#,
        start + CLAUSE.len()
    );
    let author = author.map_or("null".to_owned(), |a| format!("\"{a}\""));
    let mut revs = Vec::new();
    let mut n = 0;
    for (i, rule) in plan.iter().enumerate() {
        for j in 0..rule.len() {
            n += 1;
            revs.push(format!(
                r#"{{"id":"{}","revision":{},"origin":{{"kind":"manual","author":{author}}},"kind":"derived","target":"t{i}","type":"number","expr":{{"node":"num","t":"number","value":"{n}"}},"markers":[],"spans":[{span}]}}"#,
                rule_id(i),
                j + 1
            ));
        }
    }
    let text = format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{}],"precedence":[]}}"#,
        sha256(TEXT.as_bytes()),
        revs.join(",")
    );
    let input = InputId::new("rulebook.json").expect("input id");
    match parse_rulebook(text.as_bytes(), &input, 8 << 20, 64) {
        Ok(s) => s.into_parts().0,
        Err(r) => panic!("Rulebook rejected: {:?}", r.diagnostics().as_slice()),
    }
}

fn ledger(book: &Rulebook, plan: &[Vec<Decision>]) -> ApprovalLedger {
    let doc = sha256(TEXT.as_bytes());
    let mut out = format!("{HEADER_LINE}\n");
    let mut prev = ZERO_HASH.to_owned();
    let mut seq = 0;
    for (i, rule) in plan.iter().enumerate() {
        for (j, d) in rule.iter().enumerate() {
            let (kind, reason) = match d {
                Decision::None => continue,
                Decision::Approve => ("approval", "null"),
                Decision::Reject => ("rejection", "\"not a rule\""),
            };
            let rev = book
                .revisions
                .iter()
                .find(|r| r.key.id.as_str() == rule_id(i) && r.key.revision.get() as usize == j + 1)
                .expect("revision");
            let hash = rule_content_hash(&book.ir_version, rev, &book.documents).expect("hash");
            seq += 1;
            let head = format!(
                r#"{{"seq":{seq},"kind":"{kind}","rule":"{}","revision":{},"content_hash":"{hash}","ir_version":"1","binding":[{{"document":"rules.md","sha256":"{doc}"}}],"reviewer":"A. Reviewer","time":"2026-11-14T09:00:00Z","reason":{reason},"prev":"{prev}""#,
                rule_id(i),
                j + 1
            );
            let chain = sha256(format!("{head}}}").as_bytes()).as_str().to_owned();
            out.push_str(&format!("{head},\"hash\":\"{chain}\"}}\n"));
            prev = chain;
        }
    }
    let input = InputId::new("approvals.ledger").expect("input id");
    match parse_ledger(&input, out.as_bytes(), ParseLimits::new(1 << 22, 64)) {
        Ok(l) => l,
        Err(e) => panic!("generated ledger rejected: {e:?}"),
    }
}

fn sources() -> SourceModels {
    let mut s = SourceModels::new();
    let model = SourceModel::new(
        DocumentId::new("rules.md").expect("doc id"),
        DocumentKind::Markdown,
        TEXT.to_owned(),
        Vec::new(),
    )
    .expect("model");
    s.insert(model).expect("insert");
    s
}

/// The oracle. Returns the Effective set (highest approved revision of each
/// rule) and whether any revision is unresolved (Proposed with no higher
/// approved or rejected revision).
fn oracle(plan: &[Vec<Decision>]) -> (BTreeSet<(String, u32)>, bool) {
    let mut effective = BTreeSet::new();
    let mut unresolved = false;
    for (i, rule) in plan.iter().enumerate() {
        let rev = |j: usize| u32::try_from(j + 1).expect("small");
        if let Some(j) = rule.iter().rposition(|d| *d == Decision::Approve) {
            effective.insert((rule_id(i), rev(j)));
        }
        let last_decided = rule.iter().rposition(|d| *d != Decision::None);
        for (j, d) in rule.iter().enumerate() {
            if *d == Decision::None && last_decided.is_none_or(|l| l < j) {
                unresolved = true;
            }
        }
    }
    (effective, unresolved)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// **Validates: Requirements 5.2, 9.1, 9.2, 9.10, 9.11, 9.12, 25.17, 25.18,
    /// 7.6, 7.7**
    #[test]
    fn compiled_set_equals_the_effective_set(p in plan(), author in "[a-z]{1,8}") {
        let book = rulebook(&p, None);
        let result = validate(&book, &ledger(&book, &p), &sources(), None);
        let (effective, unresolved) = oracle(&p);
        let outcome = compile(&result);

        if unresolved {
            let rejected = outcome.expect_err("an unresolved revision blocks compilation");
            let codes: Vec<Code> = rejected.diagnostics().as_slice().iter().map(|d| d.code()).collect();
            prop_assert!(codes.contains(&Code::ApprovalUnresolved));
            // The blocking Diagnostics are those of the validation, unchanged
            // and in order.
            let blocking: Vec<_> = result.state().as_slice().iter().filter(|d| d.severity().is_blocking()).collect();
            let got: Vec<_> = rejected.diagnostics().as_slice().iter().collect();
            prop_assert_eq!(got, blocking);
        } else if effective.is_empty() {
            let rejected = outcome.expect_err("no Effective revision blocks compilation");
            let codes: Vec<Code> = rejected.diagnostics().as_slice().iter().map(|d| d.code()).collect();
            prop_assert_eq!(codes, vec![Code::ApprovalNoExecutableRules]);
        } else {
            let (compiled, _) = outcome.expect("a clean Rulebook with an Effective set compiles").into_parts();
            let got: BTreeSet<(String, u32)> = compiled
                .rules()
                .iter()
                .map(|r| (r.id.as_str().to_owned(), r.revision.get()))
                .collect();
            prop_assert_eq!(compiled.rules().len(), effective.len());
            prop_assert_eq!(compiled.order().len(), effective.len());
            prop_assert_eq!(got, effective);

            // Origin and the order of the revision list do not change the
            // compiled output.
            let mut changed = book.clone();
            for rev in &mut changed.revisions {
                rev.origin = Origin::Manual { author: Some(Identity::new(author.clone()).expect("identity")) };
            }
            changed.revisions.reverse();
            let again = validate(&changed, &ledger(&book, &p), &sources(), None);
            let (other, _) = compile(&again).expect("same Effective set compiles").into_parts();
            prop_assert_eq!(other.rulebook_hash(), compiled.rulebook_hash());
        }
    }
}
