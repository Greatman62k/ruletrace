//! The hosted provider adapter of `ruletrace-ai` (task 10.5, UD-37).
//!
//! The configuration decides everything except the schema and the profile
//! constants, which are fixed data of this crate. The adapter receives its
//! own copy of the credential, so the integration crate is the only code
//! that puts it on the wire.

use crate::config::AiConfig;
use crate::credential::Credential;
use crate::proposal_schema::{proposal_schema_bytes, SCHEMA_NAME};
use crate::vocab::Ud32Option;
use rt_ai_adapter::{ProviderId, StructuredOutput};
use rt_ai_responses::{Credential as WireCredential, ResponsesAdapter, Settings};
use rt_foundation::{Code, Diagnostic};

fn internal(config: &AiConfig, what: &str) -> Diagnostic {
    Diagnostic::new(
        Code::InternalError,
        format!("the provider adapter could not be built: {what}"),
        config.provider_at.clone(),
    )
}

// Test builds of this crate must never open a connection to a real host.
// Every hosted test adapter is built here, so this is the one choke point.
#[cfg(test)]
fn assert_loopback_in_tests(url: &str) {
    let loopback = crate::endpoint::parse(url)
        .is_some_and(|parsed| crate::endpoint::is_loopback(&parsed.host));
    assert!(
        loopback,
        "a test built a provider adapter for a non-loopback endpoint; tests may use loopback stubs only"
    );
}

/// Builds the Responses adapter for a validated hosted configuration.
///
/// # Errors
/// An internal error only when the validated configuration contradicts the adapter
/// settings, which the configuration checks make unreachable.
pub fn responses_adapter(
    config: &AiConfig,
    credential: &Credential,
) -> Result<ResponsesAdapter, Diagnostic> {
    let endpoint = config
        .endpoint
        .as_ref()
        .ok_or_else(|| internal(config, "no endpoint"))?;
    #[cfg(test)]
    assert_loopback_in_tests(&endpoint.url);
    let provider =
        ProviderId::new(config.profile.id).map_err(|_| internal(config, "provider identifier"))?;
    let schema = proposal_schema_bytes(Ud32Option::CONFIRMED)
        .ok_or_else(|| internal(config, "no proposal schema"))?;
    let wire = WireCredential::new(credential.expose().to_owned())
        .ok_or_else(|| internal(config, "empty credential"))?;
    ResponsesAdapter::new(
        Settings {
            provider,
            endpoint: endpoint.url.clone(),
            structured_output: StructuredOutput {
                name: SCHEMA_NAME.to_owned(),
                schema,
            },
            max_output_tokens: config.profile.max_output_tokens,
        },
        wire,
    )
    .map_err(|_| internal(config, "settings"))
}
