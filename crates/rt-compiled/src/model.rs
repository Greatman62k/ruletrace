//! The Compiled_Rulebook data model (design §4.1.6).
//!
//! The model holds Effective rules only. It stores no Origin, no approval data and no validity
//! flag (Req 22.6): there is no field that could. Names are resolved to indices, nodes live in
//! one arena per rule, and every node carries its resolved type.

use crate::index::{
    CollectionIdx, EnumIdx, FieldIdx, InputIdx, MemberIdx, NodeIdx, RuleIdx, SetIdx, TargetIdx,
};
use rt_foundation::{DocumentId, RevisionNumber, RuleId};
use rt_hash::Sha256Hex;
use rt_ir::{AggFn, BinaryOp, CmpOp, Date, Direction, IrVersion, Name, Rounding, Ties};
use rt_numeric::ExactNumber;
use std::collections::BTreeSet;

/// A resolved type. `Error` of the IR cannot occur in a compiled form, and an enum type names
/// its declaration by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CType {
    /// An exact number.
    Number,
    /// A boolean.
    Boolean,
    /// Text.
    Text,
    /// A calendar date.
    Date,
    /// A member of the enum at this index.
    Enum(EnumIdx),
    /// The result type of a ranking rule.
    Ranking,
}

/// A literal with resolved names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompiledLiteral {
    /// An exact number.
    Number(ExactNumber),
    /// A boolean.
    Boolean(bool),
    /// Text.
    Text(String),
    /// A date.
    Date(Date),
    /// A member of an enum.
    Enum {
        /// The enum.
        enumeration: EnumIdx,
        /// The member within it.
        member: MemberIdx,
    },
}

impl CompiledLiteral {
    /// The type of the literal.
    #[must_use]
    pub fn ty(&self) -> CType {
        match self {
            CompiledLiteral::Number(_) => CType::Number,
            CompiledLiteral::Boolean(_) => CType::Boolean,
            CompiledLiteral::Text(_) => CType::Text,
            CompiledLiteral::Date(_) => CType::Date,
            CompiledLiteral::Enum { enumeration, .. } => CType::Enum(*enumeration),
        }
    }
}

/// A Source_Document that a compiled rule references, with its content hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledDocument {
    /// The document identifier.
    pub id: DocumentId,
    /// The SHA-256 of the document as declared by the Rulebook.
    pub sha256: Sha256Hex,
}

/// A declared scalar input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledInput {
    /// The input name.
    pub name: Name,
    /// The declared type.
    pub ty: CType,
}

/// A field of a collection element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledField {
    /// The field name.
    pub name: Name,
    /// The declared type.
    pub ty: CType,
}

/// A declared collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledCollection {
    /// The collection name.
    pub name: Name,
    /// Its element fields in declared order.
    pub fields: Vec<CompiledField>,
}

/// A declared enum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledEnum {
    /// The enum name.
    pub name: Name,
    /// Its members in declared order.
    pub members: Vec<Name>,
}

/// A declared set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledSet {
    /// The set name.
    pub name: Name,
    /// The type of every member.
    pub element_type: CType,
    /// The members in declared order.
    pub members: Vec<CompiledLiteral>,
}

/// A Source_Span as retained from the Rulebook, with the hash of its document. It is plain
/// data: the Explainer verifies it against a Source_Model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledSpan {
    /// The document identifier.
    pub document: DocumentId,
    /// The hash the Compiled_Rulebook records for that document.
    pub sha256: Sha256Hex,
    /// Start byte offset.
    pub start: u32,
    /// End byte offset (exclusive).
    pub end: u32,
    /// Line of `start` (1-based).
    pub line: u32,
    /// Column of `start` (1-based).
    pub column: u32,
}

/// One node of a rule's arena. Children are indices of nodes that come earlier in the arena.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// A literal.
    Literal(CompiledLiteral),
    /// A declared scalar input.
    Field(InputIdx),
    /// The value Target of another rule.
    Ref(TargetIdx),
    /// Arithmetic.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// Left operand.
        left: NodeIdx,
        /// Right operand.
        right: NodeIdx,
    },
    /// Comparison.
    Cmp {
        /// The operator.
        op: CmpOp,
        /// Left operand.
        left: NodeIdx,
        /// Right operand.
        right: NodeIdx,
    },
    /// Conjunction of 2 to 64 operands.
    And(Vec<NodeIdx>),
    /// Disjunction of 2 to 64 operands.
    Or(Vec<NodeIdx>),
    /// Negation.
    Not(NodeIdx),
    /// Set membership.
    In {
        /// The tested value.
        value: NodeIdx,
        /// The declared set.
        set: SetIdx,
    },
    /// Aggregate over a collection of the participant.
    Agg {
        /// The aggregate function.
        func: AggFn,
        /// The collection.
        collection: CollectionIdx,
        /// The element field (`None` exactly for `count`).
        field: Option<FieldIdx>,
    },
    /// Declared rounding.
    Round {
        /// The rounded operand.
        arg: NodeIdx,
        /// The declared places and mode.
        rounding: Rounding,
    },
    /// The rank position of the participant under a ranking Target.
    RankReference(TargetIdx),
}

/// A node with its resolved type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledNode {
    /// The resolved type.
    pub ty: CType,
    /// The node.
    pub kind: NodeKind,
}

/// One term of a weighted score.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledTerm {
    /// The weight expression.
    pub weight: NodeIdx,
    /// The value expression.
    pub value: NodeIdx,
}

/// One tie-breaker of a ranking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledTieBreaker {
    /// The ordering expression.
    pub expr: NodeIdx,
    /// The direction.
    pub direction: Direction,
}

/// The lowered body of a rule. There is no unrecognized variant: such a rule cannot compile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompiledBody {
    /// A boolean Target per participant.
    Eligibility {
        /// Root of the expression.
        expr: NodeIdx,
    },
    /// A numeric Target per participant.
    Derived {
        /// Root of the expression.
        expr: NodeIdx,
    },
    /// Sum of weight times value with optional declared rounding of the sum.
    WeightedScore {
        /// The terms in declared order.
        terms: Vec<CompiledTerm>,
        /// The single declared rounding, if any.
        rounding: Option<Rounding>,
    },
    /// An ordered ranking.
    Ranking {
        /// Root of the key expression.
        key: NodeIdx,
        /// The key direction.
        direction: Direction,
        /// Tie-breakers in order.
        tie_breakers: Vec<CompiledTieBreaker>,
        /// Tie handling.
        ties: Ties,
    },
    /// A declared default for the input field named by the Target.
    Default {
        /// The default value.
        value: CompiledLiteral,
    },
}

impl CompiledBody {
    /// The kind name as written in the artifact.
    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        match self {
            CompiledBody::Eligibility { .. } => "eligibility",
            CompiledBody::Derived { .. } => "derived",
            CompiledBody::WeightedScore { .. } => "weighted_score",
            CompiledBody::Ranking { .. } => "ranking",
            CompiledBody::Default { .. } => "default",
        }
    }
}

/// One compiled rule: the Effective revision of a rule identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledRule {
    /// The rule identifier.
    pub id: RuleId,
    /// The Effective revision number.
    pub revision: RevisionNumber,
    /// The Target the rule defines.
    pub target: Name,
    /// The declared type.
    pub ty: CType,
    /// Every Source_Span of the revision, in order, at least one.
    pub spans: Vec<CompiledSpan>,
    /// The node arena.
    pub nodes: Vec<CompiledNode>,
    /// The body, referring to nodes by index.
    pub body: CompiledBody,
}

/// A Precedence_Declaration: a total order over the rules of one Target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledPrecedence {
    /// The declaration identifier.
    pub id: RuleId,
    /// The Target it orders.
    pub target: TargetIdx,
    /// The rules in order of precedence.
    pub order: Vec<RuleIdx>,
    /// The declaration's Source_Spans.
    pub spans: Vec<CompiledSpan>,
}

/// Everything a [`CompiledRulebook`] holds, before the checks of
/// [`CompiledRulebook::from_parts`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledParts {
    /// The Compiler version (build metadata, not a clock).
    pub compiler_version: String,
    /// The IR_Version of the source Rulebook.
    pub ir_version: IrVersion,
    /// The Rulebook content hash (design §3.6.2).
    pub rulebook_hash: Sha256Hex,
    /// The referenced documents, ascending by identifier.
    pub documents: Vec<CompiledDocument>,
    /// The input schema.
    pub inputs: Vec<CompiledInput>,
    /// The collection schema.
    pub collections: Vec<CompiledCollection>,
    /// The enums.
    pub enums: Vec<CompiledEnum>,
    /// The sets.
    pub sets: Vec<CompiledSet>,
    /// The evaluation order: every rule once.
    pub order: Vec<RuleIdx>,
    /// The rules, strictly ascending by identifier.
    pub rules: Vec<CompiledRule>,
    /// The precedence table, strictly ascending by identifier.
    pub precedence: Vec<CompiledPrecedence>,
}

/// An immutable, self-contained Compiled_Rulebook. It owns all of its data, has no interior
/// mutability and shares nothing with any other value (Req 9.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledRulebook {
    parts: CompiledParts,
    targets: Vec<Name>,
}

/// The distinct Target names of `rules` in ascending byte order. A [`TargetIdx`] is a position
/// in this list.
#[must_use]
pub fn targets_of(rules: &[CompiledRule]) -> Vec<Name> {
    let set: BTreeSet<&Name> = rules.iter().map(|r| &r.target).collect();
    set.into_iter().cloned().collect()
}

impl CompiledRulebook {
    /// Checks every structural invariant of `parts` and takes ownership.
    ///
    /// # Errors
    /// The first violated invariant, with its path in the artifact.
    pub fn from_parts(parts: CompiledParts) -> Result<Self, crate::CompiledError> {
        crate::check::check(&parts)?;
        let targets = targets_of(&parts.rules);
        Ok(Self { parts, targets })
    }

    /// The Compiler version.
    #[must_use]
    pub fn compiler_version(&self) -> &str {
        &self.parts.compiler_version
    }

    /// The IR_Version.
    #[must_use]
    pub fn ir_version(&self) -> IrVersion {
        self.parts.ir_version
    }

    /// The Rulebook content hash.
    #[must_use]
    pub fn rulebook_hash(&self) -> &Sha256Hex {
        &self.parts.rulebook_hash
    }

    /// The referenced documents with their hashes.
    #[must_use]
    pub fn documents(&self) -> &[CompiledDocument] {
        &self.parts.documents
    }

    /// The input schema.
    #[must_use]
    pub fn inputs(&self) -> &[CompiledInput] {
        &self.parts.inputs
    }

    /// The collection schema.
    #[must_use]
    pub fn collections(&self) -> &[CompiledCollection] {
        &self.parts.collections
    }

    /// The enums.
    #[must_use]
    pub fn enums(&self) -> &[CompiledEnum] {
        &self.parts.enums
    }

    /// The sets.
    #[must_use]
    pub fn sets(&self) -> &[CompiledSet] {
        &self.parts.sets
    }

    /// The evaluation order, every rule once.
    #[must_use]
    pub fn order(&self) -> &[RuleIdx] {
        &self.parts.order
    }

    /// The rules, ascending by identifier.
    #[must_use]
    pub fn rules(&self) -> &[CompiledRule] {
        &self.parts.rules
    }

    /// The rule at an index.
    #[must_use]
    pub fn rule(&self, idx: RuleIdx) -> Option<&CompiledRule> {
        self.parts.rules.get(idx.at())
    }

    /// The precedence table.
    #[must_use]
    pub fn precedence(&self) -> &[CompiledPrecedence] {
        &self.parts.precedence
    }

    /// The distinct Target names, ascending. A [`TargetIdx`] is a position in this list.
    #[must_use]
    pub fn targets(&self) -> &[Name] {
        &self.targets
    }

    /// The Target name at an index.
    #[must_use]
    pub fn target(&self, idx: TargetIdx) -> Option<&Name> {
        self.targets.get(idx.at())
    }
}
