use super::*;
use rt_foundation::{Code, DocumentId, InputId, Location, SourceSpan, SpanError, SpanSource};
use rt_hash::sha256;
use rt_json::ParseLimits;

fn doc(name: &str) -> DocumentId {
    DocumentId::new(name).unwrap()
}

fn element(id: u32, kind: ElementKind, start: u32, end: u32, line: u32, column: u32) -> Element {
    Element {
        id,
        kind,
        level: (kind == ElementKind::Heading).then_some(1),
        label: None,
        start,
        end,
        line,
        column,
        parent: None,
    }
}

/// "# Title\n1. été\r\nend" : heading 0..7, list item 8..15 (é is two bytes), paragraph 17..20.
fn sample() -> SourceModel {
    let text = "# Title\n1. été\r\nend";
    let mut item = element(1, ElementKind::ListItem, 8, 16, 2, 1);
    item.label = Some("1.".to_owned());
    item.parent = Some(0);
    SourceModel::new(
        doc("rules.md"),
        DocumentKind::Markdown,
        text.to_owned(),
        vec![
            element(0, ElementKind::Heading, 0, 7, 1, 1),
            item,
            element(2, ElementKind::Paragraph, 18, 21, 3, 1),
        ],
    )
    .unwrap()
}

fn load(bytes: &[u8]) -> Result<SourceModel, Box<rt_foundation::Diagnostic>> {
    SourceModel::load(
        &InputId::new("model.json").unwrap(),
        bytes,
        default_load_limits(),
    )
}

#[test]
fn print_and_load_round_trip_byte_identically() {
    let model = sample();
    let printed = model.print();
    assert!(printed.ends_with('\n'));
    let loaded = load(printed.as_bytes()).unwrap();
    assert_eq!(loaded, model);
    assert_eq!(loaded.print(), printed);
    assert_eq!(sample().print(), printed);
    assert!(printed.starts_with("{\n  \"format\": \"ruletrace-source-model\""));
}

#[test]
fn one_altered_content_byte_fails_with_prv_0005() {
    let printed = sample().print();
    let altered = printed.replacen("Title", "Titlf", 1);
    let d = load(altered.as_bytes()).unwrap_err();
    assert_eq!(d.code(), Code::ProvSourceModelInvalid);
    assert_eq!(d.primary(), &Location::Document(doc("rules.md")));
}

#[test]
fn wrong_recorded_length_or_hash_fails() {
    let model = sample();
    let printed = model.print();
    let bad_len = printed.replacen("\"bytes\": 21", "\"bytes\": 22", 1);
    assert_ne!(bad_len, printed);
    assert_eq!(
        load(bad_len.as_bytes()).unwrap_err().code(),
        Code::ProvSourceModelInvalid
    );
    let other = sha256(b"x");
    let bad_hash = printed.replacen(model.sha256().as_str(), other.as_str(), 1);
    assert_eq!(
        load(bad_hash.as_bytes()).unwrap_err().code(),
        Code::ProvSourceModelInvalid
    );
}

#[test]
fn shape_errors_have_input_locations() {
    let printed = sample().print();
    for bad in [
        printed.replacen("\"kind\": \"markdown\"", "\"kind\": \"pdf\"", 1),
        printed.replacen("\"version\": 1", "\"version\": 2", 1),
        printed.replacen(
            "\"format\": \"ruletrace-source-model\",",
            "\"extra\": 1,",
            1,
        ),
        "{".to_owned(),
        "[]".to_owned(),
    ] {
        let d = load(bad.as_bytes()).unwrap_err();
        assert_eq!(d.code(), Code::ProvSourceModelInvalid, "{bad}");
        assert!(matches!(d.primary(), Location::Input(_)), "{bad}");
    }
    let d = load(
        printed
            .replacen("\"kind\": \"markdown\"", "\"kind\": \"pdf\"", 1)
            .as_bytes(),
    )
    .unwrap_err();
    match d.primary() {
        Location::Input(i) => {
            assert_eq!(i.field.as_deref(), Some("kind"));
            assert!(i.line.is_some() && i.column.is_some());
        }
        other => panic!("unexpected location {other:?}"),
    }
}

#[test]
fn size_limit_is_reported_as_lim_0001() {
    let printed = sample().print();
    let d = SourceModel::load(
        &InputId::new("model.json").unwrap(),
        printed.as_bytes(),
        ParseLimits::new(10, 64),
    )
    .unwrap_err();
    assert_eq!(d.code(), Code::LimitFileTooLarge);
}

#[test]
fn resolve_returns_exact_bytes() {
    let model = sample();
    assert_eq!(model.text(8, 16), Ok("1. été"));
    assert_eq!(model.resolve(model.sha256(), 0, 7), Ok("# Title"));
    let span = SourceSpan::new(&model, 8, 16, 2, 1).unwrap();
    assert_eq!(model.span_text(&span), Some("1. été"));
    for e in model.elements() {
        assert!(model.text(e.start, e.end).is_ok());
    }
}

#[test]
fn resolve_failures_follow_the_explainer_order() {
    let model = sample();
    let other = sha256(b"other");
    assert_eq!(model.resolve(&other, 99, 3), Err(SpanFailure::HashMismatch));
    assert_eq!(model.verify_hash(&other), Err(SpanFailure::HashMismatch));
    assert_eq!(model.text(3, 3), Err(SpanFailure::OutOfRange));
    assert_eq!(model.text(5, 4), Err(SpanFailure::OutOfRange));
    assert_eq!(model.text(0, 22), Err(SpanFailure::OutOfRange));
    // Offset 12 is inside the two-byte é at 11..13.
    assert_eq!(model.text(8, 12), Err(SpanFailure::NotOnBoundary));
    assert_eq!(SpanFailure::HashMismatch.code(), Code::ProvHashMismatch);
    assert_eq!(SpanFailure::OutOfRange.code(), Code::ProvSpanOutOfRange);
    assert_eq!(
        SpanFailure::NotOnBoundary.code(),
        Code::ProvSpanNotOnBoundary
    );
}

#[test]
fn span_text_refuses_another_document() {
    let model = sample();
    let other = SourceModel::new(
        doc("other.md"),
        DocumentKind::Text,
        "abc".to_owned(),
        vec![],
    )
    .unwrap();
    let span = SourceSpan::new(&other, 0, 2, 1, 1).unwrap();
    assert_eq!(model.span_text(&span), None);
}

#[test]
fn line_and_column_handle_terminators_bom_and_scalars() {
    let text = "\u{feff}a\r\nb\rc\nd é";
    let m = SourceModel::new(doc("t"), DocumentKind::Text, text.to_owned(), vec![]).unwrap();
    // BOM is 3 bytes and one scalar; "a" is at column 2.
    assert_eq!(m.line_column(0), Some((1, 1)));
    assert_eq!(m.line_column(3), Some((1, 2)));
    assert_eq!(m.line_column(5), Some((1, 4))); // between CR and LF, still line 1
    assert_eq!(m.line_column(6), Some((2, 1)));
    assert_eq!(m.line_column(8), Some((3, 1)));
    assert_eq!(m.line_column(10), Some((4, 1)));
    assert_eq!(m.line_column(14), Some((4, 4))); // after "d é"
    assert_eq!(m.line_column(15), None);
    assert_eq!(m.line_column(1), None); // inside the BOM
    assert!(!m.is_char_boundary(1));
    assert_eq!(m.len_bytes(), 14);
}

#[test]
fn invalid_elements_are_refused() {
    let build = |elements: Vec<Element>| {
        SourceModel::new(doc("t"), DocumentKind::Text, "ab\ncd".to_owned(), elements)
    };
    let p = |id, s, e, l, c| element(id, ElementKind::Paragraph, s, e, l, c);
    assert!(build(vec![p(0, 0, 2, 1, 1), p(1, 3, 5, 2, 1)]).is_ok());
    assert_eq!(
        build(vec![p(1, 0, 2, 1, 1)]),
        Err(ModelError::IdNotSequential(0))
    );
    assert_eq!(
        build(vec![p(0, 3, 5, 2, 1), p(1, 0, 2, 1, 1)]),
        Err(ModelError::NotOrdered(1))
    );
    assert_eq!(
        build(vec![p(0, 2, 2, 1, 3)]),
        Err(ModelError::SpanOutOfRange(0))
    );
    assert_eq!(
        build(vec![p(0, 0, 6, 1, 1)]),
        Err(ModelError::SpanOutOfRange(0))
    );
    assert_eq!(
        build(vec![p(0, 3, 5, 1, 1)]),
        Err(ModelError::LineColumnMismatch(0))
    );
    let mut child = p(0, 0, 2, 1, 1);
    child.parent = Some(0);
    assert_eq!(build(vec![child]), Err(ModelError::BadParent(0)));
    let mut heading = element(0, ElementKind::Heading, 0, 2, 1, 1);
    heading.level = Some(7);
    assert_eq!(build(vec![heading]), Err(ModelError::BadLevel(0)));
    let mut para = p(0, 0, 2, 1, 1);
    para.level = Some(1);
    assert_eq!(build(vec![para]), Err(ModelError::BadLevel(0)));
    let split = SourceModel::new(
        doc("t"),
        DocumentKind::Text,
        "é".to_owned(),
        vec![p(0, 0, 1, 1, 1)],
    );
    assert_eq!(split, Err(ModelError::SpanNotOnBoundary(0)));
}

#[test]
fn empty_document_has_zero_elements_and_the_empty_hash() {
    let m = SourceModel::new(doc("e"), DocumentKind::Text, String::new(), vec![]).unwrap();
    assert_eq!(m.sha256(), &sha256(b""));
    assert_eq!(load(m.print().as_bytes()).unwrap(), m);
}

#[test]
fn candidate_clause_carries_a_verified_span_and_exact_text() {
    let model = sample();
    let c = CandidateClause::new(&model, 8, 16, Some("1.".to_owned())).unwrap();
    assert_eq!(c.text(), "1. été");
    assert_eq!(c.label(), Some("1."));
    assert_eq!((c.span().line(), c.span().column()), (2, 1));
    assert_eq!(
        CandidateClause::new(&model, 8, 8, None),
        Err(SpanError::EmptyOrReversed)
    );
    assert_eq!(
        CandidateClause::new(&model, 0, 99, None),
        Err(SpanError::OutOfBounds)
    );
    assert_eq!(
        CandidateClause::new(&model, 99, 100, None),
        Err(SpanError::OutOfBounds)
    );
    assert_eq!(
        CandidateClause::new(&model, 8, 12, None),
        Err(SpanError::NotOnBoundary)
    );
}

#[test]
fn source_models_reject_a_second_model_for_one_document() {
    let mut set = SourceModels::new();
    assert!(set.is_empty());
    set.insert(sample()).unwrap();
    let again = set.insert(sample());
    assert_eq!(again, Err(DuplicateDocument(doc("rules.md"))));
    assert_eq!(set.len(), 1);
    assert_eq!(set.get(&doc("rules.md")), Some(&sample()));
    assert_eq!(set.iter().count(), 1);
}

/// Every text up to four pieces from a mixed alphabet, with one paragraph per non-empty line:
/// the model prints, reloads to an equal model, and every element resolves to its own text.
#[test]
fn exhaustive_small_documents_round_trip_and_resolve() {
    let pieces = ["a", "é", "\n", "\r", "\r\n", "\u{feff}", " "];
    let mut count = 0u32;
    let mut stack: Vec<Vec<usize>> = vec![vec![]];
    while let Some(choice) = stack.pop() {
        let text: String = choice.iter().map(|&i| pieces[i]).collect();
        check_document(&text);
        count += 1;
        if choice.len() < 4 {
            for i in 0..pieces.len() {
                let mut next = choice.clone();
                next.push(i);
                stack.push(next);
            }
        }
    }
    assert!(count > 2000);
}

fn check_document(text: &str) {
    // Naive line/column used as the reference.
    let mut elements = Vec::new();
    let mut line = 1u32;
    let mut column = 1u32;
    let mut start: Option<(usize, u32, u32)> = None;
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    let flush = |start: &mut Option<(usize, u32, u32)>, end: usize, elements: &mut Vec<Element>| {
        if let Some((s, l, c)) = start.take() {
            let id = u32::try_from(elements.len()).unwrap();
            elements.push(element(
                id,
                ElementKind::Paragraph,
                u32::try_from(s).unwrap(),
                u32::try_from(end).unwrap(),
                l,
                c,
            ));
        }
    };
    while let Some(&(offset, ch)) = bytes.get(i) {
        match ch {
            '\r' | '\n' => {
                flush(&mut start, offset, &mut elements);
                if ch == '\r' && bytes.get(i + 1).map(|b| b.1) == Some('\n') {
                    i += 1;
                }
                line += 1;
                column = 1;
            }
            _ => {
                if start.is_none() {
                    start = Some((offset, line, column));
                }
                column += 1;
            }
        }
        i += 1;
    }
    flush(&mut start, text.len(), &mut elements);
    let model = SourceModel::new(doc("d"), DocumentKind::Text, text.to_owned(), elements).unwrap();
    let printed = model.print();
    assert_eq!(load(printed.as_bytes()).unwrap(), model, "{text:?}");
    for e in model.elements() {
        let slice = &text[e.start as usize..e.end as usize];
        assert_eq!(model.resolve(model.sha256(), e.start, e.end), Ok(slice));
        assert!(!slice.contains(['\r', '\n']));
    }
}
