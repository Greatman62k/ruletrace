//! The Responses envelope (Req 35.13).
//!
//! The envelope is untrusted. It is read with the strict RT-JSON parser and
//! mapped by shape. Nothing in it is executed or followed.

use rt_foundation::InputId;
use rt_json::{parse, Kind, ParseLimits, Position, Value};

const MAX_DEPTH: u32 = 32;

/// What the envelope says, before it becomes an adapter response.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Mapped {
    pub verdict: Verdict,
    pub reported_model: Option<String>,
    pub response_id: Option<String>,
    pub usage: Option<Counts>,
}

/// The token counts as reported, each only when it is a non-negative integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Counts {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub total: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// `status` completed with one message and one `output_text`.
    Text(String),
    /// The message holds one refusal and no text.
    Refused,
    /// `status` is not `completed`.
    Incomplete,
    /// Not JSON, or any other shape. The reason is a fixed text.
    Malformed(&'static str),
}

fn malformed(reason: &'static str) -> Mapped {
    Mapped {
        verdict: Verdict::Malformed(reason),
        reported_model: None,
        response_id: None,
        usage: None,
    }
}

fn string_member(root: &Value<Position>, key: &str) -> Option<String> {
    root.get(key)
        .and_then(|m| m.value.as_str())
        .map(str::to_owned)
}

fn count(usage: &Value<Position>, key: &str) -> Option<u64> {
    match usage.get(key).map(|m| m.value.kind()) {
        Some(Kind::Integer(n)) => Some(*n),
        _ => None,
    }
}

fn usage_of(root: &Value<Position>) -> Option<Counts> {
    let usage = &root.get("usage")?.value;
    usage.as_object()?;
    let counts = Counts {
        input: count(usage, "input_tokens"),
        output: count(usage, "output_tokens"),
        total: count(usage, "total_tokens"),
    };
    (counts.input.is_some() || counts.output.is_some() || counts.total.is_some()).then_some(counts)
}

/// Maps the body of a 2xx response.
pub(crate) fn map(body: &[u8]) -> Mapped {
    let Ok(id) = InputId::new("provider response") else {
        return malformed("the envelope cannot be named");
    };
    let limits = ParseLimits::new(u64::try_from(body.len()).unwrap_or(u64::MAX), MAX_DEPTH);
    let Ok(root) = parse(&id, body, limits) else {
        return malformed("the envelope is not valid JSON");
    };
    if root.as_object().is_none() {
        return malformed("the envelope is not an object");
    }
    let mut mapped = Mapped {
        verdict: Verdict::Malformed("unset"),
        reported_model: string_member(&root, "model"),
        response_id: string_member(&root, "id"),
        usage: usage_of(&root),
    };
    mapped.verdict = verdict_of(&root);
    mapped
}

fn verdict_of(root: &Value<Position>) -> Verdict {
    let Some(status) = root.get("status").and_then(|m| m.value.as_str()) else {
        return Verdict::Malformed("the envelope has no status");
    };
    if status != "completed" {
        return Verdict::Incomplete;
    }
    let Some(output) = root.get("output").and_then(|m| m.value.as_array()) else {
        return Verdict::Malformed("the envelope has no output array");
    };
    let mut message: Option<&Value<Position>> = None;
    for item in output {
        let Some(kind) = item.get("type").and_then(|m| m.value.as_str()) else {
            return Verdict::Malformed("an output item has no type");
        };
        match kind {
            "reasoning" => {}
            "message" if message.is_none() => message = Some(item),
            "message" => return Verdict::Malformed("the output has more than one message"),
            _ => return Verdict::Malformed("the output has an item that is not a message"),
        }
    }
    match message {
        Some(item) => message_verdict(item),
        None => Verdict::Malformed("the output has no message"),
    }
}

fn message_verdict(message: &Value<Position>) -> Verdict {
    let Some(content) = message.get("content").and_then(|m| m.value.as_array()) else {
        return Verdict::Malformed("the message has no content array");
    };
    let mut text: Option<&str> = None;
    let mut refusals = 0usize;
    for part in content {
        match part.get("type").and_then(|m| m.value.as_str()) {
            Some("output_text") => {
                let Some(value) = part.get("text").and_then(|m| m.value.as_str()) else {
                    return Verdict::Malformed("an output_text has no text");
                };
                if text.replace(value).is_some() {
                    return Verdict::Malformed("the message has more than one output_text");
                }
            }
            Some("refusal") => refusals += 1,
            _ => return Verdict::Malformed("the message has content that is not output_text"),
        }
    }
    match (text, refusals) {
        (Some(text), 0) => Verdict::Text(text.to_owned()),
        (None, 1) => Verdict::Refused,
        (None, 0) => Verdict::Malformed("the message has no output_text"),
        _ => Verdict::Malformed("the message mixes a refusal with other content"),
    }
}
