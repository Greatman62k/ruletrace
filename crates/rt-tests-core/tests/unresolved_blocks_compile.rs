//! Property 10: unresolved approvals block compilation.
//!
//! **Validates: Requirements 5.4, 5.5, 5.7, 5.14, 9.13, 25.13, 25.21, 26.12,
//! 26.13, 26.28, 26.29**
//!
//! A Rulebook of derived rules with one or two revisions each is generated,
//! together with a ledger plan per revision (no record, a matching approval or
//! a matching rejection). An oracle computes from the plan alone which
//! revisions are unresolved (Proposed with no higher Approved or Rejected
//! revision) and which rules have an Effective revision. Validation must give
//! one `RT-APR-0001` per unresolved revision and a failed result, `RT-APR-0002`
//! when no revision is Effective, and compilation must return no artifact in
//! either case.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;
use proptest::collection::vec;
use proptest::prelude::*;
use proptest_floor::property_tests;
use rt_compile::compile;
use rt_foundation::{Code, DocumentId, InputId};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, rule_content_hash, Rulebook};
use rt_json::ParseLimits;
use rt_ledger::{parse_ledger, ApprovalLedger, HEADER_LINE, ZERO_HASH};
use rt_srcmodel::{DocumentKind, SourceModel, SourceModels};

const TEXT: &str = "# Rules\n\n1. A competitor is eligible if age is at least 16.\n";
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
        Just(Decision::Reject)
    ];
    vec(vec(decision, 1..3), 1..4)
}

fn book(plan: &[Vec<Decision>]) -> Rulebook {
    let start = TEXT.find(CLAUSE).expect("clause");
    let span = format!(
        r#"{{"document":"rules.md","start":{start},"end":{},"line":3,"column":1}}"#,
        start + CLAUSE.len()
    );
    let mut rules = Vec::new();
    for (i, revs) in plan.iter().enumerate() {
        for j in 0..revs.len() {
            rules.push(format!(
                r#"{{"id":"d.r{i}","revision":{},"origin":{{"kind":"manual","author":null}},"kind":"derived","target":"t{i}","type":"number","expr":{{"node":"num","t":"number","value":"{}"}},"markers":[],"spans":[{span}]}}"#,
                j + 1,
                j + 1
            ));
        }
    }
    let doc = format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{}"}}],"types":{{"inputs":[],"enums":[],"sets":[],"collections":[]}},"rules":[{}],"precedence":[]}}"#,
        sha256(TEXT.as_bytes()),
        rules.join(",")
    );
    let input = InputId::new("rb.json").expect("input id");
    match parse_rulebook(doc.as_bytes(), &input, 1 << 22, 64) {
        Ok(s) => s.into_parts().0,
        Err(r) => panic!("rulebook rejected: {:?}", r.diagnostics().as_slice()),
    }
}

fn ledger(book: &Rulebook, plan: &[Vec<Decision>]) -> ApprovalLedger {
    let doc = sha256(TEXT.as_bytes());
    let mut out = format!("{HEADER_LINE}\n");
    let mut prev = ZERO_HASH.to_owned();
    let mut seq = 0;
    for (i, revs) in plan.iter().enumerate() {
        for (j, d) in revs.iter().enumerate() {
            if *d == Decision::None {
                continue;
            }
            let rev = book
                .revisions
                .iter()
                .find(|r| {
                    r.key.id.as_str() == format!("d.r{i}") && r.key.revision.get() as usize == j + 1
                })
                .expect("revision");
            let hash = rule_content_hash(&book.ir_version, rev, &book.documents).expect("hash");
            let (kind, reason) = if *d == Decision::Reject {
                ("rejection", "\"no\"")
            } else {
                ("approval", "null")
            };
            seq += 1;
            let head = format!(
                r#"{{"seq":{seq},"kind":"{kind}","rule":"d.r{i}","revision":{},"content_hash":"{hash}","ir_version":"1","binding":[{{"document":"rules.md","sha256":"{doc}"}}],"reviewer":"A. Reviewer","time":"2026-11-14T09:00:00Z","reason":{reason},"prev":"{prev}""#,
                j + 1
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

fn sources() -> SourceModels {
    let model = SourceModel::new(
        DocumentId::new("rules.md").expect("document id"),
        DocumentKind::Markdown,
        TEXT.to_owned(),
        Vec::new(),
    )
    .expect("model");
    let mut s = SourceModels::new();
    s.insert(model).expect("insert");
    s
}

property_tests! {
    #![below_floor_config(ProptestConfig::with_cases(64))]

    /// **Validates: Requirements 5.4, 5.5, 5.7, 5.14, 9.13, 25.13, 25.21,
    /// 26.12, 26.13, 26.28, 26.29**
    #[test]
    fn unresolved_or_missing_effective_revisions_block_compilation(p in plan()) {
        let rb = book(&p);
        let l = ledger(&rb, &p);
        let result = rt_validate::validate(&rb, &l, &sources(), None);
        let count = |c: Code| result.state().as_slice().iter().filter(|d| d.code() == c).count();

        // Oracle from the plan alone.
        let unresolved = p
            .iter()
            .map(|revs| {
                (0..revs.len())
                    .filter(|&j| {
                        revs[j] == Decision::None
                            && !revs[j + 1..].iter().any(|d| *d != Decision::None)
                    })
                    .count()
            })
            .sum::<usize>();
        let any_effective = p.iter().any(|revs| revs.contains(&Decision::Approve));

        prop_assert_eq!(count(Code::ApprovalUnresolved), unresolved);
        prop_assert_eq!(count(Code::ApprovalNoExecutableRules), usize::from(!any_effective));
        prop_assert_eq!(result.state().has_unresolved_approval(), unresolved > 0);
        if unresolved > 0 || !any_effective {
            prop_assert!(result.state().has_errors());
            let rejected = compile(&result).expect_err("blocked rulebook must not compile");
            let codes: Vec<Code> = rejected.diagnostics().as_slice().iter().map(|d| d.code()).collect();
            prop_assert!(codes.contains(&if unresolved > 0 {
                Code::ApprovalUnresolved
            } else {
                Code::ApprovalNoExecutableRules
            }), "{codes:?}");
        }
    }
}
