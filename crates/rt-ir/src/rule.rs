//! Rule revisions, origins, rule kinds, markers and spans (design §3.4.4, §4.1.2, §4.1.3).

use crate::expr::{Expr, Literal, OpaqueConstruct, Rounding};
use crate::loc::Loc;
use crate::names::{Identity, Name};
use crate::types::Type;
use rt_foundation::{DocumentId, RevisionNumber, RuleId};
use rt_hash::Sha256Hex;

/// Rule identifier and revision. Ordered by identifier bytes, then revision (Req 3.11).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleKey {
    /// The rule.
    pub id: RuleId,
    /// The revision.
    pub revision: RevisionNumber,
}

/// A source span in Rule_IR form. It is plain data until the Validator checks it against a
/// Source_Model (Req 7.15). `line` and `column` are convenience values and are not part of the
/// Rule_Content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// Where the span was read.
    pub loc: Loc,
    /// Declared document.
    pub document: DocumentId,
    /// Start byte offset.
    pub start: u32,
    /// End byte offset (exclusive).
    pub end: u32,
    /// 1-based line of `start`, as declared.
    pub line: u32,
    /// 1-based column of `start`, as declared.
    pub column: u32,
}

/// Provenance of an AI-proposed revision (Req 25.2). Never part of the Rule_Content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiOrigin {
    /// The proposing identity.
    pub proposer: Identity,
    /// AI provider identifier.
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// AI adapter version.
    pub adapter_version: String,
    /// SHA-256 of the provider request.
    pub request_hash: Sha256Hex,
    /// SHA-256 of the provider response.
    pub response_hash: Sha256Hex,
    /// Proposal identifier.
    pub proposal_id: String,
    /// Identities that count as AI (they can never be a Reviewer).
    pub ai_identities: Vec<Identity>,
}

/// Where a revision came from. Provenance only: it never changes semantics or compiled output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Written by a person.
    Manual {
        /// Optional author identity.
        author: Option<Identity>,
    },
    /// Proposed by an AI provider.
    AiProposal(AiOrigin),
}

/// An ambiguity or incompleteness marker (UD-5). Counts (2 to 16 interpretations, 1 to 16
/// missing names, 1 to 1000 characters each) are checked by the Validator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Marker {
    /// The source text allows several readings. None is selected.
    Ambiguous {
        /// The readings, in recorded order.
        interpretations: Vec<String>,
    },
    /// The source text leaves elements out.
    Incomplete {
        /// The missing elements, in recorded order.
        missing: Vec<String>,
    },
}

/// Sort direction of a ranking key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    /// `asc`
    Asc,
    /// `desc`
    Desc,
}

impl Direction {
    /// The Rule_IR name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Direction::Asc => "asc",
            Direction::Desc => "desc",
        }
    }

    /// Parses a Rule_IR name.
    #[must_use]
    pub fn from_name(text: &str) -> Option<Self> {
        match text {
            "asc" => Some(Direction::Asc),
            "desc" => Some(Direction::Desc),
            _ => None,
        }
    }
}

/// Tie handling of a ranking. The first version has one policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ties {
    /// Equal keys form one Tie_Group (`"group"`).
    Group,
}

/// One `{weight, value}` term of a weighted score.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightedTerm {
    /// Where the term was read.
    pub loc: Loc,
    /// Weight expression.
    pub weight: Expr,
    /// Value expression.
    pub value: Expr,
}

/// One tie-breaker of a ranking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TieBreaker {
    /// Where it was read.
    pub loc: Loc,
    /// Key expression.
    pub expr: Expr,
    /// Direction.
    pub direction: Direction,
}

/// The declared default of a `default` rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultValue {
    /// Where it was read.
    pub loc: Loc,
    /// The constant.
    pub literal: Literal,
}

/// The five rule kinds (Req 14.1) and the carrier for any other well-formed kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleBody {
    /// `eligibility`: a Boolean Target per participant.
    Eligibility {
        /// The condition.
        expr: Expr,
    },
    /// `derived`: a numeric Target per participant.
    Derived {
        /// The value.
        expr: Expr,
    },
    /// `weighted_score`: the sum of weight times value, optionally rounded once.
    WeightedScore {
        /// Terms in declared order.
        terms: Vec<WeightedTerm>,
        /// One declared rounding of the sum.
        rounding: Option<Rounding>,
    },
    /// `ranking`: ordered ranking with explicit tie handling.
    Ranking {
        /// Primary key.
        key: Expr,
        /// Direction of the primary key.
        direction: Direction,
        /// Tie-breakers in declared order.
        tie_breakers: Vec<TieBreaker>,
        /// Tie policy.
        ties: Ties,
    },
    /// `default`: a declared default for the input field named by the Target.
    Default {
        /// The constant.
        value: DefaultValue,
    },
    /// A well-formed kind name this version does not define. It holds the fields after `type`.
    Unrecognized(OpaqueConstruct),
}

impl RuleBody {
    /// The `kind` string of the body.
    #[must_use]
    pub fn kind_name(&self) -> &str {
        match self {
            RuleBody::Eligibility { .. } => "eligibility",
            RuleBody::Derived { .. } => "derived",
            RuleBody::WeightedScore { .. } => "weighted_score",
            RuleBody::Ranking { .. } => "ranking",
            RuleBody::Default { .. } => "default",
            RuleBody::Unrecognized(o) => o.name.as_str(),
        }
    }
}

/// The Rule_Content of a revision (Req 25.4) apart from `id` and `revision`, which are in the
/// [`RuleKey`]: Target, declared type, body, markers and spans. It excludes the Origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleContent {
    /// The Target the rule decides or defines.
    pub target: Name,
    /// Declared `type`. `None` only when the input omitted it (the Validator reports it).
    pub ty: Option<Type>,
    /// Kind-specific fields.
    pub body: RuleBody,
    /// Ambiguity and incompleteness markers.
    pub markers: Vec<Marker>,
    /// Source spans. A valid rule has at least one.
    pub spans: Vec<Span>,
}

/// One revision of a rule. It has no approval field: the status is derived from the
/// Approval_Ledger (Req 5.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleRevision {
    /// Where the revision was read.
    pub loc: Loc,
    /// Identifier and revision number.
    pub key: RuleKey,
    /// Provenance.
    pub origin: Origin,
    /// The content that is hashed and approved.
    pub content: RuleContent,
}
