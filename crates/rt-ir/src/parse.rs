//! The Rule_IR parser (design §4.1.1 to §4.1.5, Req 3, 4, 19, 22).
//!
//! [`parse_rulebook`] reads strict RT-JSON with `rt-json` and decodes it into a
//! [`Rulebook`]. It gathers every parse diagnostic of the document, sorts them
//! into the canonical order of Req 7.10 and returns no Rulebook when there is
//! one. Nothing is inferred, defaulted from another field or guessed: an absent
//! `type` or `t` stays absent for the Validator to report.
//!
//! Diagnostic codes of this stage:
//!
//! | Code | Where |
//! | --- | --- |
//! | `RT-LIM-0001`, `RT-LIM-0002` | size and nesting limits (from `rt-json`) |
//! | `RT-IR-0001` | syntax error (from `rt-json`) |
//! | `RT-DGN-0001` to `0003` | flag and lifecycle keys, before any unknown-name check |
//! | `RT-IR-0002` | structural name or rounding mode that the IR_Version does not define |
//! | `RT-IR-0003` | unsupported `ir_version` |
//! | `RT-IR-0004` | bare JSON number or unrepresentable literal where an Exact_Number is expected |
//! | `RT-IR-0005` | repeated (identifier, revision) |
//! | `RT-IR-0006` | `ir_version` missing or repeated |
//! | `RT-IR-0007`, `RT-IR-0008` | malformed or repeated document declaration |
//! | `RT-IR-0009` | revision number outside 1 to 4,294,967,295, or a gap in a rule's revisions |
//! | `RT-IR-0010` | missing required field, wrong JSON type or invalid value |
//! | `RT-LIM-0004` | more than 10,000 rules, 100,000 revisions or 64 spans per rule |
//!
//! Values that the data model cannot hold are refused here, not by the
//! Validator: rounding places outside 0 to 28 or a missing rounding mode or
//! places (`RT-IR-0010`), and an unknown rounding mode name (`RT-IR-0002`,
//! design §4.2.5).

use crate::expr::{
    AggFn, BinaryOp, CmpOp, Expr, ExprKind, Literal, OpaqueConstruct, Places, Rounding,
};
use crate::loc::Loc;
use crate::names::{Date, Identity, Name, OpaqueName};
use crate::print::{value_to_out, write, Out};
use crate::rule::{
    AiOrigin, DefaultValue, Direction, Marker, Origin, RuleBody, RuleContent, RuleKey,
    RuleRevision, Span, TieBreaker, Ties, WeightedTerm,
};
use crate::rulebook::{DocumentDecl, IrVersion, Precedence, Rulebook, FORMAT_NAME};
use crate::types::{CollectionDecl, EnumDecl, Field, InputDecl, SetDecl, Type, TypeDecls};
use rt_foundation::{
    escape_controls, stage_result, Code, Diagnostic, DiagnosticState, DocumentId, InputId,
    InputLocation, Location, Rejected, RevisionNumber, RuleId, RuleRef, StageResult,
    MAX_REVISIONS_PER_RULEBOOK, MAX_RULES_PER_RULEBOOK, MAX_SPANS_PER_RULE,
};
use rt_hash::Sha256Hex;
use rt_json::{
    check_flag_keys, Kind, Member, ParseLimits, Position, Value, FLAG_KEYS, LIFECYCLE_KEYS,
};
use rt_numeric::{ExactNumber, RoundingMode};
use std::collections::BTreeMap;

/// The decoder is recursive over expression nesting. An overridden depth limit
/// above this value is applied as this value, so a hostile document cannot
/// exhaust the stack.
pub const DECODER_DEPTH_CAP: u32 = 512;

/// Deepest nesting accepted when an opaque subtree is rebuilt.
const OPAQUE_DEPTH: u32 = 1024;

const ROOT_KEYS: [&str; 8] = [
    "format",
    "ir_version",
    "name",
    "version",
    "documents",
    "types",
    "rules",
    "precedence",
];
const RULE_KEYS: [&str; 8] = [
    "id", "revision", "origin", "kind", "target", "type", "markers", "spans",
];

/// Parses a Rule_IR document.
///
/// `max_bytes` and `max_depth` are the effective size and container-depth
/// limits. A document of exactly the limit is accepted. A depth limit above
/// [`DECODER_DEPTH_CAP`] is applied as the cap.
///
/// # Errors
/// All parse diagnostics of the document in the canonical order of Req 7.10,
/// with no Rulebook (Req 4.8). A size, nesting or syntax failure stops at the
/// first one, because the document cannot be read past it. Every other failure
/// is collected, so one call reports them all.
pub fn parse_rulebook(
    bytes: &[u8],
    input: &InputId,
    max_bytes: u64,
    max_depth: u32,
) -> StageResult<Rulebook> {
    let limits = ParseLimits::new(max_bytes, max_depth.min(DECODER_DEPTH_CAP));
    let root = match rt_json::parse(input, bytes, limits) {
        Ok(root) => root,
        Err(d) => {
            let found = repeated_version(input, bytes, limits, &d).unwrap_or_else(|| vec![*d]);
            return Err(reject(found));
        }
    };
    let mut dec = Dec {
        input,
        diags: Vec::new(),
    };
    if let Err(found) = check_flag_keys(input, &root) {
        dec.diags.extend(found);
    }
    let rulebook = dec.rulebook(&root);
    match rulebook {
        Some(rulebook) if dec.diags.is_empty() => stage_result(rulebook, DiagnosticState::empty()),
        Some(_) => Err(reject(dec.diags)),
        None => {
            if dec.diags.is_empty() {
                dec.diags.push(Diagnostic::new(
                    Code::InternalError,
                    "the Rule_IR decoder produced no Rulebook and no diagnostic".to_owned(),
                    Location::None(Code::InternalError),
                ));
            }
            Err(reject(dec.diags))
        }
    }
}

/// Builds the rejection. Every code of this stage is blocking, so the list is
/// always accepted.
fn reject(mut diags: Vec<Diagnostic>) -> Rejected {
    diags.sort();
    let mut state = DiagnosticState::empty();
    for d in &diags {
        state.push(d.clone());
    }
    match Rejected::new(state) {
        Ok(rejected) => rejected,
        Err(_) => {
            // Unreachable while the catalogue gives every code above a blocking
            // severity; the internal error is itself blocking, so this
            // recursion ends at once.
            diags.push(Diagnostic::new(
                Code::InternalError,
                "a Rule_IR parse diagnostic was not blocking".to_owned(),
                Location::None(Code::InternalError),
            ));
            reject(diags)
        }
    }
}

/// `rt-json` reports a repeated key as a syntax error. When the repeated key is
/// the top-level `ir_version`, Req 3.15 wants `RT-IR-0006` with the first
/// declaration as a related Location. The repeat is renamed in a copy of the
/// same length, the copy is parsed so that every position is unchanged, and the
/// diagnostic is built from the two key positions.
fn repeated_version(
    input: &InputId,
    bytes: &[u8],
    limits: ParseLimits,
    error: &Diagnostic,
) -> Option<Vec<Diagnostic>> {
    if error.code() != Code::IrSyntaxError || !error.message().contains("duplicate object key") {
        return None;
    }
    let Location::Input(at) = error.primary() else {
        return None;
    };
    let (line, column) = (at.line?, at.column?);
    let start = offset_of(bytes, line, column)?;
    let key = b"\"ir_version\"";
    let end = start.checked_add(key.len())?;
    if bytes.get(start..end)? != key {
        return None;
    }
    let mut copy = bytes.to_vec();
    *copy.get_mut(end.checked_sub(2)?)? = b'X';
    let root = rt_json::parse(input, &copy, limits).ok()?;
    let members = root.as_object()?;
    let first = members.iter().find(|m| m.key == "ir_version")?;
    let second = members
        .iter()
        .find(|m| m.key == "ir_versioX" && m.key_pos.line == line && m.key_pos.column == column)?;
    let here = |p: Position| {
        Location::Input(InputLocation {
            input: input.clone(),
            line: Some(p.line),
            column: Some(p.column),
            field: Some("ir_version".to_owned()),
        })
    };
    let mut found = vec![Diagnostic::new(
        Code::IrVersionMissingOrRepeated,
        format!(
            "ir_version is repeated; it was first declared at line {} column {}",
            first.key_pos.line, first.key_pos.column
        ),
        here(second.key_pos),
    )
    .with_related(vec![here(first.key_pos)])];
    if let Err(flags) = check_flag_keys(input, &root) {
        found.extend(flags);
    }
    Some(found)
}

/// Byte offset of a 1-based line and column (counted in Unicode scalar values).
fn offset_of(bytes: &[u8], line: u32, column: u32) -> Option<usize> {
    let text = std::str::from_utf8(bytes).ok()?;
    let (mut l, mut c) = (1u32, 1u32);
    for (index, ch) in text.char_indices() {
        if l == line && c == column {
            return Some(index);
        }
        if ch == '\n' {
            l = l.saturating_add(1);
            c = 1;
        } else {
            c = c.saturating_add(1);
        }
    }
    None
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        escape_controls(key)
    } else {
        format!("{path}.{}", escape_controls(key))
    }
}

fn idx(path: &str, index: usize) -> String {
    format!("{path}[{index}]")
}

fn is_flag_key(key: &str) -> bool {
    FLAG_KEYS.contains(&key) || LIFECYCLE_KEYS.contains(&key)
}

/// An opened JSON object with its position and field path.
struct Obj<'a> {
    members: &'a [Member<Position>],
    pos: Position,
    path: String,
}

impl<'a> Obj<'a> {
    fn get(&self, key: &str) -> Option<&'a Member<Position>> {
        self.members.iter().find(|m| m.key == key)
    }

    fn child(&self, key: &str) -> String {
        join(&self.path, key)
    }
}

struct Dec<'i> {
    input: &'i InputId,
    diags: Vec<Diagnostic>,
}

enum Version {
    Supported(IrVersion),
    Missing,
    Unsupported,
    Invalid,
}

impl<'i> Dec<'i> {
    // ----- reporting -----

    fn location(&self, at: Position, field: &str) -> Location {
        Location::Input(InputLocation {
            input: self.input.clone(),
            line: Some(at.line),
            column: Some(at.column),
            field: (!field.is_empty()).then(|| field.to_owned()),
        })
    }

    fn report(&mut self, code: Code, message: String, at: Position, field: &str) {
        let d = Diagnostic::new(code, message, self.location(at, field));
        self.diags.push(d);
    }

    fn invalid(&mut self, message: String, at: Position, field: &str) {
        self.report(Code::IrFieldInvalid, message, at, field);
    }

    // ----- generic readers -----

    fn open<'a>(&mut self, v: &'a Value<Position>, path: &str) -> Option<Obj<'a>> {
        match v.as_object() {
            Some(members) => Some(Obj {
                members,
                pos: *v.pos(),
                path: path.to_owned(),
            }),
            None => {
                self.invalid("expected an object".to_owned(), *v.pos(), path);
                None
            }
        }
    }

    /// One `RT-IR-0002` per name of the object that `allowed` does not list.
    /// Flag and lifecycle keys are not unknown names: they were reported
    /// before.
    fn unknown_in(&mut self, o: &Obj<'_>, allowed: &[&str]) {
        for m in o.members {
            if !allowed.contains(&m.key.as_str()) && !is_flag_key(&m.key) {
                let message = format!(
                    "unknown field '{}': the declared ir_version does not define it",
                    escape_controls(&m.key)
                );
                self.report(Code::IrUnknownField, message, m.key_pos, &o.child(&m.key));
            }
        }
    }

    fn req<'a>(&mut self, o: &Obj<'a>, key: &str) -> Option<&'a Value<Position>> {
        if let Some(m) = o.get(key) {
            Some(&m.value)
        } else {
            self.invalid(format!("missing required field '{key}'"), o.pos, &o.path);
            None
        }
    }

    fn str_of<'a>(&mut self, v: &'a Value<Position>, path: &str) -> Option<&'a str> {
        let text = v.as_str();
        if text.is_none() {
            self.invalid("expected a string".to_owned(), *v.pos(), path);
        }
        text
    }

    fn u32_of(&mut self, v: &Value<Position>, path: &str) -> Option<u32> {
        let n = v.as_u64().and_then(|n| u32::try_from(n).ok());
        if n.is_none() {
            self.invalid(
                "expected an integer from 0 to 4294967295".to_owned(),
                *v.pos(),
                path,
            );
        }
        n
    }

    fn arr_of<'a>(&mut self, v: &'a Value<Position>, path: &str) -> Option<&'a [Value<Position>]> {
        let items = v.as_array();
        if items.is_none() {
            self.invalid("expected an array".to_owned(), *v.pos(), path);
        }
        items
    }

    fn name_of(&mut self, v: &Value<Position>, path: &str) -> Option<Name> {
        let text = self.str_of(v, path)?;
        match Name::new(text) {
            Ok(name) => Some(name),
            Err(e) => {
                self.invalid(format!("invalid name: {e}"), *v.pos(), path);
                None
            }
        }
    }

    fn rule_id(&mut self, v: &Value<Position>, path: &str) -> Option<RuleId> {
        let text = self.str_of(v, path)?;
        match RuleId::new(text) {
            Ok(id) => Some(id),
            Err(e) => {
                self.invalid(format!("invalid rule identifier: {e}"), *v.pos(), path);
                None
            }
        }
    }

    fn type_of(&mut self, v: &Value<Position>, path: &str) -> Option<Type> {
        let text = self.str_of(v, path)?;
        let ty = Type::parse_declared(text);
        if ty.is_none() {
            self.invalid(
                format!(
                    "'{}' is not a declared type (number, boolean, text, date, ranking or \
                     enum:<Name>)",
                    escape_controls(text)
                ),
                *v.pos(),
                path,
            );
        }
        ty
    }

    fn identity_of(&mut self, v: &Value<Position>, path: &str) -> Option<Identity> {
        let text = self.str_of(v, path)?;
        match Identity::new(text) {
            Ok(i) => Some(i),
            Err(e) => {
                self.invalid(format!("invalid identity: {e}"), *v.pos(), path);
                None
            }
        }
    }

    fn string_list(&mut self, v: &Value<Position>, path: &str) -> Option<Vec<String>> {
        let items = self.arr_of(v, path)?;
        let mut out = Vec::new();
        let mut ok = true;
        for (i, item) in items.iter().enumerate() {
            match self.str_of(item, &idx(path, i)) {
                Some(t) => out.push(t.to_owned()),
                None => ok = false,
            }
        }
        ok.then_some(out)
    }

    /// An Exact_Number: a JSON string that `rt-numeric` accepts as a decimal
    /// literal. A bare JSON number (any form) is `RT-IR-0004` with its text
    /// (Req 4.4).
    fn exact_number(&mut self, v: &Value<Position>, path: &str) -> Option<ExactNumber> {
        let (text, bare) = match v.kind() {
            Kind::String(t) => (t.clone(), false),
            Kind::Integer(n) => (n.to_string(), true),
            Kind::Number(t) => (t.clone(), true),
            _ => {
                self.invalid("expected a numeric string".to_owned(), *v.pos(), path);
                return None;
            }
        };
        if bare {
            let message = format!(
                "numeric literal {text} is a JSON number, which is not accepted: write an \
                 Exact_Number as a string such as \"{text}\""
            );
            self.report(Code::IrNumericLiteralInvalid, message, *v.pos(), path);
            return None;
        }
        match rt_numeric::parse_literal(&text) {
            Ok(n) => Some(n),
            Err(e) => {
                let message = format!(
                    "numeric literal \"{}\" is not an Exact_Number: {e}",
                    escape_controls(&text)
                );
                self.report(Code::IrNumericLiteralInvalid, message, *v.pos(), path);
                None
            }
        }
    }

    // ----- Rulebook -----

    fn version<'a>(&mut self, o: &Obj<'a>) -> Version {
        let Some(m) = o.get("ir_version") else {
            self.report(
                Code::IrVersionMissingOrRepeated,
                "ir_version is missing".to_owned(),
                o.pos,
                "ir_version",
            );
            return Version::Missing;
        };
        let text = match m.value.kind() {
            Kind::String(t) => t.clone(),
            Kind::Integer(n) => n.to_string(),
            Kind::Number(t) => t.clone(),
            _ => {
                self.invalid(
                    "ir_version must be a string".to_owned(),
                    *m.value.pos(),
                    "ir_version",
                );
                return Version::Invalid;
            }
        };
        match IrVersion::parse(&text) {
            Some(v) if m.value.as_str().is_some() => Version::Supported(v),
            _ => {
                let message = format!(
                    "ir_version \"{}\" is not supported; supported versions: {}",
                    escape_controls(&text),
                    crate::SUPPORTED_IR_VERSIONS
                        .iter()
                        .map(|v| format!("\"{v}\""))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                self.report(Code::IrVersionUnsupported, message, m.key_pos, "ir_version");
                Version::Unsupported
            }
        }
    }

    fn rulebook(&mut self, root: &Value<Position>) -> Option<Rulebook> {
        let o = self.open(root, "")?;
        self.unknown_in(&o, &ROOT_KEYS);
        let ir_version = match self.version(&o) {
            Version::Supported(v) => Some(v),
            Version::Missing => Some(IrVersion::V1),
            Version::Invalid => None,
            Version::Unsupported => return None,
        };
        let format = self
            .req(&o, "format")
            .and_then(|v| self.str_of(v, "format").map(|t| (t, *v.pos())));
        if let Some((text, at)) = format {
            if text != FORMAT_NAME {
                self.invalid(format!("format must be \"{FORMAT_NAME}\""), at, "format");
            }
        }
        let name = self
            .req(&o, "name")
            .and_then(|v| self.str_of(v, "name"))
            .map(str::to_owned);
        let version = self
            .req(&o, "version")
            .and_then(|v| self.str_of(v, "version"))
            .map(str::to_owned);
        let documents = self.req(&o, "documents").and_then(|v| self.documents(v));
        let types = self.req(&o, "types").and_then(|v| self.types(v, "types"));
        let revisions = self.req(&o, "rules").and_then(|v| self.rules(v));
        let precedence = self.req(&o, "precedence").and_then(|v| self.precedence(v));
        Some(Rulebook {
            ir_version: ir_version?,
            name: name?,
            version: version?,
            documents: documents?,
            types: types?,
            revisions: revisions?,
            precedence: precedence?,
        })
    }

    fn documents(&mut self, v: &Value<Position>) -> Option<Vec<DocumentDecl>> {
        let items = self.arr_of(v, "documents")?;
        let mut out = Vec::new();
        let mut ok = true;
        let mut seen: BTreeMap<DocumentId, Position> = BTreeMap::new();
        for (i, item) in items.iter().enumerate() {
            let path = idx("documents", i);
            let Some(o) = self.open(item, &path) else {
                ok = false;
                continue;
            };
            self.unknown_in(&o, &["id", "sha256"]);
            let id = self.req(&o, "id").and_then(|v| {
                let text = self.str_of(v, &o.child("id"))?;
                match DocumentId::new(text) {
                    Ok(id) => Some(id),
                    Err(e) => {
                        self.invalid(format!("invalid document identifier: {e}"), *v.pos(), &path);
                        None
                    }
                }
            });
            let sha = self.req(&o, "sha256").and_then(|v| {
                let text = self.str_of(v, &o.child("sha256"))?;
                let parsed = Sha256Hex::parse(text);
                if parsed.is_none() {
                    let message = format!(
                        "document hash of '{}' is malformed: expected exactly 64 lowercase \
                         hexadecimal characters",
                        id.as_ref()
                            .map_or(String::new(), |d| escape_controls(d.as_str()))
                    );
                    self.report(Code::IrDocumentHashInvalid, message, *item.pos(), &path);
                }
                parsed
            });
            if let Some(id) = &id {
                if let Some(first) = seen.get(id) {
                    let first = *first;
                    let message = format!(
                        "document identifier '{}' is declared more than once",
                        escape_controls(id.as_str())
                    );
                    let related = vec![self.location(first, &join(&path, "id"))];
                    let d = Diagnostic::new(
                        Code::IrDocumentIdDuplicate,
                        message,
                        self.location(*item.pos(), &path),
                    )
                    .with_related(related);
                    self.diags.push(d);
                } else {
                    seen.insert(id.clone(), *item.pos());
                }
            }
            match (id, sha) {
                (Some(id), Some(sha256)) => out.push(DocumentDecl {
                    loc: Loc::at(*item.pos()),
                    id,
                    sha256,
                }),
                _ => ok = false,
            }
        }
        ok.then_some(out)
    }

    // ----- types -----

    fn types(&mut self, v: &Value<Position>, path: &str) -> Option<TypeDecls> {
        let o = self.open(v, path)?;
        self.unknown_in(&o, &["inputs", "enums", "sets", "collections"]);
        let inputs = self
            .req(&o, "inputs")
            .and_then(|v| self.each(v, &o.child("inputs"), |d, item, p| d.input_decl(item, p)));
        let enums = self
            .req(&o, "enums")
            .and_then(|v| self.each(v, &o.child("enums"), |d, item, p| d.enum_decl(item, p)));
        let sets = self
            .req(&o, "sets")
            .and_then(|v| self.each(v, &o.child("sets"), |d, item, p| d.set_decl(item, p)));
        let collections = self.req(&o, "collections").and_then(|v| {
            self.each(v, &o.child("collections"), |d, item, p| {
                d.collection_decl(item, p)
            })
        });
        Some(TypeDecls {
            inputs: inputs?,
            enums: enums?,
            sets: sets?,
            collections: collections?,
        })
    }

    /// Decodes each element of an array. Every element is decoded, so all
    /// diagnostics appear.
    fn each<T>(
        &mut self,
        v: &Value<Position>,
        path: &str,
        mut f: impl FnMut(&mut Self, &Value<Position>, &str) -> Option<T>,
    ) -> Option<Vec<T>> {
        let items = self.arr_of(v, path)?;
        let mut out = Vec::new();
        let mut ok = true;
        for (i, item) in items.iter().enumerate() {
            match f(self, item, &idx(path, i)) {
                Some(t) => out.push(t),
                None => ok = false,
            }
        }
        ok.then_some(out)
    }

    fn input_decl(&mut self, v: &Value<Position>, path: &str) -> Option<InputDecl> {
        let o = self.open(v, path)?;
        self.unknown_in(&o, &["name", "type"]);
        let name = self
            .req(&o, "name")
            .and_then(|v| self.name_of(v, &o.child("name")));
        let ty = self
            .req(&o, "type")
            .and_then(|v| self.type_of(v, &o.child("type")));
        Some(InputDecl {
            loc: Loc::at(*v.pos()),
            name: name?,
            ty: ty?,
        })
    }

    fn enum_decl(&mut self, v: &Value<Position>, path: &str) -> Option<EnumDecl> {
        let o = self.open(v, path)?;
        self.unknown_in(&o, &["name", "members"]);
        let name = self
            .req(&o, "name")
            .and_then(|v| self.name_of(v, &o.child("name")));
        let members = self
            .req(&o, "members")
            .and_then(|v| self.each(v, &o.child("members"), |d, item, p| d.name_of(item, p)));
        Some(EnumDecl {
            loc: Loc::at(*v.pos()),
            name: name?,
            members: members?,
        })
    }

    fn set_decl(&mut self, v: &Value<Position>, path: &str) -> Option<SetDecl> {
        let o = self.open(v, path)?;
        self.unknown_in(&o, &["name", "element_type", "members"]);
        let name = self
            .req(&o, "name")
            .and_then(|v| self.name_of(v, &o.child("name")));
        let element_type = self
            .req(&o, "element_type")
            .and_then(|v| self.type_of(v, &o.child("element_type")));
        let members = self
            .req(&o, "members")
            .and_then(|v| self.each(v, &o.child("members"), |d, item, p| d.literal_node(item, p)));
        Some(SetDecl {
            loc: Loc::at(*v.pos()),
            name: name?,
            element_type: element_type?,
            members: members?,
        })
    }

    fn collection_decl(&mut self, v: &Value<Position>, path: &str) -> Option<CollectionDecl> {
        let o = self.open(v, path)?;
        self.unknown_in(&o, &["name", "fields"]);
        let name = self
            .req(&o, "name")
            .and_then(|v| self.name_of(v, &o.child("name")));
        let fields = self
            .req(&o, "fields")
            .and_then(|v| self.each(v, &o.child("fields"), |d, item, p| d.field_decl(item, p)));
        Some(CollectionDecl {
            loc: Loc::at(*v.pos()),
            name: name?,
            fields: fields?,
        })
    }

    fn field_decl(&mut self, v: &Value<Position>, path: &str) -> Option<Field> {
        let d = self.input_decl(v, path)?;
        Some(Field {
            loc: d.loc,
            name: d.name,
            ty: d.ty,
        })
    }

    // ----- rules -----

    fn rules(&mut self, v: &Value<Position>) -> Option<Vec<RuleRevision>> {
        let items = self.arr_of(v, "rules")?;
        let mut out = Vec::new();
        let mut ok = true;
        let mut seen: BTreeMap<RuleKey, Position> = BTreeMap::new();
        let mut by_id: BTreeMap<RuleId, Vec<(u32, Position)>> = BTreeMap::new();
        let mut rules_reported = false;
        for (i, item) in items.iter().enumerate() {
            let path = idx("rules", i);
            if i == MAX_REVISIONS_PER_RULEBOOK as usize {
                let message = format!(
                    "the Rulebook has more than {MAX_REVISIONS_PER_RULEBOOK} rule revisions"
                );
                self.report(Code::LimitCountExceeded, message, *item.pos(), &path);
            }
            let Some(rev) = self.revision(item, &path) else {
                ok = false;
                continue;
            };
            let at = *item.pos();
            if let Some(first) = seen.get(&rev.key).copied() {
                let message = format!(
                    "rule '{}' revision {} is declared more than once",
                    rev.key.id, rev.key.revision
                );
                let d =
                    Diagnostic::new(Code::IrDuplicateRevision, message, self.location(at, &path))
                        .with_related(vec![self.location(first, "")])
                        .with_rules(vec![RuleRef {
                            rule: rev.key.id.clone(),
                            revision: Some(rev.key.revision),
                        }]);
                self.diags.push(d);
                continue;
            }
            seen.insert(rev.key.clone(), at);
            let list = by_id.entry(rev.key.id.clone()).or_default();
            list.push((rev.key.revision.get(), at));
            if by_id.len() == MAX_RULES_PER_RULEBOOK as usize + 1 && !rules_reported {
                rules_reported = true;
                let message = format!("the Rulebook has more than {MAX_RULES_PER_RULEBOOK} rules");
                self.report(Code::LimitCountExceeded, message, at, &path);
            }
            out.push(rev);
        }
        for (id, mut list) in by_id {
            list.sort();
            for (k, (rev, at)) in list.iter().enumerate() {
                let expected = u32::try_from(k + 1).unwrap_or(u32::MAX);
                if *rev != expected {
                    let message = format!(
                        "revisions of rule '{id}' have a gap: expected revision {expected}, \
                         found {rev}"
                    );
                    let d = Diagnostic::new(
                        Code::IrRevisionSequenceInvalid,
                        message,
                        self.location(*at, ""),
                    )
                    .with_rules(vec![RuleRef {
                        rule: id.clone(),
                        revision: RevisionNumber::new(*rev).ok(),
                    }]);
                    self.diags.push(d);
                    break;
                }
            }
        }
        ok.then_some(out)
    }

    fn revision_number(&mut self, v: &Value<Position>, path: &str) -> Option<RevisionNumber> {
        match v.kind() {
            Kind::Integer(n) => match u32::try_from(*n)
                .ok()
                .and_then(|x| RevisionNumber::new(x).ok())
            {
                Some(r) => Some(r),
                None => {
                    self.report(
                        Code::IrRevisionSequenceInvalid,
                        format!("revision number {n} is outside 1 to 4294967295"),
                        *v.pos(),
                        path,
                    );
                    None
                }
            },
            Kind::Number(text) => {
                self.report(
                    Code::IrRevisionSequenceInvalid,
                    format!(
                        "revision number {} is not an integer from 1 to 4294967295",
                        escape_controls(text)
                    ),
                    *v.pos(),
                    path,
                );
                None
            }
            _ => {
                self.invalid("expected an integer".to_owned(), *v.pos(), path);
                None
            }
        }
    }

    fn revision(&mut self, v: &Value<Position>, path: &str) -> Option<RuleRevision> {
        let o = self.open(v, path)?;
        let kind_value = self.req(&o, "kind")?;
        let kind_text = self.str_of(kind_value, &o.child("kind"))?;
        let extra: Option<&[&str]> = match kind_text {
            "eligibility" | "derived" => Some(&["expr"]),
            "weighted_score" => Some(&["terms", "rounding"]),
            "ranking" => Some(&["key", "direction", "tie_breakers", "ties"]),
            "default" => Some(&["value"]),
            _ => None,
        };
        if let Some(extra) = extra {
            let allowed = [RULE_KEYS.as_slice(), extra].concat();
            self.unknown_in(&o, &allowed);
        }
        let id = self
            .req(&o, "id")
            .and_then(|v| self.rule_id(v, &o.child("id")));
        let revision = self
            .req(&o, "revision")
            .and_then(|v| self.revision_number(v, &o.child("revision")));
        let origin = self
            .req(&o, "origin")
            .and_then(|v| self.origin(v, &o.child("origin")));
        let target = self
            .req(&o, "target")
            .and_then(|v| self.name_of(v, &o.child("target")));
        let ty = match o.get("type") {
            Some(m) => self.type_of(&m.value, &o.child("type")).map(Some),
            None => Some(None),
        };
        let body = self.body(kind_text, kind_value, &o);
        let markers = match o.get("markers") {
            Some(m) => self.markers(&m.value, &o.child("markers")),
            None => Some(Vec::new()),
        };
        let spans = match o.get("spans") {
            Some(m) => self.spans(&m.value, &o.child("spans")),
            None => Some(Vec::new()),
        };
        Some(RuleRevision {
            loc: Loc::at(*v.pos()),
            key: RuleKey {
                id: id?,
                revision: revision?,
            },
            origin: origin?,
            content: RuleContent {
                target: target?,
                ty: ty?,
                body: body?,
                markers: markers?,
                spans: spans?,
            },
        })
    }

    fn body(&mut self, kind: &str, kind_value: &Value<Position>, o: &Obj<'_>) -> Option<RuleBody> {
        match kind {
            "eligibility" | "derived" => {
                let expr = self
                    .req(o, "expr")
                    .and_then(|v| self.expr(v, &o.child("expr")))?;
                Some(if kind == "eligibility" {
                    RuleBody::Eligibility { expr }
                } else {
                    RuleBody::Derived { expr }
                })
            }
            "weighted_score" => {
                let terms = self
                    .req(o, "terms")
                    .and_then(|v| self.each(v, &o.child("terms"), |d, item, p| d.term(item, p)));
                let rounding = self.req(o, "rounding").and_then(|v| {
                    if matches!(v.kind(), Kind::Null) {
                        Some(None)
                    } else {
                        self.rounding(v, &o.child("rounding")).map(Some)
                    }
                });
                Some(RuleBody::WeightedScore {
                    terms: terms?,
                    rounding: rounding?,
                })
            }
            "ranking" => {
                let key = self
                    .req(o, "key")
                    .and_then(|v| self.expr(v, &o.child("key")));
                let direction = self
                    .req(o, "direction")
                    .and_then(|v| self.direction(v, &o.child("direction")));
                let tie_breakers = self.req(o, "tie_breakers").and_then(|v| {
                    self.each(v, &o.child("tie_breakers"), |d, item, p| {
                        d.tie_breaker(item, p)
                    })
                });
                let ties = self.req(o, "ties").and_then(|v| {
                    let text = self.str_of(v, &o.child("ties"))?;
                    if text == "group" {
                        Some(Ties::Group)
                    } else {
                        self.invalid(
                            "ties must be \"group\"".to_owned(),
                            *v.pos(),
                            &o.child("ties"),
                        );
                        None
                    }
                });
                Some(RuleBody::Ranking {
                    key: key?,
                    direction: direction?,
                    tie_breakers: tie_breakers?,
                    ties: ties?,
                })
            }
            "default" => {
                let value = self.req(o, "value").and_then(|v| {
                    self.literal_node(v, &o.child("value"))
                        .map(|l| (l, *v.pos()))
                })?;
                Some(RuleBody::Default {
                    value: DefaultValue {
                        loc: Loc::at(value.1),
                        literal: value.0,
                    },
                })
            }
            other => {
                let members = o
                    .members
                    .iter()
                    .filter(|m| !RULE_KEYS.contains(&m.key.as_str()));
                self.opaque(other, members, *kind_value.pos(), &o.child("kind"))
                    .map(RuleBody::Unrecognized)
            }
        }
    }

    fn term(&mut self, v: &Value<Position>, path: &str) -> Option<WeightedTerm> {
        let o = self.open(v, path)?;
        self.unknown_in(&o, &["weight", "value"]);
        let weight = self
            .req(&o, "weight")
            .and_then(|v| self.expr(v, &o.child("weight")));
        let value = self
            .req(&o, "value")
            .and_then(|v| self.expr(v, &o.child("value")));
        Some(WeightedTerm {
            loc: Loc::at(*v.pos()),
            weight: weight?,
            value: value?,
        })
    }

    fn tie_breaker(&mut self, v: &Value<Position>, path: &str) -> Option<TieBreaker> {
        let o = self.open(v, path)?;
        self.unknown_in(&o, &["expr", "direction"]);
        let expr = self
            .req(&o, "expr")
            .and_then(|v| self.expr(v, &o.child("expr")));
        let direction = self
            .req(&o, "direction")
            .and_then(|v| self.direction(v, &o.child("direction")));
        Some(TieBreaker {
            loc: Loc::at(*v.pos()),
            expr: expr?,
            direction: direction?,
        })
    }

    fn direction(&mut self, v: &Value<Position>, path: &str) -> Option<Direction> {
        let text = self.str_of(v, path)?;
        let d = Direction::from_name(text);
        if d.is_none() {
            self.invalid(
                format!(
                    "direction must be \"asc\" or \"desc\", not '{}'",
                    escape_controls(text)
                ),
                *v.pos(),
                path,
            );
        }
        d
    }

    fn rounding_mode(&mut self, v: &Value<Position>, path: &str) -> Option<RoundingMode> {
        let text = self.str_of(v, path)?;
        let mode = RoundingMode::from_name(text);
        if mode.is_none() {
            let message = format!(
                "unknown rounding mode '{}': the declared ir_version defines half_even, \
                 half_away_from_zero and toward_zero",
                escape_controls(text)
            );
            self.report(Code::IrUnknownField, message, *v.pos(), path);
        }
        mode
    }

    fn places(&mut self, v: &Value<Position>, path: &str) -> Option<Places> {
        let places = v.as_u64().and_then(|n| Places::new(n).ok());
        if places.is_none() {
            self.invalid(
                "rounding places must be an integer from 0 to 28".to_owned(),
                *v.pos(),
                path,
            );
        }
        places
    }

    fn rounding(&mut self, v: &Value<Position>, path: &str) -> Option<Rounding> {
        let o = self.open(v, path)?;
        self.unknown_in(&o, &["mode", "places"]);
        let mode = self
            .req(&o, "mode")
            .and_then(|v| self.rounding_mode(v, &o.child("mode")));
        let places = self
            .req(&o, "places")
            .and_then(|v| self.places(v, &o.child("places")));
        Some(Rounding {
            mode: mode?,
            places: places?,
        })
    }

    // ----- origin, markers, spans -----

    fn origin(&mut self, v: &Value<Position>, path: &str) -> Option<Origin> {
        let o = self.open(v, path)?;
        let kind_value = self.req(&o, "kind")?;
        let kind = self.str_of(kind_value, &o.child("kind"))?;
        match kind {
            "manual" => {
                self.unknown_in(&o, &["kind", "author"]);
                match o.get("author") {
                    None => Some(Origin::Manual { author: None }),
                    Some(m) if matches!(m.value.kind(), Kind::Null) => {
                        Some(Origin::Manual { author: None })
                    }
                    Some(m) => {
                        let author = self.identity_of(&m.value, &o.child("author"))?;
                        Some(Origin::Manual {
                            author: Some(author),
                        })
                    }
                }
            }
            "ai_proposal" => {
                self.unknown_in(
                    &o,
                    &[
                        "kind",
                        "proposer",
                        "provider",
                        "model",
                        "adapter_version",
                        "request_hash",
                        "response_hash",
                        "proposal_id",
                        "ai_identities",
                    ],
                );
                self.ai_origin(&o).map(Origin::AiProposal)
            }
            other => {
                self.invalid(
                    format!(
                        "origin kind must be \"manual\" or \"ai_proposal\", not '{}'",
                        escape_controls(other)
                    ),
                    *kind_value.pos(),
                    &o.child("kind"),
                );
                None
            }
        }
    }

    fn ai_origin(&mut self, o: &Obj<'_>) -> Option<AiOrigin> {
        let text = |d: &mut Self, key: &str| {
            d.req(o, key)
                .and_then(|v| d.str_of(v, &o.child(key)))
                .map(str::to_owned)
        };
        let hash = |d: &mut Self, key: &str| {
            let v = d.req(o, key)?;
            let text = d.str_of(v, &o.child(key))?;
            let parsed = Sha256Hex::parse(text);
            if parsed.is_none() {
                d.invalid(
                    "expected exactly 64 lowercase hexadecimal characters".to_owned(),
                    *v.pos(),
                    &o.child(key),
                );
            }
            parsed
        };
        let proposer = self
            .req(o, "proposer")
            .and_then(|v| self.identity_of(v, &o.child("proposer")));
        let provider = text(self, "provider");
        let model = text(self, "model");
        let adapter_version = text(self, "adapter_version");
        let request_hash = hash(self, "request_hash");
        let response_hash = hash(self, "response_hash");
        let proposal_id = text(self, "proposal_id");
        let ai_identities = self.req(o, "ai_identities").and_then(|v| {
            self.each(v, &o.child("ai_identities"), |d, item, p| {
                d.identity_of(item, p)
            })
        });
        Some(AiOrigin {
            proposer: proposer?,
            provider: provider?,
            model: model?,
            adapter_version: adapter_version?,
            request_hash: request_hash?,
            response_hash: response_hash?,
            proposal_id: proposal_id?,
            ai_identities: ai_identities?,
        })
    }

    fn markers(&mut self, v: &Value<Position>, path: &str) -> Option<Vec<Marker>> {
        self.each(v, path, |d, item, p| d.marker(item, p))
    }

    fn marker(&mut self, v: &Value<Position>, path: &str) -> Option<Marker> {
        let o = self.open(v, path)?;
        let kind_value = self.req(&o, "kind")?;
        let kind = self.str_of(kind_value, &o.child("kind"))?;
        match kind {
            "ambiguous" => {
                self.unknown_in(&o, &["kind", "interpretations"]);
                let list = self
                    .req(&o, "interpretations")
                    .and_then(|v| self.string_list(v, &o.child("interpretations")))?;
                Some(Marker::Ambiguous {
                    interpretations: list,
                })
            }
            "incomplete" => {
                self.unknown_in(&o, &["kind", "missing"]);
                let list = self
                    .req(&o, "missing")
                    .and_then(|v| self.string_list(v, &o.child("missing")))?;
                Some(Marker::Incomplete { missing: list })
            }
            other => {
                self.invalid(
                    format!(
                        "marker kind must be \"ambiguous\" or \"incomplete\", not '{}'",
                        escape_controls(other)
                    ),
                    *kind_value.pos(),
                    &o.child("kind"),
                );
                None
            }
        }
    }

    fn spans(&mut self, v: &Value<Position>, path: &str) -> Option<Vec<Span>> {
        let items = self.arr_of(v, path)?;
        if items.len() > MAX_SPANS_PER_RULE as usize {
            let message = format!(
                "{} spans are declared, above the limit of {MAX_SPANS_PER_RULE}",
                items.len()
            );
            self.report(Code::LimitCountExceeded, message, *v.pos(), path);
        }
        self.each(v, path, |d, item, p| d.span(item, p))
    }

    fn span(&mut self, v: &Value<Position>, path: &str) -> Option<Span> {
        let o = self.open(v, path)?;
        self.unknown_in(&o, &["document", "start", "end", "line", "column"]);
        let document = self.req(&o, "document").and_then(|v| {
            let text = self.str_of(v, &o.child("document"))?;
            match DocumentId::new(text) {
                Ok(d) => Some(d),
                Err(e) => {
                    self.invalid(
                        format!("invalid document identifier: {e}"),
                        *v.pos(),
                        &o.child("document"),
                    );
                    None
                }
            }
        });
        let number =
            |d: &mut Self, key: &str| d.req(&o, key).and_then(|v| d.u32_of(v, &o.child(key)));
        let start = number(self, "start");
        let end = number(self, "end");
        let line = number(self, "line");
        let column = number(self, "column");
        Some(Span {
            loc: Loc::at(*v.pos()),
            document: document?,
            start: start?,
            end: end?,
            line: line?,
            column: column?,
        })
    }

    fn precedence(&mut self, v: &Value<Position>) -> Option<Vec<Precedence>> {
        self.each(v, "precedence", |d, item, path| {
            let o = d.open(item, path)?;
            d.unknown_in(&o, &["id", "target", "order", "spans"]);
            let id = d.req(&o, "id").and_then(|v| d.rule_id(v, &o.child("id")));
            let target = d
                .req(&o, "target")
                .and_then(|v| d.name_of(v, &o.child("target")));
            let order = d
                .req(&o, "order")
                .and_then(|v| d.each(v, &o.child("order"), |d, item, p| d.rule_id(item, p)));
            let spans = match o.get("spans") {
                Some(m) => d.spans(&m.value, &o.child("spans")),
                None => Some(Vec::new()),
            };
            Some(Precedence {
                loc: Loc::at(*item.pos()),
                id: id?,
                target: target?,
                order: order?,
                spans: spans?,
            })
        })
    }

    // ----- opaque constructs -----

    /// Carries a well-formed but undefined kind or node: the members `members`
    /// as compact canonical RT-JSON text.
    fn opaque<'a>(
        &mut self,
        name: &str,
        members: impl Iterator<Item = &'a Member<Position>>,
        at: Position,
        path: &str,
    ) -> Option<OpaqueConstruct> {
        let Ok(name) = OpaqueName::new(name) else {
            self.invalid(
                format!(
                    "'{}' is not a well-formed name ([a-z][a-z0-9_]*)",
                    escape_controls(name)
                ),
                at,
                path,
            );
            return None;
        };
        let tree = Out::Obj(
            members
                .map(|m| (m.key.clone(), value_to_out(&m.value)))
                .collect(),
        );
        let text = write(&tree, false);
        let limits = ParseLimits::new(u64::from(u32::MAX), OPAQUE_DEPTH);
        match rt_json::parse(self.input, text.as_bytes(), limits) {
            Ok(value) => Some(OpaqueConstruct::new(name, &value)),
            Err(_) => {
                self.report(
                    Code::InternalError,
                    "an unrecognized construct could not be carried".to_owned(),
                    at,
                    path,
                );
                None
            }
        }
    }

    // ----- literals and expressions -----

    /// The fields of a literal node after `node` and `t`, read from the object
    /// `o`.
    fn literal_fields(&mut self, node: &str, o: &Obj<'_>) -> Option<Literal> {
        match node {
            "num" => {
                let v = self.req(o, "value")?;
                self.exact_number(v, &o.child("value")).map(Literal::Number)
            }
            "bool" => {
                let v = self.req(o, "value")?;
                match v.kind() {
                    Kind::Bool(b) => Some(Literal::Boolean(*b)),
                    _ => {
                        self.invalid(
                            "expected true or false".to_owned(),
                            *v.pos(),
                            &o.child("value"),
                        );
                        None
                    }
                }
            }
            "text" => {
                let v = self.req(o, "value")?;
                self.str_of(v, &o.child("value"))
                    .map(|t| Literal::Text(t.to_owned()))
            }
            "date" => {
                let v = self.req(o, "value")?;
                let text = self.str_of(v, &o.child("value"))?;
                match Date::parse(text) {
                    Ok(d) => Some(Literal::Date(d)),
                    Err(e) => {
                        self.invalid(
                            format!("'{}' is not a date: {e}", escape_controls(text)),
                            *v.pos(),
                            &o.child("value"),
                        );
                        None
                    }
                }
            }
            _ => {
                let enum_name = self
                    .req(o, "enum")
                    .and_then(|v| self.name_of(v, &o.child("enum")));
                let member = self
                    .req(o, "member")
                    .and_then(|v| self.name_of(v, &o.child("member")));
                Some(Literal::Enum {
                    enum_name: enum_name?,
                    member: member?,
                })
            }
        }
    }

    /// A literal in a position without its own `t` (set member, default value).
    /// A `t` that is present must equal the type the literal implies.
    fn literal_node(&mut self, v: &Value<Position>, path: &str) -> Option<Literal> {
        let o = self.open(v, path)?;
        let node_value = self.req(&o, "node")?;
        let node = self.str_of(node_value, &o.child("node"))?;
        let allowed: &[&str] = match node {
            "num" | "bool" | "text" | "date" => &["node", "t", "value"],
            "enum" => &["node", "t", "enum", "member"],
            other => {
                self.invalid(
                    format!(
                        "expected a literal node (num, bool, text, date or enum), not '{}'",
                        escape_controls(other)
                    ),
                    *node_value.pos(),
                    &o.child("node"),
                );
                return None;
            }
        };
        self.unknown_in(&o, allowed);
        let literal = self.literal_fields(node, &o)?;
        if let Some(m) = o.get("t") {
            let declared = self.type_of(&m.value, &o.child("t"))?;
            let derived = match &literal {
                Literal::Number(_) => Type::Number,
                Literal::Boolean(_) => Type::Boolean,
                Literal::Text(_) => Type::Text,
                Literal::Date(_) => Type::Date,
                Literal::Enum { enum_name, .. } => Type::Enum(enum_name.clone()),
            };
            if declared != derived {
                self.invalid(
                    format!("t must be '{}' for this literal", derived.text()),
                    *m.value.pos(),
                    &o.child("t"),
                );
                return None;
            }
        }
        Some(literal)
    }

    /// True when `key` holds a well-formed name that `known` does not accept:
    /// the node is then carried as an unrecognized construct (Req 4.9).
    fn unknown_operator(o: &Obj<'_>, key: &str, known: fn(&str) -> bool) -> bool {
        o.get(key)
            .and_then(|m| m.value.as_str())
            .is_some_and(|s| !known(s) && OpaqueName::new(s).is_ok())
    }

    fn expr(&mut self, v: &Value<Position>, path: &str) -> Option<Expr> {
        let o = self.open(v, path)?;
        let node_value = self.req(&o, "node")?;
        let node = self.str_of(node_value, &o.child("node"))?;
        let forced_opaque = match node {
            "binary" => Self::unknown_operator(&o, "op", |s| BinaryOp::from_name(s).is_some()),
            "cmp" => Self::unknown_operator(&o, "op", |s| CmpOp::from_name(s).is_some()),
            "agg" => Self::unknown_operator(&o, "fn", |s| AggFn::from_name(s).is_some()),
            _ => false,
        };
        let fields: Option<&[&str]> = if forced_opaque {
            None
        } else {
            match node {
                "num" | "bool" | "text" | "date" => Some(&["node", "t", "value"]),
                "enum" => Some(&["node", "t", "enum", "member"]),
                "field" => Some(&["node", "t", "name"]),
                "ref" | "rank_reference" => Some(&["node", "t", "target"]),
                "binary" | "cmp" => Some(&["node", "t", "op", "left", "right"]),
                "and" | "or" => Some(&["node", "t", "args"]),
                "not" => Some(&["node", "t", "arg"]),
                "in" => Some(&["node", "t", "value", "set"]),
                "agg" => Some(&["node", "t", "fn", "collection", "field"]),
                "round" => Some(&["node", "t", "arg", "places", "mode"]),
                _ => None,
            }
        };
        if let Some(fields) = fields {
            self.unknown_in(&o, fields);
        }
        let ty = match o.get("t") {
            Some(m) => self.type_of(&m.value, &o.child("t")).map(Some),
            None => Some(None),
        };
        let kind = if fields.is_none() {
            let members = o.members.iter().filter(|m| m.key != "node" && m.key != "t");
            self.opaque(node, members, *node_value.pos(), &o.child("node"))
                .map(ExprKind::Opaque)
        } else {
            self.expr_kind(node, &o)
        };
        Some(Expr {
            loc: Loc::at(*v.pos()),
            ty: ty?,
            kind: kind?,
        })
    }

    fn expr_kind(&mut self, node: &str, o: &Obj<'_>) -> Option<ExprKind> {
        match node {
            "num" | "bool" | "text" | "date" | "enum" => {
                self.literal_fields(node, o).map(ExprKind::Literal)
            }
            "field" => self
                .req(o, "name")
                .and_then(|v| self.name_of(v, &o.child("name")))
                .map(|name| ExprKind::Field { name }),
            "ref" => self
                .req(o, "target")
                .and_then(|v| self.name_of(v, &o.child("target")))
                .map(|target| ExprKind::Ref { target }),
            "rank_reference" => self
                .req(o, "target")
                .and_then(|v| self.name_of(v, &o.child("target")))
                .map(|target| ExprKind::RankReference { target }),
            "binary" => {
                let op = self.operator(o, "op", BinaryOp::from_name);
                let (left, right) = self.operands(o);
                Some(ExprKind::Binary {
                    op: op?,
                    left: Box::new(left?),
                    right: Box::new(right?),
                })
            }
            "cmp" => {
                let op = self.operator(o, "op", CmpOp::from_name);
                let (left, right) = self.operands(o);
                Some(ExprKind::Cmp {
                    op: op?,
                    left: Box::new(left?),
                    right: Box::new(right?),
                })
            }
            "and" | "or" => {
                let args = self
                    .req(o, "args")
                    .and_then(|v| self.each(v, &o.child("args"), |d, item, p| d.expr(item, p)))?;
                Some(if node == "and" {
                    ExprKind::And(args)
                } else {
                    ExprKind::Or(args)
                })
            }
            "not" => {
                let arg = self
                    .req(o, "arg")
                    .and_then(|v| self.expr(v, &o.child("arg")))?;
                Some(ExprKind::Not(Box::new(arg)))
            }
            "in" => {
                let value = self
                    .req(o, "value")
                    .and_then(|v| self.expr(v, &o.child("value")));
                let set = self
                    .req(o, "set")
                    .and_then(|v| self.name_of(v, &o.child("set")));
                Some(ExprKind::In {
                    value: Box::new(value?),
                    set: set?,
                })
            }
            "agg" => {
                let func = self.operator(o, "fn", AggFn::from_name);
                let collection = self
                    .req(o, "collection")
                    .and_then(|v| self.name_of(v, &o.child("collection")));
                let field = self.req(o, "field").and_then(|v| {
                    if matches!(v.kind(), Kind::Null) {
                        Some(None)
                    } else {
                        self.name_of(v, &o.child("field")).map(Some)
                    }
                });
                Some(ExprKind::Agg {
                    func: func?,
                    collection: collection?,
                    field: field?,
                })
            }
            _ => {
                let arg = self
                    .req(o, "arg")
                    .and_then(|v| self.expr(v, &o.child("arg")));
                let places = self
                    .req(o, "places")
                    .and_then(|v| self.places(v, &o.child("places")));
                let mode = self
                    .req(o, "mode")
                    .and_then(|v| self.rounding_mode(v, &o.child("mode")));
                Some(ExprKind::Round {
                    arg: Box::new(arg?),
                    places: places?,
                    mode: mode?,
                })
            }
        }
    }

    fn operator<T>(&mut self, o: &Obj<'_>, key: &str, parse: fn(&str) -> Option<T>) -> Option<T> {
        let v = self.req(o, key)?;
        let text = self.str_of(v, &o.child(key))?;
        let op = parse(text);
        if op.is_none() {
            self.invalid(
                format!("unknown operator '{}'", escape_controls(text)),
                *v.pos(),
                &o.child(key),
            );
        }
        op
    }

    fn operands(&mut self, o: &Obj<'_>) -> (Option<Expr>, Option<Expr>) {
        let left = self
            .req(o, "left")
            .and_then(|v| self.expr(v, &o.child("left")));
        let right = self
            .req(o, "right")
            .and_then(|v| self.expr(v, &o.child("right")));
        (left, right)
    }
}
