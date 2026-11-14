//! Default limits of design §4.1.5. A value equal to a limit is accepted; only
//! a strictly greater value is rejected (Req 19.9). The CLI may override the
//! byte and depth limits.

/// Source_Document size in bytes (16 MiB).
pub const MAX_SOURCE_DOCUMENT_BYTES: u64 = 16_777_216;
/// Source_Model file size in bytes (64 MiB).
pub const MAX_SOURCE_MODEL_BYTES: u64 = 64 * 1024 * 1024;
/// Rulebook file size in bytes (8 MiB).
pub const MAX_RULEBOOK_BYTES: u64 = 8 * 1024 * 1024;
/// Approval ledger file size in bytes (8 MiB).
pub const MAX_LEDGER_BYTES: u64 = 8 * 1024 * 1024;
/// Competition_Data file size in bytes (32 MiB).
pub const MAX_COMPETITION_DATA_BYTES: u64 = 32 * 1024 * 1024;
/// Proposal batch file size in bytes (16 MiB).
pub const MAX_PROPOSAL_BATCH_BYTES: u64 = 16 * 1024 * 1024;
/// AI_Configuration file size in bytes (64 KiB).
pub const MAX_AI_CONFIGURATION_BYTES: u64 = 64 * 1024;
/// Mock provider script size in bytes (4 MiB).
pub const MAX_MOCK_SCRIPT_BYTES: u64 = 4 * 1024 * 1024;

/// JSON container nesting depth for Rulebook, Source_Model and data.
pub const MAX_CONTAINER_DEPTH: u32 = 64;
/// JSON container nesting depth for provider responses.
pub const MAX_CONTAINER_DEPTH_PROVIDER_RESPONSE: u32 = 32;
/// Expression node depth, checked by the validator.
pub const MAX_EXPRESSION_DEPTH: u32 = 16;

/// Rules per Rulebook.
pub const MAX_RULES_PER_RULEBOOK: u32 = 10_000;
/// Revisions per Rulebook.
pub const MAX_REVISIONS_PER_RULEBOOK: u32 = 100_000;
/// Source spans per rule.
pub const MAX_SPANS_PER_RULE: u32 = 64;
/// Participants in one evaluation.
pub const MAX_PARTICIPANTS: u32 = 100_000;
/// Collection elements per participant.
pub const MAX_PER_PARTICIPANT_ELEMENTS: u32 = 100_000;
