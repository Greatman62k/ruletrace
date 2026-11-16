//! The model catalogue query (Req 35.18, 35.19; design 3.9.2).
//!
//! The query follows the call policy of Req 29: the same retry decision,
//! the same Warnings per transient attempt and a Transmission_Log entry
//! before every attempt. It sends the credential only. A failure is
//! `RT-AI-0109` and names the kind of failure.

use crate::config::AiConfig;
use crate::run::transient_code;
use crate::txlog::{CatalogContext, LogSink, TransmissionLog};
use rt_ai_adapter::{
    next_step, AttemptRecord, CatalogFailure, ModelCatalog, ProviderFailure, RetryStep, RetryStop,
};
use rt_foundation::{Code, Diagnostic};
use std::time::Duration;

/// A catalogue that was read, with the Warnings of failed attempts.
pub(crate) struct Queried {
    /// The identifiers as reported.
    pub models: Vec<String>,
    /// One Warning per transient attempt that failed.
    pub warnings: Vec<Diagnostic>,
}

fn failure_kind(failure: ProviderFailure) -> String {
    match failure {
        ProviderFailure::AuthFailed => "authentication failed".to_owned(),
        ProviderFailure::TlsFailed => "TLS failure".to_owned(),
        ProviderFailure::Timeout => "timeout".to_owned(),
        ProviderFailure::Unavailable => "provider unavailable".to_owned(),
        ProviderFailure::RateLimited { .. } => "rate limited".to_owned(),
        ProviderFailure::Other {
            http_status: Some(status),
        } => format!("HTTP status {status}"),
        ProviderFailure::Other { http_status: None } => "other failure".to_owned(),
    }
}

fn discovery_failed(config: &AiConfig, notes: Vec<Diagnostic>, why: &str) -> Vec<Diagnostic> {
    let mut found = notes;
    found.push(Diagnostic::new(
        Code::AiModelDiscoveryFailed,
        format!("the model catalogue could not be read: {why}"),
        config.provider_at.clone(),
    ));
    found
}

/// Queries the catalogue, with retries on transient failures only.
///
/// # Errors
/// The Warnings of failed attempts followed by one `RT-AI-0109`, or a
/// single `RT-AI-0010` when a log entry cannot be written (nothing is sent
/// for that attempt).
pub(crate) fn query_catalogue<S: LogSink>(
    config: &AiConfig,
    catalog: &mut dyn ModelCatalog,
    log: &mut TransmissionLog<S>,
    context: &CatalogContext,
) -> Result<Queried, Vec<Diagnostic>> {
    let policy = &config.policy;
    let started = catalog.clock().now();
    let mut history: Vec<AttemptRecord> = Vec::new();
    let mut notes: Vec<Diagnostic> = Vec::new();
    let mut last: Option<Code> = None;
    loop {
        let elapsed = catalog.clock().now().saturating_sub(started);
        match next_step(policy, &history, elapsed) {
            RetryStep::Attempt { wait_before } => {
                if wait_before > Duration::ZERO {
                    catalog.clock().wait(wait_before);
                }
                let attempt = history.len() + 1;
                let number = u32::try_from(attempt).unwrap_or(u32::MAX);
                log.record_catalogue(context, number).map_err(|d| vec![d])?;
                match catalog.list_models(policy.attempt_timeout(), config.limits.response_bytes) {
                    Ok(models) => {
                        return Ok(Queried {
                            models,
                            warnings: notes,
                        })
                    }
                    Err(CatalogFailure::Malformed(why)) => {
                        let why = format!("the answer is not a catalogue ({why})");
                        return Err(discovery_failed(config, notes, &why));
                    }
                    Err(CatalogFailure::TooLarge) => {
                        let why = "the answer is larger than the response limit";
                        return Err(discovery_failed(config, notes, why));
                    }
                    Err(CatalogFailure::Provider(failure)) => {
                        if failure.is_transient() {
                            let code = transient_code(failure);
                            last = Some(code);
                            notes.push(Diagnostic::new(
                                code,
                                format!("attempt {attempt} failed: {}", code.mnemonic()),
                                config.provider_at.clone(),
                            ));
                        }
                        history.push(AttemptRecord::Failure(failure));
                    }
                }
            }
            RetryStep::Stop(RetryStop::Success) => {
                return Err(discovery_failed(config, notes, "internal retry state"));
            }
            RetryStep::Stop(RetryStop::Exhausted { attempts }) => {
                let why = format!(
                    "{attempts} attempts failed; the last was {}",
                    last.map_or("none", Code::mnemonic)
                );
                return Err(discovery_failed(config, notes, &why));
            }
            RetryStep::Stop(RetryStop::DeadlineExceeded { attempts }) => {
                let why = format!(
                    "the deadline cannot fit another attempt after {attempts}; \
                     the last failure was {}",
                    last.map_or("none", Code::mnemonic)
                );
                return Err(discovery_failed(config, notes, &why));
            }
            RetryStep::Stop(RetryStop::NonTransient(failure)) => {
                return Err(discovery_failed(config, notes, &failure_kind(failure)));
            }
        }
    }
}

/// The `RT-AI-0109` for a provider that has no model catalogue.
pub(crate) fn no_catalogue(config: &AiConfig) -> Diagnostic {
    Diagnostic::new(
        Code::AiModelDiscoveryFailed,
        format!(
            "the provider '{}' has no model catalogue in this build",
            config.profile.id
        ),
        config.provider_at.clone(),
    )
}
