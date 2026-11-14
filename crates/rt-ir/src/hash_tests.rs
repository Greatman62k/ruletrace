//! Tests of the Rule_Content hash and the Draft hash (task 4.6, design §4.1.7).

use super::*;
use rt_foundation::InputId;
use rt_hash::sha256;

const DOC_HASH: &str = "eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a";
const GOLDEN: &str = "d1b4158c4852a5cd3d15a8d1226143461defc7c238eea3f84fef425a5d94fe34";

/// The canonical Rule_Content of design §4.1.7 (hash input).
const CANONICAL: &str = r#"{"ir_version":"1","id":"elig.basic","revision":1,"kind":"eligibility","target":"eligible","type":"boolean","expr":{"node":"and","t":"boolean","args":[{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}},{"node":"field","t":"boolean","name":"licence"}]},"markers":[],"spans":[{"document":"rules.md","sha256":"eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a","start":21,"end":91}]}"#;

/// The rulebook of design §4.1.7, with `origin`, `end` and `line` made
/// variable.
fn rulebook_text(origin: &str, value: &str, end: u32, line: u32, revision: u32) -> String {
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"spring-open","version":"2026.1","documents":[{{"id":"rules.md","sha256":"{DOC_HASH}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}},{{"name":"licence","type":"boolean"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{{"id":"elig.basic","revision":{revision},"origin":{origin},"kind":"eligibility","target":"eligible","type":"boolean","expr":{{"node":"and","t":"boolean","args":[{{"node":"cmp","t":"boolean","op":"ge","left":{{"node":"field","t":"number","name":"age"}},"right":{{"node":"num","t":"number","value":"{value}"}}}},{{"node":"field","t":"boolean","name":"licence"}}]}},"markers":[],"spans":[{{"document":"rules.md","start":21,"end":{end},"line":{line},"column":1}}]}}],"precedence":[]}}"#
    )
}

const MANUAL: &str = r#"{"kind":"manual","author":null}"#;

fn parse(text: &str) -> Rulebook {
    let input = InputId::new("rulebook.json").unwrap();
    parse_rulebook(text.as_bytes(), &input, 8 << 20, 64)
        .map_err(|r| r.into_diagnostics().as_slice().to_vec())
        .unwrap()
        .into_parts()
        .0
}

fn content_hash(book: &Rulebook) -> String {
    rule_content_hash(&book.ir_version, &book.revisions[0], &book.documents)
        .unwrap()
        .as_str()
        .to_owned()
}

fn draft(book: &Rulebook) -> String {
    let rev = &book.revisions[0];
    draft_hash(&book.ir_version, &rev.key.id, &rev.content, &book.documents)
        .unwrap()
        .as_str()
        .to_owned()
}

fn with_revision(mut book: Rulebook, revision: u32) -> Rulebook {
    book.revisions[0].key.revision = rt_foundation::RevisionNumber::new(revision).unwrap();
    book
}

fn base() -> Rulebook {
    parse(&rulebook_text(MANUAL, "16", 91, 3, 1))
}

#[test]
fn worked_example_matches_golden_hash() {
    assert_eq!(sha256(CANONICAL.as_bytes()).as_str(), GOLDEN);
    assert_eq!(content_hash(&base()), GOLDEN);
}

#[test]
fn origin_and_span_line_do_not_change_hashes() {
    let ai = r#"{"kind":"ai_proposal","proposer":"ai.bot","provider":"p","model":"m","adapter_version":"1","request_hash":"eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a","response_hash":"eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a","proposal_id":"x","ai_identities":["ai.bot"]}"#;
    let other = parse(&rulebook_text(ai, "16", 91, 9, 1));
    assert_eq!(content_hash(&other), GOLDEN);
    assert_eq!(draft(&other), draft(&base()));
}

#[test]
fn content_changes_change_the_hash() {
    for changed in [
        parse(&rulebook_text(MANUAL, "17", 91, 3, 1)),
        parse(&rulebook_text(MANUAL, "16", 92, 3, 1)),
        with_revision(base(), 2),
    ] {
        assert_ne!(content_hash(&changed), GOLDEN);
    }
}

#[test]
fn draft_hash_ignores_revision_but_not_content() {
    let r2 = with_revision(base(), 2);
    assert_eq!(draft(&r2), draft(&base()));
    assert_ne!(draft(&base()), content_hash(&base()));
    let changed = parse(&rulebook_text(MANUAL, "17", 91, 3, 1));
    assert_ne!(draft(&changed), draft(&base()));
}

#[test]
fn declared_document_hash_is_part_of_the_input() {
    let mut book = base();
    book.documents[0].sha256 = rt_hash::Sha256Hex::parse(&"0".repeat(64)).unwrap();
    assert_ne!(content_hash(&book), GOLDEN);
}

#[test]
fn undeclared_document_has_no_hash() {
    let mut book = base();
    book.documents.clear();
    let err = rule_content_hash(&book.ir_version, &book.revisions[0], &book.documents);
    assert!(matches!(err, Err(ContentHashError::UndeclaredDocument(_))));
}
