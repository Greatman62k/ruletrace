//! OpenAI Responses protocol integration (class aux, UD-37, Req 23.29).
//!
//! One crate serves the Meta and OpenAI Provider_Profiles. It builds the
//! request body, sends it over HTTPS, and maps the answer to the types of
//! `rt-ai-adapter`. It holds the only network client and the only credential
//! read of the tool. It knows no vocabulary and no Rule_IR: the schema
//! arrives as bytes from the caller.
//!
//! Every response is untrusted. Error bodies are never read, so nothing a
//! provider echoes back reaches a Diagnostic from this crate.
#![forbid(unsafe_code)]

mod adapter;
mod body;
mod catalog;
mod clock;
mod credential;
mod endpoint;
mod envelope;
mod status;
mod transport;

#[cfg(test)]
mod tests_body;
#[cfg(test)]
mod tests_catalog;
#[cfg(test)]
mod tests_credential;
#[cfg(test)]
mod tests_endpoint;
#[cfg(test)]
mod tests_envelope;
#[cfg(test)]
mod tests_status;

pub use adapter::{ResponsesAdapter, Settings, SettingsError, ADAPTER_VERSION, PROMPT_PROFILE};
pub use body::{INPUT_PREAMBLE, MIN_MAX_OUTPUT_TOKENS};
pub use clock::SystemClock;
pub use credential::{Credential, CredentialError, EnvReader, ProcessEnv};
pub use endpoint::{Endpoint, EndpointError};
