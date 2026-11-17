//! CSV dialect tests. Every expected position is counted by hand from the
//! literal text in the test.

use super::*;
use rt_foundation::{Code, DiagnosticState, InputId, Location};

pub(super) fn input() -> InputId {
    InputId::new("in/file.csv").unwrap()
}

pub(super) fn csv(text: &str) -> Result<Table, DiagnosticState> {
    parse_csv(text.as_bytes(), &input(), ByteLimit::DEFAULT)
}

/// `(code, line, column, field)` of the primary Input_Location of each
/// diagnostic, in the order of the state.
pub(super) type View = (Code, Option<u32>, Option<u32>, Option<String>);

pub(super) fn view(state: &DiagnosticState) -> Vec<View> {
    state
        .as_slice()
        .iter()
        .map(|d| match d.primary() {
            Location::Input(l) => {
                assert_eq!(l.input.as_str(), "in/file.csv");
                (d.code(), l.line, l.column, l.field.clone())
            }
            other => panic!("not an input location: {other:?}"),
        })
        .collect()
}

fn fail(text: &str) -> Vec<View> {
    view(&csv(text).unwrap_err())
}

fn one(text: &str, code: Code, line: u32, column: u32) {
    one_in(text, code, line, column, None);
}

fn one_in(text: &str, code: Code, line: u32, column: u32, field: Option<&str>) {
    assert_eq!(
        fail(text),
        vec![(code, Some(line), Some(column), field.map(str::to_owned))],
        "text {text:?}"
    );
}

fn cells(row: &[Cell]) -> Vec<(&str, u32, u32)> {
    row.iter()
        .map(|c| (c.text.as_str(), c.line, c.column))
        .collect()
}

#[test]
fn reads_header_and_rows_with_positions() {
    let t = csv("a,b\n1,2\n3,4\n").unwrap();
    assert_eq!(cells(&t.header), vec![("a", 1, 1), ("b", 1, 3)]);
    assert_eq!(t.rows.len(), 2);
    assert_eq!(cells(&t.rows[0]), vec![("1", 2, 1), ("2", 2, 3)]);
    assert_eq!(cells(&t.rows[1]), vec![("3", 3, 1), ("4", 3, 3)]);
    assert_eq!(t.input.as_str(), "in/file.csv");
}

#[test]
fn final_line_end_is_optional_and_a_header_alone_is_a_table() {
    let t = csv("a,b\n1,2").unwrap();
    assert_eq!(cells(&t.rows[0]), vec![("1", 2, 1), ("2", 2, 3)]);
    let t = csv("a,b").unwrap();
    assert!(t.rows.is_empty());
    assert_eq!(cells(&t.header), vec![("a", 1, 1), ("b", 1, 3)]);
}

#[test]
fn crlf_gives_the_same_texts_and_positions_as_lf() {
    let lf = csv("a,b\n1,2\n3,4\n").unwrap();
    let crlf = csv("a,b\r\n1,2\r\n3,4\r\n").unwrap();
    assert_eq!(lf.header, crlf.header);
    assert_eq!(lf.rows, crlf.rows);
}

#[test]
fn one_leading_bom_is_skipped_and_columns_count_after_it() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"a,b\n1,2\n");
    let t = parse_csv(&bytes, &input(), ByteLimit::DEFAULT).unwrap();
    assert_eq!(cells(&t.header), vec![("a", 1, 1), ("b", 1, 3)]);
    assert_eq!(cells(&t.rows[0]), vec![("1", 2, 1), ("2", 2, 3)]);
}

#[test]
fn a_second_bom_is_ordinary_text_in_the_first_cell() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"a,b\n");
    let t = parse_csv(&bytes, &input(), ByteLimit::DEFAULT).unwrap();
    assert_eq!(cells(&t.header), vec![("\u{feff}a", 1, 1), ("b", 1, 4)]);
}

#[test]
fn a_bom_in_the_middle_of_the_file_is_ordinary_text() {
    let t = csv("a,b\n\u{feff}1,2\n").unwrap();
    assert_eq!(cells(&t.rows[0]), vec![("\u{feff}1", 2, 1), ("2", 2, 4)]);
}

#[test]
fn quoted_cells_hold_separators_and_line_ends() {
    let t = csv("a,b\n\"x,y\",\"l1\nl2\"\nz,w\n").unwrap();
    assert_eq!(cells(&t.rows[0]), vec![("x,y", 2, 1), ("l1\nl2", 2, 7)]);
    assert_eq!(cells(&t.rows[1]), vec![("z", 4, 1), ("w", 4, 3)]);
}

#[test]
fn a_quoted_crlf_is_kept_and_counts_one_line() {
    let t = csv("a,b\r\n\"p\r\nq\",2\r\nz,w\r\n").unwrap();
    assert_eq!(cells(&t.rows[0]), vec![("p\r\nq", 2, 1), ("2", 3, 4)]);
    assert_eq!(cells(&t.rows[1]), vec![("z", 4, 1), ("w", 4, 3)]);
}

#[test]
fn a_doubled_quote_is_one_quote() {
    let t = csv("a,b\n\"say \"\"hi\"\"\",\"\"\"\"\n").unwrap();
    assert_eq!(t.rows[0][0].text, "say \"hi\"");
    assert_eq!(t.rows[0][1].text, "\"");
    assert_eq!(t.rows[0][1].column, 14);
}

#[test]
fn columns_count_unicode_scalar_values_not_bytes() {
    let t = csv("a,b\n\u{e9},x\n\u{e7}\u{e7},\u{f1}\n\u{1F600},y\ne\u{301},z\n").unwrap();
    assert_eq!(cells(&t.rows[0]), vec![("\u{e9}", 2, 1), ("x", 2, 3)]);
    assert_eq!(
        cells(&t.rows[1]),
        vec![("\u{e7}\u{e7}", 3, 1), ("\u{f1}", 3, 4)]
    );
    assert_eq!(cells(&t.rows[2]), vec![("\u{1F600}", 4, 1), ("y", 4, 3)]);
    assert_eq!(cells(&t.rows[3]), vec![("e\u{301}", 5, 1), ("z", 5, 4)]);
}

#[test]
fn a_position_after_a_multibyte_quoted_line_end_restarts_at_column_one() {
    let t = csv("a,b\n\"\u{e9}\n\u{e9}\u{e9}\",\u{e9}\n").unwrap();
    assert_eq!(
        cells(&t.rows[0]),
        vec![("\u{e9}\n\u{e9}\u{e9}", 2, 1), ("\u{e9}", 3, 5)]
    );
}

#[test]
fn cells_are_never_trimmed() {
    let t = csv(" a , b\n 1 ,\" 2 \"\n").unwrap();
    assert_eq!(cells(&t.header), vec![(" a ", 1, 1), (" b", 1, 5)]);
    assert_eq!(cells(&t.rows[0]), vec![(" 1 ", 2, 1), (" 2 ", 2, 5)]);
}

#[test]
fn a_hash_after_the_first_character_of_a_line_is_text() {
    let t = csv("a,b\n1,#2\n\"#x\",y\n").unwrap();
    assert_eq!(t.rows[0][1].text, "#2");
    assert_eq!(t.rows[1][0].text, "#x");
}

#[test]
fn an_empty_quoted_last_cell_is_allowed_and_empty() {
    let t = csv("a,b\n1,\"\"\n").unwrap();
    assert_eq!(cells(&t.rows[0]), vec![("1", 2, 1), ("", 2, 3)]);
}

#[test]
fn an_empty_interior_cell_is_allowed() {
    let t = csv("a,b,c\n1,,3\n").unwrap();
    assert_eq!(
        cells(&t.rows[0]),
        vec![("1", 2, 1), ("", 2, 3), ("3", 2, 4)]
    );
}

#[test]
fn backslashes_in_the_file_name_are_normalized() {
    let id = InputId::new("dir\\sub\\file.csv").unwrap();
    let t = parse_csv(b"a\n1\n", &id, ByteLimit::DEFAULT).unwrap();
    assert_eq!(t.input.as_str(), "dir/sub/file.csv");
    let e = parse_csv(b"a\n\n", &id, ByteLimit::DEFAULT).unwrap_err();
    match e.as_slice()[0].primary() {
        Location::Input(l) => assert_eq!(l.input.as_str(), "dir/sub/file.csv"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_blank_line_is_an_error_at_its_start() {
    one("a,b\n\n1,2\n", Code::AppInputMalformed, 2, 1);
    one("a,b\r\n\r\n1,2\r\n", Code::AppInputMalformed, 2, 1);
    one("a,b\n1,2\n\n", Code::AppInputMalformed, 3, 1);
    one("\na,b\n", Code::AppInputMalformed, 1, 1);
}

#[test]
fn a_comment_line_is_an_error() {
    one("a,b\n# note\n1,2\n", Code::AppInputMalformed, 2, 1);
    one("# first\na,b\n", Code::AppInputMalformed, 1, 1);
}

#[test]
fn a_trailing_separator_is_an_error_at_the_separator() {
    one("a,b\n1,2,\n", Code::AppInputMalformed, 2, 4);
    one("a,b,\n1,2\n", Code::AppInputMalformed, 1, 4);
    one("a,b\n1,2,", Code::AppInputMalformed, 2, 4);
    one("a,b\r\n1,\r\n", Code::AppInputMalformed, 2, 2);
}

#[test]
fn an_unterminated_quote_is_reported_at_the_opening_quote() {
    one("a,b\n\"x,y\n", Code::AppInputMalformed, 2, 1);
    one("a,b\n1,\"abc\ndef", Code::AppInputMalformed, 2, 3);
}

#[test]
fn a_quote_inside_an_unquoted_cell_is_an_error() {
    one("a,b\n1,x\"y\n", Code::AppInputMalformed, 2, 4);
}

#[test]
fn text_after_a_closing_quote_is_an_error() {
    one("a,b\n\"x\"y,2\n", Code::AppInputMalformed, 2, 4);
}

#[test]
fn a_lone_carriage_return_is_an_error() {
    one("a,b\n1,2\rx\n", Code::AppInputMalformed, 2, 4);
}

#[test]
fn a_row_with_too_many_cells_is_reported_at_the_first_extra_cell() {
    one("a,b\n1,2,3\n", Code::AppInputMalformed, 2, 5);
}

#[test]
fn a_row_with_too_few_cells_is_reported_at_its_end() {
    one_in("a,b\n1\n", Code::AppInputMalformed, 2, 2, Some("b"));
    one_in("a,b\n1", Code::AppInputMalformed, 2, 2, Some("b"));
    one_in("a,b,c\r\n1,2\r\n", Code::AppInputMalformed, 2, 4, Some("c"));
}

#[test]
fn invalid_utf8_is_reported_at_the_first_bad_byte() {
    let e = parse_csv(b"a,b\n1,\xFF\n", &input(), ByteLimit::DEFAULT).unwrap_err();
    assert_eq!(
        view(&e),
        vec![(Code::AppInputMalformed, Some(2), Some(3), None)]
    );
    let e = parse_csv(b"a,\xC3\n", &input(), ByteLimit::DEFAULT).unwrap_err();
    assert_eq!(
        view(&e),
        vec![(Code::AppInputMalformed, Some(1), Some(3), None)]
    );
    let truncated = "a,b\n\u{e9}".as_bytes();
    let e = parse_csv(
        &truncated[..truncated.len() - 1],
        &input(),
        ByteLimit::DEFAULT,
    )
    .unwrap_err();
    assert_eq!(
        view(&e),
        vec![(Code::AppInputMalformed, Some(2), Some(1), None)]
    );
}

#[test]
fn a_nul_character_is_an_error_at_its_position() {
    one("a,b\n1,\0\n", Code::AppInputMalformed, 2, 3);
    one("a,b\n1,\"x\0y\"\n", Code::AppInputMalformed, 2, 5);
}

#[test]
fn an_empty_file_or_a_bom_alone_has_no_header() {
    one("", Code::AppInputMalformed, 1, 1);
    let e = parse_csv(&[0xEF, 0xBB, 0xBF], &input(), ByteLimit::DEFAULT).unwrap_err();
    assert_eq!(
        view(&e),
        vec![(Code::AppInputMalformed, Some(1), Some(1), None)]
    );
}

#[test]
fn every_fault_of_the_file_is_reported_in_line_order() {
    let got = fail("a,b\n\n# c\n1,2,3\n1\n");
    let want: Vec<View> = vec![
        (Code::AppInputMalformed, Some(2), Some(1), None),
        (Code::AppInputMalformed, Some(3), Some(1), None),
        (Code::AppInputMalformed, Some(4), Some(5), None),
        (
            Code::AppInputMalformed,
            Some(5),
            Some(2),
            Some("b".to_owned()),
        ),
    ];
    assert_eq!(got, want);
}

#[test]
fn the_size_limit_is_inclusive() {
    let text = b"a,b\n1,2\n";
    assert_eq!(text.len(), 8);
    assert!(parse_csv(text, &input(), ByteLimit::new(8)).is_ok());
    let e = parse_csv(text, &input(), ByteLimit::new(7)).unwrap_err();
    assert_eq!(view(&e), vec![(Code::LimitFileTooLarge, None, None, None)]);
}

#[test]
fn the_default_size_limit_is_32_mebibytes() {
    assert_eq!(ByteLimit::DEFAULT.get(), 33_554_432);
    assert_eq!(ByteLimit::default(), ByteLimit::DEFAULT);
}

#[test]
fn the_row_limit_is_inclusive_at_100000_data_rows() {
    assert_eq!(MAX_DATA_ROWS, 100_000);
    let mut text = String::from("h\n");
    for _ in 0..100_000 {
        text.push_str("x\n");
    }
    let t = csv(&text).unwrap();
    assert_eq!(t.rows.len(), 100_000);
    text.push_str("x\n");
    let e = csv(&text).unwrap_err();
    assert_eq!(
        view(&e),
        vec![(Code::LimitCountExceeded, Some(100_002), Some(1), None)]
    );
}
