//! Plain data of a stored batch.

use rt_foundation::{Code, DocumentId, RuleId};
use rt_hash::{sha256, Sha256Hex};
use rt_ir::Identity;

/// The `format` string of a batch file.
pub const FORMAT_NAME: &str = "ruletrace-proposals";

/// The `version` number of a batch file.
pub const FORMAT_VERSION: u32 = 1;

/// How the stored provider response ended. Only a complete response passes
/// the acceptance checks, so no other value can be stored (Req 30.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    /// A complete, non-refusal response.
    Complete,
}

impl Completion {
    /// The `completion` text.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Completion::Complete => "complete",
        }
    }
}

/// The five proposal kinds (Req 18.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProposalKind {
    /// An interpretation of a clause.
    ClauseInterpretation,
    /// A suggested rule. The only kind that can become a Pending_Proposal.
    RuleSuggestion,
    /// A relationship between clauses.
    ClauseRelationship,
    /// A possible conflict.
    ConflictHighlight,
    /// A candidate test scenario.
    CandidateTest,
}

impl ProposalKind {
    /// The `kind` text.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ProposalKind::ClauseInterpretation => "clause_interpretation",
            ProposalKind::RuleSuggestion => "rule_suggestion",
            ProposalKind::ClauseRelationship => "clause_relationship",
            ProposalKind::ConflictHighlight => "conflict_highlight",
            ProposalKind::CandidateTest => "candidate_test",
        }
    }

    /// Parses a `kind` text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        [
            ProposalKind::ClauseInterpretation,
            ProposalKind::RuleSuggestion,
            ProposalKind::ClauseRelationship,
            ProposalKind::ConflictHighlight,
            ProposalKind::CandidateTest,
        ]
        .into_iter()
        .find(|k| k.as_str() == text)
    }
}

/// A source span cited by a proposal, with the quoted text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    /// The cited document.
    pub document: DocumentId,
    /// Its SHA-256 as the proposer saw it.
    pub sha256: Sha256Hex,
    /// Start byte offset.
    pub start: u32,
    /// End byte offset.
    pub end: u32,
    /// 1-based line of the start.
    pub line: u32,
    /// 1-based column of the start.
    pub column: u32,
    /// The quoted source text.
    pub quote: String,
}

/// A rule draft: Rule_Content without `revision` and `origin`, as canonical
/// compact RT-JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleDraft {
    /// The `id` member of the draft.
    pub id: RuleId,
    /// Canonical compact text of the whole draft object.
    pub json: String,
}

/// Where a stored Semantic_Outcome diagnostic points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoredLocation {
    /// A span of a document.
    Source {
        /// The document.
        document: DocumentId,
        /// Start byte offset.
        start: u32,
        /// End byte offset.
        end: u32,
    },
    /// A whole document.
    Document(DocumentId),
    /// A field path of the proposal, for example `rule.spans[0]`.
    Field(String),
}

/// One diagnostic of a Semantic_Outcome. Severity is derived from the code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredDiagnostic {
    /// Catalogue code.
    pub code: Code,
    /// Message.
    pub message: String,
    /// Location.
    pub location: StoredLocation,
}

/// One proposal of a batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    /// `SHA-256(request_id ":" index)`.
    pub proposal_id: Sha256Hex,
    /// The kind.
    pub kind: ProposalKind,
    /// The proposer identity.
    pub proposer: Identity,
    /// The proposer's rationale.
    pub rationale: String,
    /// Cited spans, at least one.
    pub evidence: Vec<Evidence>,
    /// The draft and its Draft hash. Present exactly for a `rule_suggestion`.
    pub rule: Option<(RuleDraft, Sha256Hex)>,
    /// Findings of the deterministic validator for this proposal.
    pub outcome: Vec<StoredDiagnostic>,
}

/// The accepted response, kept for audit and for the demonstration export
/// (UD-36).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredResponse {
    /// How it ended.
    pub completion: Completion,
    /// The exact payload text.
    pub payload: String,
}

/// One accepted batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    /// SHA-256 of the request bytes.
    pub request_id: Sha256Hex,
    /// SHA-256 of the payload bytes.
    pub response_sha256: Sha256Hex,
    /// The accepted response.
    pub response: StoredResponse,
    /// Provider kind.
    pub provider: String,
    /// Model name.
    pub model: String,
    /// Adapter version.
    pub adapter_version: String,
    /// AI identities from the configuration.
    pub ai_identities: Vec<Identity>,
    /// Caller-supplied creation time, stored verbatim.
    pub created_at: String,
    /// Proposals in response order.
    pub proposals: Vec<Proposal>,
}

/// The `proposal_id` of the proposal at `index` of a request.
#[must_use]
pub fn proposal_id(request_id: &Sha256Hex, index: usize) -> Sha256Hex {
    sha256(format!("{request_id}:{index}").as_bytes())
}
