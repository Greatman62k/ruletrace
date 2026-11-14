//! The Compiled_Rulebook: types, canonical serialization and a strict parser
//! (design §4.1.6).
//!
//! The crate holds data only. The Compiler builds a [`CompiledRulebook`] from a
//! `CompiledParts`, the engine reads one, and neither needs the other's logic.
//!
//! * A [`CompiledRulebook`] owns all of its data. It has no interior
//!   mutability, no global and no shared state, so two rulebooks cannot affect
//!   each other (Req 9.7).
//! * Names are resolved to indices. Each rule has an arena of nodes in which a
//!   child always has a lower index than its parent, so the arena is acyclic by
//!   construction. Types are resolved and no node carries an unrecognized
//!   construct.
//! * Every compiled rule keeps at least one Source_Span with the hash of its
//!   document (Req 9.3).
//! * No Origin, approval data or validity flag is stored: there is no field for
//!   one (Req 22.6), and [`parse_compiled`] rejects such keys with
//!   `RT-DGN-0001` to `0003`.
//! * Numbers are [`rt_numeric::ExactNumber`]; no floating-point type appears in
//!   any public item.
//! * Serialization is canonical RT-JSON, so `serialize(parse(serialize(x)))`
//!   equals `serialize(x)` byte for byte.
#![forbid(unsafe_code)]

mod check;
mod index;
mod model;
mod parse;
mod serial;

pub use check::{CompiledError, CompiledErrorKind};
pub use index::{
    CollectionIdx, EnumIdx, FieldIdx, InputIdx, MemberIdx, NodeIdx, RuleIdx, SetIdx, TargetIdx,
};
pub use model::{
    targets_of, CType, CompiledBody, CompiledCollection, CompiledDocument, CompiledEnum,
    CompiledField, CompiledInput, CompiledLiteral, CompiledNode, CompiledParts, CompiledPrecedence,
    CompiledRule, CompiledRulebook, CompiledSet, CompiledSpan, CompiledTerm, CompiledTieBreaker,
    NodeKind,
};
pub use parse::parse_compiled;
pub use serial::FORMAT_NAME;

#[cfg(test)]
mod tests;
