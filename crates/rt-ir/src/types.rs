//! Declared types and the `types` section of a Rulebook (design §4.1.2, §4.1.5).

use crate::expr::Literal;
use crate::loc::Loc;
use crate::names::Name;

/// A declared type (Req 14.3). `Ranking` is the type of a ranking rule. `Error` is internal to
/// the Validator: it is never produced from data.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    /// An Exact_Number.
    Number,
    /// A Boolean.
    Boolean,
    /// Text.
    Text,
    /// A date without time.
    Date,
    /// A member of the enumeration declared under this name.
    Enum(Name),
    /// The result of a ranking rule (internal).
    Ranking,
    /// The type of an erroneous expression (internal).
    Error,
}

impl Type {
    /// Parses a declared type text: `number`, `boolean`, `text`, `date`, `ranking` or
    /// `enum:<Name>`. The internal `error` type is never accepted.
    #[must_use]
    pub fn parse_declared(text: &str) -> Option<Type> {
        match text {
            "number" => Some(Type::Number),
            "boolean" => Some(Type::Boolean),
            "text" => Some(Type::Text),
            "date" => Some(Type::Date),
            "ranking" => Some(Type::Ranking),
            _ => text
                .strip_prefix("enum:")
                .and_then(|n| Name::new(n).ok())
                .map(Type::Enum),
        }
    }

    /// The declared text, the inverse of [`Type::parse_declared`] (`error` for the internal type).
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Type::Number => "number".to_owned(),
            Type::Boolean => "boolean".to_owned(),
            Type::Text => "text".to_owned(),
            Type::Date => "date".to_owned(),
            Type::Enum(n) => format!("enum:{n}"),
            Type::Ranking => "ranking".to_owned(),
            Type::Error => "error".to_owned(),
        }
    }
}

/// A declared input field of a participant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputDecl {
    /// Where it was declared.
    pub loc: Loc,
    /// Field name.
    pub name: Name,
    /// Declared type.
    pub ty: Type,
}

/// A declared enumeration. Members keep declared order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumDecl {
    /// Where it was declared.
    pub loc: Loc,
    /// Enumeration name.
    pub name: Name,
    /// Members in declared order.
    pub members: Vec<Name>,
}

/// A declared set. Members keep declared order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetDecl {
    /// Where it was declared.
    pub loc: Loc,
    /// Set name.
    pub name: Name,
    /// Declared type of every member.
    pub element_type: Type,
    /// Members in declared order.
    pub members: Vec<Literal>,
}

/// A field of a collection element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// Where it was declared.
    pub loc: Loc,
    /// Field name.
    pub name: Name,
    /// Declared type.
    pub ty: Type,
}

/// A declared collection of elements per participant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionDecl {
    /// Where it was declared.
    pub loc: Loc,
    /// Collection name.
    pub name: Name,
    /// Element fields in declared order.
    pub fields: Vec<Field>,
}

/// The `types` section: inputs, enums, sets and collections.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeDecls {
    /// Declared inputs.
    pub inputs: Vec<InputDecl>,
    /// Declared enumerations.
    pub enums: Vec<EnumDecl>,
    /// Declared sets.
    pub sets: Vec<SetDecl>,
    /// Declared collections.
    pub collections: Vec<CollectionDecl>,
}
