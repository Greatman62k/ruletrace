//! The canonical bytes of one new ledger record (design §3.8.1).

use rt_hash::{sha256, Sha256Hex};
use rt_json::{write_compact, Value};
use rt_ledger::BindingPair;

/// The fields of the record to append, before its `hash` is computed.
pub(crate) struct NewRecord<'a> {
    pub seq: u32,
    pub rejection: bool,
    pub rule: &'a str,
    pub revision: u32,
    pub content_hash: &'a str,
    pub ir_version: &'a str,
    pub binding: &'a [BindingPair],
    pub reviewer: &'a str,
    pub time: &'a str,
    pub reason: Option<&'a str>,
    pub prev: &'a str,
}

fn object(members: Vec<(&str, Value<()>)>) -> Option<Value<()>> {
    Value::object(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
    .ok()
}

fn value(r: &NewRecord<'_>, hash: Option<&str>) -> Option<Value<()>> {
    let mut binding = Vec::new();
    for b in r.binding {
        binding.push(object(vec![
            ("document", Value::string(b.document.as_str())),
            ("sha256", Value::string(b.sha256.as_str())),
        ])?);
    }
    let mut members = vec![
        ("seq", Value::integer(r.seq)),
        (
            "kind",
            Value::string(if r.rejection { "rejection" } else { "approval" }),
        ),
        ("rule", Value::string(r.rule)),
        ("revision", Value::integer(r.revision)),
        ("content_hash", Value::string(r.content_hash)),
        ("ir_version", Value::string(r.ir_version)),
        ("binding", Value::array(binding)),
        ("reviewer", Value::string(r.reviewer)),
        ("time", Value::string(r.time)),
        ("reason", r.reason.map_or_else(Value::null, Value::string)),
        ("prev", Value::string(r.prev)),
    ];
    if let Some(h) = hash {
        members.push(("hash", Value::string(h)));
    }
    object(members)
}

/// The record line with its `hash` (SHA-256 of the compact form without the `hash` key), and
/// that hash. `None` only if the fixed key set were duplicated, which it cannot be.
pub(crate) fn record_line(r: &NewRecord<'_>) -> Option<(String, Sha256Hex)> {
    let hash = sha256(write_compact(&value(r, None)?).as_bytes());
    let line = write_compact(&value(r, Some(hash.as_str()))?);
    Some((line, hash))
}
