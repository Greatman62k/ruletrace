//! Participant and Target names.

use std::fmt;

/// Longest name in Unicode scalar values (design §8.3).
const MAX_SCALARS: usize = 200;

/// Why a participant identifier or target name was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    /// The name is empty.
    Empty,
    /// The name is longer than 200 scalar values.
    TooLong,
    /// The name contains a control character.
    ControlCharacter,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NameError::Empty => "name is empty",
            NameError::TooLong => "name is longer than 200 scalar values",
            NameError::ControlCharacter => "name contains a control character",
        })
    }
}

impl std::error::Error for NameError {}

fn check(text: &str) -> Result<(), NameError> {
    if text.is_empty() {
        Err(NameError::Empty)
    } else if text.chars().any(char::is_control) {
        Err(NameError::ControlCharacter)
    } else if text.chars().count() > MAX_SCALARS {
        Err(NameError::TooLong)
    } else {
        Ok(())
    }
}

macro_rules! name {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Builds the name: 1 to 200 scalar values, no control character.
            ///
            /// # Errors Returns [`NameError`] for an empty, overlong or
            /// control-character name.
            pub fn new(text: impl Into<String>) -> Result<Self, NameError> {
                let text = text.into();
                check(&text)?;
                Ok(Self(text))
            }

            /// The name text.
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

name!(
    /// A participant identifier. Ordered bytewise.
    ParticipantId
);
name!(
    /// The name of a Target of the Compiled_Rulebook. Ordered bytewise.
    TargetName
);
