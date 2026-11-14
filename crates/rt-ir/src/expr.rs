//! The expression AST of design §4.1.4.

use crate::loc::Loc;
use crate::names::{Date, Name, NameError, OpaqueName};
use crate::types::Type;
use rt_json::Value;
use rt_numeric::{ExactNumber, RoundingMode};
use std::fmt;

macro_rules! named_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name {
            $($(#[$vdoc])* $variant),+
        }

        impl $name {
            /// The Rule_IR name.
            #[must_use]
            pub fn name(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }

            /// Parses a Rule_IR name. Any other text gives `None`.
            #[must_use]
            pub fn from_name(text: &str) -> Option<Self> {
                match text { $($text => Some($name::$variant),)+ _ => None }
            }
        }
    };
}

named_enum!(
    /// Arithmetic operation of a `binary` node.
    BinaryOp {
        /// `add`
        Add => "add",
        /// `sub`
        Sub => "sub",
        /// `mul`
        Mul => "mul",
        /// `div`
        Div => "div",
    }
);

named_enum!(
    /// Comparison operation of a `cmp` node.
    CmpOp {
        /// `eq`
        Eq => "eq",
        /// `ne`
        Ne => "ne",
        /// `lt`
        Lt => "lt",
        /// `le`
        Le => "le",
        /// `gt`
        Gt => "gt",
        /// `ge`
        Ge => "ge",
    }
);

named_enum!(
    /// Aggregation function of an `agg` node.
    AggFn {
        /// `sum`
        Sum => "sum",
        /// `min`
        Min => "min",
        /// `max`
        Max => "max",
        /// `count`
        Count => "count",
    }
);

/// Rounding places, 0 to 28 (Req 8.14).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Places(u8);

/// Places outside 0 to 28.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacesError;

impl fmt::Display for PlacesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("rounding places outside 0 to 28")
    }
}

impl std::error::Error for PlacesError {}

impl Places {
    /// Builds the places.
    ///
    /// # Errors
    /// Returns [`PlacesError`] above 28.
    pub fn new(value: u64) -> Result<Self, PlacesError> {
        u8::try_from(value)
            .ok()
            .filter(|v| u32::from(*v) <= rt_numeric::limits::ROUNDING_MAX_PLACES)
            .map(Self)
            .ok_or(PlacesError)
    }

    /// The number of places.
    #[must_use]
    pub fn get(self) -> u32 {
        u32::from(self.0)
    }
}

/// A declared rounding: a mode and a number of places.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rounding {
    /// Rounding mode.
    pub mode: RoundingMode,
    /// Places after the decimal point.
    pub places: Places,
}

/// A typed constant: an expression leaf, a set member or a declared default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Literal {
    /// An Exact_Number.
    Number(ExactNumber),
    /// A Boolean.
    Boolean(bool),
    /// Text.
    Text(String),
    /// A date.
    Date(Date),
    /// A member of a declared enumeration.
    Enum {
        /// Enumeration name.
        enum_name: Name,
        /// Member name.
        member: Name,
    },
}

/// A well-formed construct this version does not define (rule kind or
/// expression node), carried for the Validator, which reports it under
/// `RT-VAL-0003`. It is never compiled.
///
/// `raw` is the compact canonical RT-JSON text of the subtree. Input positions
/// are not kept in it, so equality does not depend on layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueConstruct {
    /// The unrecognized kind or node name.
    pub name: OpaqueName,
    raw: String,
}

impl OpaqueConstruct {
    /// Carries the subtree `value` under `name`.
    #[must_use]
    pub fn new<P>(name: OpaqueName, value: &Value<P>) -> Self {
        Self {
            name,
            raw: rt_json::write_compact(value),
        }
    }

    /// Carries the subtree under a name given as text.
    ///
    /// # Errors
    /// Returns [`NameError::NotWellFormed`] when the name is not
    /// `[a-z][a-z0-9_]*`.
    pub fn from_text<P>(name: &str, value: &Value<P>) -> Result<Self, NameError> {
        Ok(Self::new(OpaqueName::new(name)?, value))
    }

    /// The compact canonical RT-JSON text of the subtree.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// One expression node: where it was read, its declared type, and its form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expr {
    /// Input position of the node.
    pub loc: Loc,
    /// The declared type `t`. `None` only when the input omitted it, which the
    /// Validator reports (Req 3.9). It is never inferred (Req 3.5).
    pub ty: Option<Type>,
    /// The form of the node.
    pub kind: ExprKind,
}

/// The node forms of design §4.1.4. Argument counts of `and` and `or` (2 to 64)
/// and the expression depth limit are checked by the Validator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    /// `num`, `bool`, `text`, `date` and `enum` leaves.
    Literal(Literal),
    /// `field`: a declared input, or a collection element field inside `agg`.
    Field {
        /// Field name.
        name: Name,
    },
    /// `ref`: the value Target of another rule.
    Ref {
        /// Referenced Target.
        target: Name,
    },
    /// `binary`.
    Binary {
        /// Operation.
        op: BinaryOp,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// `cmp`.
    Cmp {
        /// Operation.
        op: CmpOp,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// `and`.
    And(Vec<Expr>),
    /// `or`.
    Or(Vec<Expr>),
    /// `not`.
    Not(Box<Expr>),
    /// `in`: membership in a declared set.
    In {
        /// The tested value.
        value: Box<Expr>,
        /// Declared set name.
        set: Name,
    },
    /// `agg`: aggregation over a declared collection.
    Agg {
        /// Function.
        func: AggFn,
        /// Declared collection.
        collection: Name,
        /// Element field, `None` for `count`.
        field: Option<Name>,
    },
    /// `round`.
    Round {
        /// Rounded operand.
        arg: Box<Expr>,
        /// Places.
        places: Places,
        /// Mode.
        mode: RoundingMode,
    },
    /// `rank_reference` (UD-32). Where the option is not selected the Validator
    /// reports it.
    RankReference {
        /// Ranking Target.
        target: Name,
    },
    /// A well-formed node name this version does not define.
    Opaque(OpaqueConstruct),
}
