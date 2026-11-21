//! Header analysis shared by the three mappers.

use super::csv::Cell;
use super::report::{shown, Reporter};
use super::Table;
use rt_foundation::Code;
use std::collections::BTreeMap;

/// The header names by the index of their first occurrence.
pub(super) struct Header<'t> {
    pub(super) by_name: BTreeMap<&'t str, usize>,
}

/// Indexes the header. An empty or a repeated name is reported as malformed:
/// a column that cannot be told apart cannot be mapped.
pub(super) fn index<'t>(table: &'t Table, rep: &mut Reporter<'_>) -> Header<'t> {
    let mut by_name = BTreeMap::new();
    for (i, cell) in table.header.iter().enumerate() {
        if cell.text.is_empty() {
            let at = rep.at(cell.line, cell.column, None);
            rep.push(
                Code::AppInputMalformed,
                at,
                "a column name is empty".to_owned(),
            );
        } else if by_name.contains_key(cell.text.as_str()) {
            let at = rep.cell(cell, &cell.text);
            rep.push(
                Code::AppInputMalformed,
                at,
                format!("column `{}` is repeated", shown(&cell.text)),
            );
        } else {
            by_name.insert(cell.text.as_str(), i);
        }
    }
    Header { by_name }
}

/// Reports a required column or declared field that has no column, at the
/// first header cell with the missing name as `field`.
pub(super) fn missing(table: &Table, rep: &mut Reporter<'_>, name: &str) {
    let (line, column) = table.header.first().map_or((1, 1), |c| (c.line, c.column));
    let at = rep.at(line, column, Some(name));
    rep.push(
        Code::AppInputFieldMissing,
        at,
        format!("required column `{}` is absent", shown(name)),
    );
}

/// Reports an unknown column.
pub(super) fn unknown(rep: &mut Reporter<'_>, cell: &Cell) {
    let at = rep.cell(cell, &cell.text);
    rep.push(
        Code::AppInputMalformed,
        at,
        format!(
            "unknown column `{}` (name it with --ignore-column to skip it)",
            shown(&cell.text)
        ),
    );
}

/// True when the row has one cell per header cell. A row of another length
/// cannot come from [`parse_csv`](super::parse_csv); a hand-built one is
/// reported instead of read.
pub(super) fn row_fits(table: &Table, row: &[Cell], rep: &mut Reporter<'_>) -> bool {
    if row.len() == table.header.len() {
        return true;
    }
    let (line, column) = row.first().map_or((1, 1), |c| (c.line, c.column));
    let at = rep.at(line, column, None);
    rep.push(
        Code::AppInputMalformed,
        at,
        format!(
            "the row has {} cells, the header has {}",
            row.len(),
            table.header.len()
        ),
    );
    false
}

/// Reports a cell that is not valid for its declared type.
pub(super) fn invalid(rep: &mut Reporter<'_>, cell: &Cell, field: &str, why: &str) {
    let at = rep.cell(cell, field);
    rep.push(
        Code::AppInputValueInvalid,
        at,
        format!("{field}: `{}` is invalid: {why}", shown(&cell.text)),
    );
}
