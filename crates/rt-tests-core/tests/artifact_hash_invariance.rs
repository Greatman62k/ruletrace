//! Property 13 extended to compiled-artifact bytes.
//!
//! **Validates: Requirements 9.5, 16.18, 18.5**
//!
//! Two Rulebooks with equal Rule_Content that differ only in Origin (manual
//! with or without an author, or AI proposal) and in the ledger's Reviewer,
//! time and rejection reason of an unrelated rejected revision compile to
//! byte-identical canonical artifacts.

#![allow(clippy::expect_used, clippy::panic)]

#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;
use proptest::prelude::*;
use proptest_floor::property_tests;
use rt_compile::compile;
use rt_foundation::{DocumentId, InputId};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, rule_content_hash, Rulebook};
use rt_json::ParseLimits;
use rt_ledger::{parse_ledger, ApprovalLedger, HEADER_LINE, ZERO_HASH};
use rt_srcmodel::{DocumentKind, SourceModel, SourceModels};
use rt_validate::validate;

const TEXT: &str = "# Spring Open Rules\n\n1. A competitor is eligible if age is at least 16.\n";
const CLAUSE: &str = "1. A competitor is eligible if age is at least 16.";

fn origin() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(r#"{"kind":"manual","author":null}"#.to_owned()),
        "[a-z]{1,8}".prop_map(|a| format!(r#"{{"kind":"manual","author":"{a}"}}"#)),
        ("[a-z]{1,8}", "[a-z0-9]{1,8}").prop_map(|(bot, id)| {
            let h = "1".repeat(64);
            format!(
                r#"{{"kind":"ai_proposal","proposer":"{bot}","provider":"p","model":"m","adapter_version":"1","request_hash":"{h}","response_hash":"{h}","proposal_id":"{id}","ai_identities":["{bot}"]}}"#
            )
        }),
    ]
}

/// Revision 1 is approved. Revision 2 of `r.other` is rejected, with its own
/// content.
fn rulebook(origin: &str, value: &str) -> Rulebook {
    let start = TEXT.find(CLAUSE).expect("clause");
    let span = format!(
        r#"{{"document":"rules.md","start":{start},"end":{},"line":3,"column":1}}"#,
        start + CLAUSE.len()
    );
    let rule = |id: &str, rev: u32, v: &str| {
        format!(
            r#"{{"id":"{id}","revision":{rev},"origin":{origin},"kind":"derived","target":"t_{id}","type":"number","expr":{{"node":"num","t":"number","value":"{v}"}},"markers":[],"spans":[{span}]}}"#
        )
    };
    let text = format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{},{}],"precedence":[]}}"#,
        sha256(TEXT.as_bytes()),
        rule("r.main", 1, value),
        rule("r.other", 1, "7")
    );
    let input = InputId::new("rulebook.json").expect("input id");
    match parse_rulebook(text.as_bytes(), &input, 8 << 20, 64) {
        Ok(s) => s.into_parts().0,
        Err(r) => panic!("Rulebook rejected: {:?}", r.diagnostics().as_slice()),
    }
}

fn ledger(book: &Rulebook, reviewer: &str, time: &str, reason: &str) -> ApprovalLedger {
    let doc = sha256(TEXT.as_bytes());
    let mut out = format!("{HEADER_LINE}\n");
    let mut prev = ZERO_HASH.to_owned();
    for (seq, (id, kind, reason)) in [
        ("r.main", "approval", "null".to_owned()),
        ("r.other", "rejection", format!("\"{reason}\"")),
    ]
    .into_iter()
    .enumerate()
    {
        let rev = book
            .revisions
            .iter()
            .find(|r| r.key.id.as_str() == id)
            .expect("revision");
        let hash = rule_content_hash(&book.ir_version, rev, &book.documents).expect("hash");
        let head = format!(
            r#"{{"seq":{},"kind":"{kind}","rule":"{id}","revision":1,"content_hash":"{hash}","ir_version":"1","binding":[{{"document":"rules.md","sha256":"{doc}"}}],"reviewer":"{reviewer}","time":"{time}","reason":{reason},"prev":"{prev}""#,
            seq + 1
        );
        let chain = sha256(format!("{head}}}").as_bytes()).as_str().to_owned();
        out.push_str(&format!("{head},\"hash\":\"{chain}\"}}\n"));
        prev = chain;
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

/// Compiles and returns the canonical bytes in both forms, plus the Rulebook
/// hash.
fn artifact(book: &Rulebook, ledger: &ApprovalLedger) -> (String, String, String) {
    let result = validate(book, ledger, &sources(), None);
    let (compiled, _) = compile(&result)
        .expect("Rulebook with an approved rule compiles")
        .into_parts();
    (
        compiled.to_canonical_compact(),
        compiled.to_canonical_pretty(),
        compiled.rulebook_hash().as_str().to_owned(),
    )
}

property_tests! {
    #![below_floor_config(ProptestConfig::with_cases(64))]

    /// **Validates: Requirements 9.5, 16.18, 18.5**
    #[test]
    fn origin_and_ledger_differences_leave_artifact_bytes_equal(
        a in origin(),
        b in origin(),
        value in "-?[0-9]{1,6}(\\.[0-9]{1,4})?",
        reviewer in "[A-Z][a-z]{1,8}",
        reason in "[a-z ]{1,12}",
        minute in 0u32..60,
    ) {
        let base_book = rulebook(&origin_manual(), &value);
        let base = artifact(&base_book, &ledger(&base_book, "A. Reviewer", "2026-11-14T09:00:00Z", "not a rule"));
        for o in [&a, &b] {
            let book = rulebook(o, &value);
            let time = format!("2026-12-01T10:{minute:02}:00Z");
            let got = artifact(&book, &ledger(&book, &reviewer, &time, &reason));
            prop_assert_eq!(&got, &base);
        }
    }
}

fn origin_manual() -> String {
    r#"{"kind":"manual","author":null}"#.to_owned()
}
