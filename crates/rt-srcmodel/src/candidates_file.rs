//! The candidates file: the printed and parsed form of a Candidate_Clause list
//! (design §3.5).
//!
//! File form: RT-JSON pretty, fixed key order: `format`, `version`, `document`,
//! `clauses`; a clause has `document`, `start`, `end`, `line`, `column`,
//! `label`, `text`. Parsing checks every clause against the Source_Model of the
//! document, so a parsed list holds verified spans and the exact source text
//! only.

use crate::{CandidateClause, SourceModel};
use rt_foundation::{Code, Diagnostic, DocumentId, InputId, SourceSpan};
use rt_json::{parse, write_pretty, Kind, Member, ParseLimits, Position, Value};
use std::fmt;

const FORMAT: &str = "ruletrace-candidates";
const VERSION: u32 = 1;
const FILE_KEYS: [&str; 4] = ["format", "version", "document", "clauses"];
const CLAUSE_KEYS: [&str; 7] = [
    "document", "start", "end", "line", "column", "label", "text",
];

/// Why a candidates file was refused. The catalogue has no code for this
/// failure, so the caller that owns the command chooses how to report it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidatesError {
    /// The file exceeds a size or nesting limit (`RT-LIM-0001` or
    /// `RT-LIM-0002`).
    Limit(Box<Diagnostic>),
    /// The file is not valid RT-JSON, has a wrong shape, or disagrees with the
    /// Source_Model.
    Invalid {
        /// What is wrong.
        message: String,
        /// 1-based line and column in the file, when known.
        at: Option<(u32, u32)>,
        /// Field path, such as `clauses[1].text`, when known.
        field: Option<String>,
    },
}

impl fmt::Display for CandidatesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CandidatesError::Limit(d) => f.write_str(d.message()),
            CandidatesError::Invalid { message, field, .. } => match field {
                Some(field) => write!(f, "candidates file is invalid: {field}: {message}"),
                None => write!(f, "candidates file is invalid: {message}"),
            },
        }
    }
}

impl std::error::Error for CandidatesError {}

fn invalid(at: Option<Position>, field: &str, message: &str) -> CandidatesError {
    CandidatesError::Invalid {
        message: message.to_owned(),
        at: at.map(|p| (p.line, p.column)),
        field: (!field.is_empty()).then(|| field.to_owned()),
    }
}

fn member_object(members: Vec<(&str, Value<()>)>) -> Value<()> {
    // The keys used are distinct constants, so building cannot fail; the
    // fallback is unreachable.
    Value::object(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
    .unwrap_or_else(|_| Value::null())
}

/// The canonical candidates file: RT-JSON pretty with the fixed key order,
/// ending in a line feed. The same list always gives the same bytes. Offsets
/// are 32-bit by the model's limit.
#[must_use]
pub fn print_candidates(document: &DocumentId, clauses: &[CandidateClause]) -> String {
    let items = clauses
        .iter()
        .map(|c| {
            let span = c.span();
            member_object(vec![
                ("document", Value::string(span.doc().as_str())),
                ("start", Value::integer(span.start().get())),
                ("end", Value::integer(span.end().get())),
                ("line", Value::integer(span.line())),
                ("column", Value::integer(span.column())),
                ("label", c.label().map_or_else(Value::null, Value::string)),
                ("text", Value::string(c.text())),
            ])
        })
        .collect();
    write_pretty(&member_object(vec![
        ("format", Value::string(FORMAT)),
        ("version", Value::integer(VERSION)),
        ("document", Value::string(document.as_str())),
        ("clauses", Value::array(items)),
    ]))
}

/// Parses a candidates file for `model` and verifies it against the model.
///
/// Every clause must name the document of `model`, have a span that the model
/// accepts (inside the document, on character boundaries, with the stated line
/// and column), carry text equal to the span bytes, and start after the
/// previous clause.
///
/// # Errors
/// [`CandidatesError`] for the first failure found.
pub fn parse_candidates(
    input: &InputId,
    bytes: &[u8],
    limits: ParseLimits,
    model: &SourceModel,
) -> Result<Vec<CandidateClause>, CandidatesError> {
    let root = parse(input, bytes, limits).map_err(|d| {
        if matches!(
            d.code(),
            Code::LimitFileTooLarge | Code::LimitNestingTooDeep
        ) {
            CandidatesError::Limit(d)
        } else {
            let at = match d.primary() {
                rt_foundation::Location::Input(l) => l.line.zip(l.column),
                _ => None,
            };
            CandidatesError::Invalid {
                message: format!("not valid RT-JSON: {}", d.message()),
                at,
                field: None,
            }
        }
    })?;
    let members = exact_object(&root, &FILE_KEYS, "")?;
    let file = Fields { path: "", members };
    if file.string("format")? != FORMAT {
        return Err(file.err("format", "unknown format"));
    }
    if file.u32("version")? != VERSION {
        return Err(file.err("version", "unsupported version"));
    }
    if file.string("document")? != model.document().as_str() {
        return Err(file.err(
            "document",
            "names a different document than the Source_Model",
        ));
    }
    let items = file
        .value("clauses")?
        .as_array()
        .ok_or_else(|| file.err("clauses", "expected an array"))?;
    let mut clauses: Vec<CandidateClause> = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let clause = decode_clause(item, index, model)?;
        if let Some(previous) = clauses.last() {
            if clause.span().start() <= previous.span().start() {
                let path = format!("clauses[{index}]");
                return Err(invalid(
                    Some(*item.pos()),
                    &path,
                    "clauses are not in ascending start order",
                ));
            }
        }
        clauses.push(clause);
    }
    Ok(clauses)
}

fn decode_clause(
    item: &Value<Position>,
    index: usize,
    model: &SourceModel,
) -> Result<CandidateClause, CandidatesError> {
    let path = format!("clauses[{index}]");
    let members = exact_object(item, &CLAUSE_KEYS, &path)?;
    let f = Fields {
        path: &path,
        members,
    };
    if f.string("document")? != model.document().as_str() {
        return Err(f.err(
            "document",
            "names a different document than the Source_Model",
        ));
    }
    let (start, end) = (f.u32("start")?, f.u32("end")?);
    let label = f.opt_string("label")?;
    let clause = CandidateClause::new(model, start, end, label)
        .map_err(|e| f.err("start", &e.to_string()))?;
    check_position(&f, clause.span())?;
    if f.string("text")? != clause.text() {
        return Err(f.err("text", "differs from the source text of the span"));
    }
    Ok(clause)
}

fn check_position(f: &Fields<'_>, span: &SourceSpan) -> Result<(), CandidatesError> {
    if f.u32("line")? != span.line() {
        return Err(f.err("line", "does not match the start offset"));
    }
    if f.u32("column")? != span.column() {
        return Err(f.err("column", "does not match the start offset"));
    }
    Ok(())
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// The members of an object that has exactly the given keys.
fn exact_object<'a>(
    value: &'a Value<Position>,
    keys: &[&str],
    path: &str,
) -> Result<&'a [Member<Position>], CandidatesError> {
    let at = Some(*value.pos());
    let members = value
        .as_object()
        .ok_or_else(|| invalid(at, path, "expected an object"))?;
    for m in members {
        if !keys.contains(&m.key.as_str()) {
            return Err(invalid(Some(m.key_pos), &join(path, &m.key), "unknown key"));
        }
    }
    for key in keys {
        if !members.iter().any(|m| m.key == *key) {
            return Err(invalid(at, &join(path, key), "missing key"));
        }
    }
    Ok(members)
}

struct Fields<'a> {
    path: &'a str,
    members: &'a [Member<Position>],
}

impl<'a> Fields<'a> {
    fn value(&self, key: &str) -> Result<&'a Value<Position>, CandidatesError> {
        self.members
            .iter()
            .find(|m| m.key == key)
            .map(|m| &m.value)
            .ok_or_else(|| invalid(None, &join(self.path, key), "missing key"))
    }

    fn err(&self, key: &str, message: &str) -> CandidatesError {
        let at = self.value(key).ok().map(|v| *v.pos());
        invalid(at, &join(self.path, key), message)
    }

    fn string(&self, key: &str) -> Result<&'a str, CandidatesError> {
        self.value(key)?
            .as_str()
            .ok_or_else(|| self.err(key, "expected a string"))
    }

    fn u32(&self, key: &str) -> Result<u32, CandidatesError> {
        self.value(key)?
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| self.err(key, "expected an integer from 0 to 4294967295"))
    }

    fn opt_string(&self, key: &str) -> Result<Option<String>, CandidatesError> {
        if matches!(self.value(key)?.kind(), Kind::Null) {
            return Ok(None);
        }
        self.string(key).map(|s| Some(s.to_owned()))
    }
}
