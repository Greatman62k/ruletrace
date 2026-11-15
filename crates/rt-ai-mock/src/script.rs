//! The `ruletrace-mock-script` format (design §3.7.10).

use crate::retry_after::{failure_from_http, parse_retry_after};
use crate::support::{invalid, members, text, ScriptError};
use rt_ai_adapter::{Completion, ProviderFailure};
use rt_foundation::InputId;
use rt_json::{check_flag_keys, parse, ParseLimits, Position, Value};
use std::time::Duration;

const MAX_SCRIPT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DEPTH: u32 = 8;
const MAX_RETRY_AFTER_SECS: u64 = 999_999_999;

/// What one step produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptResult {
    /// A complete response with its exact payload bytes.
    Response {
        /// How the provider ended the response.
        completion: Completion,
        /// The payload, exactly as scripted.
        payload: Vec<u8>,
    },
    /// A payload kept in a file. The test harness reads the file and calls
    /// [`MockScript::resolve_payload_files`]; the mock never reads files.
    PayloadFile {
        /// How the provider ended the response.
        completion: Completion,
        /// Relative path named by the script.
        path: String,
    },
    /// A failed attempt.
    Failure(ProviderFailure),
}

/// One scripted attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Simulated time the attempt takes.
    pub elapsed: Duration,
    /// What the attempt produces.
    pub result: ScriptResult,
}

/// A parsed step script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockScript {
    pub(crate) steps: Vec<Step>,
}

impl MockScript {
    /// A script from steps built in code.
    #[must_use]
    pub fn from_steps(steps: Vec<Step>) -> Self {
        Self { steps }
    }

    /// The steps in the order they are consumed.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Parses a `ruletrace-mock-script` version 1 document.
    ///
    /// # Errors
    /// [`ScriptError`] for bad JSON, a flag or lifecycle key, an unknown or
    /// missing field, or a value outside its range.
    pub fn parse(input: &InputId, bytes: &[u8]) -> Result<Self, ScriptError> {
        let limits = ParseLimits::new(MAX_SCRIPT_BYTES, MAX_DEPTH);
        let doc = parse(input, bytes, limits).map_err(ScriptError::Parse)?;
        check_flag_keys(input, &doc).map_err(ScriptError::FlagKeys)?;
        members(
            &doc,
            "",
            &["format", "version", "steps"],
            &["format", "version", "steps"],
        )?;
        if text(&doc, "format", "")? != "ruletrace-mock-script" {
            return Err(invalid(&doc, ".format", "expected ruletrace-mock-script"));
        }
        if doc.get("version").and_then(|m| m.value.as_u64()) != Some(1) {
            return Err(invalid(&doc, ".version", "expected version 1"));
        }
        let steps = doc.get("steps").map(|m| &m.value);
        let Some(list) = steps.and_then(Value::as_array) else {
            return Err(invalid(&doc, ".steps", "expected an array"));
        };
        let mut out = Vec::with_capacity(list.len());
        for (index, item) in list.iter().enumerate() {
            out.push(step(item, &format!("steps[{index}]"))?);
        }
        Ok(Self { steps: out })
    }

    /// Replaces each `PayloadFile` step with the text the harness read.
    ///
    /// # Errors
    /// The path of the first step whose file the resolver could not supply.
    pub fn resolve_payload_files(
        &mut self,
        mut read: impl FnMut(&str) -> Option<String>,
    ) -> Result<(), String> {
        for step in &mut self.steps {
            if let ScriptResult::PayloadFile { completion, path } = &step.result {
                let Some(text) = read(path) else {
                    return Err(path.clone());
                };
                step.result = ScriptResult::Response {
                    completion: *completion,
                    payload: text.into_bytes(),
                };
            }
        }
        Ok(())
    }
}

fn step(item: &Value<Position>, path: &str) -> Result<Step, ScriptError> {
    members(
        item,
        path,
        &["elapsed_ms", "result"],
        &["elapsed_ms", "result"],
    )?;
    let elapsed_path = format!("{path}.elapsed_ms");
    let elapsed = item.get("elapsed_ms").map(|m| &m.value);
    let Some(ms) = elapsed.and_then(Value::as_u64) else {
        return Err(invalid(item, &elapsed_path, "expected a whole number"));
    };
    let result = item.get("result").map(|m| &m.value);
    let Some(result) = result else {
        return Err(invalid(item, &format!("{path}.result"), "missing field"));
    };
    Ok(Step {
        elapsed: Duration::from_millis(ms),
        result: result_of(result, &format!("{path}.result"))?,
    })
}

fn result_of(value: &Value<Position>, path: &str) -> Result<ScriptResult, ScriptError> {
    members(
        value,
        path,
        &[
            "response",
            "failure",
            "retry_after_secs",
            "retry_after_header",
            "http_status",
        ],
        &[],
    )?;
    let response = value.get("response");
    let failure = value.get("failure");
    match (response, failure) {
        (Some(r), None) => {
            members(value, path, &["response"], &["response"])?;
            response_of(&r.value, &format!("{path}.response"))
        }
        (None, Some(_)) => failure_of(value, path).map(ScriptResult::Failure),
        _ => Err(invalid(
            value,
            path,
            "expected exactly one of response or failure",
        )),
    }
}

fn response_of(value: &Value<Position>, path: &str) -> Result<ScriptResult, ScriptError> {
    members(
        value,
        path,
        &["completion", "payload", "payload_file"],
        &["completion"],
    )?;
    let completion = match text(value, "completion", path)? {
        "complete" => Completion::Complete,
        "refused" => Completion::Refused,
        "incomplete" => Completion::Incomplete,
        _ => {
            return Err(invalid(
                value,
                &format!("{path}.completion"),
                "unknown completion",
            ))
        }
    };
    match (value.get("payload"), value.get("payload_file")) {
        (Some(p), None) => match p.value.as_str() {
            Some(s) => Ok(ScriptResult::Response {
                completion,
                payload: s.as_bytes().to_vec(),
            }),
            None => Err(invalid(
                &p.value,
                &format!("{path}.payload"),
                "expected a string",
            )),
        },
        (None, Some(f)) => match f.value.as_str() {
            Some(s) => Ok(ScriptResult::PayloadFile {
                completion,
                path: s.to_owned(),
            }),
            None => Err(invalid(
                &f.value,
                &format!("{path}.payload_file"),
                "expected a string",
            )),
        },
        _ => Err(invalid(
            value,
            path,
            "expected exactly one of payload or payload_file",
        )),
    }
}

fn failure_of(value: &Value<Position>, path: &str) -> Result<ProviderFailure, ScriptError> {
    let kind = text(value, "failure", path)?;
    let extras: &[&str] = match kind {
        "rate_limited" => &["retry_after_secs", "retry_after_header"],
        "other" => &["http_status", "retry_after_header"],
        _ => &[],
    };
    let mut allowed = vec!["failure"];
    allowed.extend_from_slice(extras);
    members(value, path, &allowed, &["failure"])?;
    match kind {
        "timeout" => Ok(ProviderFailure::Timeout),
        "unavailable" => Ok(ProviderFailure::Unavailable),
        "auth_failed" => Ok(ProviderFailure::AuthFailed),
        "tls_failed" => Ok(ProviderFailure::TlsFailed),
        "rate_limited" => rate_limited(value, path),
        "other" => other(value, path),
        _ => Err(invalid(
            value,
            &format!("{path}.failure"),
            "unknown failure kind",
        )),
    }
}

fn rate_limited(value: &Value<Position>, path: &str) -> Result<ProviderFailure, ScriptError> {
    let secs = value.get("retry_after_secs");
    let header = value.get("retry_after_header");
    let retry_after_secs = match (secs, header) {
        (None, None) => None,
        (Some(s), None) => match s.value.as_u64() {
            Some(n) if n <= MAX_RETRY_AFTER_SECS => u32::try_from(n).ok(),
            _ => {
                let field = format!("{path}.retry_after_secs");
                return Err(invalid(&s.value, &field, "expected 0 to 999999999"));
            }
        },
        (None, Some(h)) => match h.value.as_str() {
            Some(raw) => parse_retry_after(raw),
            None => {
                let field = format!("{path}.retry_after_header");
                return Err(invalid(&h.value, &field, "expected a string"));
            }
        },
        (Some(_), Some(_)) => {
            return Err(invalid(
                value,
                path,
                "give retry_after_secs or retry_after_header",
            ));
        }
    };
    Ok(ProviderFailure::RateLimited { retry_after_secs })
}

fn other(value: &Value<Position>, path: &str) -> Result<ProviderFailure, ScriptError> {
    let header = match value.get("retry_after_header") {
        None => None,
        Some(h) => match h.value.as_str() {
            Some(raw) => Some(raw),
            None => {
                let field = format!("{path}.retry_after_header");
                return Err(invalid(&h.value, &field, "expected a string"));
            }
        },
    };
    let Some(status) = value.get("http_status") else {
        if header.is_some() {
            return Err(invalid(value, path, "retry_after_header needs http_status"));
        }
        return Ok(ProviderFailure::Other { http_status: None });
    };
    match status.value.as_u64().and_then(|n| u16::try_from(n).ok()) {
        Some(code @ 100..=599) => Ok(failure_from_http(code, header)),
        _ => {
            let field = format!("{path}.http_status");
            Err(invalid(&status.value, &field, "expected 100 to 599"))
        }
    }
}
