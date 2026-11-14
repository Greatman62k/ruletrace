//! Canonical Source_Model serialization and the loader (design §3.5).
//!
//! File form: RT-JSON pretty, fixed key order, full document text embedded:
//! `format`, `version`, `document`, `kind`, `sha256`, `bytes`, `content`,
//! `elements`; an element has `id`, `kind`, `level`, `label`, `start`, `end`,
//! `line`, `column`, `parent`.

use crate::model::{DocumentKind, Element, ElementKind, SourceModel};
use rt_foundation::{
    Code, Diagnostic, DocumentId, InputId, InputLocation, Location, MAX_CONTAINER_DEPTH,
    MAX_SOURCE_MODEL_BYTES,
};
use rt_hash::Sha256Hex;
use rt_json::{parse, write_compact, write_pretty, Kind, ParseLimits, Position, Value};

const FORMAT: &str = "ruletrace-source-model";
const VERSION: u32 = 1;
const MODEL_KEYS: [&str; 8] = [
    "format", "version", "document", "kind", "sha256", "bytes", "content", "elements",
];
const ELEMENT_KEYS: [&str; 9] = [
    "id", "kind", "level", "label", "start", "end", "line", "column", "parent",
];

/// The default limits for loading a Source_Model file: 64 MiB and container
/// depth 64.
#[must_use]
pub fn default_load_limits() -> ParseLimits {
    ParseLimits::new(MAX_SOURCE_MODEL_BYTES, MAX_CONTAINER_DEPTH)
}

fn opt_u32(value: Option<u32>) -> Value<()> {
    value.map_or_else(Value::null, Value::integer)
}

impl SourceModel {
    fn to_value(&self) -> Value<()> {
        let elements = self
            .elements()
            .iter()
            .map(|e| {
                member_object(vec![
                    ("id", Value::integer(e.id)),
                    ("kind", Value::string(e.kind.as_str())),
                    ("level", opt_u32(e.level.map(u32::from))),
                    (
                        "label",
                        e.label.as_ref().map_or_else(Value::null, Value::string),
                    ),
                    ("start", Value::integer(e.start)),
                    ("end", Value::integer(e.end)),
                    ("line", Value::integer(e.line)),
                    ("column", Value::integer(e.column)),
                    ("parent", opt_u32(e.parent)),
                ])
            })
            .collect();
        member_object(vec![
            ("format", Value::string(FORMAT)),
            ("version", Value::integer(VERSION)),
            ("document", Value::string(self.document().as_str())),
            ("kind", Value::string(self.kind().as_str())),
            ("sha256", Value::string(self.sha256().as_str())),
            (
                "bytes",
                // The size check at construction keeps the length within 32
                // bits.
                Value::integer(u32::try_from(self.byte_len()).unwrap_or(u32::MAX)),
            ),
            ("content", Value::string(self.content())),
            ("elements", Value::array(elements)),
        ])
    }

    /// The canonical file form: RT-JSON pretty with the fixed key order, ending
    /// in a line feed. The same model always gives the same bytes.
    #[must_use]
    pub fn print(&self) -> String {
        write_pretty(&self.to_value())
    }

    /// The compact form of the same value, which is the form that is hashed.
    #[must_use]
    pub fn print_compact(&self) -> String {
        write_compact(&self.to_value())
    }

    /// Loads a Source_Model file. Every check of the model is made: the shape
    /// of the file, the SHA-256 of `content` against `sha256`, `bytes` against
    /// the content length, and each element against the text.
    ///
    /// # Errors
    /// - `RT-LIM-0001` or `RT-LIM-0002` when the file exceeds the limits.
    /// - `RT-PRV-0005` for any other failure: not valid RT-JSON, a wrong or
    ///   missing field, a hash or length mismatch, or an element that does not
    ///   match the text. A failure inside the file has an input location with
    ///   line and column and a field path. A hash, length or element failure
    ///   has the Document_Location of the document.
    pub fn load(
        input: &InputId,
        bytes: &[u8],
        limits: ParseLimits,
    ) -> Result<SourceModel, Box<Diagnostic>> {
        let root = match parse(input, bytes, limits) {
            Ok(root) => root,
            Err(d)
                if matches!(
                    d.code(),
                    Code::LimitFileTooLarge | Code::LimitNestingTooDeep
                ) =>
            {
                return Err(d)
            }
            Err(d) => {
                return Err(Box::new(Diagnostic::new(
                    Code::ProvSourceModelInvalid,
                    format!("Source_Model is not valid RT-JSON: {}", d.message()),
                    d.primary().clone(),
                )))
            }
        };
        decode(input, &root)
    }
}

fn member_object(members: Vec<(&str, Value<()>)>) -> Value<()> {
    // The keys above are distinct constants, so building cannot fail; the
    // fallback is unreachable.
    Value::object(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
    .unwrap_or_else(|_| Value::null())
}

fn invalid(input: &InputId, at: Option<Position>, field: &str, message: &str) -> Box<Diagnostic> {
    Box::new(Diagnostic::new(
        Code::ProvSourceModelInvalid,
        format!("Source_Model is invalid: {field}: {message}"),
        Location::Input(InputLocation {
            input: input.clone(),
            line: at.map(|p| p.line),
            column: at.map(|p| p.column),
            field: Some(field.to_owned()),
        }),
    ))
}

/// The members of an object that has exactly the given keys.
fn exact_object<'a>(
    input: &InputId,
    value: &'a Value<Position>,
    keys: &[&str],
    path: &str,
) -> Result<&'a [rt_json::Member<Position>], Box<Diagnostic>> {
    let at = Some(*value.pos());
    let members = value
        .as_object()
        .ok_or_else(|| invalid(input, at, path, "expected an object"))?;
    for m in members {
        if !keys.contains(&m.key.as_str()) {
            let field = join(path, &m.key);
            return Err(invalid(input, Some(m.key_pos), &field, "unknown key"));
        }
    }
    for key in keys {
        if !members.iter().any(|m| m.key == *key) {
            let field = join(path, key);
            return Err(invalid(input, at, &field, "missing key"));
        }
    }
    Ok(members)
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

struct Fields<'a> {
    input: &'a InputId,
    path: String,
    members: &'a [rt_json::Member<Position>],
}

impl<'a> Fields<'a> {
    fn value(&self, key: &str) -> Result<&'a Value<Position>, Box<Diagnostic>> {
        self.members
            .iter()
            .find(|m| m.key == key)
            .map(|m| &m.value)
            .ok_or_else(|| invalid(self.input, None, &join(&self.path, key), "missing key"))
    }

    fn err(&self, key: &str, value: &Value<Position>, message: &str) -> Box<Diagnostic> {
        invalid(
            self.input,
            Some(*value.pos()),
            &join(&self.path, key),
            message,
        )
    }

    fn string(&self, key: &str) -> Result<&'a str, Box<Diagnostic>> {
        let v = self.value(key)?;
        v.as_str()
            .ok_or_else(|| self.err(key, v, "expected a string"))
    }

    fn u32(&self, key: &str) -> Result<u32, Box<Diagnostic>> {
        let v = self.value(key)?;
        v.as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| self.err(key, v, "expected an integer from 0 to 4294967295"))
    }

    fn opt_u32(&self, key: &str) -> Result<Option<u32>, Box<Diagnostic>> {
        let v = self.value(key)?;
        if matches!(v.kind(), Kind::Null) {
            return Ok(None);
        }
        self.u32(key).map(Some)
    }

    fn opt_string(&self, key: &str) -> Result<Option<String>, Box<Diagnostic>> {
        let v = self.value(key)?;
        if matches!(v.kind(), Kind::Null) {
            return Ok(None);
        }
        self.string(key).map(|s| Some(s.to_owned()))
    }
}

fn decode(input: &InputId, root: &Value<Position>) -> Result<SourceModel, Box<Diagnostic>> {
    let members = exact_object(input, root, &MODEL_KEYS, "")?;
    let f = Fields {
        input,
        path: String::new(),
        members,
    };
    if f.string("format")? != FORMAT {
        return Err(f.err("format", f.value("format")?, "unknown format"));
    }
    if f.u32("version")? != VERSION {
        return Err(f.err("version", f.value("version")?, "unsupported version"));
    }
    let document = DocumentId::new(f.string("document")?).map_err(|e| {
        f.err(
            "document",
            f.value("document").unwrap_or(root),
            &e.to_string(),
        )
    })?;
    let kind = match f.string("kind")? {
        "markdown" => DocumentKind::Markdown,
        "text" => DocumentKind::Text,
        _ => {
            return Err(f.err(
                "kind",
                f.value("kind")?,
                "expected \"markdown\" or \"text\"",
            ))
        }
    };
    let sha = Sha256Hex::parse(f.string("sha256")?).ok_or_else(|| {
        f.err(
            "sha256",
            f.value("sha256").unwrap_or(root),
            "expected 64 lowercase hexadecimal characters",
        )
    })?;
    let recorded_bytes = f.u32("bytes")?;
    let content = f.string("content")?;
    let elements_value = f.value("elements")?;
    let items = elements_value
        .as_array()
        .ok_or_else(|| f.err("elements", elements_value, "expected an array"))?;
    let mut elements = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        elements.push(decode_element(input, item, index)?);
    }

    let at_document = |message: String| {
        Box::new(Diagnostic::new(
            Code::ProvSourceModelInvalid,
            format!("Source_Model is invalid: {message}"),
            Location::Document(document.clone()),
        ))
    };
    if usize::try_from(recorded_bytes).ok() != Some(content.len()) {
        return Err(at_document(crate::ModelError::LengthMismatch.to_string()));
    }
    SourceModel::from_claimed(document.clone(), kind, sha, content.to_owned(), elements)
        .map_err(|e| at_document(e.to_string()))
}

fn decode_element(
    input: &InputId,
    item: &Value<Position>,
    index: usize,
) -> Result<Element, Box<Diagnostic>> {
    let path = format!("elements[{index}]");
    let members = exact_object(input, item, &ELEMENT_KEYS, &path)?;
    let f = Fields {
        input,
        path,
        members,
    };
    let kind_text = f.string("kind")?;
    let kind = ElementKind::from_name(kind_text).ok_or_else(|| {
        f.err(
            "kind",
            f.value("kind").unwrap_or(item),
            "unknown element kind",
        )
    })?;
    let level = match f.opt_u32("level")? {
        None => None,
        Some(n) => Some(u8::try_from(n).map_err(|_| {
            f.err(
                "level",
                f.value("level").unwrap_or(item),
                "level out of range",
            )
        })?),
    };
    Ok(Element {
        id: f.u32("id")?,
        kind,
        level,
        label: f.opt_string("label")?,
        start: f.u32("start")?,
        end: f.u32("end")?,
        line: f.u32("line")?,
        column: f.u32("column")?,
        parent: f.opt_u32("parent")?,
    })
}
