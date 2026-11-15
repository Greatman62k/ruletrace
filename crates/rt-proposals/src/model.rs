//! Plain data of a stored batch.

use rt_foundation::{Code, DocumentId, RuleId};
use rt_hash::{sha256, Sha256Hex};
use rt_ir::Identity;

/// The `format` string of a batch file.
pub const FORMAT_NAME: &str = "ruletrace-proposals";

/// The first `version` number of a batch file. A batch of this version is
/// read and printed unchanged.
pub const FORMAT_VERSION_1: u32 = 1;

/// The `version` number of a batch file that new batches are written in
/// (UD-37, Req 35.16).
pub const FORMAT_VERSION_2: u32 = 2;

/// Token counts that the provider reported, each present only when the
/// provider gave it as a non-negative integer. Nothing is derived or summed.
/// At least one count is present: a report with none is no usage at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

impl StoredUsage {
    /// The usage with these counts, or `None` when all are absent.
    #[must_use]
    pub fn new(
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        total_tokens: Option<u64>,
    ) -> Option<Self> {
        let any = input_tokens.is_some() || output_tokens.is_some() || total_tokens.is_some();
        any.then_some(Self {
            input_tokens,
            output_tokens,
            total_tokens,
        })
    }

    /// Tokens of the request, if reported.
    #[must_use]
    pub fn input_tokens(&self) -> Option<u64> {
        self.input_tokens
    }

    /// Tokens of the answer, if reported.
    #[must_use]
    pub fn output_tokens(&self) -> Option<u64> {
        self.output_tokens
    }

    /// Total tokens, if reported.
    #[must_use]
    pub fn total_tokens(&self) -> Option<u64> {
        self.total_tokens
    }
}

/// The members that only a version 2 batch has, all optional (Req 35.16).
/// Reported values are untrusted audit data: no identifier, identity or
/// decision reads them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extras {
    /// The protocol of the provider.
    pub protocol: Option<String>,
    /// The model name that the provider reported, apart from `model`.
    pub reported_model: Option<String>,
    /// The response identifier that the provider reported.
    pub response_id: Option<String>,
    /// The token counts that the provider reported.
    pub usage: Option<StoredUsage>,
}

/// The format version of a batch file. A version 1 batch cannot hold a
/// version 2 member, so the pair is never inconsistent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Format {
    /// Version 1, as written before UD-37.
    V1,
    /// Version 2, with its optional members.
    V2(Extras),
}

impl Format {
    /// The `version` number.
    #[must_use]
    pub fn version(&self) -> u32 {
        match self {
            Format::V1 => FORMAT_VERSION_1,
            Format::V2(_) => FORMAT_VERSION_2,
        }
    }
}

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
    /// The format version and the members that only version 2 has.
    pub format: Format,
    /// SHA-256 of the request bytes.
    pub request_id: Sha256Hex,
    /// SHA-256 of the payload bytes.
    pub response_sha256: Sha256Hex,
    /// The accepted response.
    pub response: StoredResponse,
    /// The provider identifier in version 2, the provider kind in version 1.
    pub provider: String,
    /// The configured model name, never the reported one.
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
