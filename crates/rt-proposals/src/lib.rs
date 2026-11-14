//! The Proposal_Store batch data model (design §3.7.5; UD-36, UD-26).
//!
//! * One file per accepted batch, `batches/<request_id>.json`, in canonical
//!   compact form followed by one line feed. [`parse_batch`] accepts only
//!   that form, so a parsed batch prints back to identical bytes.
//! * "Accepted" means the response passed the protocol, schema and
//!   provenance checks. It does not mean a Reviewer approved anything. A
//!   batch holds no approval or lifecycle data, and the decoder rejects
//!   flag and lifecycle keys with the `RT-DGN` codes.
//! * The accepted response is kept as `completion` and the exact `payload`.
//!   Any key outside the documented set is refused, so credentials,
//!   authorization headers and other transport data cannot be stored.
//! * A rule draft is carried as canonical RT-JSON with its identifier and
//!   Draft hash. The draft hash is computed by the proposal pipeline with
//!   `rt_ir::draft_hash`, which needs the declared documents of a Rulebook.
//! * The crate does no file access and has no write or append function. The
//!   `ruletrace` workflow never writes the store (design D-8).
#![forbid(unsafe_code)]

mod model;
mod parse;
mod pending;
mod print;

pub use model::{
    proposal_id, Batch, Completion, Evidence, Proposal, ProposalKind, RuleDraft, StoredDiagnostic,
    StoredLocation, StoredResponse, FORMAT_NAME, FORMAT_VERSION,
};
pub use parse::parse_batch;
pub use pending::{pending_proposals, PendingProposal};
pub use print::print_batch;

#[cfg(test)]
mod tests;
