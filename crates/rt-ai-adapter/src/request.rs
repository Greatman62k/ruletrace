//! Provider_Request and its Request_Identifier (Req 29.7).

use rt_hash::{sha256, Sha256Hex};
use rt_json::{write_compact, Value};

/// Limits that are part of the canonical request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestLimits {
    /// Total deadline in seconds.
    pub deadline_s: u32,
    /// Per-attempt timeout in seconds.
    pub attempt_s: u32,
    /// Maximum retries.
    pub retries: u32,
    /// Response size limit in bytes.
    pub response_bytes: u32,
}

/// The SHA-256 of the canonical request bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RequestId(Sha256Hex);

impl RequestId {
    /// The 64 lowercase hexadecimal characters.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// A provider-neutral request. `context` holds the remaining top-level
/// members (clauses, declarations, vocabulary, revise) in their fixed order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRequest {
    /// Task name.
    pub task: String,
    /// Model identifier.
    pub model: String,
    /// Deadline, retry and size limits.
    pub limits: RequestLimits,
    /// The reasoning effort, part of the canonical bytes only when set
    /// (Req 35.11).
    pub reasoning_effort: Option<String>,
    /// Further ordered members of the canonical object.
    pub context: Vec<(String, Value<()>)>,
}

impl ProviderRequest {
    /// RT-JSON compact bytes with the fixed key order `v`, `task`, `model`,
    /// `limits`, `reasoning_effort` (only when set), then the context
    /// members. Returns `None` when a context key repeats one of the fixed
    /// keys or another context key.
    #[must_use]
    pub fn canonical_bytes(&self) -> Option<Vec<u8>> {
        let limits = Value::object(vec![
            (
                "deadline_s".to_owned(),
                Value::integer(self.limits.deadline_s),
            ),
            (
                "attempt_s".to_owned(),
                Value::integer(self.limits.attempt_s),
            ),
            ("retries".to_owned(), Value::integer(self.limits.retries)),
            (
                "response_bytes".to_owned(),
                Value::integer(self.limits.response_bytes),
            ),
        ])
        .ok()?;
        let mut members = vec![
            ("v".to_owned(), Value::integer(1)),
            ("task".to_owned(), Value::string(self.task.as_str())),
            ("model".to_owned(), Value::string(self.model.as_str())),
            ("limits".to_owned(), limits),
        ];
        if let Some(effort) = &self.reasoning_effort {
            members.push((
                "reasoning_effort".to_owned(),
                Value::string(effort.as_str()),
            ));
        }
        members.extend(self.context.iter().cloned());
        let object = Value::object(members).ok()?;
        Some(write_compact(&object).into_bytes())
    }

    /// SHA-256 of the canonical bytes, or `None` when they cannot be built.
    #[must_use]
    pub fn request_id(&self) -> Option<RequestId> {
        self.canonical_bytes().map(|b| RequestId(sha256(&b)))
    }
}
