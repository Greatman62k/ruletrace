//! Recorded replay (`provider = "mock-replay"`, design §8.11.7).

use crate::clock::SimulatedClock;
use crate::support::{invalid, members, text, ScriptError};
use rt_ai_adapter::{
    AiAdapter, AttemptOutcome, AttemptReport, Completion, MonotonicClock, ProviderFailure,
    ProviderMeta, ProviderRequest, ProviderResponse,
};
use rt_foundation::InputId;
use rt_hash::{sha256, Sha256Hex};
use rt_json::{check_flag_keys, parse, ParseLimits, Position, Value};
use std::collections::BTreeMap;
use std::time::Duration;

const MAX_SCRIPT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DEPTH: u32 = 8;

/// Who produced an entry. An authored entry is never provider output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Captured from a real provider response.
    Recorded,
    /// Written or edited by a person.
    Authored,
}

/// One response keyed by Request_Identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedEntry {
    /// 64 lowercase hex characters.
    pub request_id: String,
    /// Recorded or authored.
    pub origin: Origin,
    /// The exact response bytes.
    pub payload: Vec<u8>,
}

/// A parsed `ruletrace-recorded-script`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedScript {
    /// Provider kind recorded in the file.
    pub provider_kind: String,
    /// Model identifier recorded in the file.
    pub model: String,
    /// Adapter version recorded in the file.
    pub adapter_version: String,
    /// Capture text supplied by the user.
    pub captured: String,
    entries: BTreeMap<String, RecordedEntry>,
}

impl RecordedScript {
    /// Parses a `ruletrace-recorded-script` version 1 document and checks
    /// each entry's `response_sha256` against its payload bytes.
    ///
    /// # Errors
    /// [`ScriptError`] for bad JSON, a flag or lifecycle key, an unknown or
    /// missing field, a malformed or repeated `request_id`, a completion
    /// other than `complete`, or a hash that does not match the payload.
    pub fn parse(input: &InputId, bytes: &[u8]) -> Result<Self, ScriptError> {
        let limits = ParseLimits::new(MAX_SCRIPT_BYTES, MAX_DEPTH);
        let doc = parse(input, bytes, limits).map_err(ScriptError::Parse)?;
        check_flag_keys(input, &doc).map_err(ScriptError::FlagKeys)?;
        let top = ["format", "version", "provider", "captured", "entries"];
        members(&doc, "", &top, &top)?;
        if text(&doc, "format", "")? != "ruletrace-recorded-script" {
            return Err(invalid(
                &doc,
                ".format",
                "expected ruletrace-recorded-script",
            ));
        }
        if doc.get("version").and_then(|m| m.value.as_u64()) != Some(1) {
            return Err(invalid(&doc, ".version", "expected version 1"));
        }
        let provider = doc.get("provider").map(|m| &m.value);
        let Some(provider) = provider else {
            return Err(invalid(&doc, ".provider", "missing field"));
        };
        let keys = ["kind", "model", "adapter_version"];
        members(provider, ".provider", &keys, &keys)?;
        let entries_value = doc.get("entries").map(|m| &m.value);
        let Some(list) = entries_value.and_then(Value::as_array) else {
            return Err(invalid(&doc, ".entries", "expected an array"));
        };
        let mut entries = BTreeMap::new();
        for (index, item) in list.iter().enumerate() {
            let entry = entry(item, &format!("entries[{index}]"))?;
            let id = entry.request_id.clone();
            if entries.insert(id, entry).is_some() {
                let path = format!("entries[{index}].request_id");
                return Err(invalid(item, &path, "repeated request_id"));
            }
        }
        Ok(Self {
            provider_kind: text(provider, "kind", ".provider")?.to_owned(),
            model: text(provider, "model", ".provider")?.to_owned(),
            adapter_version: text(provider, "adapter_version", ".provider")?.to_owned(),
            captured: text(&doc, "captured", "")?.to_owned(),
            entries,
        })
    }

    /// Entries in ascending `request_id` order.
    pub fn entries(&self) -> impl Iterator<Item = &RecordedEntry> {
        self.entries.values()
    }

    /// The entry for a Request_Identifier.
    #[must_use]
    pub fn get(&self, request_id: &str) -> Option<&RecordedEntry> {
        self.entries.get(request_id)
    }
}

fn entry(item: &Value<Position>, path: &str) -> Result<RecordedEntry, ScriptError> {
    let keys = [
        "request_id",
        "origin",
        "completion",
        "response_sha256",
        "payload",
    ];
    members(item, path, &keys, &keys)?;
    let id = text(item, "request_id", path)?;
    if Sha256Hex::parse(id).is_none() {
        let field = format!("{path}.request_id");
        return Err(invalid(
            item,
            &field,
            "expected 64 lowercase hex characters",
        ));
    }
    let origin = match text(item, "origin", path)? {
        "recorded" => Origin::Recorded,
        "authored" => Origin::Authored,
        _ => return Err(invalid(item, &format!("{path}.origin"), "unknown origin")),
    };
    if text(item, "completion", path)? != "complete" {
        let field = format!("{path}.completion");
        return Err(invalid(
            item,
            &field,
            "only complete responses are replayed",
        ));
    }
    let payload = text(item, "payload", path)?.as_bytes().to_vec();
    if text(item, "response_sha256", path)? != sha256(&payload).as_str() {
        let field = format!("{path}.response_sha256");
        return Err(invalid(
            item,
            &field,
            "hash does not match the payload bytes",
        ));
    }
    Ok(RecordedEntry {
        request_id: id.to_owned(),
        origin,
        payload,
    })
}

/// Replays recorded responses by Request_Identifier.
///
/// Contract: `attempt` returns the matching entry's payload with zero
/// elapsed time. With no matching entry, or a request that has no canonical
/// bytes, it returns `Other` without a status and puts the explanation,
/// naming the Request_Identifier, in `AttemptReport::detail`.
#[derive(Debug)]
pub struct ReplayProvider {
    script: RecordedScript,
    clock: SimulatedClock,
    attempts: usize,
}

impl ReplayProvider {
    /// A replay provider over a parsed script.
    #[must_use]
    pub fn new(script: RecordedScript) -> Self {
        Self {
            script,
            clock: SimulatedClock::new(),
            attempts: 0,
        }
    }

    /// Number of attempts made.
    #[must_use]
    pub fn attempts(&self) -> usize {
        self.attempts
    }

    fn miss(detail: String) -> AttemptReport {
        AttemptReport {
            outcome: AttemptOutcome::Failure(ProviderFailure::Other { http_status: None }),
            elapsed: Duration::ZERO,
            detail: Some(detail),
        }
    }
}

impl AiAdapter for ReplayProvider {
    fn attempt(&mut self, request: &ProviderRequest, _timeout: Duration) -> AttemptReport {
        self.attempts += 1;
        let Some(id) = request.request_id() else {
            return Self::miss("request has no canonical bytes, so no request id".to_owned());
        };
        let Some(entry) = self.script.get(id.as_str()) else {
            return Self::miss(format!("no entry for request {}", id.as_str()));
        };
        let payload = entry.payload.clone();
        AttemptReport {
            outcome: AttemptOutcome::Response(ProviderResponse {
                completion: Completion::Complete,
                payload,
                meta: ProviderMeta {
                    provider: "mock-replay".to_owned(),
                    model: self.script.model.clone(),
                },
            }),
            elapsed: Duration::ZERO,
            detail: None,
        }
    }

    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }
}
