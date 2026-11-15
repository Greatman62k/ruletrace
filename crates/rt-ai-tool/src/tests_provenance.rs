//! Provenance verification tests (Req 23.7, 23.8, 23.10; design 3.7.4
//! step 3). Offsets, lines and columns of the fixture documents are worked
//! out by hand, and the document hashes come from `sha256sum`.

use crate::plan::ClauseId;
use crate::provenance::verify_provenance;
use crate::tests_response::{col_nth, parse, rid, shape, wrap, DRAFT, SHA};
use rt_foundation::{Code, Diagnostic, DocumentId, Severity};
use rt_srcmodel::{DocumentKind, Element, ElementKind, SourceModel, SourceModels};

// Bytes: "Alpha beta." 0..11, LF at 11, "Gamma δ delta." 12..27 with δ at
// 18..20, LF at 27, "End." 28..32. Length 32.
pub(crate) const RULES: &str = "Alpha beta.\nGamma δ delta.\nEnd.";
pub(crate) const RULES_HASH: &str =
    "f7090ac9560b215b913bce74610d0d305454076f8a40034b0c160d0b16bccec5";
// Bytes: C a f é(2) space o k . = 9.
pub(crate) const CAFE: &str = "Café ok.";
pub(crate) const CAFE_HASH: &str =
    "bd83ae62d79924ec08a0d4af3aaf5e61650c4fa753508ec1af67ad94e1bc327c";

// Bytes: two three-byte characters, boundaries at 0, 3 and 6.
const KANJI: &str = "日日";

pub(crate) fn doc(name: &str) -> DocumentId {
    DocumentId::new(name).unwrap()
}

pub(crate) fn model(name: &str, text: &str) -> SourceModel {
    let end = u32::try_from(text.len()).unwrap();
    let whole = Element {
        id: 0,
        kind: ElementKind::Paragraph,
        level: None,
        label: None,
        start: 0,
        end,
        line: 1,
        column: 1,
        parent: None,
    };
    SourceModel::new(doc(name), DocumentKind::Text, text.to_owned(), vec![whole]).unwrap()
}

pub(crate) fn models() -> SourceModels {
    let mut all = SourceModels::new();
    all.insert(model("rules.md", RULES)).unwrap();
    all.insert(model("cafe.md", CAFE)).unwrap();
    all.insert(model("kanji.md", KANJI)).unwrap();
    all
}

fn clause(name: &str, start: u32, end: u32) -> ClauseId {
    ClauseId {
        document: doc(name),
        start,
        end,
    }
}

fn clauses() -> Vec<ClauseId> {
    vec![
        clause("rules.md", 0, 5),
        clause("rules.md", 5, 11),
        clause("rules.md", 12, 27),
        clause("rules.md", 28, 32),
        clause("cafe.md", 0, 9),
        clause("kanji.md", 0, 6),
    ]
}

#[derive(Clone)]
struct Span {
    doc: &'static str,
    sha: &'static str,
    start: u32,
    end: u32,
    line: u32,
    column: u32,
    quote: &'static str,
}

impl Span {
    fn json(&self) -> String {
        format!(
            r#"{{"document":"{}","sha256":"{}","start":{},"end":{},"line":{},"column":{},"quote":"{}"}}"#,
            self.doc,
            self.sha,
            self.start,
            self.end,
            self.line,
            self.column,
            escaped(self.quote)
        )
    }
}

pub(crate) fn escaped(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\u{0}', "\\u0000")
}

fn on(start: u32, end: u32, line: u32, column: u32, quote: &'static str) -> Span {
    Span {
        doc: "rules.md",
        sha: RULES_HASH,
        start,
        end,
        line,
        column,
        quote,
    }
}

fn cafe(quote: &'static str) -> Span {
    Span {
        doc: "cafe.md",
        sha: CAFE_HASH,
        start: 0,
        end: 9,
        line: 1,
        column: 1,
        quote,
    }
}

const CLAUSE_TWO: &str = "Gamma δ delta.";

fn clause_two() -> Span {
    on(12, 27, 2, 1, CLAUSE_TWO)
}

fn advisory(spans: &[Span]) -> String {
    let items: Vec<String> = spans.iter().map(Span::json).collect();
    format!(
        r#"{{"kind":"candidate_test","proposer":"model-1","rationale":"why","evidence":[{}]}}"#,
        items.join(",")
    )
}

fn suggestion(span: &Span) -> String {
    let spans = format!(
        r#""spans":[{{"document":"{}","start":{},"end":{},"line":{},"column":{}}}]"#,
        span.doc, span.start, span.end, span.line, span.column
    );
    let (head, _) = DRAFT.split_once(r#""spans":"#).unwrap();
    format!(
        r#"{{"kind":"rule_suggestion","proposer":"model-1","rationale":"why","evidence":[{}],"rule":{head}{spans}}}}}"#,
        span.json()
    )
}

fn run_with(text: &str, clauses: &[ClauseId]) -> Vec<Diagnostic> {
    let parsed = parse(text).unwrap();
    verify_provenance(&rid(), &parsed, &models(), clauses)
}

fn run(text: &str) -> Vec<Diagnostic> {
    run_with(text, &clauses())
}

type Shape = (Code, Severity, Option<u32>, Option<u32>, Option<String>);

/// Column of the value of the `nth` occurrence of `key` in single-line text.
fn val(text: &str, key: &str, nth: usize) -> Option<u32> {
    let needle = format!(r#""{key}":"#);
    let at = col_nth(text, &needle, nth)?;
    Some(at + u32::try_from(needle.chars().count()).unwrap())
}

fn expected(code: Code, column: Option<u32>, path: &str) -> Shape {
    (
        code,
        Severity::Error,
        Some(1),
        column,
        Some(path.to_owned()),
    )
}

fn only(text: &str) -> Shape {
    let found = run(text);
    assert_eq!(found.len(), 1, "{found:?}");
    shape(&found[0])
}

const E0: &str = "proposals[0].evidence[0]";

fn at_field(text: &str, code: Code, key: &str) -> Shape {
    expected(code, val(text, key, 0), &format!("{E0}.{key}"))
}

#[test]
fn a_fully_valid_batch_yields_no_diagnostic() {
    let valid = [
        clause_two(),
        on(0, 5, 1, 1, "Alpha"),
        on(5, 11, 1, 6, " beta."),
        on(28, 32, 3, 1, "End."),
        on(12, 17, 2, 1, "Gamma"),
        on(18, 20, 2, 7, "δ"),
        cafe("Café ok."),
    ];
    for span in valid {
        let text = wrap(&[advisory(std::slice::from_ref(&span))]);
        assert!(run(&text).is_empty(), "{}", span.json());
    }
    let batch = wrap(&[
        advisory(&[clause_two(), on(0, 5, 1, 1, "Alpha")]),
        suggestion(&clause_two()),
    ]);
    assert!(run(&batch).is_empty());
    assert!(run(&wrap(&[])).is_empty());
}

#[test]
fn each_check_fails_alone_with_its_code_and_field() {
    let mut unknown = clause_two();
    unknown.doc = "missing.md";
    let mut hash = clause_two();
    hash.sha = SHA;
    let cases: Vec<(Span, Code, &str)> = vec![
        (unknown, Code::AiProposalSpanUnresolved, "document"),
        (hash, Code::AiProposalHashMismatch, "sha256"),
        (
            on(12, 33, 2, 1, CLAUSE_TWO),
            Code::AiProposalSpanUnresolved,
            "end",
        ),
        (
            on(12, 27, 3, 1, CLAUSE_TWO),
            Code::AiProposalSpanUnresolved,
            "line",
        ),
        (
            on(12, 27, 2, 2, CLAUSE_TWO),
            Code::AiProposalSpanUnresolved,
            "column",
        ),
        (
            on(12, 27, 2, 1, "Gamma δ delta!"),
            Code::AiProposalQuoteMismatch,
            "quote",
        ),
    ];
    for (span, code, key) in cases {
        let text = wrap(&[advisory(&[span])]);
        assert_eq!(only(&text), at_field(&text, code, key), "{key}");
    }
}

#[test]
fn a_span_outside_every_clause_is_reported_at_the_evidence_item() {
    let text = wrap(&[advisory(&[on(12, 28, 2, 1, "Gamma δ delta.\n")])]);
    let item = col_nth(&text, r#"{"document""#, 0);
    assert_eq!(
        only(&text),
        expected(Code::AiProposalSpanOutsideContext, item, E0)
    );
}

#[test]
fn an_earlier_check_hides_every_later_failure() {
    let mut all_bad = on(40, 30, 9, 9, "nothing like it");
    all_bad.doc = "missing.md";
    all_bad.sha = SHA;
    let text = wrap(&[advisory(&[all_bad.clone()])]);
    assert_eq!(
        only(&text),
        at_field(&text, Code::AiProposalSpanUnresolved, "document")
    );

    all_bad.doc = "rules.md";
    let text = wrap(&[advisory(&[all_bad.clone()])]);
    assert_eq!(
        only(&text),
        at_field(&text, Code::AiProposalHashMismatch, "sha256")
    );

    all_bad.sha = RULES_HASH;
    let text = wrap(&[advisory(&[all_bad])]);
    assert_eq!(
        only(&text),
        at_field(&text, Code::AiProposalSpanUnresolved, "end")
    );

    let wrong_line = on(12, 28, 9, 9, "nothing like it");
    let text = wrap(&[advisory(&[wrong_line])]);
    assert_eq!(
        only(&text),
        at_field(&text, Code::AiProposalSpanUnresolved, "line")
    );

    let outside_and_quote = on(12, 28, 2, 1, "nothing like it");
    let text = wrap(&[advisory(&[outside_and_quote])]);
    let item = col_nth(&text, r#"{"document""#, 0);
    assert_eq!(
        only(&text),
        expected(Code::AiProposalSpanOutsideContext, item, E0)
    );
}

#[test]
fn bounds_beat_a_wrong_line_and_line_beats_a_wrong_column() {
    let text = wrap(&[advisory(&[on(12, 33, 9, 9, CLAUSE_TWO)])]);
    assert_eq!(
        only(&text),
        at_field(&text, Code::AiProposalSpanUnresolved, "end")
    );
    let text = wrap(&[advisory(&[on(12, 27, 9, 9, CLAUSE_TWO)])]);
    assert_eq!(
        only(&text),
        at_field(&text, Code::AiProposalSpanUnresolved, "line")
    );
}

#[test]
fn offsets_at_the_edges_of_the_document() {
    let reject = |span: Span, key: &str| {
        let text = wrap(&[advisory(&[span])]);
        assert_eq!(
            only(&text),
            at_field(&text, Code::AiProposalSpanUnresolved, key)
        );
    };
    reject(on(12, 12, 2, 1, ""), "end");
    reject(on(27, 12, 2, 1, CLAUSE_TWO), "end");
    reject(on(28, 33, 3, 1, "End.\u{0}"), "end");
    reject(on(40, 50, 3, 1, "x"), "end");
    reject(on(19, 27, 2, 8, "x"), "start");
    reject(on(12, 19, 2, 1, "Gamma "), "end");
}

#[test]
fn a_start_inside_a_character_is_reported_before_an_end_inside_one() {
    let span = Span {
        doc: "kanji.md",
        sha: "9b8f0ab16f74e1206346edfe9b03e90bce3ee2f8c06409ea8531ba467968e3e7",
        start: 1,
        end: 5,
        line: 1,
        column: 1,
        quote: "x",
    };
    let text = wrap(&[advisory(&[span])]);
    assert_eq!(
        only(&text),
        at_field(&text, Code::AiProposalSpanUnresolved, "start")
    );
}

#[test]
fn spans_at_clause_edges() {
    let outside = |span: Span| {
        let text = wrap(&[advisory(&[span])]);
        let item = col_nth(&text, r#"{"document""#, 0);
        assert_eq!(
            only(&text),
            expected(Code::AiProposalSpanOutsideContext, item, E0)
        );
    };
    outside(on(3, 8, 1, 4, "ha be"));
    outside(on(11, 27, 1, 12, "\nGamma δ delta."));
    outside(on(12, 28, 2, 1, "Gamma δ delta.\n"));
}

#[test]
fn a_clause_of_another_document_does_not_contain_the_span() {
    let only_cafe = [clause("cafe.md", 0, 9)];
    let text = wrap(&[advisory(&[on(0, 5, 1, 1, "Alpha")])]);
    let found = run_with(&text, &only_cafe);
    assert_eq!(found.len(), 1);
    assert_eq!(
        shape(&found[0]),
        expected(
            Code::AiProposalSpanOutsideContext,
            col_nth(&text, r#"{"document""#, 0),
            E0
        )
    );
}

#[test]
fn a_quote_that_is_not_the_exact_bytes_is_a_quote_mismatch() {
    let cases: [&'static str; 7] = [
        "Cafe\u{301} ok.",
        "Café ok. ",
        " Café ok.",
        "Café ok",
        "café ok.",
        "",
        "Caf\u{e9}\u{a0}ok.",
    ];
    for quote in cases {
        let text = wrap(&[advisory(&[cafe(quote)])]);
        assert_eq!(
            only(&text),
            at_field(&text, Code::AiProposalQuoteMismatch, "quote"),
            "{quote:?}"
        );
    }
}

#[test]
fn rule_suggestions_and_every_advisory_kind_are_checked() {
    let mut bad = clause_two();
    bad.sha = SHA;
    for kind in [
        "clause_interpretation",
        "clause_relationship",
        "conflict_highlight",
        "candidate_test",
    ] {
        let one = advisory(&[bad.clone()]).replace("candidate_test", kind);
        let text = wrap(&[one]);
        assert_eq!(
            only(&text),
            at_field(&text, Code::AiProposalHashMismatch, "sha256"),
            "{kind}"
        );
    }
    let text = wrap(&[suggestion(&bad)]);
    assert_eq!(
        only(&text),
        at_field(&text, Code::AiProposalHashMismatch, "sha256")
    );
    let mut quote = clause_two();
    quote.quote = "Gamma δ delta!";
    let text = wrap(&[suggestion(&quote)]);
    assert_eq!(
        only(&text),
        at_field(&text, Code::AiProposalQuoteMismatch, "quote")
    );
}

#[test]
fn every_failing_span_is_reported_once_in_proposal_and_evidence_order() {
    let mut unknown = clause_two();
    unknown.doc = "missing.md";
    let mut hash = clause_two();
    hash.sha = SHA;
    let text = wrap(&[
        advisory(&[on(12, 27, 2, 1, "Gamma δ delta!"), clause_two(), unknown]),
        advisory(&[hash]),
        advisory(&[clause_two()]),
        advisory(&[on(12, 28, 2, 1, "Gamma δ delta.\n")]),
    ]);
    let got: Vec<Shape> = run(&text).iter().map(shape).collect();
    let want = vec![
        expected(
            Code::AiProposalQuoteMismatch,
            val(&text, "quote", 0),
            "proposals[0].evidence[0].quote",
        ),
        expected(
            Code::AiProposalSpanUnresolved,
            val(&text, "document", 2),
            "proposals[0].evidence[2].document",
        ),
        expected(
            Code::AiProposalHashMismatch,
            val(&text, "sha256", 3),
            "proposals[1].evidence[0].sha256",
        ),
        expected(
            Code::AiProposalSpanOutsideContext,
            col_nth(&text, r#"{"document""#, 5),
            "proposals[3].evidence[0]",
        ),
    ];
    assert_eq!(got, want);
}

#[test]
fn the_diagnostics_are_errors_with_the_provenance_exit_code() {
    let mut hash = clause_two();
    hash.sha = SHA;
    let found = run(&wrap(&[advisory(&[hash])]));
    assert_eq!(found[0].code().id(), "RT-PRV-0102");
    assert_eq!(found[0].code().exit_code().value(), 3);
    assert_eq!(found[0].severity(), Severity::Error);
}
