//! Rendering of Diagnostics for standard error (Req 17.4, 19.5, 19.10).
//!
//! Each Diagnostic is a line group: the severity, code and message on the first line, then one
//! indented line for the primary Location and one for each related Location. Every piece of
//! text goes through `escape_controls`, so the only control character in the output is the line
//! feed that ends each line.

use rt_foundation::{
    escape_controls, CliLocation, Diagnostic, DiagnosticState, Location, Severity,
};
use rt_json::{write_compact, Position, Value};

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Ambiguity => "ambiguity",
        Severity::Unsupported => "unsupported",
    }
}

fn location(location: &Location) -> String {
    match location {
        Location::Source(s) => format!(
            "source {}:{}-{} (line {}, column {})",
            escape_controls(s.doc().as_str()),
            s.start().get(),
            s.end().get(),
            s.line(),
            s.column()
        ),
        Location::Document(d) => format!("document '{}'", escape_controls(d.as_str())),
        Location::Input(i) => {
            let mut text = format!("input '{}'", escape_controls(i.input.as_str()));
            if let Some(line) = i.line {
                text.push_str(&format!(" line {line}"));
            }
            if let Some(column) = i.column {
                text.push_str(&format!(" column {column}"));
            }
            if let Some(field) = &i.field {
                text.push_str(&format!(" field {}", escape_controls(field)));
            }
            text
        }
        Location::Cli(CliLocation::Argument { position, name }) => match name {
            Some(name) => format!("argument {position} ({})", escape_controls(name)),
            None => format!("argument {position}"),
        },
        Location::Cli(CliLocation::Command(name)) => {
            format!("command '{}'", escape_controls(name))
        }
        Location::None(reason) => format!("no location ({})", reason.id()),
    }
}

/// One Diagnostic as a line group, every line ending in a line feed.
#[must_use]
pub fn diagnostic(d: &Diagnostic) -> String {
    let mut text = format!(
        "{} {}: {}\n  at {}\n",
        severity_name(d.severity()),
        d.code().id(),
        escape_controls(d.message()).replace('\n', "\\u{a}"),
        location(d.primary())
    );
    for related in d.related() {
        text.push_str(&format!("  related {}\n", location(related)));
    }
    text
}

fn stored_location(v: &Value<Position>) -> Option<String> {
    let text = |key: &str| v.get(key).and_then(|m| m.value.as_str());
    let number = |key: &str| v.get(key).and_then(|m| m.value.as_u64());
    match text("kind")? {
        "source" => Some(format!(
            "source {}:{}-{} (line {}, column {})",
            escape_controls(text("document")?),
            number("start")?,
            number("end")?,
            number("line")?,
            number("column")?
        )),
        "document" => Some(format!("document '{}'", escape_controls(text("document")?))),
        "input" => {
            let mut out = format!("input '{}'", escape_controls(text("input")?));
            if let Some(line) = number("line") {
                out.push_str(&format!(" line {line}"));
            }
            if let Some(column) = number("column") {
                out.push_str(&format!(" column {column}"));
            }
            if let Some(field) = text("field") {
                out.push_str(&format!(" field {}", escape_controls(field)));
            }
            Some(out)
        }
        "cli_argument" => Some(match text("name") {
            Some(name) => format!(
                "argument {} ({})",
                number("position")?,
                escape_controls(name)
            ),
            None => format!("argument {}", number("position")?),
        }),
        "cli_command" => Some(format!("command '{}'", escape_controls(text("command")?))),
        "none" => Some(format!("no location ({})", text("reason")?)),
        _ => None,
    }
}

/// A Diagnostic that a Validation_Result artifact stores, as the line group of [`diagnostic`],
/// read from its RT-JSON form without changing any of its parts. `None` when the value is not a
/// Diagnostic of the form of [`diagnostic_value`].
#[must_use]
pub fn stored(v: &Value<Position>) -> Option<String> {
    let text = |key: &str| v.get(key).and_then(|m| m.value.as_str());
    let mut out = format!(
        "{} {}: {}\n  at {}\n",
        text("severity")?,
        text("code")?,
        escape_controls(text("message")?).replace('\n', "\\u{a}"),
        stored_location(&v.get("primary")?.value)?
    );
    for related in v.get("related")?.value.as_array()? {
        out.push_str(&format!("  related {}\n", stored_location(related)?));
    }
    Some(out)
}

/// A Diagnostic list in canonical order (Req 7.10, 17.12).
#[must_use]
pub fn diagnostics(state: &DiagnosticState) -> String {
    state
        .clone()
        .sorted()
        .as_slice()
        .iter()
        .map(diagnostic)
        .collect()
}

fn text(value: &str) -> Value<()> {
    Value::string(value)
}

fn opt_int(value: Option<u32>) -> Value<()> {
    value.map_or_else(Value::null, Value::integer)
}

fn opt_text(value: Option<&str>) -> Value<()> {
    value.map_or_else(Value::null, Value::string)
}

fn object(members: Vec<(&str, Value<()>)>) -> Value<()> {
    // The keys are distinct constants at every call, so building cannot fail; the fallback is
    // unreachable.
    Value::object(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
    .unwrap_or_else(|_| Value::null())
}

fn location_value(location: &Location) -> Value<()> {
    match location {
        Location::Source(s) => object(vec![
            ("kind", text("source")),
            ("document", text(s.doc().as_str())),
            ("start", Value::integer(s.start().get())),
            ("end", Value::integer(s.end().get())),
            ("line", Value::integer(s.line())),
            ("column", Value::integer(s.column())),
        ]),
        Location::Document(d) => object(vec![
            ("kind", text("document")),
            ("document", text(d.as_str())),
        ]),
        Location::Input(i) => object(vec![
            ("kind", text("input")),
            ("input", text(i.input.as_str())),
            ("line", opt_int(i.line)),
            ("column", opt_int(i.column)),
            ("field", opt_text(i.field.as_deref())),
        ]),
        Location::Cli(CliLocation::Argument { position, name }) => object(vec![
            ("kind", text("cli_argument")),
            ("position", Value::integer(*position)),
            ("name", opt_text(name.as_deref())),
        ]),
        Location::Cli(CliLocation::Command(c)) => {
            object(vec![("kind", text("cli_command")), ("command", text(c))])
        }
        Location::None(code) => object(vec![("kind", text("none")), ("reason", text(code.id()))]),
    }
}

/// One Diagnostic as an RT-JSON value with the keys `code`, `severity`, `message`, `primary`,
/// `related` and `rules`. This is the form of the Validation_Result artifact and of
/// `--output-format json`.
#[must_use]
pub fn diagnostic_value(d: &Diagnostic) -> Value<()> {
    let rules = d
        .rules()
        .iter()
        .map(|r| {
            object(vec![
                ("rule", text(r.rule.as_str())),
                ("revision", opt_int(r.revision.map(|v| v.get()))),
            ])
        })
        .collect();
    object(vec![
        ("code", text(d.code().id())),
        ("severity", text(severity_name(d.severity()))),
        ("message", text(d.message())),
        ("primary", location_value(d.primary())),
        (
            "related",
            Value::array(d.related().iter().map(location_value).collect()),
        ),
        ("rules", Value::array(rules)),
    ])
}

/// The Diagnostics in canonical order as one compact RT-JSON array on one line.
#[must_use]
pub fn diagnostics_json(state: &DiagnosticState) -> String {
    let sorted = state.clone().sorted();
    let items = sorted.as_slice().iter().map(diagnostic_value).collect();
    let mut line = write_compact(&Value::array(items));
    line.push('\n');
    line
}
