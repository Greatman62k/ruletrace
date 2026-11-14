//! The Rule_IR data model (design §3.4.4, §4.1.2 to §4.1.5).
//!
//! The types come first. [`parse_rulebook`] reads a Rulebook file, [`print_pretty`] and
//! [`print_compact`] write the canonical forms, and the content hash (task 4.6) is added on top.
//!
//! * Numbers are [`rt_numeric::ExactNumber`]. There is no float type in the crate.
//! * Every expression node carries a declared [`Type`] (`None` only when the input omitted it,
//!   which the Validator reports). Nothing is inferred.
//! * A well-formed but unrecognized rule kind or expression node is carried as an
//!   [`OpaqueConstruct`] for the Validator (Req 4.9, `RT-VAL-0003`).
//! * Input positions ([`Loc`]) are kept for diagnostics and never take part in equality.
//! * [`ApprovalStatus`] is derived elsewhere from a Rulebook and a verified ledger. No type in
//!   this crate stores one, and no function here builds one from data (Req 3.14, 5.1).
#![forbid(unsafe_code)]

mod expr;
mod hash;
mod loc;
mod names;
mod parse;
mod print;
mod rule;
mod rulebook;
mod status;
mod types;

pub use expr::{
    AggFn, BinaryOp, CmpOp, Expr, ExprKind, Literal, OpaqueConstruct, Places, PlacesError, Rounding,
};
pub use hash::{draft_hash, rule_content_hash, ContentHashError};
pub use loc::Loc;
pub use names::{Date, DateError, Identity, IdentityError, Name, NameError, OpaqueName};
pub use parse::{parse_rulebook, DECODER_DEPTH_CAP};
pub use print::{print_compact, print_pretty};
pub use rule::{
    AiOrigin, DefaultValue, Direction, Marker, Origin, RuleBody, RuleContent, RuleKey,
    RuleRevision, Span, TieBreaker, Ties, WeightedTerm,
};
pub use rulebook::{
    DocumentDecl, IrVersion, Precedence, Rule, Rulebook, FORMAT_NAME, SUPPORTED_IR_VERSIONS,
};
pub use status::ApprovalStatus;
pub use types::{CollectionDecl, EnumDecl, Field, InputDecl, SetDecl, Type, TypeDecls};

#[cfg(test)]
mod hash_invariance_tests;
#[cfg(test)]
mod hash_sensitivity_tests;
#[cfg(test)]
mod hash_tests;
#[cfg(test)]
mod parse_tests;
#[cfg(test)]
mod roundtrip_tests;
#[cfg(test)]
mod tests;
