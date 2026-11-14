//! Clause extraction (design §3.5, Extraction; Requirement 2).

use crate::label::leading_label;
use rt_foundation::{stage_result, Code, Diagnostic, DiagnosticState, Location, StageResult};
use rt_srcmodel::{CandidateClause, Element, ElementKind, SourceModel};

/// How clauses are chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtractionMode {
    /// One clause per list item, and per heading, paragraph or table row that
    /// starts with a numbering label.
    FirstVersion,
}

impl ExtractionMode {
    /// The first-version mode.
    #[must_use]
    pub const fn first_version() -> Self {
        ExtractionMode::FirstVersion
    }

    /// The name of the mode.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ExtractionMode::FirstVersion => "first_version",
        }
    }
}

/// Extracts the Candidate_Clauses of a Source_Model, in ascending start order.
///
/// The clause text is the exact span bytes. The Extractor creates no Rule,
/// assigns no approval state and does not interpret the text (Req 2.2, 2.4).
///
/// # Errors
/// `RT-ING-0004` when an element does not give a valid clause span. A
/// `SourceModel` is checked when it is built, so this indicates a defect and
/// gives no clauses (Req 2.7).
///
/// # Warnings
/// `RT-ING-0005` with the Document_Location of the whole document when no
/// clause is found.
pub fn extract(model: &SourceModel, mode: ExtractionMode) -> StageResult<Vec<CandidateClause>> {
    let ExtractionMode::FirstVersion = mode;
    let mut state = DiagnosticState::empty();
    let mut clauses = Vec::new();
    for element in model.elements() {
        let Some(label) = candidate_label(model, element) else {
            continue;
        };
        match CandidateClause::new(model, element.start, element.end, label) {
            Ok(clause) => clauses.push(clause),
            Err(e) => {
                state.push(Diagnostic::new(
                    Code::ExtractSourceModelInvalid,
                    format!(
                        "element {} of '{}' gives no valid clause span: {e}",
                        element.id,
                        model.document()
                    ),
                    Location::Document(model.document().clone()),
                ));
                return stage_result(Vec::new(), state);
            }
        }
    }
    if clauses.is_empty() {
        state.push(Diagnostic::new(
            Code::ExtractNoClauses,
            format!(
                "no numbered clause or list item found in '{}'",
                model.document()
            ),
            Location::Document(model.document().clone()),
        ));
    }
    stage_result(clauses, state)
}

/// `None` when the element is not a clause, otherwise `Some(label)` with the
/// label when it has one. List items are always clauses. A list item takes a
/// label at the start of its text (such as `1.`), otherwise the label after its
/// marker.
fn candidate_label(model: &SourceModel, element: &Element) -> Option<Option<String>> {
    let text = model
        .content()
        .get(element.start as usize..element.end as usize)?;
    match element.kind {
        ElementKind::ListItem => Some(
            leading_label(text)
                .map(str::to_owned)
                .or_else(|| element.label.clone()),
        ),
        ElementKind::Paragraph => leading_label(text).map(|l| Some(l.to_owned())),
        ElementKind::Heading => {
            let rest = text.trim_start_matches('#').trim_start_matches([' ', '\t']);
            leading_label(rest).map(|l| Some(l.to_owned()))
        }
        ElementKind::TableRow => leading_label(first_cell(text)).map(|l| Some(l.to_owned())),
        ElementKind::CodeBlock | ElementKind::Table => None,
    }
}

/// The text of the first cell of a pipe table row, from its first non-blank
/// byte. The cell ends at the next `|`, so a label must be followed by
/// whitespace inside the cell (as in `| 1. |`).
fn first_cell(row: &str) -> &str {
    let row = row.trim_start_matches([' ', '\t']);
    let row = row.strip_prefix('|').unwrap_or(row);
    let row = row.trim_start_matches([' ', '\t']);
    row.split('|').next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest;
    use rt_foundation::{DocumentId, InputId, MAX_SOURCE_DOCUMENT_BYTES};
    use rt_srcmodel::{default_load_limits, parse_candidates, print_candidates, CandidatesError};

    fn model(name: &str, text: &str) -> SourceModel {
        ingest(
            text.as_bytes(),
            &DocumentId::new(name).unwrap(),
            None,
            MAX_SOURCE_DOCUMENT_BYTES,
        )
        .unwrap()
    }

    fn run(m: &SourceModel) -> Vec<CandidateClause> {
        let staged = extract(m, ExtractionMode::first_version()).unwrap();
        assert!(staged.diagnostics().as_slice().is_empty());
        staged.into_parts().0
    }

    fn texts(c: &[CandidateClause]) -> Vec<&str> {
        c.iter().map(CandidateClause::text).collect()
    }

    #[test]
    fn three_labelled_clauses_in_offset_order_with_exact_text() {
        let m = model(
            "r.txt",
            "1. First\n\nintro\n\n1.2 Second\r\n\r\n(a) Third\nmore\n",
        );
        let c = run(&m);
        assert_eq!(texts(&c), ["1. First", "1.2 Second", "(a) Third\nmore"]);
        let labels: Vec<_> = c.iter().map(CandidateClause::label).collect();
        assert_eq!(labels, [Some("1."), Some("1.2"), Some("(a)")]);
        let starts: Vec<_> = c.iter().map(|x| x.span().start().get()).collect();
        assert!(starts.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(c[1].span().line(), 5);
    }

    #[test]
    fn list_items_always_and_unlabelled_text_never() {
        let m = model("r.md", "# Rules\n\n- plain item\n  - nested\n\nno label\n\n## A. Heading\n\nB) para\n\nAB. no\n");
        let c = run(&m);
        assert_eq!(
            texts(&c),
            ["- plain item", "- nested", "## A. Heading", "B) para"]
        );
        assert_eq!(c[0].label(), None);
        assert_eq!(c[2].label(), Some("A."));
    }

    #[test]
    fn table_rows_need_a_label_in_the_first_cell() {
        let m = model(
            "r.md",
            "| Place | Prize |\n|---|---|\n| 1. | 100 |\n| (b) x | 5 |\n| gold | 1. |\n",
        );
        let c = run(&m);
        assert_eq!(texts(&c), ["| 1. | 100 |", "| (b) x | 5 |"]);
        assert_eq!(c[0].label(), Some("1."));
    }

    #[test]
    fn code_blocks_and_label_without_whitespace_are_skipped() {
        let m = model("r.md", "```\n1. in code\n```\n\n1.5x text\n\n1.5\n");
        let staged = extract(&m, ExtractionMode::first_version()).unwrap();
        assert!(staged.value().is_empty());
    }

    #[test]
    fn zero_clauses_give_the_no_clauses_warning_for_the_document() {
        for text in ["", "just prose\n\nmore prose\n"] {
            let m = model("r.txt", text);
            let staged = extract(&m, ExtractionMode::first_version()).unwrap();
            assert!(staged.value().is_empty());
            let d = staged.diagnostics().as_slice();
            assert_eq!(d.len(), 1);
            assert_eq!(d[0].code(), Code::ExtractNoClauses);
            assert_eq!(d[0].primary(), &Location::Document(m.document().clone()));
            assert!(staged.diagnostics().no_errors());
        }
    }

    #[test]
    fn bom_and_nested_indentation_do_not_shift_spans() {
        let m = model("r.md", "\u{feff}1. A\n\n   (i) B\n");
        let c = run(&m);
        assert_eq!(texts(&c), ["1. A", "(i) B"]);
        assert_eq!(c[0].span().start().get(), 3);
    }

    fn input() -> InputId {
        InputId::new("c.json").unwrap()
    }

    #[test]
    fn candidates_file_round_trips_and_is_canonical() {
        let m = model("r.md", "1. É one\n\n- two \"q\"\n");
        let c = run(&m);
        let printed = print_candidates(m.document(), &c);
        assert_eq!(printed, print_candidates(m.document(), &c));
        assert!(printed.starts_with("{\n  \"format\": \"ruletrace-candidates\""));
        let back = parse_candidates(&input(), printed.as_bytes(), default_load_limits(), &m);
        assert_eq!(back.unwrap(), c);
        let empty = print_candidates(m.document(), &[]);
        assert_eq!(
            parse_candidates(&input(), empty.as_bytes(), default_load_limits(), &m).unwrap(),
            []
        );
    }

    #[test]
    fn candidates_file_is_checked_against_the_model() {
        let m = model("r.txt", "1. one\n\n2. two\n");
        let printed = print_candidates(m.document(), &run(&m));
        let bad = |text: String| match parse_candidates(
            &input(),
            text.as_bytes(),
            default_load_limits(),
            &m,
        ) {
            Err(CandidatesError::Invalid { field, .. }) => field,
            other => panic!("expected Invalid, got {other:?}"),
        };
        assert_eq!(
            bad(printed.replace("\"text\": \"2. two\"", "\"text\": \"2. tWo\"")).as_deref(),
            Some("clauses[1].text")
        );
        assert_eq!(
            bad(printed.replace("\"end\": 6", "\"end\": 99")).as_deref(),
            Some("clauses[0].start")
        );
        assert_eq!(
            bad(printed.replace("\"line\": 3", "\"line\": 4")).as_deref(),
            Some("clauses[1].line")
        );
        assert_eq!(
            bad(printed.replace("\"version\": 1", "\"version\": 2")).as_deref(),
            Some("version")
        );
        assert_eq!(
            bad(printed.replace(
                "\"document\": \"r.txt\",\n  \"clauses\"",
                "\"document\": \"o.txt\",\n  \"clauses\""
            ))
            .as_deref(),
            Some("document")
        );
        assert_eq!(bad("{".to_owned()), None);
        // Reversed order is refused.
        let a = printed.find("    {").unwrap();
        let b = printed.rfind("    {").unwrap();
        let end = printed.rfind("\n  ]").unwrap();
        let swapped = format!(
            "{}{},\n{}\n  ]\n}}\n",
            &printed[..a],
            printed[b..end].trim_end(),
            printed[a..b].trim_end().trim_end_matches(',')
        );
        assert_eq!(bad(swapped).as_deref(), Some("clauses[1]"));
    }
}
