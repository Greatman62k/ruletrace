//! The Approval_Ledger: types, JSON Lines parse and print, and hash-chain
//! verification (design §3.8.1, UD-27; Req 26).
//!
//! * Line 1 is the header `{"format":"ruletrace-ledger","version":1}`. Each
//!   further line is one record in canonical compact form. A header-only ledger
//!   is valid and empty.
//! * [`parse_ledger`] verifies each record in this order and stops at the first
//!   failing record: syntax and fields (`RT-APR-0101`), sequence (`0103`), hash
//!   chain (`0102`), reviewer validity (`0105`), contradictions (`0104`). Flag
//!   and lifecycle keys give the `RT-DGN` codes before any field is decoded. A
//!   failure is invalid input and no ledger is returned, so no status can be
//!   derived from it.
//! * `time` is stored verbatim and checked for shape only. No clock is read.
//! * This crate has **no function that appends or writes a record**. A record
//!   cannot be built outside this crate (its fields are private and it has no
//!   public constructor), and the only way to obtain one is to parse verified
//!   ledger bytes. The append operation belongs to `rt-review` (design §3.1).
//!
//! The hash chain detects edits of earlier records. It does not authenticate a
//! Reviewer and cannot detect a complete rewrite of the file by a person with
//! write access (R-8).
#![forbid(unsafe_code)]

mod derive;
mod parse;
mod print;
mod record;
mod reviewer;

pub use derive::{derive_statuses, effective_revisions, unresolved};
pub use parse::parse_ledger;
pub use print::print_ledger;
pub use record::{
    ApprovalBinding, ApprovalLedger, BindingError, BindingPair, LedgerRecord, RecordKind,
    HEADER_LINE, ZERO_HASH,
};
pub use reviewer::{AiIdentitySet, Reviewer};

#[cfg(test)]
mod tests;
