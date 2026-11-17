//! Reserved names (Req 22.22): a column header or a published `target` that
//! names a validity flag or a lifecycle flag is derived data, never input.

use super::csv::Cell;
use super::report::{shown, Reporter};
use super::Table;
use rt_foundation::Code;
use rt_json::{FLAG_KEYS, LIFECYCLE_KEYS};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Flag,
    Lifecycle,
}

fn classify(name: &str) -> Option<Kind> {
    if FLAG_KEYS.contains(&name) {
        Some(Kind::Flag)
    } else if LIFECYCLE_KEYS.contains(&name) {
        Some(Kind::Lifecycle)
    } else {
        None
    }
}

fn report_one(rep: &mut Reporter<'_>, cell: &Cell, field: &str, kind: Kind) {
    let (code, what) = match kind {
        Kind::Flag => (
            Code::DiagnosticStateContradiction,
            "a diagnostic-state flag",
        ),
        Kind::Lifecycle => (
            Code::LifecycleFlagContradiction,
            "a lifecycle flag or stored approval status",
        ),
    };
    let at = rep.cell(cell, field);
    let message = format!(
        "input supplies {what}, which is derived and never accepted: {}",
        shown(&cell.text)
    );
    rep.push(code, at, message);
}

/// Reports every reserved header name, one diagnostic per occurrence. The
/// first `has_errors` and the first `no_errors` together give one
/// `RT-DGN-0002` that names both. Returns whether anything was reported.
pub(super) fn check_header(table: &Table, rep: &mut Reporter<'_>) -> bool {
    let first = |name: &str| table.header.iter().position(|c| c.text == name);
    let pair = first("has_errors").zip(first("no_errors"));
    let mut found = false;
    if let Some((a, b)) = pair {
        let (lo, hi) = (a.min(b), a.max(b));
        if let (Some(primary), Some(other)) = (table.header.get(lo), table.header.get(hi)) {
            let at = rep.cell(primary, &primary.text);
            let related = rep.cell(other, &other.text);
            rep.push_related(
                Code::DiagnosticStateBothFlags,
                at,
                format!(
                    "input supplies both has_errors and no_errors, which are derived and never accepted: {}, {}",
                    primary.text, other.text
                ),
                related,
            );
            found = true;
        }
    }
    for (i, cell) in table.header.iter().enumerate() {
        if pair.is_some_and(|(a, b)| i == a || i == b) {
            continue;
        }
        if let Some(kind) = classify(&cell.text) {
            report_one(rep, cell, &cell.text, kind);
            found = true;
        }
    }
    found
}

/// Reports a cell whose text is a reserved name, under the column `field`.
/// Returns whether it was.
pub(super) fn check_cell(cell: &Cell, field: &str, rep: &mut Reporter<'_>) -> bool {
    match classify(&cell.text) {
        Some(kind) => {
            report_one(rep, cell, field, kind);
            true
        }
        None => false,
    }
}
