//! The credential and the only environment read of the tool (Req 28.5).

use crate::config::AiConfig;
use rt_foundation::{Code, Diagnostic, Location};

// The credential type and the environment read live in the integration
// crate (Req 28.5). The tool only maps a missing value to a Diagnostic.
pub use rt_ai_responses::{Credential, EnvReader, ProcessEnv};

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
    Credential::from_env(name, env).map_err(|_| missing("unset or empty"))
}
