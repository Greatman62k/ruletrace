//! Source_Model and Candidate_Clause types, the canonical Source_Model codec,
//! and the pure span and hash verification helpers (design §3.5).
//!
//! The crate does no file, network, clock or environment access, and it does
//! not depend on the Ingestor or Extractor (Req 24.12). A [`SourceModel`] has
//! private fields and can only be built by [`SourceModel::new`] or
//! [`SourceModel::load`], so every value of the type has a content hash, a byte
//! length and element spans that were checked against its embedded text. It
//! implements `rt_foundation::SpanSource`, which is what `SourceSpan::new`
//! verifies against.
#![forbid(unsafe_code)]

mod candidate;
mod candidates_file;
mod codec;
mod model;
mod models;
mod verify;

pub use candidate::CandidateClause;
pub use candidates_file::{parse_candidates, print_candidates, CandidatesError};
pub use codec::default_load_limits;
pub use model::{DocumentKind, Element, ElementKind, ModelError, SourceModel};
pub use models::{DuplicateDocument, SourceModels};
pub use verify::SpanFailure;

#[cfg(test)]
mod tests;
