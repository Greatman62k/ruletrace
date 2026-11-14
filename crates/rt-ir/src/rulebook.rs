//! The Rulebook (design §4.1.2).

use crate::loc::Loc;
use crate::names::Name;
use crate::rule::{RuleKey, RuleRevision, Span};
use crate::types::TypeDecls;
use rt_foundation::{DocumentId, RuleId};
use rt_hash::Sha256Hex;

/// The `format` string of a Rulebook file.
pub const FORMAT_NAME: &str = "ruletrace-rulebook";

/// Every IR_Version this build supports, as written in `ir_version`.
pub const SUPPORTED_IR_VERSIONS: [&str; 1] = ["1"];

/// The IR_Version of a Rulebook. One per Rulebook, never per rule (Req 3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IrVersion {
    /// Version `"1"`.
    V1,
}

impl IrVersion {
    /// The `ir_version` string.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            IrVersion::V1 => "1",
        }
    }

    /// Parses an `ir_version` string. Unsupported text gives `None`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "1" => Some(IrVersion::V1),
            _ => None,
        }
    }
}

/// A declared source document and its SHA-256 (Req 3.12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentDecl {
    /// Where it was read.
    pub loc: Loc,
    /// Document identifier.
    pub id: DocumentId,
    /// Declared content hash.
    pub sha256: Sha256Hex,
}

/// A Precedence_Declaration: a total order over the rules of one Target (not a
/// rule kind).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Precedence {
    /// Where it was read.
    pub loc: Loc,
    /// Declaration identifier (same grammar as a rule identifier).
    pub id: RuleId,
    /// The Target the order applies to.
    pub target: Name,
    /// Rule identifiers, highest precedence first.
    pub order: Vec<RuleId>,
    /// Source spans that justify the declaration.
    pub spans: Vec<Span>,
}

/// A parsed Rulebook. The `revisions` keep document order. The parser rejects
/// duplicate (identifier, revision) pairs and the printer sorts them (Req 3.11,
/// 4.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rulebook {
    /// The single IR_Version.
    pub ir_version: IrVersion,
    /// Rulebook name.
    pub name: String,
    /// Rulebook version text.
    pub version: String,
    /// Declared documents.
    pub documents: Vec<DocumentDecl>,
    /// Declared inputs, enums, sets and collections.
    pub types: TypeDecls,
    /// Every rule revision, in document order.
    pub revisions: Vec<RuleRevision>,
    /// Precedence declarations.
    pub precedence: Vec<Precedence>,
}

/// A rule: all revisions that share one identifier, in ascending revision
/// order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule<'a> {
    /// The rule identifier.
    pub id: &'a RuleId,
    /// Its revisions, ascending.
    pub revisions: Vec<&'a RuleRevision>,
}

impl Rulebook {
    /// The revision with this key, if present.
    #[must_use]
    pub fn revision(&self, key: &RuleKey) -> Option<&RuleRevision> {
        self.revisions.iter().find(|r| &r.key == key)
    }

    /// The rules in ascending identifier order, each with its revisions in
    /// ascending order.
    #[must_use]
    pub fn rules(&self) -> Vec<Rule<'_>> {
        let mut sorted: Vec<&RuleRevision> = self.revisions.iter().collect();
        sorted.sort_by(|a, b| a.key.cmp(&b.key));
        let mut rules: Vec<Rule<'_>> = Vec::new();
        for rev in sorted {
            match rules.last_mut() {
                Some(rule) if rule.id == &rev.key.id => rule.revisions.push(rev),
                _ => rules.push(Rule {
                    id: &rev.key.id,
                    revisions: vec![rev],
                }),
            }
        }
        rules
    }
}
