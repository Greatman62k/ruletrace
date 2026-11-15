//! The RT-JSON value tree.
//!
//! One generic tree serves both directions. A parsed document is
//! `Value<Position>`: every value and every key carries its byte offset, line
//! and column. A document built in memory for the writers is `Value<()>`, which
//! carries no position and cannot pretend to have one.

use std::collections::BTreeSet;

/// The largest integer RT-JSON accepts (2^53 - 1).
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Where a value or key starts in the parsed input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    /// Byte offset from the start of the input (0-based).
    pub byte: u32,
    /// Line (1-based). Only a line feed ends a line.
    pub line: u32,
    /// Column (1-based), counted in Unicode scalar values.
    pub column: u32,
}

/// The shape of a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind<P> {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A non-negative integer literal up to [`MAX_SAFE_INTEGER`], written
    /// without sign, fraction or exponent.
    Integer(u64),
    /// Any other valid JSON number (negative, fraction, exponent, or above the
    /// integer bound), kept as its literal text. The parser never converts it
    /// to a number. A decoder that expects an Exact_Number reports it
    /// (RT-IR-0004) with this text.
    Number(String),
    /// A string, with escapes resolved.
    String(String),
    /// An array.
    Array(Vec<Value<P>>),
    /// An object. Members keep document order. Keys are unique, except in a
    /// tree from `parse_keeping_duplicates`.
    Object(Vec<Member<P>>),
}

/// One member of an object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member<P> {
    /// The key, with escapes resolved.
    pub key: String,
    /// Where the key starts.
    pub key_pos: P,
    /// The value.
    pub value: Value<P>,
}

/// A value and the position where it starts.
///
/// `Clone`, `Debug` and `PartialEq` follow the nesting and are therefore
/// recursive. Parsing, writing and dropping are iterative and safe at any
/// nesting depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value<P> {
    pos: P,
    kind: Kind<P>,
}

impl<P> Value<P> {
    pub(crate) fn new(pos: P, kind: Kind<P>) -> Self {
        Self { pos, kind }
    }

    /// Where the value starts (`()` for a value built in memory).
    #[must_use]
    pub fn pos(&self) -> &P {
        &self.pos
    }

    /// The shape of the value.
    #[must_use]
    pub fn kind(&self) -> &Kind<P> {
        &self.kind
    }

    /// The string content, if this is a string.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match &self.kind {
            Kind::String(s) => Some(s),
            _ => None,
        }
    }

    /// The integer, if this is an [`Kind::Integer`].
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match &self.kind {
            Kind::Integer(n) => Some(*n),
            _ => None,
        }
    }

    /// The members, if this is an object.
    #[must_use]
    pub fn as_object(&self) -> Option<&[Member<P>]> {
        match &self.kind {
            Kind::Object(m) => Some(m),
            _ => None,
        }
    }

    /// The items, if this is an array.
    #[must_use]
    pub fn as_array(&self) -> Option<&[Value<P>]> {
        match &self.kind {
            Kind::Array(a) => Some(a),
            _ => None,
        }
    }

    /// The member with the given key, if this is an object that has one.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Member<P>> {
        self.as_object()?.iter().find(|m| m.key == key)
    }
}

/// Two members of a built object have the same key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateKey(pub String);

impl std::fmt::Display for DuplicateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "duplicate object key {:?}", self.0)
    }
}

impl std::error::Error for DuplicateKey {}

/// Builders for values that the writers print. The order of object members is
/// the order the caller gives, which is how a writer's fixed per-object key
/// order is expressed.
impl Value<()> {
    /// `null`.
    #[must_use]
    pub fn null() -> Self {
        Self::new((), Kind::Null)
    }

    /// A boolean.
    #[must_use]
    pub fn boolean(value: bool) -> Self {
        Self::new((), Kind::Bool(value))
    }

    /// A non-negative integer.
    #[must_use]
    pub fn integer(value: u32) -> Self {
        Self::new((), Kind::Integer(u64::from(value)))
    }

    /// A string.
    #[must_use]
    pub fn string(value: impl Into<String>) -> Self {
        Self::new((), Kind::String(value.into()))
    }

    /// An array.
    #[must_use]
    pub fn array(items: Vec<Value<()>>) -> Self {
        Self::new((), Kind::Array(items))
    }

    /// An object with members in the given order.
    ///
    /// # Errors
    /// Returns the first repeated key, because a document with a repeated key
    /// does not parse.
    pub fn object(members: Vec<(String, Value<()>)>) -> Result<Self, DuplicateKey> {
        let mut seen = BTreeSet::new();
        for (key, _) in &members {
            if !seen.insert(key.as_str()) {
                return Err(DuplicateKey(key.clone()));
            }
        }
        let members = members
            .into_iter()
            .map(|(key, value)| Member {
                key,
                key_pos: (),
                value,
            })
            .collect();
        Ok(Self::new((), Kind::Object(members)))
    }
}

/// Moves the children of a container out, leaving it empty.
fn take_children<P>(kind: &mut Kind<P>) -> Option<Vec<Value<P>>> {
    match kind {
        Kind::Array(items) => Some(std::mem::take(items)),
        Kind::Object(members) => Some(
            std::mem::take(members)
                .into_iter()
                .map(|m| m.value)
                .collect(),
        ),
        _ => None,
    }
}

/// Dropping is iterative, so a tree as deep as an overridden depth limit allows
/// cannot overflow the stack.
impl<P> Drop for Value<P> {
    fn drop(&mut self) {
        let Some(mut work) = take_children(&mut self.kind) else {
            return;
        };
        while let Some(mut value) = work.pop() {
            if let Some(children) = take_children(&mut value.kind) {
                work.extend(children);
            }
        }
    }
}
