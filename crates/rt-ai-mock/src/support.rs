//! Errors shared by the two script formats and small decoding helpers.

use rt_foundation::Diagnostic;
use rt_json::{Member, Position, Value};
use std::fmt;

/// Why a script file could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptError {
    /// The bytes are not RT-JSON, or exceed the parse limits.
    Parse(Box<Diagnostic>),
    /// The file supplies a diagnostic-state or lifecycle flag key
    /// (Req 22.22). Every finding is listed.
    FlagKeys(Vec<Diagnostic>),
    /// The file is valid JSON but not a valid script.
    Invalid(ScriptInvalid),
}

/// A schema or consistency problem with its position in the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptInvalid {
    /// 1-based line of the offending value.
    pub line: u32,
    /// 1-based column of the offending value.
    pub column: u32,
    /// Field path such as `steps[1].result`.
    pub path: String,
    /// What is wrong.
    pub reason: String,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(d) => write!(f, "script is not valid RT-JSON: {}", d.message()),
            Self::FlagKeys(list) => write!(f, "script supplies {} derived flag key(s)", list.len()),
            Self::Invalid(i) => write!(
                f,
                "invalid script at {}:{} ({}): {}",
                i.line, i.column, i.path, i.reason
            ),
        }
    }
}

pub(crate) fn invalid(at: &Value<Position>, path: &str, reason: &str) -> ScriptError {
    let pos = at.pos();
    ScriptError::Invalid(ScriptInvalid {
        line: pos.line,
        column: pos.column,
        path: path.to_owned(),
        reason: reason.to_owned(),
    })
}

/// Object members with every key checked against `allowed` and each
/// required key present exactly once (the parser rejects duplicates).
pub(crate) fn members<'a>(
    value: &'a Value<Position>,
    path: &str,
    allowed: &[&str],
    required: &[&str],
) -> Result<&'a [Member<Position>], ScriptError> {
    let Some(members) = value.as_object() else {
        return Err(invalid(value, path, "expected an object"));
    };
    for member in members {
        if !allowed.contains(&member.key.as_str()) {
            let field = format!("{path}.{}", member.key);
            let at = &member.value;
            return Err(invalid(at, &field, "unknown field"));
        }
    }
    for key in required {
        if value.get(key).is_none() {
            let field = format!("{path}.{key}");
            return Err(invalid(value, &field, "missing field"));
        }
    }
    Ok(members)
}

pub(crate) fn text<'a>(
    value: &'a Value<Position>,
    key: &str,
    path: &str,
) -> Result<&'a str, ScriptError> {
    let Some(member) = value.get(key) else {
        return Err(invalid(value, &format!("{path}.{key}"), "missing field"));
    };
    member
        .value
        .as_str()
        .ok_or_else(|| invalid(&member.value, &format!("{path}.{key}"), "expected a string"))
}
