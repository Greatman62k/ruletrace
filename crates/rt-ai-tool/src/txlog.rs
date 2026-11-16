//! The Transmission_Log (Req 28.8): one JSON line before every attempt, with
//! no document text, no response text and no credential.

use crate::redact::Redactor;
use rt_ai_adapter::{AiAdapter, AttemptReport, ProviderRequest};
use rt_foundation::{Code, Diagnostic, DocumentId, Location};
use rt_hash::Sha256Hex;
use rt_json::{write_compact, Value};
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

/// What every entry of one run shares.
#[derive(Clone, Debug)]
pub struct LogContext {
    /// Provider kind.
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Host of the endpoint; empty for a provider with no endpoint.
    pub endpoint_host: String,
    /// Documents whose clause text the request carries, with their hashes.
    pub documents: Vec<(DocumentId, Sha256Hex)>,
    /// Number of Candidate_Clauses in the request.
    pub clauses: u32,
}

/// What a catalogue query shares with the log entries of its attempts.
#[derive(Clone, Debug)]
pub struct CatalogContext {
    /// Provider identifier.
    pub provider: String,
    /// Protocol of the provider.
    pub protocol: String,
    /// The configured model identifier.
    pub model: String,
    /// Host of the endpoint.
    pub endpoint_host: String,
}

/// Destination of log lines. Each call appends one complete line.
pub trait LogSink {
    /// Appends `line` (which ends in a line feed) durably.
    ///
    /// # Errors
    /// Any failure to write or flush.
    fn append(&mut self, line: &[u8]) -> io::Result<()>;
}

/// An append-only file.
#[derive(Clone, Debug)]
pub struct FileLog {
    path: PathBuf,
}

impl FileLog {
    /// A log at `path`; the file is created on the first append.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl LogSink for FileLog {
    fn append(&mut self, line: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(line)?;
        file.sync_data()
    }
}

/// The log of one run.
pub struct TransmissionLog<S: LogSink> {
    sink: S,
    redactor: Redactor,
    at: Location,
}

fn entry_line(
    context: &LogContext,
    request_bytes: u32,
    request_id: &str,
    attempt: u32,
    time: &str,
) -> Option<String> {
    let documents = context
        .documents
        .iter()
        .map(|(id, hash)| {
            Value::object(vec![
                ("document".to_owned(), Value::string(id.as_str())),
                ("sha256".to_owned(), Value::string(hash.as_str())),
            ])
            .ok()
        })
        .collect::<Option<Vec<_>>>()?;
    let object = Value::object(vec![
        (
            "provider".to_owned(),
            Value::string(context.provider.as_str()),
        ),
        ("model".to_owned(), Value::string(context.model.as_str())),
        (
            "endpoint_host".to_owned(),
            Value::string(context.endpoint_host.as_str()),
        ),
        ("documents".to_owned(), Value::array(documents)),
        ("clauses".to_owned(), Value::integer(context.clauses)),
        ("request_bytes".to_owned(), Value::integer(request_bytes)),
        ("request_id".to_owned(), Value::string(request_id)),
        ("attempt".to_owned(), Value::integer(attempt)),
        ("time".to_owned(), Value::string(time)),
    ])
    .ok()?;
    Some(write_compact(&object))
}

impl<S: LogSink> TransmissionLog<S> {
    /// A log over `sink`. `at` is the location of the `transmission_log`
    /// field of the configuration.
    pub fn new(sink: S, redactor: Redactor, at: Location) -> Self {
        Self { sink, redactor, at }
    }

    /// A log for a run that has no credential (the offline mock provider).
    pub fn without_credential(sink: S, at: Location) -> Self {
        Self::new(sink, Redactor::no_credential(), at)
    }

    fn failure(&self, why: &str) -> Diagnostic {
        Diagnostic::new(
            Code::AiTransmissionLogFailed,
            format!("the Transmission_Log entry was not written, so nothing is sent: {why}"),
            self.at.clone(),
        )
    }

    /// Appends the entry for one attempt.
    ///
    /// # Errors
    /// `RT-AI-0010` when the line cannot be built or appended.
    pub fn record(
        &mut self,
        context: &LogContext,
        request: &ProviderRequest,
        attempt: u32,
        time: &str,
    ) -> Result<(), Diagnostic> {
        let bytes = request
            .canonical_bytes()
            .and_then(|b| u32::try_from(b.len()).ok());
        let (Some(size), Some(id)) = (bytes, request.request_id()) else {
            return Err(self.failure("the request has no canonical bytes"));
        };
        let Some(line) = entry_line(context, size, id.as_str(), attempt, time) else {
            return Err(self.failure("the entry could not be built"));
        };
        let line = self.redactor.redact_fragments(&line);
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        self.sink
            .append(&bytes)
            .map_err(|e| self.failure(&format!("{:?}", e.kind())))
    }

    /// Appends the entry of one catalogue attempt: the provider identifier,
    /// the protocol, the configured model, the endpoint host, the request
    /// kind `models`, the attempt number and no document (Req 35.18).
    ///
    /// # Errors
    /// `RT-AI-0010` when the line cannot be built or appended.
    pub fn record_catalogue(
        &mut self,
        context: &CatalogContext,
        attempt: u32,
    ) -> Result<(), Diagnostic> {
        let object = Value::object(vec![
            (
                "provider".to_owned(),
                Value::string(context.provider.as_str()),
            ),
            (
                "protocol".to_owned(),
                Value::string(context.protocol.as_str()),
            ),
            ("model".to_owned(), Value::string(context.model.as_str())),
            (
                "endpoint_host".to_owned(),
                Value::string(context.endpoint_host.as_str()),
            ),
            ("kind".to_owned(), Value::string("models")),
            ("documents".to_owned(), Value::array(Vec::new())),
            ("attempt".to_owned(), Value::integer(attempt)),
        ])
        .map_err(|_| self.failure("the entry could not be built"))?;
        let line = self.redactor.redact_fragments(&write_compact(&object));
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        self.sink
            .append(&bytes)
            .map_err(|e| self.failure(&format!("{:?}", e.kind())))
    }

    /// Appends the entry, then makes the attempt. When the append fails the
    /// adapter is not called, so nothing is sent.
    ///
    /// # Errors
    /// `RT-AI-0010` from [`TransmissionLog::record`].
    pub fn attempt_logged(
        &mut self,
        adapter: &mut dyn AiAdapter,
        context: &LogContext,
        request: &ProviderRequest,
        timeout: Duration,
        attempt: u32,
        time: &str,
    ) -> Result<AttemptReport, Diagnostic> {
        self.record(context, request, attempt, time)?;
        Ok(adapter.attempt(request, timeout))
    }
}
