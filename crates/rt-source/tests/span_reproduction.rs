//! Property 1: Source span reproduction and ordering (design §5, Requirement 1.1 to 1.4).
//!
//! For any generated UTF-8 document with LF, CRLF or CR endings, tabs and non-ASCII text, every
//! element of the ingested `SourceModel` is ordered by start offset, its span bytes equal the
//! element text, the recorded hash is checked against the content, and line and column
//! recomputed here independently equal the recorded values. The extracted clause texts equal
//! the bytes of their spans.

use proptest::prelude::*;
use rt_foundation::{DocumentId, MAX_SOURCE_DOCUMENT_BYTES};
use rt_source::{extract, ingest, ExtractionMode};
use rt_srcmodel::{default_load_limits, SourceModel};

/// One line ending: LF, CRLF or CR.
fn eol() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("\n"), Just("\r\n"), Just("\r")]
}

/// One line of document text: headings, list items (nested), table rows, fences, numbering
/// labels, tabs and non-ASCII text.
fn line() -> impl Strategy<Value = String> {
    let prefix = prop_oneof![
        Just(""),
        Just("# "),
        Just("## "),
        Just("- "),
        Just("  - "),
        Just("\t- "),
        Just("1. "),
        Just("2.3 "),
        Just("(a) "),
        Just("A) "),
        Just("| "),
        Just("|---|---|"),
        Just("```"),
        Just("\t"),
        Just("  "),
    ];
    let body = proptest::collection::vec(
        prop_oneof![
            Just("word"),
            Just(" "),
            Just("\t"),
            Just("é"),
            Just("日本"),
            Just("😀"),
            Just("|"),
            Just("€5"),
        ],
        0..6,
    )
    .prop_map(|parts| parts.concat());
    (prefix, body).prop_map(|(p, b)| format!("{p}{b}"))
}

/// A UTF-8 document, optionally starting with a BOM, with each line ending chosen independently.
fn gen_document() -> impl Strategy<Value = String> {
    (
        any::<bool>(),
        proptest::collection::vec((line(), eol()), 0..25),
        line(),
    )
        .prop_map(|(bom, lines, last)| {
            let mut text = String::new();
            if bom {
                text.push('\u{feff}');
            }
            for (l, e) in lines {
                text.push_str(&l);
                text.push_str(e);
            }
            text.push_str(&last);
            text
        })
}

/// 1-based line and column (in scalar values) of a byte offset, computed from the raw text.
/// LF, CRLF and a lone CR each end one line.
fn position(text: &str, offset: usize) -> (u32, u32) {
    let bytes = text.as_bytes();
    let mut line = 1u32;
    let mut line_start = 0usize;
    let mut i = 0usize;
    while i < offset {
        match bytes.get(i).copied() {
            Some(b'\n') => {
                line += 1;
                line_start = i + 1;
            }
            Some(b'\r') => {
                if bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                line += 1;
                line_start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    // A CRLF split by `offset` cannot occur: spans start on a line start or inside a line.
    let column = text[line_start..offset].chars().count() as u32 + 1;
    (line, column)
}

proptest! {
    /// **Validates: Requirements 1.1, 1.2, 1.3, 1.4**
    #[test]
    fn spans_reproduce_the_document_bytes_in_order(text in gen_document(), markdown in any::<bool>()) {
        let name = if markdown { "doc.md" } else { "doc.txt" };
        let id = DocumentId::new(name).unwrap();
        let model = ingest(text.as_bytes(), &id, None, MAX_SOURCE_DOCUMENT_BYTES).unwrap();

        // The content is the input and the recorded hash verifies against it. The printed
        // model loads back only when the hash equals the SHA-256 of the content.
        prop_assert_eq!(model.content().as_bytes(), text.as_bytes());
        prop_assert_eq!(model.byte_len(), text.len() as u64);
        prop_assert!(model.verify_hash(model.sha256()).is_ok());
        let printed = model.print();
        let loaded = SourceModel::load(
            &rt_foundation::InputId::new("m.json").unwrap(),
            printed.as_bytes(),
            default_load_limits(),
        ).unwrap();
        prop_assert_eq!(&loaded, &model);

        // Ordering, exact span bytes, and independently recomputed line and column.
        let mut previous_start = 0u32;
        for (index, e) in model.elements().iter().enumerate() {
            prop_assert_eq!(e.id as usize, index);
            prop_assert!(e.start >= previous_start);
            previous_start = e.start;
            prop_assert!(e.start < e.end && e.end as usize <= text.len());
            let (start, end) = (e.start as usize, e.end as usize);
            let span_bytes = &text.as_bytes()[start..end];
            prop_assert_eq!(model.text(e.start, e.end).unwrap().as_bytes(), span_bytes);
            prop_assert_eq!((e.line, e.column), position(&text, start));
        }

        // Every clause text equals the bytes of its span, in ascending start order.
        let extracted = extract(&model, ExtractionMode::first_version()).unwrap();
        let mut previous_start = 0u32;
        for clause in extracted.value() {
            let span = clause.span();
            let (start, end) = (span.start().get(), span.end().get());
            prop_assert!(start >= previous_start);
            previous_start = start;
            prop_assert_eq!(clause.text().as_bytes(), &text.as_bytes()[start as usize..end as usize]);
            prop_assert_eq!(model.span_text(span), Some(clause.text()));
            prop_assert_eq!((span.line(), span.column()), position(&text, start as usize));
        }
    }
}
