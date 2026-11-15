//! `parse_batch`: strict decoding, then the canonical-form check.

use crate::model::{
    proposal_id, Batch, Completion, Evidence, Proposal, ProposalKind, RuleDraft, StoredDiagnostic,
    StoredLocation, StoredResponse, FORMAT_NAME, FORMAT_VERSION,
};
use crate::print::print_batch;
use rt_foundation::{Code, Diagnostic, DocumentId, InputId, InputLocation, Location, RuleId};
use rt_foundation::{MAX_CONTAINER_DEPTH, MAX_PROPOSAL_BATCH_BYTES};
use rt_hash::{sha256, Sha256Hex};
use rt_ir::Identity;
use rt_json::{check_flag_keys, parse, write_compact, ParseLimits, Position, Value};

/// A decoding failure: the field path, its position and the reason.
struct Fail {
    path: String,
    pos: Position,
    message: String,
}

type Res<T> = Result<T, Fail>;

fn fail<T>(path: &str, value: &Value<Position>, message: impl Into<String>) -> Res<T> {
    Err(Fail {
        path: path.to_owned(),
        pos: *value.pos(),
        message: message.into(),
    })
}

fn child(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// Requires an object whose keys are exactly `keys` (any order; the
/// canonical check fixes the order).
fn exact<'a>(
    value: &'a Value<Position>,
    path: &str,
    keys: &[&str],
) -> Res<&'a [rt_json::Member<Position>]> {
    let Some(members) = value.as_object() else {
        return fail(path, value, "must be an object");
    };
    for m in members {
        if !keys.contains(&m.key.as_str()) {
            let at = child(path, &m.key);
            return Err(Fail {
                path: at,
                pos: m.key_pos,
                message: format!("unknown member '{}'", m.key),
            });
        }
    }
    for key in keys {
        if value.get(key).is_none() {
            return fail(path, value, format!("missing member '{key}'"));
        }
    }
    Ok(members)
}

fn member<'a>(
    value: &'a Value<Position>,
    path: &str,
    key: &str,
) -> Res<(&'a Value<Position>, String)> {
    match value.get(key) {
        Some(m) => Ok((&m.value, child(path, key))),
        None => fail(path, value, format!("missing member '{key}'")),
    }
}

fn text(value: &Value<Position>, path: &str) -> Res<String> {
    match value.as_str() {
        Some(s) if !s.is_empty() => Ok(s.to_owned()),
        _ => fail(path, value, "must be a non-empty string"),
    }
}

fn hash(value: &Value<Position>, path: &str) -> Res<Sha256Hex> {
    match value.as_str().and_then(Sha256Hex::parse) {
        Some(h) => Ok(h),
        None => fail(path, value, "must be 64 lowercase hexadecimal characters"),
    }
}

fn uint(value: &Value<Position>, path: &str) -> Res<u32> {
    match value.as_u64().map(u32::try_from) {
        Some(Ok(n)) => Ok(n),
        _ => fail(path, value, "must be a non-negative 32-bit integer"),
    }
}

fn identity(value: &Value<Position>, path: &str) -> Res<Identity> {
    match value.as_str().map(Identity::new) {
        Some(Ok(i)) => Ok(i),
        _ => fail(path, value, "must be an identity of 1 to 200 characters"),
    }
}

fn document(value: &Value<Position>, path: &str) -> Res<DocumentId> {
    match value.as_str().map(DocumentId::new) {
        Some(Ok(d)) => Ok(d),
        _ => fail(path, value, "must be a document identifier"),
    }
}

fn array<'a>(value: &'a Value<Position>, path: &str) -> Res<&'a [Value<Position>]> {
    match value.as_array() {
        Some(a) => Ok(a),
        None => fail(path, value, "must be an array"),
    }
}

fn evidence(value: &Value<Position>, path: &str) -> Res<Evidence> {
    const KEYS: [&str; 7] = [
        "document", "sha256", "start", "end", "line", "column", "quote",
    ];
    exact(value, path, &KEYS)?;
    let get = |key| member(value, path, key);
    let (v, p) = get("document")?;
    let document = document(v, &p)?;
    let (v, p) = get("sha256")?;
    let sha256 = hash(v, &p)?;
    let (v, p) = get("start")?;
    let start = uint(v, &p)?;
    let (v, p) = get("end")?;
    let end = uint(v, &p)?;
    let (v, p) = get("line")?;
    let line = uint(v, &p)?;
    let (v, p) = get("column")?;
    let column = uint(v, &p)?;
    let (v, p) = get("quote")?;
    let quote = text(v, &p)?;
    Ok(Evidence {
        document,
        sha256,
        start,
        end,
        line,
        column,
        quote,
    })
}

fn location(value: &Value<Position>, path: &str) -> Res<StoredLocation> {
    let Some(kind) = value.get("kind").and_then(|m| m.value.as_str()) else {
        return fail(path, value, "missing member 'kind'");
    };
    let get = |key| member(value, path, key);
    match kind {
        "source" => {
            exact(value, path, &["kind", "document", "start", "end"])?;
            let (v, p) = get("document")?;
            let document = document(v, &p)?;
            let (v, p) = get("start")?;
            let start = uint(v, &p)?;
            let (v, p) = get("end")?;
            Ok(StoredLocation::Source {
                document,
                start,
                end: uint(v, &p)?,
            })
        }
        "document" => {
            exact(value, path, &["kind", "document"])?;
            let (v, p) = get("document")?;
            Ok(StoredLocation::Document(document(v, &p)?))
        }
        "field" => {
            exact(value, path, &["kind", "path"])?;
            let (v, p) = get("path")?;
            Ok(StoredLocation::Field(text(v, &p)?))
        }
        _ => fail(path, value, "'kind' must be source, document or field"),
    }
}

fn diagnostic(value: &Value<Position>, path: &str) -> Res<StoredDiagnostic> {
    exact(value, path, &["code", "message", "location"])?;
    let (v, p) = member(value, path, "code")?;
    let Some(code) = v.as_str().and_then(Code::from_id) else {
        return fail(&p, v, "must be a catalogue diagnostic code");
    };
    let (v, p) = member(value, path, "message")?;
    let message = text(v, &p)?;
    let (v, p) = member(value, path, "location")?;
    Ok(StoredDiagnostic {
        code,
        message,
        location: location(v, &p)?,
    })
}

fn draft(value: &Value<Position>, path: &str) -> Res<RuleDraft> {
    let (v, p) = member(value, path, "id")?;
    let Some(Ok(id)) = v.as_str().map(RuleId::new) else {
        return fail(&p, v, "must be a rule identifier");
    };
    Ok(RuleDraft {
        id,
        json: write_compact(value),
    })
}

fn items<T>(
    value: &Value<Position>,
    path: &str,
    each: impl Fn(&Value<Position>, &str) -> Res<T>,
) -> Res<Vec<T>> {
    array(value, path)?
        .iter()
        .enumerate()
        .map(|(i, v)| each(v, &format!("{path}[{i}]")))
        .collect()
}

fn proposal(
    value: &Value<Position>,
    path: &str,
    request_id: &Sha256Hex,
    index: usize,
) -> Res<Proposal> {
    let Some(members) = value.as_object() else {
        return fail(path, value, "must be an object");
    };
    let suggestion = members.iter().any(|m| m.key == "rule");
    let mut keys = vec!["proposal_id", "kind", "proposer", "rationale", "evidence"];
    if suggestion {
        keys.extend(["rule", "draft_hash"]);
    }
    keys.push("outcome");
    exact(value, path, &keys)?;
    let (v, p) = member(value, path, "proposal_id")?;
    let id = hash(v, &p)?;
    if id != proposal_id(request_id, index) {
        return fail(&p, v, "must equal SHA-256 of request_id, ':' and the index");
    }
    let (v, p) = member(value, path, "kind")?;
    let Some(kind) = v.as_str().and_then(ProposalKind::parse) else {
        return fail(&p, v, "must be one of the five proposal kinds");
    };
    if suggestion != (kind == ProposalKind::RuleSuggestion) {
        return fail(
            path,
            value,
            "'rule' is present exactly for a rule_suggestion",
        );
    }
    let (v, p) = member(value, path, "proposer")?;
    let proposer = identity(v, &p)?;
    let (v, p) = member(value, path, "rationale")?;
    let rationale = text(v, &p)?;
    let (v, p) = member(value, path, "evidence")?;
    let evidence = items(v, &p, evidence)?;
    if evidence.is_empty() {
        return fail(&p, v, "needs at least one evidence span");
    }
    let rule = if suggestion {
        let (v, p) = member(value, path, "rule")?;
        let rule = draft(v, &p)?;
        let (v, p) = member(value, path, "draft_hash")?;
        Some((rule, hash(v, &p)?))
    } else {
        None
    };
    let (v, p) = member(value, path, "outcome")?;
    Ok(Proposal {
        proposal_id: id,
        kind,
        proposer,
        rationale,
        evidence,
        rule,
        outcome: items(v, &p, diagnostic)?,
    })
}

fn decode(root: &Value<Position>) -> Res<Batch> {
    const KEYS: [&str; 11] = [
        "format",
        "version",
        "request_id",
        "response_sha256",
        "response",
        "provider",
        "model",
        "adapter_version",
        "ai_identities",
        "created_at",
        "proposals",
    ];
    exact(root, "", &KEYS)?;
    let get = |key| member(root, "", key);
    let (v, p) = get("format")?;
    if v.as_str() != Some(FORMAT_NAME) {
        return fail(&p, v, format!("must be \"{FORMAT_NAME}\""));
    }
    let (v, p) = get("version")?;
    if v.as_u64() != Some(u64::from(FORMAT_VERSION)) {
        return fail(&p, v, format!("must be {FORMAT_VERSION}"));
    }
    let (v, p) = get("request_id")?;
    let request_id = hash(v, &p)?;
    let (v, p) = get("response_sha256")?;
    let response_sha256 = hash(v, &p)?;
    let (v, p) = get("response")?;
    exact(v, &p, &["completion", "payload"])?;
    let (c, cp) = member(v, &p, "completion")?;
    if c.as_str() != Some(Completion::Complete.as_str()) {
        return fail(&cp, c, "must be \"complete\"");
    }
    let (pv, pp) = member(v, &p, "payload")?;
    let Some(payload) = pv.as_str() else {
        return fail(&pp, pv, "must be a string");
    };
    if sha256(payload.as_bytes()) != response_sha256 {
        return fail(&pp, pv, "its SHA-256 must equal 'response_sha256'");
    }
    let response = StoredResponse {
        completion: Completion::Complete,
        payload: payload.to_owned(),
    };
    let (v, p) = get("provider")?;
    let provider = text(v, &p)?;
    let (v, p) = get("model")?;
    let model = text(v, &p)?;
    let (v, p) = get("adapter_version")?;
    let adapter_version = text(v, &p)?;
    let (v, p) = get("ai_identities")?;
    let ai_identities = items(v, &p, identity)?;
    let (v, p) = get("created_at")?;
    let created_at = text(v, &p)?;
    let (v, p) = get("proposals")?;
    let proposals = array(v, &p)?
        .iter()
        .enumerate()
        .map(|(i, item)| proposal(item, &format!("{p}[{i}]"), &request_id, i))
        .collect::<Res<Vec<_>>>()?;
    Ok(Batch {
        request_id,
        response_sha256,
        response,
        provider,
        model,
        adapter_version,
        ai_identities,
        created_at,
        proposals,
    })
}

fn invalid(
    input: &InputId,
    line: Option<u32>,
    column: Option<u32>,
    field: Option<String>,
    why: &str,
) -> Vec<Diagnostic> {
    vec![Diagnostic::new(
        Code::AiStoreInvalid,
        format!("proposal batch '{input}': {why}"),
        Location::Input(InputLocation {
            input: input.clone(),
            line,
            column,
            field,
        }),
    )]
}

/// Parses one batch file.
///
/// # Errors
/// - `RT-LIM-0001` when `bytes` is longer than `MAX_PROPOSAL_BATCH_BYTES`,
///   with the length of `bytes`. A caller that reads a bounded prefix must
///   check the size of the file first, because this length is only the
///   length it was given.
/// - `RT-DGN-0001`, `0002` or `0003` when any object has a flag or
///   lifecycle key. No other check runs first.
/// - `RT-AI-0302` for any other defect: syntax, depth, an unknown or
///   missing member, a proposal identifier that is not
///   `SHA-256(request_id ":" index)`, a payload that does not match
///   `response_sha256`, or bytes that are not the canonical form.
///
/// No batch is returned on error.
pub fn parse_batch(input: &InputId, bytes: &[u8]) -> Result<Batch, Vec<Diagnostic>> {
    let limits = ParseLimits::new(MAX_PROPOSAL_BATCH_BYTES, MAX_CONTAINER_DEPTH);
    let root = parse(input, bytes, limits).map_err(|d| {
        if d.code() == Code::LimitFileTooLarge {
            vec![*d]
        } else {
            invalid(input, None, None, None, d.message())
        }
    })?;
    check_flag_keys(input, &root)?;
    let batch = decode(&root).map_err(|f| {
        invalid(
            input,
            Some(f.pos.line),
            Some(f.pos.column),
            Some(f.path.clone()),
            &format!("field '{}': {}", f.path, f.message),
        )
    })?;
    if print_batch(&batch).as_bytes() != bytes {
        return Err(invalid(input, None, None, None, "not in canonical form"));
    }
    Ok(batch)
}
