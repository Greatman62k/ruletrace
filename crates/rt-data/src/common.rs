//! Shared decoding helpers: positioned diagnostics, header checks and the file
//! loader.

use rt_foundation::{
    escape_controls, Code, Diagnostic, DiagnosticState, InputId, InputLocation, Location,
    MAX_COMPETITION_DATA_BYTES, MAX_CONTAINER_DEPTH,
};
use rt_json::{check_flag_keys, parse, Kind, Member, ParseLimits, Position, Value};

/// The default limits for loading one of the three files: 32 MiB and container
/// depth 64.
#[must_use]
pub fn default_parse_limits() -> ParseLimits {
    ParseLimits::new(MAX_COMPETITION_DATA_BYTES, MAX_CONTAINER_DEPTH)
}

pub(crate) const VERSION: u32 = 1;

/// Collects the diagnostics of one decode so that every fault of a file is
/// reported.
pub(crate) struct Ctx<'a> {
    pub(crate) input: &'a InputId,
    pub(crate) diags: Vec<Diagnostic>,
}

pub(crate) fn join(path: &str, key: &str) -> String {
    let key = escape_controls(key);
    if path.is_empty() {
        key
    } else {
        format!("{path}.{key}")
    }
}

impl<'a> Ctx<'a> {
    pub(crate) fn location(&self, at: Option<Position>, field: &str) -> Location {
        Location::Input(InputLocation {
            input: self.input.clone(),
            line: at.map(|p| p.line),
            column: at.map(|p| p.column),
            field: Some(field.to_owned()),
        })
    }

    pub(crate) fn push(&mut self, code: Code, at: Option<Position>, field: &str, msg: &str) {
        let location = self.location(at, field);
        self.diags
            .push(Diagnostic::new(code, format!("{field}: {msg}"), location));
    }

    pub(crate) fn bad(&mut self, at: Option<Position>, field: &str, msg: &str) {
        self.push(Code::DataMalformed, at, field, msg);
    }

    /// An object whose keys are all in `allowed`; each key of `required` must
    /// be present. Faults are recorded and the members are still returned so
    /// that decoding continues.
    pub(crate) fn object<'v>(
        &mut self,
        value: &'v Value<Position>,
        path: &str,
        allowed: &[&str],
        required: &[&str],
    ) -> Option<&'v [Member<Position>]> {
        let Some(members) = value.as_object() else {
            self.bad(Some(*value.pos()), display(path), "expected an object");
            return None;
        };
        for m in members {
            if !allowed.contains(&m.key.as_str()) {
                self.bad(Some(m.key_pos), &join(path, &m.key), "unknown key");
            }
        }
        for key in required {
            if !members.iter().any(|m| m.key == *key) {
                self.bad(Some(*value.pos()), &join(path, key), "missing key");
            }
        }
        Some(members)
    }

    pub(crate) fn string<'v>(&mut self, value: &'v Value<Position>, path: &str) -> Option<&'v str> {
        let s = value.as_str();
        if s.is_none() {
            self.bad(Some(*value.pos()), path, "expected a string");
        }
        s
    }

    pub(crate) fn array<'v>(
        &mut self,
        value: &'v Value<Position>,
        path: &str,
    ) -> Option<&'v [Value<Position>]> {
        let a = value.as_array();
        if a.is_none() {
            self.bad(Some(*value.pos()), path, "expected an array");
        }
        a
    }

    /// Checks `format` and `version` of an opened root object.
    pub(crate) fn header(&mut self, members: &[Member<Position>], format: &str) {
        for key in ["format", "version"] {
            let Some(m) = members.iter().find(|m| m.key == key) else {
                continue;
            };
            let ok = if key == "format" {
                m.value.as_str() == Some(format)
            } else {
                m.value.as_u64() == Some(u64::from(VERSION))
            };
            if !ok {
                let msg = if key == "format" {
                    format!("expected format \"{format}\"")
                } else {
                    format!("unsupported version, expected {VERSION}")
                };
                self.bad(Some(*m.value.pos()), key, &msg);
            }
        }
    }

    pub(crate) fn finish<T>(self, value: Option<T>) -> Result<T, DiagnosticState> {
        match value {
            Some(v) if self.diags.is_empty() => Ok(v),
            _ => Err(into_state(self.diags)),
        }
    }
}

fn display(path: &str) -> &str {
    if path.is_empty() {
        "(document)"
    } else {
        path
    }
}

pub(crate) fn into_state(diags: Vec<Diagnostic>) -> DiagnosticState {
    let mut state = DiagnosticState::empty();
    for d in diags {
        state.push(d);
    }
    state.sorted()
}

/// Reads and parses one file. Size and depth faults keep their own codes
/// (`RT-LIM-0001`, `RT-LIM-0002`); any other syntax fault becomes
/// `RT-EXE-0009`. Flag and lifecycle keys are rejected under Req 22.22 before
/// anything else is read.
pub(crate) fn load(
    input: &InputId,
    bytes: &[u8],
    limits: ParseLimits,
) -> Result<Value<Position>, DiagnosticState> {
    let root = match parse(input, bytes, limits) {
        Ok(root) => root,
        Err(d) => {
            let d = if matches!(
                d.code(),
                Code::LimitFileTooLarge | Code::LimitNestingTooDeep
            ) {
                *d
            } else {
                Diagnostic::new(
                    Code::DataMalformed,
                    format!("not valid RT-JSON: {}", d.message()),
                    d.primary().clone(),
                )
            };
            return Err(into_state(vec![d]));
        }
    };
    check_flag_keys(input, &root).map_err(into_state)?;
    Ok(root)
}

/// A `Kind` name for messages.
pub(crate) fn describe(kind: &Kind<Position>) -> &'static str {
    match kind {
        Kind::Null => "null",
        Kind::Bool(_) => "a boolean",
        Kind::Integer(_) | Kind::Number(_) => "a number",
        Kind::String(_) => "a string",
        Kind::Array(_) => "an array",
        Kind::Object(_) => "an object",
    }
}
