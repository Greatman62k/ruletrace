//! The credential and the only environment read of the tool (Req 28.5).

use std::fmt;

/// Source of environment values, so that tests never change the process
/// environment.
pub trait EnvReader {
    /// The value of `name`, or `None` when it is unset or not valid UTF-8.
    fn get(&self, name: &str) -> Option<String>;
}

/// The process environment.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessEnv;

impl EnvReader for ProcessEnv {
    fn get(&self, name: &str) -> Option<String> {
        std::env::var_os(name).and_then(|v| v.into_string().ok())
    }
}

/// Why a credential could not be read. Carries no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialError {
    /// The variable is unset, not valid UTF-8, or empty.
    Missing,
    /// The value holds a character that cannot be sent in an HTTP header
    /// value: anything but visible ASCII.
    Invalid,
}

/// A non-empty credential value. `Debug` prints `[REDACTED]` and there is no
/// `Display`, so a format string cannot print the value by accident.
pub struct Credential(String);

impl Credential {
    /// Wraps a value, or `None` when it is empty.
    #[must_use]
    pub fn new(value: String) -> Option<Self> {
        (!value.is_empty()).then_some(Self(value))
    }

    /// Reads the one variable that the configuration names.
    ///
    /// # Errors
    /// [`CredentialError::Missing`] when it is unset or empty.
    /// [`CredentialError::Invalid`] when it holds anything but visible ASCII,
    /// which includes a trailing newline.
    pub fn from_env(name: &str, env: &dyn EnvReader) -> Result<Self, CredentialError> {
        let credential = env
            .get(name)
            .and_then(Self::new)
            .ok_or(CredentialError::Missing)?;
        if credential.0.bytes().all(|b| b.is_ascii_graphic()) {
            Ok(credential)
        } else {
            Err(CredentialError::Invalid)
        }
    }

    /// The value, for the authorization header and for building a redactor
    /// only.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credential([REDACTED])")
    }
}
