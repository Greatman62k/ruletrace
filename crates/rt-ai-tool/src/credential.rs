//! The credential and the only environment read of the tool (Req 28.5).

use crate::config::AiConfig;
use rt_foundation::{Code, Diagnostic, Location};
use std::fmt;

/// Source of environment values. The tool reads through this trait so that
/// tests never change the process environment.
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

/// A non-empty credential value. `Debug` prints `[REDACTED]` and there is no
/// `Display`, so a format string cannot print the value by accident.
pub struct Credential(String);

impl Credential {
    /// Wraps a value, or `None` when it is empty.
    #[must_use]
    pub fn new(value: String) -> Option<Self> {
        (!value.is_empty()).then_some(Self(value))
    }

    /// The value, for the authentication field of a provider request only.
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

/// Reads the one variable that the configuration names.
///
/// # Errors
/// `RT-AI-0007` at the `credential_env_var` field when the variable is unset
/// or empty. The Diagnostic holds no value.
pub(crate) fn read_credential(
    config: &AiConfig,
    env: &dyn EnvReader,
) -> Result<Credential, Diagnostic> {
    let at = config
        .credential_location()
        .cloned()
        .unwrap_or(Location::None(Code::AiCredentialMissing));
    let missing = |why: &str| {
        Diagnostic::new(
            Code::AiCredentialMissing,
            format!("the credential environment variable is {why}"),
            at.clone(),
        )
    };
    let Some(name) = config.credential_env_var.as_deref() else {
        return Err(missing("not named"));
    };
    env.get(name)
        .and_then(Credential::new)
        .ok_or_else(|| missing("unset or empty"))
}
