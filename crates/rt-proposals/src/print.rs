//! The canonical printer: compact RT-JSON and one line feed.

use crate::model::{
    Batch, Evidence, Proposal, StoredDiagnostic, StoredLocation, FORMAT_NAME, FORMAT_VERSION,
};
use rt_json::{write_compact, Value};

fn text(value: &str) -> String {
    write_compact(&Value::string(value))
}

fn int(value: u32) -> String {
    write_compact(&Value::integer(value))
}

fn array(items: Vec<String>) -> String {
    format!("[{}]", items.join(","))
}

fn object(members: Vec<(&str, String)>) -> String {
    let body: Vec<String> = members
        .into_iter()
        .map(|(key, value)| format!("{}:{value}", text(key)))
        .collect();
    format!("{{{}}}", body.join(","))
}

fn evidence(e: &Evidence) -> String {
    object(vec![
        ("document", text(e.document.as_str())),
        ("sha256", text(e.sha256.as_str())),
        ("start", int(e.start)),
        ("end", int(e.end)),
        ("line", int(e.line)),
        ("column", int(e.column)),
        ("quote", text(&e.quote)),
    ])
}

fn location(location: &StoredLocation) -> String {
    match location {
        StoredLocation::Source {
            document,
            start,
            end,
        } => object(vec![
            ("kind", text("source")),
            ("document", text(document.as_str())),
            ("start", int(*start)),
            ("end", int(*end)),
        ]),
        StoredLocation::Document(document) => object(vec![
            ("kind", text("document")),
            ("document", text(document.as_str())),
        ]),
        StoredLocation::Field(path) => object(vec![("kind", text("field")), ("path", text(path))]),
    }
}

fn diagnostic(d: &StoredDiagnostic) -> String {
    object(vec![
        ("code", text(d.code.id())),
        ("message", text(&d.message)),
        ("location", location(&d.location)),
    ])
}

fn proposal(p: &Proposal) -> String {
    let mut members = vec![
        ("proposal_id", text(p.proposal_id.as_str())),
        ("kind", text(p.kind.as_str())),
        ("proposer", text(p.proposer.as_str())),
        ("rationale", text(&p.rationale)),
        ("evidence", array(p.evidence.iter().map(evidence).collect())),
    ];
    if let Some((draft, hash)) = &p.rule {
        members.push(("rule", draft.json.clone()));
        members.push(("draft_hash", text(hash.as_str())));
    }
    members.push(("outcome", array(p.outcome.iter().map(diagnostic).collect())));
    object(members)
}

/// Prints a batch in its one canonical form. Printing the same batch twice
/// gives identical bytes, and [`crate::parse_batch`] of the output gives an
/// equal batch.
#[must_use]
pub fn print_batch(batch: &Batch) -> String {
    let response = object(vec![
        ("completion", text(batch.response.completion.as_str())),
        ("payload", text(&batch.response.payload)),
    ]);
    let identities = batch
        .ai_identities
        .iter()
        .map(|i| text(i.as_str()))
        .collect();
    let mut out = object(vec![
        ("format", text(FORMAT_NAME)),
        ("version", int(FORMAT_VERSION)),
        ("request_id", text(batch.request_id.as_str())),
        ("response_sha256", text(batch.response_sha256.as_str())),
        ("response", response),
        ("provider", text(&batch.provider)),
        ("model", text(&batch.model)),
        ("adapter_version", text(&batch.adapter_version)),
        ("ai_identities", array(identities)),
        ("created_at", text(&batch.created_at)),
        (
            "proposals",
            array(batch.proposals.iter().map(proposal).collect()),
        ),
    ]);
    out.push('\n');
    out
}
