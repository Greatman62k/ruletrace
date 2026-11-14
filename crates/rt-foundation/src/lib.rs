//! Foundation types: identifiers, locations, severity, limits and
//! control-character escaping.
//!
//! `Code` is generated from `catalogue/diagnostics.v1.json` by
//! `rt-xtask gen-catalogue`.
#![forbid(unsafe_code)]

mod code;
mod code_generated;
mod diagnostic;
mod escape;
mod exit;
mod ids;
mod limits;
mod location;
mod severity;
mod state;

pub use code_generated::Code;
pub use diagnostic::Diagnostic;
pub use escape::escape_controls;
pub use exit::ExitCode;
pub use ids::{DocumentId, IdError, InputId, RevisionNumber, RuleId, RuleRef};
pub use limits::{
    MAX_AI_CONFIGURATION_BYTES, MAX_COMPETITION_DATA_BYTES, MAX_CONTAINER_DEPTH,
    MAX_CONTAINER_DEPTH_PROVIDER_RESPONSE, MAX_EXPRESSION_DEPTH, MAX_LEDGER_BYTES,
    MAX_MOCK_SCRIPT_BYTES, MAX_PARTICIPANTS, MAX_PER_PARTICIPANT_ELEMENTS,
    MAX_PROPOSAL_BATCH_BYTES, MAX_REVISIONS_PER_RULEBOOK, MAX_RULEBOOK_BYTES,
    MAX_RULES_PER_RULEBOOK, MAX_SOURCE_DOCUMENT_BYTES, MAX_SOURCE_MODEL_BYTES, MAX_SPANS_PER_RULE,
};
pub use location::{
    ByteOffset, CliLocation, InputLocation, Location, SourceSpan, SpanError, SpanSource,
};
pub use severity::Severity;
pub use state::{stage_result, DiagnosticState, Rejected, StageResult, Staged, ValidationOutcome};
