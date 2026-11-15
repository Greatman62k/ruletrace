//! `ruletrace-ai` building blocks: configuration, consent, credential,
//! redaction and the Transmission_Log (class aux, Req 28).
//!
//! The crate depends on no compiler, engine, review or ledger crate. The
//! credential is read only through [`EnvReader`], after every configuration
//! and consent check has passed.
#![forbid(unsafe_code)]
// Diagnostic is the error type of every fallible operation (design 3.4).
#![allow(clippy::result_large_err)]

mod config;
mod consent;
mod credential;
mod endpoint;
mod locate;
mod out;
mod perm;
mod raw;
mod reader;
mod redact;
mod scan;
mod secret;
mod txlog;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_log;
#[cfg(test)]
mod tests_scan;

pub use config::{
    disabled_without_configuration, load, AiConfig, Authorized, Endpoint, ProviderKind,
};
pub use consent::Consent;
pub use credential::{Credential, EnvReader, ProcessEnv};
pub use out::{render_diagnostic, Out};
pub use perm::{file_mode, permission_warning};
pub use redact::{Redactor, REDACTED};
pub use txlog::{FileLog, LogContext, LogSink, TransmissionLog};
