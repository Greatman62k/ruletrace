use super::*;
use crate::proptest_floor::property_tests;
use proptest::prelude::*;
use rt_foundation::InputId;
use rt_srcmodel::{default_load_limits, ElementKind};

const BIG: u64 = MAX_SOURCE_DOCUMENT_BYTES;

fn id(name: &str) -> DocumentId {
    DocumentId::new(name).unwrap()
}

fn md(text: &str) -> SourceModel {
    ingest(text.as_bytes(), &id("r.md"), None, BIG).unwrap()
}

fn txt(text: &str) -> SourceModel {
    ingest(text.as_bytes(), &id("r.txt"), None, BIG).unwrap()
}

/// Kind, span text, line, column, parent of every element.
type Row<'a> = (ElementKind, &'a str, u32, u32, Option<u32>);

fn rows(m: &SourceModel) -> Vec<Row<'_>> {
    m.elements()
        .iter()
        .map(|e| {
            (
                e.kind,
                &m.content()[e.start as usize..e.end as usize],
                e.line,
                e.column,
                e.parent,
            )
        })
        .collect()
}

fn code_of(r: Result<SourceModel, Box<Diagnostic>>) -> Code {
    r.unwrap_err().code()
}

#[test]
fn markdown_maps_each_block_kind() {
    let m = md("# 1. Title\n\nFirst line\nsecond line\n\n- (a) item\n  - nested\n- next\n\n```\n# not heading\n```\n\n| h1 | h2 |\n|----|----|\n| a | b |\n\n## Sub\n\nafter\n");
    use ElementKind::*;
    assert_eq!(
        rows(&m),
        vec![
            (Heading, "# 1. Title", 1, 1, None),
            (Paragraph, "First line\nsecond line", 3, 1, Some(0)),
            (ListItem, "- (a) item", 6, 1, Some(0)),
            (ListItem, "- nested", 7, 3, Some(2)),
            (ListItem, "- next", 8, 1, Some(0)),
            (CodeBlock, "```\n# not heading\n```", 10, 1, Some(0)),
            (Table, "| h1 | h2 |\n|----|----|\n| a | b |", 14, 1, Some(0)),
            (TableRow, "| h1 | h2 |", 14, 1, Some(6)),
            (TableRow, "| a | b |", 16, 1, Some(6)),
            (Heading, "## Sub", 18, 1, Some(0)),
            (Paragraph, "after", 20, 1, Some(9)),
        ]
    );
    let e = m.elements();
    assert_eq!(e[0].level, Some(1));
    assert_eq!(e[0].label.as_deref(), Some("1."));
    assert_eq!(e[2].label.as_deref(), Some("(a)"));
    assert_eq!(e[3].label, None);
    assert_eq!(e[9].level, Some(2));
}

#[test]
fn setext_html_and_quote_are_paragraphs() {
    let m = md("Title\n=====\n\n<div>x</div>\n\n> quote\n");
    let kinds: Vec<_> = rows(&m).iter().map(|r| (r.0, r.1)).collect();
    assert_eq!(
        kinds,
        vec![
            (ElementKind::Paragraph, "Title\n====="),
            (ElementKind::Paragraph, "<div>x</div>"),
            (ElementKind::Paragraph, "> quote"),
        ]
    );
}

#[test]
fn plain_text_has_no_markup_except_lists_and_labels() {
    let m = txt("# not a heading\n\n1.1 Clause text\n\n- item\n```\n| a |\n|---|\n");
    use ElementKind::*;
    assert_eq!(
        rows(&m),
        vec![
            (Paragraph, "# not a heading", 1, 1, None),
            (Paragraph, "1.1 Clause text", 3, 1, None),
            (ListItem, "- item\n```\n| a |\n|---|", 5, 1, None),
        ]
    );
    assert_eq!(m.elements()[1].label.as_deref(), Some("1.1"));
    assert_eq!(m.kind(), DocumentKind::Text);
}

#[test]
fn line_endings_bom_and_columns() {
    let m = md("\u{feff}# H\r\n\r\n  é para\r  second\n\n\tx");
    assert_eq!(m.elements()[0].start, 3);
    assert_eq!(m.elements()[0].column, 2);
    assert_eq!(
        rows(&m),
        vec![
            (ElementKind::Heading, "# H", 1, 2, None),
            (ElementKind::Paragraph, "é para\r  second", 3, 3, Some(0)),
            (ElementKind::Paragraph, "x", 6, 2, Some(0)),
        ]
    );
    // CR, LF and CRLF each end exactly one line.
    let lf = md("a\n\nb");
    let cr = md("a\r\rb");
    let crlf = md("a\r\n\r\nb");
    for m in [&lf, &cr, &crlf] {
        assert_eq!(m.elements()[1].line, 3);
    }
}

#[test]
fn empty_and_blank_documents_have_no_elements() {
    let m = md("");
    assert!(m.elements().is_empty());
    assert_eq!(
        m.sha256().as_str(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert!(md("\u{feff}").elements().is_empty());
    assert!(md(" \n\t\n").elements().is_empty());
}

#[test]
fn unclosed_fence_runs_to_the_end() {
    let m = md("```\ncode\n\nmore");
    assert_eq!(
        rows(&m),
        vec![(ElementKind::CodeBlock, "```\ncode\n\nmore", 1, 1, None)]
    );
}

#[test]
fn unclosed_fence_excludes_trailing_blank_lines() {
    let m = md("```\n\n");
    assert_eq!(rows(&m), vec![(ElementKind::CodeBlock, "```", 1, 1, None)]);
}

#[test]
fn table_ends_at_blank_line_and_list_item() {
    let m = md("a | b\n--|--\n1 | 2\n- item\n");
    let kinds: Vec<_> = rows(&m).iter().map(|r| r.0).collect();
    assert_eq!(
        kinds,
        vec![
            ElementKind::Table,
            ElementKind::TableRow,
            ElementKind::TableRow,
            ElementKind::ListItem
        ]
    );
}

#[test]
fn size_limit_comes_first_with_one_code_each() {
    let bad = vec![0xffu8; 11];
    // Over the effective limit below 16 MiB: RT-LIM-0001, even though the
    // format is also bad.
    assert_eq!(
        code_of(ingest(&bad, &id("x.pdf"), None, 10)),
        Code::LimitFileTooLarge
    );
    // Equal to the limit is accepted by the size check and fails the later
    // UTF-8 check.
    assert_eq!(
        code_of(ingest(&bad, &id("x.md"), None, 11)),
        Code::IngestNotUtf8
    );
    let huge = vec![b'a'; (MAX_SOURCE_DOCUMENT_BYTES + 1) as usize];
    assert_eq!(
        code_of(ingest(&huge, &id("x.md"), None, BIG)),
        Code::IngestDocumentTooLarge
    );
    assert_eq!(
        code_of(ingest(&huge, &id("x.md"), None, u64::MAX)),
        Code::IngestDocumentTooLarge
    );
    let exact = vec![b'a'; MAX_SOURCE_DOCUMENT_BYTES as usize];
    assert_eq!(
        ingest(&exact, &id("x.md"), None, BIG)
            .unwrap()
            .elements()
            .len(),
        1
    );
}

#[test]
fn format_is_checked_before_utf8() {
    let d = ingest(b"%PDF-1.7\xff", &id("x.md"), None, BIG).unwrap_err();
    assert_eq!(d.code(), Code::IngestFormatUnsupported);
    assert_eq!(d.primary(), &Location::Document(id("x.md")));
    for (name, bytes) in [
        ("a.docx", &b"text"[..]),
        ("a.md", b"PK\x03\x04zz"),
        ("a.md", b"\x89PNG\r\n"),
        ("a.md", b"  <!DOCTYPE HTML><p>"),
        ("a.txt", b"x <HTML>"),
    ] {
        assert_eq!(
            code_of(ingest(bytes, &id(name), None, BIG)),
            Code::IngestFormatUnsupported,
            "{name}"
        );
    }
    // The hint replaces the extension but never the content check.
    assert_eq!(
        ingest(b"x", &id("a.docx"), Some(DocumentKind::Text), BIG)
            .unwrap()
            .kind(),
        DocumentKind::Text
    );
    assert_eq!(
        code_of(ingest(
            b"%PDF-",
            &id("a.md"),
            Some(DocumentKind::Markdown),
            BIG
        )),
        Code::IngestFormatUnsupported
    );
    // HTML beyond the first 512 bytes is not detected.
    let late = format!("{}<html>", "a".repeat(512));
    assert!(ingest(late.as_bytes(), &id("a.md"), None, BIG).is_ok());
}

#[test]
fn extension_selects_the_kind() {
    for (name, kind) in [
        ("a.md", DocumentKind::Markdown),
        ("dir\\a.MARKDOWN", DocumentKind::Markdown),
        ("a.txt", DocumentKind::Text),
        ("a.text", DocumentKind::Text),
        ("dir.v1/a", DocumentKind::Text),
    ] {
        assert_eq!(
            ingest(b"x", &id(name), None, BIG).unwrap().kind(),
            kind,
            "{name}"
        );
    }
}

#[test]
fn invalid_utf8_reports_the_first_bad_offset() {
    let d = ingest(b"ab\xc3\xa9\xe2\x28", &id("a.md"), None, BIG).unwrap_err();
    assert_eq!(d.code(), Code::IngestNotUtf8);
    assert!(d.message().contains("offset 4"), "{}", d.message());
    assert_eq!(d.primary(), &Location::Document(id("a.md")));
}

#[test]
fn ingesting_twice_gives_identical_bytes_and_loads_back() {
    let text = "\u{feff}# 1 A\r\n- (a) x\r\n  y\r\n\r\n| a | b |\r\n|--|--|\r\n| 1 | é |\r\n";
    let a = md(text).print();
    let b = md(text).print();
    assert_eq!(a, b);
    let loaded = SourceModel::load(
        &InputId::new("m.json").unwrap(),
        a.as_bytes(),
        default_load_limits(),
    )
    .unwrap();
    assert_eq!(loaded, md(text));
}

fn alphabet() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            Just("\n"),
            Just("\r\n"),
            Just("\r"),
            Just(" "),
            Just("\t"),
            Just("#"),
            Just("- "),
            Just("1. "),
            Just("```"),
            Just("|"),
            Just("-"),
            Just("é"),
            Just("(a) "),
            Just("x"),
            Just("A) "),
            Just("\u{feff}"),
            Just("*"),
            Just("2.3 "),
        ],
        0..60,
    )
    .prop_map(|parts| parts.concat())
}

property_tests! {
    /// **Validates: Requirements 1.2, 1.4, 1.7**
    #[test]
    fn any_utf8_text_ingests_deterministically_with_exact_spans(text in alphabet(), markdown in any::<bool>()) {
        let name = if markdown { "d.md" } else { "d.txt" };
        let one = ingest(text.as_bytes(), &id(name), None, BIG).unwrap();
        let two = ingest(text.as_bytes(), &id(name), None, BIG).unwrap();
        prop_assert_eq!(one.print(), two.print());
        let mut previous = 0;
        for e in one.elements() {
            prop_assert!(e.start >= previous);
            previous = e.start;
            let span = &text.as_bytes()[e.start as usize..e.end as usize];
            prop_assert_eq!(span, one.content().as_bytes().get(e.start as usize..e.end as usize).unwrap());
            prop_assert!(!span.ends_with(b"\n") && !span.ends_with(b"\r"));
        }
    }
}
