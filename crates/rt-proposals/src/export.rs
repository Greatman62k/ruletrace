//! Export of stored batches to a Recorded_Provider_Script (UD-36, design
//! §8.11.7, Req 33.8, 28.15).
//!
//! Pure: batches in, canonical script bytes out. The script format is
//! `ruletrace-recorded-script` version 1. Each entry keeps the request
//! identifier, the completion and the exact accepted payload with its hash.
//! The format has no member for the reported model, the response identifier,
//! the usage or a document hash, so those are not exported.

use crate::model::{Batch, Format};
use rt_hash::sha256;
use rt_json::{write_pretty, Value};

/// What the caller states about the recording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptHeader {
    /// The protocol label, the script's `provider.kind`.
    pub kind: String,
    /// The provider identifier, the optional `provider.id`.
    pub provider_id: Option<String>,
    /// The configured model that every batch must record.
    pub model: String,
    /// The capture text supplied by the user.
    pub captured: String,
}

/// One batch and the name of the file it was read from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedBatch {
    /// The file name, used in every error about this batch.
    pub file: String,
    /// The parsed batch.
    pub batch: Batch,
}

/// Why no script was produced. Every variant that concerns one batch names
/// its file and Request_Identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportError {
    /// No batch was given.
    Empty,
    /// The payload does not hash to the recorded `response_sha256`.
    HashMismatch {
        /// File of the batch.
        file: String,
        /// Its Request_Identifier.
        request_id: String,
        /// The hash recorded in the batch.
        recorded: String,
        /// The hash of the payload bytes.
        computed: String,
    },
    /// A batch disagrees with the header or with the first batch. The
    /// export never relabels a batch and never merges recordings.
    Mismatch {
        /// File of the batch.
        file: String,
        /// Its Request_Identifier.
        request_id: String,
        /// The member that differs.
        member: &'static str,
        /// The value that was expected.
        expected: String,
        /// The value that the batch records.
        found: String,
    },
    /// The batch was produced by the offline replay, not by a provider, so
    /// labelling it `recorded` would call a replayed or authored entry
    /// provider output (Req 33.8).
    NotProviderOutput {
        /// File of the batch.
        file: String,
        /// Its Request_Identifier.
        request_id: String,
    },
    /// Two batches have the same Request_Identifier.
    Duplicate {
        /// File of the later batch.
        file: String,
        /// The repeated Request_Identifier.
        request_id: String,
    },
    /// A credential-looking string appears in the script text. The value is
    /// never echoed, only the pattern and where it was found.
    CredentialLooking {
        /// File of the batch, or `header` for a header value.
        file: String,
        /// Its Request_Identifier, empty for a header value.
        request_id: String,
        /// The pattern that matched.
        pattern: &'static str,
    },
    /// The script object could not be built (a repeated member name).
    Internal,
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "the store has no batch to export"),
            Self::HashMismatch {
                file,
                request_id,
                recorded,
                computed,
            } => write!(
                f,
                "{file}: request {request_id}: recorded response_sha256 {recorded} \
                 but the payload hashes to {computed}"
            ),
            Self::Mismatch {
                file,
                request_id,
                member,
                expected,
                found,
            } => write!(
                f,
                "{file}: request {request_id}: {member} is '{found}', expected '{expected}'; \
                 batches are never relabelled or merged"
            ),
            Self::NotProviderOutput { file, request_id } => write!(
                f,
                "{file}: request {request_id}: the batch comes from the offline replay \
                 (protocol mock-replay), not from a provider, so it is not exported as recorded"
            ),
            Self::Duplicate { file, request_id } => {
                write!(f, "{file}: request {request_id} appears in two batches")
            }
            Self::CredentialLooking {
                file,
                request_id,
                pattern,
            } => write!(
                f,
                "{file}: request {request_id}: a credential-looking string (pattern '{pattern}') \
                 appears; no script is written"
            ),
            Self::Internal => write!(f, "the script object could not be built"),
        }
    }
}

// The value prefixes of Req 28.4. A match anywhere at the start of a word
// refuses the script, so an echoed key in a payload is never committed.
const PREFIXES: [&str; 5] = ["sk-", "sk_", "Bearer", "ghp_", "AKIA"];

fn credential_pattern(text: &str) -> Option<&'static str> {
    let bytes = text.as_bytes();
    for (at, _) in text.char_indices() {
        let starts_word = at == 0
            || !bytes
                .get(at - 1)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-');
        if !starts_word {
            continue;
        }
        if let Some(p) = PREFIXES.iter().find(|p| text[at..].starts_with(**p)) {
            return Some(p);
        }
    }
    None
}

fn protocol_of(batch: &Batch) -> Option<&str> {
    match &batch.format {
        Format::V1 => Some(batch.provider.as_str()),
        Format::V2(extras) => extras.protocol.as_deref(),
    }
}

fn provider_id_of(batch: &Batch) -> Option<&str> {
    match &batch.format {
        Format::V1 => None,
        Format::V2(_) => Some(batch.provider.as_str()),
    }
}

fn check(
    named: &NamedBatch,
    header: &ScriptHeader,
    adapter_version: &str,
) -> Result<(), ExportError> {
    let b = &named.batch;
    let file = || named.file.clone();
    let request_id = || b.request_id.as_str().to_owned();
    let computed = sha256(b.response.payload.as_bytes());
    if computed != b.response_sha256 {
        return Err(ExportError::HashMismatch {
            file: file(),
            request_id: request_id(),
            recorded: b.response_sha256.as_str().to_owned(),
            computed: computed.as_str().to_owned(),
        });
    }
    const REPLAY: &str = "mock-replay";
    if protocol_of(b) == Some(REPLAY) || header.kind == REPLAY {
        return Err(ExportError::NotProviderOutput {
            file: file(),
            request_id: request_id(),
        });
    }
    let mismatch = |member, expected: &str, found: &str| ExportError::Mismatch {
        file: file(),
        request_id: request_id(),
        member,
        expected: expected.to_owned(),
        found: found.to_owned(),
    };
    if b.model != header.model {
        return Err(mismatch("model", &header.model, &b.model));
    }
    if b.adapter_version != adapter_version {
        return Err(mismatch(
            "adapter_version",
            adapter_version,
            &b.adapter_version,
        ));
    }
    if let Some(protocol) = protocol_of(b) {
        if protocol != header.kind {
            return Err(mismatch("protocol", &header.kind, protocol));
        }
    }
    if let (Some(want), Some(found)) = (&header.provider_id, provider_id_of(b)) {
        if want != found {
            return Err(mismatch("provider", want, found));
        }
    }
    if let Some(pattern) = credential_pattern(&b.response.payload) {
        return Err(ExportError::CredentialLooking {
            file: file(),
            request_id: request_id(),
            pattern,
        });
    }
    Ok(())
}

fn header_pattern(header: &ScriptHeader, adapter_version: &str) -> Option<&'static str> {
    let mut texts = vec![
        header.kind.as_str(),
        header.model.as_str(),
        header.captured.as_str(),
        adapter_version,
    ];
    texts.extend(header.provider_id.as_deref());
    texts.into_iter().find_map(credential_pattern)
}

/// The canonical script bytes for `batches`, ordered by Request_Identifier.
///
/// Every entry has origin `recorded`. The adapter version is the one the
/// batches record, which must be the same for all of them: two recordings
/// of different prompt profiles are exported to separate scripts.
///
/// # Errors
/// [`ExportError`] for an empty list, a payload whose hash differs from the
/// recorded hash, a batch of the offline replay, a batch whose model,
/// adapter version, protocol or provider differs, a repeated
/// Request_Identifier, or a credential-looking string in any written text.
pub fn export_script(
    header: &ScriptHeader,
    batches: &[NamedBatch],
) -> Result<Vec<u8>, ExportError> {
    let mut sorted: Vec<&NamedBatch> = batches.iter().collect();
    sorted.sort_by(|a, b| a.batch.request_id.cmp(&b.batch.request_id));
    let Some(first) = sorted.first() else {
        return Err(ExportError::Empty);
    };
    let adapter_version = first.batch.adapter_version.clone();
    if let Some(pattern) = header_pattern(header, &adapter_version) {
        return Err(ExportError::CredentialLooking {
            file: "header".to_owned(),
            request_id: String::new(),
            pattern,
        });
    }
    let mut entries = Vec::with_capacity(sorted.len());
    let mut previous: Option<&str> = None;
    for named in &sorted {
        check(named, header, &adapter_version)?;
        let id = named.batch.request_id.as_str();
        if previous == Some(id) {
            return Err(ExportError::Duplicate {
                file: named.file.clone(),
                request_id: id.to_owned(),
            });
        }
        previous = Some(id);
        entries.push(entry(&named.batch)?);
    }
    let mut provider = vec![("kind".to_owned(), Value::string(&header.kind))];
    if let Some(id) = &header.provider_id {
        provider.push(("id".to_owned(), Value::string(id)));
    }
    provider.push(("model".to_owned(), Value::string(&header.model)));
    provider.push((
        "adapter_version".to_owned(),
        Value::string(&adapter_version),
    ));
    let script = object(vec![
        (
            "format".to_owned(),
            Value::string("ruletrace-recorded-script"),
        ),
        ("version".to_owned(), Value::integer(1)),
        ("provider".to_owned(), object(provider)?),
        ("captured".to_owned(), Value::string(&header.captured)),
        ("entries".to_owned(), Value::array(entries)),
    ])?;
    Ok(write_pretty(&script).into_bytes())
}

fn object(members: Vec<(String, Value<()>)>) -> Result<Value<()>, ExportError> {
    Value::object(members).map_err(|_| ExportError::Internal)
}

fn entry(batch: &Batch) -> Result<Value<()>, ExportError> {
    object(vec![
        (
            "request_id".to_owned(),
            Value::string(batch.request_id.as_str()),
        ),
        ("origin".to_owned(), Value::string("recorded")),
        (
            "completion".to_owned(),
            Value::string(batch.response.completion.as_str()),
        ),
        (
            "response_sha256".to_owned(),
            Value::string(batch.response_sha256.as_str()),
        ),
        ("payload".to_owned(), Value::string(&batch.response.payload)),
    ])
}
