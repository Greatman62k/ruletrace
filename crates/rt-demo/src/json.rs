//! Small accessors over the positioned RT-JSON tree, shared by the manifest and
//! runbook parsers.

use rt_foundation::InputId;
use rt_json::{check_flag_keys, parse, Kind, ParseLimits, Position, Value};

/// Parse limits for demonstration files.
const LIMITS: ParseLimits = ParseLimits::new(8 * 1024 * 1024, 64);

/// Parses `bytes` strictly and rejects flag and lifecycle keys (Req 22.22).
pub(crate) fn parse_strict(label: &str, bytes: &[u8]) -> Result<Value<Position>, String> {
    let input = InputId::new(label).map_err(|e| format!("bad input name: {e}"))?;
    let doc = parse(&input, bytes, LIMITS).map_err(|d| d.message().to_owned())?;
    check_flag_keys(&input, &doc).map_err(|ds| {
        ds.first()
            .map_or_else(String::new, |d| d.message().to_owned())
    })?;
    Ok(doc)
}

/// A required field of an object: the member value, or the reason it is
/// unusable.
pub(crate) fn field<'a>(
    object: &'a Value<Position>,
    key: &str,
) -> Result<&'a Value<Position>, String> {
    match object.kind() {
        Kind::Object(_) => object
            .get(key)
            .map(|m| &m.value)
            .ok_or_else(|| "field is missing".to_owned()),
        _ => Err("value is not an object".to_owned()),
    }
}

/// A required string field. An empty or blank string counts as missing.
pub(crate) fn string<'a>(object: &'a Value<Position>, key: &str) -> Result<&'a str, String> {
    let v = field(object, key)?;
    match v.as_str() {
        Some(s) if s.trim().is_empty() => Err("field is empty".to_owned()),
        Some(s) => Ok(s),
        None => Err("field is not a string".to_owned()),
    }
}

/// A required boolean field.
pub(crate) fn boolean(object: &Value<Position>, key: &str) -> Result<bool, String> {
    match field(object, key)?.kind() {
        Kind::Bool(b) => Ok(*b),
        _ => Err("field is not a boolean".to_owned()),
    }
}

/// A required non-negative integer field.
pub(crate) fn integer(object: &Value<Position>, key: &str) -> Result<u64, String> {
    field(object, key)?
        .as_u64()
        .ok_or_else(|| "field is not a non-negative integer".to_owned())
}

/// A required array field.
pub(crate) fn array<'a>(
    object: &'a Value<Position>,
    key: &str,
) -> Result<&'a [Value<Position>], String> {
    field(object, key)?
        .as_array()
        .ok_or_else(|| "field is not an array".to_owned())
}

/// A relative path with no `..` segment, no root and no drive letter.
pub(crate) fn check_relative(path: &str) -> Result<(), String> {
    let drive = path.as_bytes().get(1) == Some(&b':');
    if path.starts_with('/') || path.starts_with('\\') || drive {
        return Err(format!("'{path}' is not a relative path"));
    }
    if path.split(['/', '\\']).any(|s| s == "..") {
        return Err(format!("'{path}' leaves the demonstration directory"));
    }
    Ok(())
}
