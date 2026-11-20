//! Property tests. Each runs at least 256 generated cases. Expected values
//! come from the generator's own model or from a separate reference
//! algorithm, never from the code under test.

use super::tests_csv::{input, view};
use super::tests_map::{base, kinds, requested, row_with, schema};
use super::*;
use crate::proptest_floor::property_tests;
use crate::{
    parse_data, parse_published, CompetitionData, DataLimits, DataValue, Participant,
    ParticipantId, PublishedNumber, PublishedResult, PublishedValue, TargetName,
};
use proptest::prelude::*;
use rt_foundation::{Code, InputId, Location};
use std::collections::BTreeMap;

fn cases(n: u32) -> ProptestConfig {
    ProptestConfig::with_cases(n)
}

fn pid(s: &str) -> ParticipantId {
    ParticipantId::new(s).unwrap()
}

fn tn(s: &str) -> TargetName {
    TargetName::new(s).unwrap()
}

fn byte_soup() -> impl Strategy<Value = Vec<u8>> {
    let hot: Vec<u8> = b",\"\n\r# a1\xEF\xBB\xBF\xC3\xA9\0e-.".to_vec();
    proptest::collection::vec(
        prop_oneof![any::<u8>(), proptest::sample::select(hot)],
        0..160,
    )
}

fn is_input_location(l: &Location) -> bool {
    matches!(l, Location::Input(_))
}

property_tests! {
    #![proptest_config(cases(512))]

    #[test]
    fn arbitrary_bytes_never_panic_and_every_failure_is_positioned(bytes in byte_soup()) {
        let id = input();
        match parse_csv(&bytes, &id, ByteLimit::DEFAULT) {
            Err(state) => {
                prop_assert!(!state.as_slice().is_empty());
                for d in state.as_slice() {
                    prop_assert!(is_input_location(d.primary()));
                    prop_assert_eq!(d.code(), Code::AppInputMalformed);
                }
            }
            Ok(table) => {
                for row in &table.rows {
                    prop_assert_eq!(row.len(), table.header.len());
                }
                let s = schema();
                let published = (
                    map_published(&table, &kinds(), &requested(&["eligible"]), &base(&["p1"])),
                    map_participants(&table, &s, &MappingOptions::default()),
                    map_collections(&table, &s, CompetitionData::default()),
                );
                for state in [published.0.err(), published.1.err(), published.2.err()]
                    .into_iter()
                    .flatten()
                {
                    prop_assert!(!state.as_slice().is_empty());
                    for d in state.as_slice() {
                        prop_assert!(is_input_location(d.primary()));
                    }
                }
            }
        }
    }
}

const GLYPHS: [char; 11] = [
    'a',
    '\u{e9}',
    '\u{1F600}',
    ',',
    '"',
    '\n',
    '\r',
    ' ',
    '#',
    '1',
    '\u{301}',
];

fn cell_text() -> impl Strategy<Value = String> {
    proptest::collection::vec(proptest::sample::select(GLYPHS.to_vec()), 0..6)
        .prop_map(|v| v.into_iter().collect())
}

type Grid = Vec<Vec<(String, bool)>>;

fn grid() -> impl Strategy<Value = Grid> {
    (1usize..4).prop_flat_map(|cols| {
        proptest::collection::vec(
            proptest::collection::vec((cell_text(), any::<bool>()), cols..=cols),
            1..6,
        )
    })
}

type Expected = Vec<Vec<(String, u32, u32)>>;

/// Renders a grid and counts the position of each cell as it writes, so the
/// expectation does not come from the parser.
fn render(grid: &Grid, crlf: bool, bom: bool, final_eol: bool) -> (Vec<u8>, Expected) {
    let mut out = String::new();
    let (mut line, mut col) = (1u32, 1u32);
    let mut expected = Vec::new();
    for (r, row) in grid.iter().enumerate() {
        let mut cells = Vec::new();
        for (i, (text, force)) in row.iter().enumerate() {
            cells.push((text.clone(), line, col));
            let last = i + 1 == row.len();
            let quote = *force
                || text.starts_with('#')
                || text.contains([',', '"', '\n', '\r'])
                || (text.is_empty() && last && (row.len() == 1 || r == 0));
            if quote {
                out.push('"');
                col += 1;
                for ch in text.chars() {
                    match ch {
                        '"' => {
                            out.push_str("\"\"");
                            col += 2;
                        }
                        '\n' => {
                            out.push('\n');
                            line += 1;
                            col = 1;
                        }
                        c => {
                            out.push(c);
                            col += 1;
                        }
                    }
                }
                out.push('"');
                col += 1;
            } else {
                out.push_str(text);
                col += u32::try_from(text.chars().count()).unwrap();
            }
            if !last {
                out.push(',');
                col += 1;
            }
        }
        if r + 1 < grid.len() || final_eol {
            out.push_str(if crlf { "\r\n" } else { "\n" });
            line += 1;
            col = 1;
        }
        expected.push(cells);
    }
    let mut bytes = Vec::new();
    if bom {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    bytes.extend_from_slice(out.as_bytes());
    (bytes, expected)
}

property_tests! {
    #![proptest_config(cases(512))]

    #[test]
    fn cell_texts_and_positions_match_a_reference_writer(
        g in grid(), crlf in any::<bool>(), bom in any::<bool>(), eol in any::<bool>()
    ) {
        let (bytes, expected) = render(&g, crlf, bom, eol);
        let table = parse_csv(&bytes, &input(), ByteLimit::DEFAULT)
            .map_err(|e| TestCaseError::fail(format!("{:?}", view(&e))))?;
        let got: Expected = std::iter::once(&table.header)
            .chain(table.rows.iter())
            .map(|row| row.iter().map(|c| (c.text.clone(), c.line, c.column)).collect())
            .collect();
        prop_assert_eq!(got, expected);
    }
}

fn scalar_prefix() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        proptest::sample::select(vec!['a', '\u{e9}', '\u{1F600}', '\u{4e16}', '\u{301}', 'z']),
        1..20,
    )
    .prop_map(|v| v.into_iter().collect())
}

property_tests! {
    #![proptest_config(cases(512))]

    #[test]
    fn a_bad_number_is_reported_at_the_scalar_column_after_any_prefix(
        id in scalar_prefix(),
        bad in proptest::sample::select(vec!["1e3", "+5", "NaN", "5 ", "0x1", "1,5x", "-", "1/2"]),
    ) {
        let s = InputSchema::new([("age".to_owned(), FieldType::Number)], []);
        let quoted = format!("\"{bad}\"");
        let cell = if bad.contains(',') { quoted.as_str() } else { bad };
        let file = format!("participant,age\n{id},{cell}\n");
        let t = parse_csv(file.as_bytes(), &input(), ByteLimit::DEFAULT).unwrap();
        let e = map_participants(&t, &s, &MappingOptions::default()).unwrap_err();
        let want_col = u32::try_from(id.chars().count()).unwrap() + 2;
        prop_assert_eq!(
            view(&e),
            vec![(
                Code::AppInputValueInvalid,
                Some(2),
                Some(want_col),
                Some("age".to_owned())
            )]
        );
    }
}

type Model = Vec<(String, Option<String>, Option<bool>, Option<String>)>;

fn model() -> impl Strategy<Value = Model> {
    let age = proptest::option::of(prop_oneof![
        Just("0".to_owned()),
        Just("-0.0".to_owned()),
        "-?[0-9]{1,8}(\\.[0-9]{1,8})?",
    ]);
    let note = proptest::option::of(
        proptest::collection::vec(
            proptest::sample::select(vec!['a', ' ', ',', '"', '\n', '\u{e9}', '0']),
            1..6,
        )
        .prop_map(|v| v.into_iter().collect::<String>()),
    );
    proptest::collection::btree_set("[a-z0-9]{1,6}", 0..6).prop_flat_map(move |ids| {
        let n = ids.len();
        (
            Just(ids),
            proptest::collection::vec(
                (
                    age.clone(),
                    proptest::option::of(any::<bool>()),
                    note.clone(),
                ),
                n..=n,
            ),
        )
            .prop_map(|(ids, vals)| {
                ids.into_iter()
                    .rev()
                    .zip(vals)
                    .map(|(id, (a, l, n))| (id, a, l, n))
                    .collect()
            })
    })
}

fn csv_quote(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) || text.is_empty() {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

property_tests! {
    #![proptest_config(cases(512))]

    #[test]
    fn a_missing_cell_is_never_a_value_and_an_explicit_value_is_kept(m in model()) {
        let mut file = String::from("participant,age,licence,note\n");
        for (id, age, licence, note) in &m {
            let l = licence.map(|b| b.to_string());
            file.push_str(&format!(
                "{id},{},{},{}\n",
                age.as_deref().map_or(String::new(), csv_quote),
                l.as_deref().map_or(String::new(), csv_quote),
                note.as_deref().map_or("\"\"".to_owned(), csv_quote),
            ));
        }
        let s = InputSchema::new(
            [
                ("age".to_owned(), FieldType::Number),
                ("licence".to_owned(), FieldType::Boolean),
                ("note".to_owned(), FieldType::Text),
            ],
            [],
        );
        let t = parse_csv(file.as_bytes(), &input(), ByteLimit::DEFAULT)
            .map_err(|e| TestCaseError::fail(format!("{:?}\n{file}", view(&e))))?;
        let d = map_participants(&t, &s, &MappingOptions::default())
            .map_err(|e| TestCaseError::fail(format!("{:?}\n{file}", view(&e))))?;
        prop_assert_eq!(d.participants().len(), m.len());
        let mut want = BTreeMap::new();
        for (id, age, licence, note) in &m {
            let p = d.participant(&pid(id)).unwrap();
            let mut fields = BTreeMap::new();
            match age {
                Some(a) => { fields.insert("age".to_owned(), DataValue::Text(a.clone())); }
                None => prop_assert_eq!(p.field("age"), None),
            }
            match licence {
                Some(b) => { fields.insert("licence".to_owned(), DataValue::Bool(*b)); }
                None => prop_assert_eq!(p.field("licence"), None),
            }
            match note {
                Some(n) => { fields.insert("note".to_owned(), DataValue::Text(n.clone())); }
                None => prop_assert_eq!(p.field("note"), None),
            }
            prop_assert_eq!(p.fields().len(), fields.len());
            want.insert(pid(id), Participant::new(fields, BTreeMap::new()));
        }
        let want = CompetitionData::new(want);
        prop_assert_eq!(&d, &want);
        let bytes = data_bytes(&d);
        let back = parse_data(&InputId::new("d.json").unwrap(), &bytes, DataLimits::default())
            .map_err(|e| TestCaseError::fail(format!("{:?}", e.as_slice())))?;
        prop_assert_eq!(&back, &d);
        prop_assert_eq!(data_bytes(&back), bytes);
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Hinnant's civil
/// calendar algorithm), used only as a reference for the date check.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn reference_valid(y: i64, m: i64, d: i64) -> bool {
    (1..=9999).contains(&y)
        && (1..=12).contains(&m)
        && (1..=31).contains(&d)
        && civil_from_days(days_from_civil(y, m, d)) == (y, m, d)
}

/// Dates where the leap rules matter: multiples of 4 and of 100 in February
/// and the end of other months, mixed with the full range.
fn date_parts() -> impl Strategy<Value = (i64, i64, i64)> {
    let wide = (0i64..=9999, 0i64..=14, 0i64..=33);
    let leap_years = (0i64..=2499, Just(2i64), 27i64..=30).prop_map(|(k, m, d)| (k * 4, m, d));
    let centuries = (0i64..=99, Just(2i64), 27i64..=30).prop_map(|(c, m, d)| (c * 100, m, d));
    let month_ends = (1i64..=9999, 1i64..=12, 28i64..=32);
    prop_oneof![wide, leap_years, centuries, month_ends]
}

property_tests! {
    #![proptest_config(cases(1024))]

    #[test]
    fn dates_are_accepted_exactly_when_a_reference_calendar_says_so(
        (y, m, d) in date_parts()
    ) {
        let cell = format!("{y:04}-{m:02}-{d:02}");
        let r = super::tests_map::parts(&row_with(2, &cell));
        prop_assert_eq!(r.is_ok(), reference_valid(y, m, d), "{}", cell);
    }
}

/// A header width, then which cells of the data row hold `x` (the others are
/// empty) and the line end style. Positions are counted while writing.
fn width_model() -> impl Strategy<Value = (usize, Vec<bool>, bool)> {
    (
        1usize..=5,
        proptest::collection::vec(any::<bool>(), 1..=7),
        any::<bool>(),
    )
}

property_tests! {
    #![proptest_config(cases(512))]

    #[test]
    fn a_row_is_kept_only_when_its_cell_count_equals_the_header_count(
        (width, filled, crlf) in width_model()
    ) {
        let names = ["a", "b", "c", "d", "e"];
        let eol = if crlf { "\r\n" } else { "\n" };
        let header = names[..width].join(",");
        let mut row = String::new();
        let mut starts = Vec::new();
        let mut col = 1u32;
        for (i, has_text) in filled.iter().enumerate() {
            starts.push(col);
            if i > 0 {
                row.push(',');
            }
            if *has_text {
                row.push('x');
                col += 1;
            }
            col += 1;
        }
        let lone_empty = filled.len() == 1 && !filled[0];
        if lone_empty {
            row = "\"\"".to_owned();
            col = 3;
        }
        let end = if lone_empty { col } else { col - 1 };
        let text = format!("{header}{eol}{row}{eol}");
        let got = parse_csv(text.as_bytes(), &input(), ByteLimit::DEFAULT);
        let cells = filled.len();
        if cells == width {
            let table = got.map_err(|e| TestCaseError::fail(format!("{:?}", view(&e))))?;
            prop_assert_eq!(table.rows.len(), 1);
            prop_assert_eq!(table.rows[0].len(), width);
            for (cell, has_text) in table.rows[0].iter().zip(&filled) {
                prop_assert_eq!(cell.text.as_str(), if *has_text { "x" } else { "" });
            }
        } else if cells > width {
            let want = vec![(Code::AppInputMalformed, Some(2), Some(starts[width]), None)];
            prop_assert_eq!(view(&got.unwrap_err()), want);
        } else {
            let field = Some(names[cells].to_owned());
            let want = vec![(Code::AppInputMalformed, Some(2), Some(end), field)];
            prop_assert_eq!(view(&got.unwrap_err()), want);
        }
    }
}

fn published_model() -> impl Strategy<Value = Vec<(String, u8, String)>> {
    proptest::collection::btree_map(
        ("[a-c]", 0u8..3),
        prop_oneof![
            Just("true".to_owned()),
            Just("false".to_owned()),
            "-?[0-9]{1,6}(\\.[0-9]{1,6})?",
            "[1-9][0-9]{0,3}",
        ],
        0..8,
    )
    .prop_map(|m| m.into_iter().map(|((p, t), v)| (p, t, v)).collect())
}

property_tests! {
    #![proptest_config(cases(512))]

    #[test]
    fn published_rows_map_to_the_expected_entries_and_round_trip_canonically(
        rows in published_model()
    ) {
        let names = ["eligible", "score", "standing"];
        let mut file = String::from("participant,target,value\n");
        let mut want = BTreeMap::new();
        for (p, t, v) in &rows {
            let target = names[usize::from(*t)];
            let is_bool = v == "true" || v == "false";
            let is_rank = !v.contains(['-', '.']) && !is_bool && v.chars().any(|c| c != '0');
            let fits = match target {
                "eligible" => is_bool,
                "score" => !is_bool,
                _ => is_rank,
            };
            if !fits {
                continue;
            }
            file.push_str(&format!("{p},{target},{v}\n"));
            let value = if is_bool {
                PublishedValue::Bool(v == "true")
            } else {
                PublishedValue::Number(PublishedNumber::parse(v).unwrap())
            };
            want.insert((pid(p), tn(target)), value);
        }
        let t = parse_csv(file.as_bytes(), &input(), ByteLimit::DEFAULT).unwrap();
        let m = map_published(&t, &kinds(), &requested(&names), &base(&["a", "b", "c"]))
            .map_err(|e| TestCaseError::fail(format!("{:?}\n{file}", view(&e))))?;
        prop_assert!(m.unused.is_empty());
        prop_assert_eq!(&m.result, &PublishedResult::new(want));
        let bytes = published_bytes(&m.result);
        let back = parse_published(
            &InputId::new("p.json").unwrap(),
            &bytes,
            crate::default_parse_limits(),
        )
        .map_err(|e| TestCaseError::fail(format!("{:?}", e.as_slice())))?;
        prop_assert_eq!(&back, &m.result);
        prop_assert_eq!(published_bytes(&back), bytes);
    }
}
