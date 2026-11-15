//! Identifier newtypes. Identifiers are opaque strings and are never joined to
//! a path.

use std::fmt;

/// Why an identifier or revision number was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdError {
    /// The identifier is empty.
    Empty,
    /// The identifier contains a Unicode control character (`Cc`).
    ControlCharacter,
    /// A rule identifier is longer than 128 bytes.
    TooLong,
    /// A rule identifier does not start with `a` to `z`.
    BadFirstCharacter,
    /// A rule identifier contains a character outside `[a-z0-9_.-]`.
    BadCharacter,
    /// A revision number is outside 1 to 4,294,967,295.
    RevisionOutOfRange,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            IdError::Empty => "identifier is empty",
            IdError::ControlCharacter => "identifier contains a control character",
            IdError::TooLong => "rule identifier is longer than 128 bytes",
            IdError::BadFirstCharacter => {
                "rule identifier must start with a lowercase ASCII letter"
            }
            IdError::BadCharacter => "rule identifier may contain only a-z, 0-9, '_', '.' and '-'",
            IdError::RevisionOutOfRange => "revision number must be between 1 and 4294967295",
        };
        f.write_str(text)
    }
}

impl std::error::Error for IdError {}

fn check_opaque(text: &str) -> Result<(), IdError> {
    if text.is_empty() {
        Err(IdError::Empty)
    } else if text.chars().any(char::is_control) {
        Err(IdError::ControlCharacter)
    } else {
        Ok(())
    }
}

macro_rules! opaque_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Builds the identifier. It must be non-empty and contain no
            /// control character.
            ///
            /// # Errors
            /// Returns [`IdError`] when the text is empty or contains a control
            /// character.
            pub fn new(text: impl Into<String>) -> Result<Self, IdError> {
                let text = text.into();
                check_opaque(&text)?;
                Ok(Self(text))
            }

            /// The identifier text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

opaque_id!(
    /// Opaque UTF-8 identifier of a Source_Document (backslashes are normalised
    /// by the CLI). Ordered bytewise.
    DocumentId
);
opaque_id!(
    /// Opaque UTF-8 identifier of an input file. Ordered bytewise.
    InputId
);

/// Rule identifier matching `[a-z][a-z0-9_.-]{0,127}`. Ordered bytewise on
/// UTF-8, which for this ASCII alphabet is the derived `String` order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleId(String);

impl RuleId {
    /// Maximum length in bytes.
    pub const MAX_LEN: usize = 128;

    /// Builds a rule identifier.
    ///
    /// # Errors
    /// Returns [`IdError`] when the text does not match
    /// `[a-z][a-z0-9_.-]{0,127}`.
    pub fn new(text: impl Into<String>) -> Result<Self, IdError> {
        let text = text.into();
        let mut bytes = text.bytes();
        match bytes.next() {
            None => return Err(IdError::Empty),
            Some(b'a'..=b'z') => {}
            Some(_) => return Err(IdError::BadFirstCharacter),
        }
        if text.len() > Self::MAX_LEN {
            return Err(IdError::TooLong);
        }
        if bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-')) {
            Ok(Self(text))
        } else {
            Err(IdError::BadCharacter)
        }
    }

    /// The identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Revision number of a rule, 1 to 4,294,967,295.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RevisionNumber(u32);

impl RevisionNumber {
    /// The first revision of a rule.
    pub const FIRST: Self = Self(1);

    /// Builds a revision number.
    ///
    /// # Errors
    /// Returns [`IdError::RevisionOutOfRange`] for 0.
    pub fn new(value: u32) -> Result<Self, IdError> {
        if value == 0 {
            Err(IdError::RevisionOutOfRange)
        } else {
            Ok(Self(value))
        }
    }

    /// Builds a revision number from a wider integer, rejecting values outside
    /// the range.
    ///
    /// # Errors
    /// Returns [`IdError::RevisionOutOfRange`] for values below 1 or above
    /// `u32::MAX`.
    pub fn from_i128(value: i128) -> Result<Self, IdError> {
        u32::try_from(value)
            .map_err(|_| IdError::RevisionOutOfRange)
            .and_then(Self::new)
    }

    /// The numeric value.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for RevisionNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A reference to a rule, optionally to one revision. Ordered by rule id
/// bytewise, then revision (an absent revision sorts first).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleRef {
    /// The rule.
    pub rule: RuleId,
    /// The revision, when one is named.
    pub revision: Option<RevisionNumber>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_id_accepts_grammar() {
        for ok in ["a", "rule-1", "a.b_c-9", &format!("a{}", "z".repeat(127))] {
            assert!(RuleId::new(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn rule_id_rejects_invalid() {
        assert_eq!(RuleId::new(""), Err(IdError::Empty));
        assert_eq!(RuleId::new("1a"), Err(IdError::BadFirstCharacter));
        assert_eq!(RuleId::new("A"), Err(IdError::BadFirstCharacter));
        assert_eq!(RuleId::new("-a"), Err(IdError::BadFirstCharacter));
        assert_eq!(RuleId::new("aB"), Err(IdError::BadCharacter));
        assert_eq!(RuleId::new("a b"), Err(IdError::BadCharacter));
        assert_eq!(RuleId::new("aé"), Err(IdError::BadCharacter));
        assert_eq!(
            RuleId::new(format!("a{}", "z".repeat(128))),
            Err(IdError::TooLong)
        );
    }

    #[test]
    fn rule_id_order_is_bytewise() {
        let mut ids: Vec<RuleId> = ["b", "a-", "a.", "a0", "a_", "a"]
            .iter()
            .filter_map(|s| RuleId::new(*s).ok())
            .collect();
        ids.sort();
        let got: Vec<&str> = ids.iter().map(RuleId::as_str).collect();
        assert_eq!(got, ["a", "a-", "a.", "a0", "a_", "b"]);
    }

    #[test]
    fn revision_range() {
        assert_eq!(RevisionNumber::new(0), Err(IdError::RevisionOutOfRange));
        assert_eq!(RevisionNumber::new(1).map(RevisionNumber::get), Ok(1));
        assert_eq!(RevisionNumber::new(1), Ok(RevisionNumber::FIRST));
        assert_eq!(RevisionNumber::FIRST.get(), 1);
        assert_eq!(
            RevisionNumber::new(u32::MAX).map(RevisionNumber::get),
            Ok(u32::MAX)
        );
        assert_eq!(
            RevisionNumber::from_i128(4_294_967_296),
            Err(IdError::RevisionOutOfRange)
        );
        assert_eq!(
            RevisionNumber::from_i128(-1),
            Err(IdError::RevisionOutOfRange)
        );
        assert_eq!(
            RevisionNumber::from_i128(4_294_967_295).map(RevisionNumber::get),
            Ok(u32::MAX)
        );
    }

    #[test]
    fn opaque_ids_reject_empty_and_controls() {
        assert_eq!(DocumentId::new(""), Err(IdError::Empty));
        assert_eq!(DocumentId::new("a\u{1b}b"), Err(IdError::ControlCharacter));
        assert_eq!(InputId::new("x\n"), Err(IdError::ControlCharacter));
        assert_eq!(
            DocumentId::new("docs/rules.md").map(|d| d.to_string()),
            Ok("docs/rules.md".to_owned())
        );
    }

    #[test]
    fn rule_ref_orders_by_rule_then_revision() {
        let r = |id: &str, rev: Option<u32>| RuleRef {
            rule: RuleId::new(id).unwrap(),
            revision: rev.map(|v| RevisionNumber::new(v).unwrap()),
        };
        assert!(r("a", None) < r("a", Some(1)));
        assert!(r("a", Some(2)) < r("b", None));
    }
}
