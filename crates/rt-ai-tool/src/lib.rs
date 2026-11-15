//! `ruletrace-ai` building blocks: configuration, consent, credential,
//! redaction and the Transmission_Log (class aux, Req 28).
//!
//! The crate depends on no compiler, engine, review or ledger crate. The
//! credential is read only through [`EnvReader`], after every configuration
//! and consent check has passed.
#![forbid(unsafe_code)]
// Diagnostic is the error type of every fallible operation (design 3.4).
#![allow(clippy::result_large_err)]

mod accept;
mod config;
mod consent;
mod credential;
mod decls;
mod dryrun;
mod endpoint;
mod locate;
mod out;
mod perm;
mod plan;
pub mod provenance;
mod raw;
mod reader;
mod redact;
pub mod response;
mod scan;
mod secret;
mod txlog;
mod vocab;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_accept;
#[cfg(test)]
mod tests_accept_credential;
#[cfg(test)]
mod tests_config_order;
#[cfg(test)]
mod tests_identifier_property;
#[cfg(test)]
mod tests_log;
#[cfg(test)]
mod tests_plan;
#[cfg(test)]
mod tests_provenance;
#[cfg(test)]
mod tests_provenance_property;
#[cfg(test)]
mod tests_redaction_property;
#[cfg(test)]
mod tests_response;
#[cfg(test)]
mod tests_response_closed;
#[cfg(test)]
mod tests_response_objects;
#[cfg(test)]
mod tests_scan;

pub use accept::{accept_batch, AcceptInput, AcceptedBatch};
pub use config::{
    disabled_without_configuration, load, AiConfig, Authorized, Endpoint, ProviderKind,
};
pub use consent::Consent;
pub use credential::{Credential, EnvReader, ProcessEnv};
pub use dryrun::{announcement, dry_run};
pub use out::{render_diagnostic, Out};
pub use perm::{file_mode, permission_warning};
pub use plan::{
    build_plan, ClauseId, PlanInput, PlannedRequest, RunPlan, MAX_CLAUSE_BYTES, MAX_REQUEST_BYTES,
    TASK,
};
pub use redact::{Redactor, REDACTED};
pub use txlog::{FileLog, LogContext, LogSink, TransmissionLog};
pub use vocab::Ud32Option;
