//! `parse_ledger`: line splitting, field decoding and the ordered verification.

use crate::record::{
    computed_hash, record_line, ApprovalBinding, ApprovalLedger, BindingPair, LedgerRecord, Parts,
    RecordKind, HEADER_LINE, ZERO_HASH,
};
use rt_foundation::{
    escape_controls, Code, Diagnostic, DocumentId, InputId, InputLocation, Location,
    RevisionNumber, RuleId, RuleRef,
};
use rt_hash::Sha256Hex;
use rt_ir::Identity;
use rt_json::{check_flag_keys, parse, Kind, ParseLimits, Position, Value};
use std::collections::BTreeMap;

const KEYS: [&str; 12] = [
    "seq",
    "kind",
    "rule",
    "revision",
    "content_hash",
    "ir_version",
    "binding",
    "reviewer",
    "time",
    "reason",
    "prev",
    "hash",
];

/// Longest rejection reason, in Unicode scalar values (design §3.8.1).
const MAX_REASON_CHARS: usize = 2000;

/// A field-level decoding failure: where, and why.
struct FieldError {
    field: String,
    column: Option<u32>,
    message: String,
}

impl FieldError {
    fn new(field: &str, column: Option<u32>, message: impl Into<String>) -> Self {
        Self {
            field: field.to_owned(),
            column,
            message: message.into(),
        }
    }
}

/// The twelve fields as decoded from one line. The reviewer is still text: its validity is
/// checked after the sequence and the chain (design §3.8.1).
struct Decoded {
    seq: u32,
    kind: RecordKind,
    rule: RuleId,
    revision: RevisionNumber,
    content_hash: Sha256Hex,
    ir_version: String,
    binding: ApprovalBinding,
    reviewer: String,
    time: String,
    reason: Option<String>,
    prev: Sha256Hex,
    hash: Sha256Hex,
}

impl Decoded {
    fn parts(&self) -> Parts<'_> {
        Parts {
            seq: self.seq,
            kind: self.kind,
            rule: self.rule.as_str(),
            revision: self.revision.get(),
            content_hash: self.content_hash.as_str(),
            ir_version: &self.ir_version,
            binding: self.binding.pairs(),
            reviewer: &self.reviewer,
            time: &self.time,
            reason: self.reason.as_deref(),
            prev: self.prev.as_str(),
            hash: self.hash.as_str(),
        }
    }
}

/// A decision seen earlier in the file, for the contradiction check.
struct Prior {
    seq: u32,
    kind: RecordKind,
    content_hash: Sha256Hex,
    line: u32,
}

fn line_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn location(input: &InputId, line: u32, column: Option<u32>, field: Option<&str>) -> Location {
    Location::Input(InputLocation {
        input: input.clone(),
        line: Some(line),
        column,
        field: field.map(str::to_owned),
    })
}

fn one(code: Code, message: String, at: Location) -> Vec<Diagnostic> {
    vec![Diagnostic::new(code, message, at)]
}

/// The column of `key` in a parsed record object, when it has that key.
fn key_column(value: &Value<Position>, key: &str) -> Option<u32> {
    value.get(key).map(|m| m.key_pos.column)
}

/// Moves a location of a single-line parse onto the file line.
fn on_line(loc: &Location, line: u32) -> Location {
    match loc {
        Location::Input(l) => Location::Input(InputLocation {
            line: Some(line),
            ..l.clone()
        }),
        other => other.clone(),
    }
}

fn column_of(loc: &Location) -> Option<u32> {
    match loc {
        Location::Input(l) => l.column,
        _ => None,
    }
}

/// Parses one line as RT-JSON and runs the flag and lifecycle key check (`RT-DGN-0001` to
/// `0003`). Locations are moved to the file line.
fn parse_line(
    input: &InputId,
    line_no: u32,
    text: &str,
    limits: ParseLimits,
) -> Result<Value<Position>, Vec<Diagnostic>> {
    let value = parse(input, text.as_bytes(), limits).map_err(|d| {
        let column = column_of(d.primary());
        one(
            Code::LedgerMalformed,
            format!(
                "ledger line {line_no} is not valid RT-JSON: {}",
                d.message()
            ),
            location(input, line_no, column, None),
        )
    })?;
    check_flag_keys(input, &value).map_err(|found| {
        found
            .iter()
            .map(|d| {
                Diagnostic::new(
                    d.code(),
                    d.message().to_owned(),
                    on_line(d.primary(), line_no),
                )
                .with_related(d.related().iter().map(|l| on_line(l, line_no)).collect())
            })
            .collect::<Vec<_>>()
    })?;
    Ok(value)
}

fn str_field<'a>(obj: &'a Value<Position>, key: &str, what: &str) -> Result<&'a str, FieldError> {
    let m = obj
        .get(key)
        .ok_or_else(|| FieldError::new(key, None, format!("missing field '{key}'")))?;
    m.value.as_str().ok_or_else(|| {
        FieldError::new(
            key,
            Some(m.value.pos().column),
            format!("field '{key}' must be {what}"),
        )
    })
}

fn hash_field(obj: &Value<Position>, key: &str) -> Result<Sha256Hex, FieldError> {
    let text = str_field(obj, key, "a string")?;
    Sha256Hex::parse(text).ok_or_else(|| {
        FieldError::new(
            key,
            key_column(obj, key),
            format!("field '{key}' must be 64 lowercase hexadecimal characters"),
        )
    })
}

fn u32_field(obj: &Value<Position>, key: &str) -> Result<u32, FieldError> {
    let m = obj
        .get(key)
        .ok_or_else(|| FieldError::new(key, None, format!("missing field '{key}'")))?;
    m.value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| {
            FieldError::new(
                key,
                Some(m.value.pos().column),
                format!("field '{key}' must be an integer from 0 to 4294967295"),
            )
        })
}

/// `YYYY-MM-DDTHH:MM:SSZ` with ASCII digits. Shape only: no calendar or clock check.
fn time_shape_ok(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

fn decode_binding(obj: &Value<Position>) -> Result<ApprovalBinding, FieldError> {
    let m = obj
        .get("binding")
        .ok_or_else(|| FieldError::new("binding", None, "missing field 'binding'"))?;
    let items = m.value.as_array().ok_or_else(|| {
        FieldError::new(
            "binding",
            Some(m.value.pos().column),
            "field 'binding' must be an array",
        )
    })?;
    let mut pairs: Vec<BindingPair> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let path = format!("binding[{i}]");
        let col = Some(item.pos().column);
        let members = item
            .as_object()
            .ok_or_else(|| FieldError::new(&path, col, format!("'{path}' must be an object")))?;
        if let Some(extra) = members
            .iter()
            .find(|k| k.key != "document" && k.key != "sha256")
        {
            return Err(FieldError::new(
                &path,
                Some(extra.key_pos.column),
                format!(
                    "unknown field '{}' in '{path}'",
                    escape_controls(&extra.key)
                ),
            ));
        }
        let doc_text = str_field(item, "document", "a string")
            .map_err(|e| FieldError::new(&path, e.column.or(col), e.message))?;
        let document = DocumentId::new(doc_text).map_err(|e| {
            FieldError::new(&format!("{path}.document"), col, format!("document: {e}"))
        })?;
        let sha256 = hash_field(item, "sha256")
            .map_err(|e| FieldError::new(&format!("{path}.sha256"), e.column.or(col), e.message))?;
        if let Some(prev) = pairs.last() {
            if prev.document >= document {
                return Err(FieldError::new(
                    &path,
                    col,
                    "binding must be sorted by document identifier with no document twice",
                ));
            }
        }
        pairs.push(BindingPair { document, sha256 });
    }
    // Strictly ascending was checked above, so this cannot fail; the mapping keeps it total.
    ApprovalBinding::new(pairs).map_err(|e| FieldError::new("binding", None, e.to_string()))
}

fn decode(value: &Value<Position>) -> Result<Decoded, FieldError> {
    let members = value.as_object().ok_or_else(|| {
        FieldError::new("", Some(value.pos().column), "a record must be an object")
    })?;
    if let Some(extra) = members.iter().find(|m| !KEYS.contains(&m.key.as_str())) {
        return Err(FieldError::new(
            &escape_controls(&extra.key),
            Some(extra.key_pos.column),
            format!("unknown field '{}'", escape_controls(&extra.key)),
        ));
    }
    if let Some(missing) = KEYS.iter().find(|k| value.get(k).is_none()) {
        return Err(FieldError::new(
            missing,
            None,
            format!("missing field '{missing}'"),
        ));
    }
    let seq = u32_field(value, "seq")?;
    let kind_text = str_field(value, "kind", "a string")?;
    let kind = RecordKind::from_name(kind_text).ok_or_else(|| {
        FieldError::new(
            "kind",
            key_column(value, "kind"),
            "field 'kind' must be \"approval\" or \"rejection\"",
        )
    })?;
    let rule = RuleId::new(str_field(value, "rule", "a string")?)
        .map_err(|e| FieldError::new("rule", key_column(value, "rule"), format!("rule: {e}")))?;
    let revision = RevisionNumber::new(u32_field(value, "revision")?).map_err(|e| {
        FieldError::new(
            "revision",
            key_column(value, "revision"),
            format!("revision: {e}"),
        )
    })?;
    let content_hash = hash_field(value, "content_hash")?;
    let ir_version = str_field(value, "ir_version", "a string")?;
    if ir_version.is_empty() || ir_version.chars().any(char::is_control) {
        return Err(FieldError::new(
            "ir_version",
            key_column(value, "ir_version"),
            "field 'ir_version' must be non-empty text without control characters",
        ));
    }
    let binding = decode_binding(value)?;
    let reviewer = str_field(value, "reviewer", "a string")?.to_owned();
    let time = str_field(value, "time", "a string")?;
    if !time_shape_ok(time) {
        return Err(FieldError::new(
            "time",
            key_column(value, "time"),
            "field 'time' must have the shape YYYY-MM-DDTHH:MM:SSZ",
        ));
    }
    let reason = decode_reason(value, kind)?;
    let prev = hash_field(value, "prev")?;
    let hash = hash_field(value, "hash")?;
    Ok(Decoded {
        seq,
        kind,
        rule,
        revision,
        content_hash,
        ir_version: ir_version.to_owned(),
        binding,
        reviewer,
        time: time.to_owned(),
        reason,
        prev,
        hash,
    })
}

fn decode_reason(value: &Value<Position>, kind: RecordKind) -> Result<Option<String>, FieldError> {
    let col = key_column(value, "reason");
    let Some(m) = value.get("reason") else {
        return Err(FieldError::new("reason", None, "missing field 'reason'"));
    };
    match (kind, m.value.kind()) {
        (RecordKind::Approval, Kind::Null) => Ok(None),
        (RecordKind::Approval, _) => Err(FieldError::new(
            "reason",
            col,
            "field 'reason' must be null for an approval",
        )),
        (RecordKind::Rejection, Kind::String(s))
            if !s.is_empty() && s.chars().nth(MAX_REASON_CHARS).is_none() =>
        {
            Ok(Some(s.clone()))
        }
        (RecordKind::Rejection, _) => Err(FieldError::new(
            "reason",
            col,
            "field 'reason' of a rejection must be 1 to 2000 characters",
        )),
    }
}

/// Verifies one line against everything the earlier records established.
struct Walk<'a> {
    input: &'a InputId,
    limits: ParseLimits,
    previous: Option<Sha256Hex>,
    seen: BTreeMap<(RuleId, RevisionNumber), Vec<Prior>>,
}

impl Walk<'_> {
    fn record(
        &mut self,
        line_no: u32,
        text: &str,
        expected_seq: u32,
    ) -> Result<LedgerRecord, Vec<Diagnostic>> {
        let input = self.input;
        let value = parse_line(input, line_no, text, self.limits)?;

        // 1. Syntax and fields (RT-APR-0101), including the canonical compact form.
        let d = decode(&value).map_err(|e| {
            one(
                Code::LedgerMalformed,
                format!("ledger line {line_no}: {}", e.message),
                location(input, line_no, e.column, Some(&e.field)),
            )
        })?;
        if record_line(&d.parts()) != text {
            return Err(one(
                Code::LedgerMalformed,
                format!("ledger line {line_no} is not the canonical compact form of its record"),
                location(input, line_no, None, None),
            ));
        }

        // 2. Sequence (RT-APR-0103).
        if d.seq != expected_seq {
            return Err(one(
                Code::LedgerSequenceInvalid,
                format!(
                    "ledger line {line_no}: expected sequence number {expected_seq}, found {}",
                    d.seq
                ),
                location(input, line_no, key_column(&value, "seq"), Some("seq")),
            ));
        }

        // 3. Hash chain (RT-APR-0102).
        let expected_prev = self.previous.as_ref().map_or(ZERO_HASH, Sha256Hex::as_str);
        if d.prev.as_str() != expected_prev {
            return Err(one(
                Code::LedgerChainBroken,
                format!(
                    "ledger line {line_no}: 'prev' does not equal the hash of the previous record"
                ),
                location(input, line_no, key_column(&value, "prev"), Some("prev")),
            ));
        }
        if computed_hash(&d.parts()) != d.hash {
            return Err(one(
                Code::LedgerChainBroken,
                format!("ledger line {line_no}: 'hash' does not match the content of the record"),
                location(input, line_no, key_column(&value, "hash"), Some("hash")),
            ));
        }

        // 4. Reviewer validity (RT-APR-0105).
        let reviewer = Identity::new(d.reviewer.clone()).map_err(|e| {
            one(
                Code::LedgerReviewerInvalid,
                format!("ledger line {line_no}: reviewer: {e}"),
                location(
                    input,
                    line_no,
                    key_column(&value, "reviewer"),
                    Some("reviewer"),
                ),
            )
        })?;

        // 5. Contradictions (RT-APR-0104).
        self.check_contradiction(line_no, &d, &value)?;

        self.previous = Some(d.hash.clone());
        Ok(LedgerRecord {
            seq: d.seq,
            kind: d.kind,
            rule: d.rule,
            revision: d.revision,
            content_hash: d.content_hash,
            ir_version: d.ir_version,
            binding: d.binding,
            reviewer,
            time: d.time,
            reason: d.reason,
            prev: d.prev,
            hash: d.hash,
        })
    }

    /// A decision after a rejection of the same revision is contradictory (Rejected is
    /// terminal). So is a second decision of the same revision with the same content hash
    /// (Req 25.16). An approval of different content is a legitimate later approval.
    fn check_contradiction(
        &mut self,
        line_no: u32,
        d: &Decoded,
        value: &Value<Position>,
    ) -> Result<(), Vec<Diagnostic>> {
        let key = (d.rule.clone(), d.revision);
        let priors = self.seen.entry(key).or_default();
        let clash = priors
            .iter()
            .find(|p| p.kind == RecordKind::Rejection || p.content_hash == d.content_hash);
        if let Some(p) = clash {
            let why = if p.kind == RecordKind::Rejection {
                "the revision was already rejected"
            } else {
                "the revision already has a decision for the same content hash"
            };
            let diag = Diagnostic::new(
                Code::LedgerRecordContradictory,
                format!(
                    "ledger line {line_no}: record {} contradicts record {} for rule '{}' \
                     revision {}: {why}",
                    d.seq, p.seq, d.rule, d.revision
                ),
                location(self.input, line_no, key_column(value, "kind"), Some("kind")),
            )
            .with_related(vec![location(self.input, p.line, None, None)])
            .with_rules(vec![RuleRef {
                rule: d.rule.clone(),
                revision: Some(d.revision),
            }]);
            return Err(vec![diag]);
        }
        priors.push(Prior {
            seq: d.seq,
            kind: d.kind,
            content_hash: d.content_hash.clone(),
            line: line_no,
        });
        Ok(())
    }
}

/// Parses and verifies a ledger file.
///
/// Verification runs record by record, in file order, and stops at the first failing record.
/// The checks for one record are, in order: syntax and fields (`RT-APR-0101`, which includes
/// the canonical compact form of the line), sequence number (`0103`), hash chain (`0102`),
/// reviewer validity (`0105`) and contradictions (`0104`). The flag and lifecycle key check
/// (`RT-DGN-0001` to `0003`) runs on each line right after it parses. A header-only ledger is
/// valid and empty. The file must end with a line feed.
///
/// # Errors
/// The diagnostics of the first failing record (or of the file as a whole for size, encoding
/// or a missing header). Their Input_Location names the file line and, where known, the
/// column and field. Nothing is returned on failure, so no status can be derived.
pub fn parse_ledger(
    input: &InputId,
    bytes: &[u8],
    limits: ParseLimits,
) -> Result<ApprovalLedger, Vec<Diagnostic>> {
    let actual = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if actual > limits.max_bytes() {
        return Err(one(
            Code::LimitFileTooLarge,
            format!(
                "input file '{input}' is {actual} bytes, larger than the effective limit of {} bytes",
                limits.max_bytes()
            ),
            Location::Input(InputLocation {
                input: input.clone(),
                line: None,
                column: None,
                field: None,
            }),
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|e| {
        let line = bytes
            .get(..e.valid_up_to())
            .map_or(0, |p| p.iter().filter(|b| **b == b'\n').count())
            + 1;
        one(
            Code::LedgerMalformed,
            format!("ledger line {line} is not valid UTF-8"),
            location(input, line_u32(line), None, None),
        )
    })?;
    let Some(body) = text.strip_suffix('\n') else {
        let line = text.matches('\n').count() + 1;
        return Err(one(
            Code::LedgerMalformed,
            if text.is_empty() {
                "the ledger is empty: the header line is missing".to_owned()
            } else {
                format!("ledger line {line} does not end with a line feed")
            },
            location(input, line_u32(line), None, None),
        ));
    };
    let mut lines = body.split('\n');
    let header = lines.next().unwrap_or("");
    parse_line(input, 1, header, limits)?;
    if header != HEADER_LINE {
        return Err(one(
            Code::LedgerMalformed,
            format!("ledger line 1 must be exactly the header {HEADER_LINE}"),
            location(input, 1, None, None),
        ));
    }
    let mut walk = Walk {
        input,
        limits,
        previous: None,
        seen: BTreeMap::new(),
    };
    let mut records = Vec::new();
    for (index, line) in lines.enumerate() {
        let line_no = line_u32(index + 2);
        let expected = line_u32(index + 1);
        records.push(walk.record(line_no, line, expected)?);
    }
    Ok(ApprovalLedger { records })
}
