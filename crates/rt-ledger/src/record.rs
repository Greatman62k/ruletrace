//! Ledger data types and the canonical form of a record.

use rt_foundation::{DocumentId, RevisionNumber, RuleId};
use rt_hash::{sha256, Sha256Hex};
use rt_ir::Identity;
use rt_json::{write_compact, Value};
use std::fmt;

/// The header line of every ledger (without its line feed).
pub const HEADER_LINE: &str = r#"{"format":"ruletrace-ledger","version":1}"#;

/// The `prev` value of the first record: 64 zero characters.
pub const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Whether a record approves or rejects a revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RecordKind {
    /// An approval record.
    Approval,
    /// A rejection record. It carries a reason.
    Rejection,
}

impl RecordKind {
    /// The `kind` text of the file format.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            RecordKind::Approval => "approval",
            RecordKind::Rejection => "rejection",
        }
    }

    pub(crate) fn from_name(text: &str) -> Option<Self> {
        match text {
            "approval" => Some(RecordKind::Approval),
            "rejection" => Some(RecordKind::Rejection),
            _ => None,
        }
    }
}

/// One bound document: its identifier and SHA-256 document hash.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingPair {
    /// The document identifier.
    pub document: DocumentId,
    /// The document hash that the Rulebook declared when the record was made.
    pub sha256: Sha256Hex,
}

/// Why a binding cannot be built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingError {
    /// Two pairs name the same document.
    DuplicateDocument(DocumentId),
}

impl fmt::Display for BindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BindingError::DuplicateDocument(id) => {
                write!(f, "binding names document '{id}' more than once")
            }
        }
    }
}

impl std::error::Error for BindingError {}

/// The Approval_Binding: the pairs of document identifier and document hash,
/// sorted by document identifier in UTF-8 byte order, with no document twice
/// (Req 26.7).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct ApprovalBinding(Vec<BindingPair>);

impl ApprovalBinding {
    /// Builds a binding. The pairs are sorted by document identifier bytes.
    ///
    /// # Errors
    /// [`BindingError::DuplicateDocument`] when a document appears twice.
    pub fn new(mut pairs: Vec<BindingPair>) -> Result<Self, BindingError> {
        pairs.sort_by(|a, b| a.document.cmp(&b.document));
        if let Some(w) = pairs
            .windows(2)
            .find(|w| w.first().map(|p| &p.document) == w.get(1).map(|p| &p.document))
        {
            if let Some(p) = w.first() {
                return Err(BindingError::DuplicateDocument(p.document.clone()));
            }
        }
        Ok(Self(pairs))
    }

    /// The pairs, sorted by document identifier.
    #[must_use]
    pub fn pairs(&self) -> &[BindingPair] {
        &self.0
    }
}

/// One verified record of the ledger.
///
/// The fields are private and there is no public constructor: a value of this
/// type exists only as the result of [`parse_ledger`](crate::parse_ledger),
/// which has verified it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LedgerRecord {
    pub(crate) seq: u32,
    pub(crate) kind: RecordKind,
    pub(crate) rule: RuleId,
    pub(crate) revision: RevisionNumber,
    pub(crate) content_hash: Sha256Hex,
    pub(crate) ir_version: String,
    pub(crate) binding: ApprovalBinding,
    pub(crate) reviewer: Identity,
    pub(crate) time: String,
    pub(crate) reason: Option<String>,
    pub(crate) prev: Sha256Hex,
    pub(crate) hash: Sha256Hex,
}

impl LedgerRecord {
    /// Sequence number, starting at 1.
    #[must_use]
    pub fn seq(&self) -> u32 {
        self.seq
    }
    /// Approval or rejection.
    #[must_use]
    pub fn kind(&self) -> RecordKind {
        self.kind
    }
    /// The rule the record names.
    #[must_use]
    pub fn rule(&self) -> &RuleId {
        &self.rule
    }
    /// The revision the record names.
    #[must_use]
    pub fn revision(&self) -> RevisionNumber {
        self.revision
    }
    /// The Rule_Content_Hash the record names.
    #[must_use]
    pub fn content_hash(&self) -> &Sha256Hex {
        &self.content_hash
    }
    /// The IR_Version text the record binds.
    #[must_use]
    pub fn ir_version(&self) -> &str {
        &self.ir_version
    }
    /// The Approval_Binding.
    #[must_use]
    pub fn binding(&self) -> &ApprovalBinding {
        &self.binding
    }
    /// The Reviewer identity.
    #[must_use]
    pub fn reviewer(&self) -> &Identity {
        &self.reviewer
    }
    /// The caller-supplied time, verbatim (`YYYY-MM-DDTHH:MM:SSZ`).
    #[must_use]
    pub fn time(&self) -> &str {
        &self.time
    }
    /// The rejection reason; `None` for an approval.
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
    /// The hash of the previous record (64 zeros for the first).
    #[must_use]
    pub fn prev(&self) -> &Sha256Hex {
        &self.prev
    }
    /// The hash of this record.
    #[must_use]
    pub fn hash(&self) -> &Sha256Hex {
        &self.hash
    }
}

/// A verified ledger: records with sequence numbers 1.. in file order and an
/// intact chain.
///
/// There is no constructor from records and no mutating method. A value comes
/// from [`parse_ledger`](crate::parse_ledger) or is the empty ledger.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ApprovalLedger {
    pub(crate) records: Vec<LedgerRecord>,
}

impl ApprovalLedger {
    /// The empty ledger (a file with only the header).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// The records in sequence order.
    #[must_use]
    pub fn records(&self) -> &[LedgerRecord] {
        &self.records
    }

    /// Number of records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// True when the ledger has no record.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Borrowed view of the twelve fields, used to build the canonical bytes of a
/// record before and after its fields are fully validated.
pub(crate) struct Parts<'a> {
    pub seq: u32,
    pub kind: RecordKind,
    pub rule: &'a str,
    pub revision: u32,
    pub content_hash: &'a str,
    pub ir_version: &'a str,
    pub binding: &'a [BindingPair],
    pub reviewer: &'a str,
    pub time: &'a str,
    pub reason: Option<&'a str>,
    pub prev: &'a str,
    pub hash: &'a str,
}

impl<'a> Parts<'a> {
    pub(crate) fn of(r: &'a LedgerRecord) -> Self {
        Parts {
            seq: r.seq,
            kind: r.kind,
            rule: r.rule.as_str(),
            revision: r.revision.get(),
            content_hash: r.content_hash.as_str(),
            ir_version: &r.ir_version,
            binding: r.binding.pairs(),
            reviewer: r.reviewer.as_str(),
            time: &r.time,
            reason: r.reason.as_deref(),
            prev: r.prev.as_str(),
            hash: r.hash.as_str(),
        }
    }
}

/// Builds an object value. The keys of every caller are fixed and distinct, so
/// the error case cannot occur; the fallback keeps the function total.
fn object(members: Vec<(&str, Value<()>)>) -> Value<()> {
    Value::object(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
    .unwrap_or_else(|_| Value::null())
}

/// The record as a value, key order of design §3.8.1, with or without the
/// `hash` key.
pub(crate) fn record_value(p: &Parts<'_>, with_hash: bool) -> Value<()> {
    let binding = p
        .binding
        .iter()
        .map(|b| {
            object(vec![
                ("document", Value::string(b.document.as_str())),
                ("sha256", Value::string(b.sha256.as_str())),
            ])
        })
        .collect();
    let mut members = vec![
        ("seq", Value::integer(p.seq)),
        ("kind", Value::string(p.kind.name())),
        ("rule", Value::string(p.rule)),
        ("revision", Value::integer(p.revision)),
        ("content_hash", Value::string(p.content_hash)),
        ("ir_version", Value::string(p.ir_version)),
        ("binding", Value::array(binding)),
        ("reviewer", Value::string(p.reviewer)),
        ("time", Value::string(p.time)),
        ("reason", p.reason.map_or_else(Value::null, Value::string)),
        ("prev", Value::string(p.prev)),
    ];
    if with_hash {
        members.push(("hash", Value::string(p.hash)));
    }
    object(members)
}

/// SHA-256 of the compact canonical bytes of the record without its `hash` key.
pub(crate) fn computed_hash(p: &Parts<'_>) -> Sha256Hex {
    sha256(write_compact(&record_value(p, false)).as_bytes())
}

/// The canonical compact line of a record, with its `hash`, without the line
/// feed.
pub(crate) fn record_line(p: &Parts<'_>) -> String {
    write_compact(&record_value(p, true))
}
