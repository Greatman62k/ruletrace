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
mod cli;
mod command;
mod config;
mod consent;
mod credential;
mod decls;
mod dryrun;
mod endpoint;
mod fs;
mod inputs;
mod locate;
mod out;
mod perm;
mod plan;
pub mod provenance;
mod raw;
mod reader;
mod redact;
pub mod response;
mod revise;
mod run;
mod scan;
mod secret;
mod store;
mod store_check;
mod summary;
mod txlog;
mod vocab;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_accept;
#[cfg(test)]
mod tests_accept_credential;
#[cfg(test)]
mod tests_batch_property;
#[cfg(test)]
mod tests_cli_args;
#[cfg(test)]
mod tests_cli_canary;
#[cfg(test)]
mod tests_cli_order;
#[cfg(test)]
mod tests_cli_rig;
#[cfg(test)]
mod tests_cli_run;
#[cfg(test)]
mod tests_config_order;
#[cfg(test)]
mod tests_exit_order_property;
#[cfg(test)]
mod tests_gate_failures;
#[cfg(test)]
mod tests_gate_rig;
#[cfg(test)]
mod tests_identifier_property;
#[cfg(test)]
mod tests_log;
#[cfg(test)]
mod tests_memfs;
#[cfg(test)]
mod tests_out_canary;
#[cfg(test)]
mod tests_overredaction;
#[cfg(test)]
mod tests_plan;
#[cfg(test)]
mod tests_provenance;
#[cfg(test)]
mod tests_provenance_property;
#[cfg(test)]
mod tests_provider_link;
#[cfg(test)]
mod tests_redact;
#[cfg(test)]
mod tests_redact_fragments;
#[cfg(test)]
mod tests_redaction_property;
#[cfg(test)]
mod tests_response;
#[cfg(test)]
mod tests_response_closed;
#[cfg(test)]
mod tests_response_objects;
#[cfg(test)]
mod tests_revise;
#[cfg(test)]
mod tests_rig;
#[cfg(test)]
mod tests_run;
#[cfg(test)]
mod tests_run_canary;
#[cfg(test)]
mod tests_run_provider;
#[cfg(test)]
mod tests_run_support;
#[cfg(test)]
mod tests_scan;
#[cfg(test)]
mod tests_scratch;
#[cfg(test)]
mod tests_store;
#[cfg(test)]
mod tests_store_invalid;
#[cfg(test)]
mod tests_summary;

pub use accept::{accept_batch, AcceptInput, AcceptedBatch};
pub use cli::{parse, ProposeArgs, ReviseArgs};
pub use command::{run, run_with, BuiltInProviders, Providers};
pub use config::{
    disabled_without_configuration, load, load_config, AiConfig, Authorized, Endpoint, ProviderKind,
};
pub use consent::Consent;
pub use credential::{Credential, EnvReader, ProcessEnv};
pub use dryrun::{announcement, dry_run};
pub use fs::{StdFs, StoreFile, StoreFs};
pub use out::{render_diagnostic, Out};
pub use perm::{file_mode, permission_warning};
pub use plan::{
    build_plan, ClauseId, PlanInput, PlannedRequest, RunPlan, MAX_CLAUSE_BYTES, MAX_REQUEST_BYTES,
    TASK,
};
pub use redact::{Redactor, REDACTED};
pub use revise::{ReviseContext, MAX_FEEDBACK_BYTES};
pub use run::{run_requests, RunInput, RunReport};
pub use store::{
    already_stored_warning, batch_exists, batch_path, store_batch, FailureOutcome, StoreFailure,
    StoreMeta, StoreOutcome, StoreReport,
};
pub use store_check::check_store;
pub use summary::RunSummary;
pub use txlog::{FileLog, LogContext, LogSink, TransmissionLog};
pub use vocab::Ud32Option;
