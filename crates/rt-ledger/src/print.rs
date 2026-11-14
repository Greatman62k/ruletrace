//! The canonical printer.

use crate::record::{record_line, ApprovalLedger, Parts, HEADER_LINE};

/// Prints a ledger: the header line, then one canonical compact record per
/// line, each line ended by a line feed. Printing the same ledger twice gives
/// identical bytes, and parsing the output gives an equal ledger (Req 26.20).
#[must_use]
pub fn print_ledger(ledger: &ApprovalLedger) -> String {
    let mut out = String::from(HEADER_LINE);
    out.push('\n');
    for record in ledger.records() {
        out.push_str(&record_line(&Parts::of(record)));
        out.push('\n');
    }
    out
}
