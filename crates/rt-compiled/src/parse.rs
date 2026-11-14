//! The parser of the compiled artifact.
//!
//! Strict: an unknown key, a missing key, a wrong JSON type or a failed invariant is a
//! Diagnostic with a position, and no value is returned. Flag and lifecycle keys are rejected
//! first with `RT-DGN-0001` to `0003` (Req 22.6), whatever their value.
//!
//! Codes. Syntax and limit faults keep the codes of `rt-json`. A key the format does not define
//! is `RT-IR-0002`; an unsupported `ir_version` is `RT-IR-0003`; an absent one `RT-IR-0006`; a
//! number that is not an Exact_Number string is `RT-IR-0004`; a malformed document hash is
//! `RT-IR-0007`; every other fault, including a failed structural invariant, is `RT-IR-0010`.

use crate::check::CompiledError;
use crate::index::{
    CollectionIdx, EnumIdx, FieldIdx, InputIdx, MemberIdx, NodeIdx, RuleIdx, SetIdx, TargetIdx,
};
use crate::model::{
    CType, CompiledBody, CompiledCollection, CompiledDocument, CompiledEnum, CompiledField,
    CompiledInput, CompiledLiteral, CompiledNode, CompiledParts, CompiledPrecedence, CompiledRule,
    CompiledRulebook, CompiledSet, CompiledSpan, CompiledTerm, CompiledTieBreaker, NodeKind,
};
use crate::serial::FORMAT_NAME;
use rt_foundation::{
    escape_controls, Code, Diagnostic, DiagnosticState, DocumentId, InputId, InputLocation,
    Location, RevisionNumber, RuleId,
};
use rt_hash::Sha256Hex;
use rt_ir::{AggFn, BinaryOp, CmpOp, Date, Direction, IrVersion, Name, Places, Rounding, Ties};
use rt_json::{check_flag_keys, parse, Kind, Member, ParseLimits, Position, Value};
use rt_numeric::RoundingMode;
use std::collections::{BTreeMap, BTreeSet};

type Json = Value<Position>;

const TOP: [&str; 12] = [
    "format",
    "compiler_version",
    "ir_version",
    "rulebook_hash",
    "documents",
    "inputs",
    "collections",
    "enums",
    "sets",
    "order",
    "rules",
    "precedence",
];
const RULE_BASE: [&str; 7] = ["id", "revision", "kind", "target", "type", "spans", "nodes"];

/// Parses a compiled artifact.
///
/// # Errors
/// All Diagnostics found, in canonical order. No value is returned with any of them.
pub fn parse_compiled(
    bytes: &[u8],
    input: &InputId,
    max_bytes: u64,
    max_depth: u32,
) -> Result<CompiledRulebook, DiagnosticState> {
    let limits = ParseLimits::new(max_bytes, max_depth);
    let root = parse(input, bytes, limits).map_err(|d| state(vec![*d]))?;
    check_flag_keys(input, &root).map_err(state)?;
    let mut dec = Dec {
        input,
        diags: Vec::new(),
        targets: scan_targets(&root),
    };
    let parts = dec.top(&root);
    match parts {
        Some(parts) if dec.diags.is_empty() => match CompiledRulebook::from_parts(parts) {
            Ok(rb) => Ok(rb),
            Err(e) => {
                dec.invalid(*root.pos(), &e);
                Err(state(dec.diags))
            }
        },
        _ => {
            if dec.diags.is_empty() {
                dec.diags.push(Diagnostic::new(
                    Code::InternalError,
                    "the compiled artifact decoder produced no value and no diagnostic".to_owned(),
                    Location::None(Code::InternalError),
                ));
            }
            Err(state(dec.diags))
        }
    }
}

fn state(mut diags: Vec<Diagnostic>) -> DiagnosticState {
    diags.sort();
    let mut s = DiagnosticState::empty();
    for d in diags {
        s.push(d);
    }
    s
}

/// The distinct Target names of `rules`, read before the rules are decoded. A malformed rule
/// contributes nothing here and is reported when it is decoded.
fn scan_targets(root: &Json) -> Vec<String> {
    let mut set = BTreeSet::new();
    let rules = root
        .get("rules")
        .and_then(|m| m.value.as_array())
        .unwrap_or(&[]);
    for rule in rules {
        if let Some(t) = rule.get("target").and_then(|m| m.value.as_str()) {
            set.insert(t.to_owned());
        }
    }
    set.into_iter().collect()
}

/// The rule identifiers by position in `rules`, read before the rules are decoded so that a
/// fault in one rule does not make every later reference to it look dangling.
fn scan_ids(root: &Json) -> BTreeMap<String, RuleIdx> {
    let rules = root
        .get("rules")
        .and_then(|m| m.value.as_array())
        .unwrap_or(&[]);
    let mut ids = BTreeMap::new();
    for (i, rule) in rules.iter().enumerate() {
        let id = rule.get("id").and_then(|m| m.value.as_str());
        if let (Some(id), Some(at)) = (id, RuleIdx::from_usize(i)) {
            ids.entry(id.to_owned()).or_insert(at);
        }
    }
    ids
}

struct Dec<'a> {
    input: &'a InputId,
    diags: Vec<Diagnostic>,
    targets: Vec<String>,
}

fn join(path: &str, key: &str) -> String {
    let key = escape_controls(key);
    if path.is_empty() {
        key
    } else {
        format!("{path}.{key}")
    }
}

fn get<'v>(members: &'v [Member<Position>], key: &str) -> Option<&'v Json> {
    members.iter().find(|m| m.key == key).map(|m| &m.value)
}

impl<'a> Dec<'a> {
    fn push(&mut self, code: Code, at: Position, path: &str, msg: &str) {
        let location = Location::Input(InputLocation {
            input: self.input.clone(),
            line: Some(at.line),
            column: Some(at.column),
            field: Some(if path.is_empty() { "(document)" } else { path }.to_owned()),
        });
        self.diags
            .push(Diagnostic::new(code, format!("{path}: {msg}"), location));
    }

    fn bad(&mut self, at: Position, path: &str, msg: &str) {
        self.push(Code::IrFieldInvalid, at, path, msg);
    }

    fn invalid(&mut self, at: Position, e: &CompiledError) {
        self.push(Code::IrFieldInvalid, at, &e.at, &e.to_string());
    }

    /// An object whose keys are exactly `keys`. Faults are recorded and the members are still
    /// returned so that decoding continues and every fault of the file is reported.
    fn obj<'v>(
        &mut self,
        v: &'v Json,
        path: &str,
        keys: &[&str],
    ) -> Option<&'v [Member<Position>]> {
        let Some(members) = v.as_object() else {
            self.bad(*v.pos(), path, "expected an object");
            return None;
        };
        for m in members {
            if !keys.contains(&m.key.as_str()) {
                self.push(
                    Code::IrUnknownField,
                    m.key_pos,
                    &join(path, &m.key),
                    "key not defined by the compiled format",
                );
            }
        }
        for key in keys {
            if !members.iter().any(|m| m.key == *key) {
                let code = if *key == "ir_version" {
                    Code::IrVersionMissingOrRepeated
                } else {
                    Code::IrFieldInvalid
                };
                self.push(code, *v.pos(), &join(path, key), "missing key");
            }
        }
        Some(members)
    }

    fn str_<'v>(&mut self, v: &'v Json, path: &str) -> Option<&'v str> {
        let s = v.as_str();
        if s.is_none() {
            self.bad(*v.pos(), path, "expected a string");
        }
        s
    }

    fn u32_(&mut self, v: &Json, path: &str) -> Option<u32> {
        let n = v.as_u64().and_then(|n| u32::try_from(n).ok());
        if n.is_none() {
            self.bad(*v.pos(), path, "expected an integer from 0 to 4294967295");
        }
        n
    }

    fn arr<'v>(&mut self, v: &'v Json, path: &str) -> Option<&'v [Json]> {
        let a = v.as_array();
        if a.is_none() {
            self.bad(*v.pos(), path, "expected an array");
        }
        a
    }

    /// Decodes every element, so that all faults are reported. `None` if any element failed.
    fn list<T>(
        &mut self,
        v: &Json,
        path: &str,
        mut f: impl FnMut(&mut Self, &Json, &str) -> Option<T>,
    ) -> Option<Vec<T>> {
        let items = self.arr(v, path)?;
        let mut out = Vec::with_capacity(items.len());
        let mut ok = true;
        for (i, item) in items.iter().enumerate() {
            match f(self, item, &format!("{path}[{i}]")) {
                Some(x) => out.push(x),
                None => ok = false,
            }
        }
        ok.then_some(out)
    }

    fn name(&mut self, v: &Json, path: &str) -> Option<Name> {
        let s = self.str_(v, path)?;
        match Name::new(s) {
            Ok(n) => Some(n),
            Err(e) => {
                self.bad(*v.pos(), path, &e.to_string());
                None
            }
        }
    }

    fn rule_id(&mut self, v: &Json, path: &str) -> Option<RuleId> {
        let s = self.str_(v, path)?;
        match RuleId::new(s) {
            Ok(n) => Some(n),
            Err(e) => {
                self.bad(*v.pos(), path, &e.to_string());
                None
            }
        }
    }

    fn doc_id(&mut self, v: &Json, path: &str) -> Option<DocumentId> {
        let s = self.str_(v, path)?;
        match DocumentId::new(s) {
            Ok(n) => Some(n),
            Err(e) => {
                self.bad(*v.pos(), path, &e.to_string());
                None
            }
        }
    }

    fn sha(&mut self, v: &Json, path: &str) -> Option<Sha256Hex> {
        let s = self.str_(v, path)?;
        let h = Sha256Hex::parse(s);
        if h.is_none() {
            self.push(
                Code::IrDocumentHashInvalid,
                *v.pos(),
                path,
                "expected 64 lowercase hexadecimal characters",
            );
        }
        h
    }

    fn number(&mut self, v: &Json, path: &str) -> Option<rt_numeric::ExactNumber> {
        match v.kind() {
            Kind::String(s) => match rt_numeric::parse_canonical(s) {
                Ok(n) => Some(n),
                Err(e) => {
                    self.push(
                        Code::IrNumericLiteralInvalid,
                        *v.pos(),
                        path,
                        &e.to_string(),
                    );
                    None
                }
            },
            Kind::Integer(_) | Kind::Number(_) => {
                self.push(
                    Code::IrNumericLiteralInvalid,
                    *v.pos(),
                    path,
                    "an Exact_Number is a string, not a JSON number",
                );
                None
            }
            _ => {
                self.bad(*v.pos(), path, "expected an Exact_Number string");
                None
            }
        }
    }

    fn ctype(&mut self, v: &Json, path: &str) -> Option<CType> {
        let s = self.str_(v, path)?;
        let t = match s {
            "number" => Some(CType::Number),
            "boolean" => Some(CType::Boolean),
            "text" => Some(CType::Text),
            "date" => Some(CType::Date),
            "ranking" => Some(CType::Ranking),
            _ => s
                .strip_prefix("enum:")
                .filter(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
                .filter(|d| d.len() == 1 || !d.starts_with('0'))
                .and_then(|d| d.parse::<u32>().ok())
                .map(|n| CType::Enum(EnumIdx::new(n))),
        };
        if t.is_none() {
            self.bad(*v.pos(), path, "not a resolved type");
        }
        t
    }

    fn direction(&mut self, v: &Json, path: &str) -> Option<Direction> {
        let s = self.str_(v, path)?;
        let d = Direction::from_name(s);
        if d.is_none() {
            self.bad(*v.pos(), path, "expected \"asc\" or \"desc\"");
        }
        d
    }

    fn target(&mut self, v: &Json, path: &str) -> Option<TargetIdx> {
        let s = self.str_(v, path)?;
        let found = self
            .targets
            .binary_search_by(|t| t.as_str().cmp(s))
            .ok()
            .and_then(TargetIdx::from_usize);
        if found.is_none() {
            self.bad(*v.pos(), path, "no rule defines this Target");
        }
        found
    }

    fn top(&mut self, root: &Json) -> Option<CompiledParts> {
        let m = self.obj(root, "", &TOP)?;
        if let Some(f) = get(m, "format") {
            if f.as_str() != Some(FORMAT_NAME) {
                self.bad(*f.pos(), "format", "expected \"ruletrace-compiled\"");
            }
        }
        let ir_version = get(m, "ir_version").and_then(|v| {
            let s = self.str_(v, "ir_version")?;
            let parsed = IrVersion::parse(s);
            if parsed.is_none() {
                self.push(
                    Code::IrVersionUnsupported,
                    *v.pos(),
                    "ir_version",
                    "unsupported IR_Version; supported: [\"1\"]",
                );
            }
            parsed
        });
        let compiler_version = get(m, "compiler_version")
            .and_then(|v| self.str_(v, "compiler_version"))
            .map(str::to_owned);
        let rulebook_hash = get(m, "rulebook_hash").and_then(|v| self.sha(v, "rulebook_hash"));
        let documents = get(m, "documents").and_then(|v| self.list(v, "documents", Self::document));
        let inputs = get(m, "inputs").and_then(|v| self.list(v, "inputs", Self::input));
        let collections =
            get(m, "collections").and_then(|v| self.list(v, "collections", Self::collection));
        let enums = get(m, "enums").and_then(|v| self.list(v, "enums", Self::enumeration));
        let sets = get(m, "sets").and_then(|v| self.list(v, "sets", Self::set));
        let rules = get(m, "rules").and_then(|v| self.list(v, "rules", Self::rule));
        let ids = scan_ids(root);
        let order = get(m, "order").and_then(|v| {
            self.list(v, "order", |d, v, p| {
                let s = d.str_(v, p)?;
                let r = ids.get(s).copied();
                if r.is_none() {
                    d.bad(*v.pos(), p, "no rule has this identifier");
                }
                r
            })
        });
        let precedence = get(m, "precedence")
            .and_then(|v| self.list(v, "precedence", |d, v, p| d.precedence(v, p, &ids)));
        Some(CompiledParts {
            compiler_version: compiler_version?,
            ir_version: ir_version?,
            rulebook_hash: rulebook_hash?,
            documents: documents?,
            inputs: inputs?,
            collections: collections?,
            enums: enums?,
            sets: sets?,
            order: order?,
            rules: rules?,
            precedence: precedence?,
        })
    }

    fn document(&mut self, v: &Json, path: &str) -> Option<CompiledDocument> {
        let m = self.obj(v, path, &["id", "sha256"])?;
        let id = get(m, "id").and_then(|v| self.doc_id(v, &join(path, "id")));
        let sha256 = get(m, "sha256").and_then(|v| self.sha(v, &join(path, "sha256")));
        Some(CompiledDocument {
            id: id?,
            sha256: sha256?,
        })
    }

    fn input(&mut self, v: &Json, path: &str) -> Option<CompiledInput> {
        let m = self.obj(v, path, &["name", "type"])?;
        let name = get(m, "name").and_then(|v| self.name(v, &join(path, "name")));
        let ty = get(m, "type").and_then(|v| self.ctype(v, &join(path, "type")));
        Some(CompiledInput {
            name: name?,
            ty: ty?,
        })
    }

    fn field(&mut self, v: &Json, path: &str) -> Option<CompiledField> {
        let i = self.input(v, path)?;
        Some(CompiledField {
            name: i.name,
            ty: i.ty,
        })
    }

    fn collection(&mut self, v: &Json, path: &str) -> Option<CompiledCollection> {
        let m = self.obj(v, path, &["name", "fields"])?;
        let name = get(m, "name").and_then(|v| self.name(v, &join(path, "name")));
        let fields =
            get(m, "fields").and_then(|v| self.list(v, &join(path, "fields"), Self::field));
        Some(CompiledCollection {
            name: name?,
            fields: fields?,
        })
    }

    fn enumeration(&mut self, v: &Json, path: &str) -> Option<CompiledEnum> {
        let m = self.obj(v, path, &["name", "members"])?;
        let name = get(m, "name").and_then(|v| self.name(v, &join(path, "name")));
        let members =
            get(m, "members").and_then(|v| self.list(v, &join(path, "members"), Self::name));
        Some(CompiledEnum {
            name: name?,
            members: members?,
        })
    }

    fn set(&mut self, v: &Json, path: &str) -> Option<CompiledSet> {
        let m = self.obj(v, path, &["name", "element_type", "members"])?;
        let name = get(m, "name").and_then(|v| self.name(v, &join(path, "name")));
        let element_type =
            get(m, "element_type").and_then(|v| self.ctype(v, &join(path, "element_type")));
        let members =
            get(m, "members").and_then(|v| self.list(v, &join(path, "members"), Self::literal));
        Some(CompiledSet {
            name: name?,
            element_type: element_type?,
            members: members?,
        })
    }

    fn span(&mut self, v: &Json, path: &str) -> Option<CompiledSpan> {
        let keys = ["document", "sha256", "start", "end", "line", "column"];
        let m = self.obj(v, path, &keys)?;
        let document = get(m, "document").and_then(|v| self.doc_id(v, &join(path, "document")));
        let sha256 = get(m, "sha256").and_then(|v| self.sha(v, &join(path, "sha256")));
        let start = get(m, "start").and_then(|v| self.u32_(v, &join(path, "start")));
        let end = get(m, "end").and_then(|v| self.u32_(v, &join(path, "end")));
        let line = get(m, "line").and_then(|v| self.u32_(v, &join(path, "line")));
        let column = get(m, "column").and_then(|v| self.u32_(v, &join(path, "column")));
        Some(CompiledSpan {
            document: document?,
            sha256: sha256?,
            start: start?,
            end: end?,
            line: line?,
            column: column?,
        })
    }

    fn spans(&mut self, v: &Json, path: &str) -> Option<Vec<CompiledSpan>> {
        self.list(v, path, Self::span)
    }

    fn precedence(
        &mut self,
        v: &Json,
        path: &str,
        ids: &BTreeMap<String, RuleIdx>,
    ) -> Option<CompiledPrecedence> {
        let m = self.obj(v, path, &["id", "target", "order", "spans"])?;
        let id = get(m, "id").and_then(|v| self.rule_id(v, &join(path, "id")));
        let target = get(m, "target").and_then(|v| self.target(v, &join(path, "target")));
        let order = get(m, "order").and_then(|v| {
            self.list(v, &join(path, "order"), |d, v, p| {
                let s = d.str_(v, p)?;
                let r = ids.get(s).copied();
                if r.is_none() {
                    d.bad(*v.pos(), p, "no rule has this identifier");
                }
                r
            })
        });
        let spans = get(m, "spans").and_then(|v| self.spans(v, &join(path, "spans")));
        Some(CompiledPrecedence {
            id: id?,
            target: target?,
            order: order?,
            spans: spans?,
        })
    }

    /// A literal object: `{"node":"num","value":".."}` and the other kinds of a literal.
    fn literal(&mut self, v: &Json, path: &str) -> Option<CompiledLiteral> {
        let kind = self.peek(v, "node", path)?;
        let keys: &[&str] = if kind == "enum" {
            &["node", "enum", "member"]
        } else {
            &["node", "value"]
        };
        let m = self.obj(v, path, keys)?;
        self.literal_payload(m, kind, v, path)
    }

    /// The member `key` of an object as text, for dispatch before the strict key check.
    fn peek<'v>(&mut self, v: &'v Json, key: &str, path: &str) -> Option<&'v str> {
        let Some(members) = v.as_object() else {
            self.bad(*v.pos(), path, "expected an object");
            return None;
        };
        let Some(value) = get(members, key) else {
            self.bad(*v.pos(), &join(path, key), "missing key");
            return None;
        };
        self.str_(value, &join(path, key))
    }

    fn literal_payload(
        &mut self,
        m: &[Member<Position>],
        kind: &str,
        whole: &Json,
        path: &str,
    ) -> Option<CompiledLiteral> {
        let vp = join(path, "value");
        let value = get(m, "value");
        match kind {
            "num" => value
                .and_then(|v| self.number(v, &vp))
                .map(CompiledLiteral::Number),
            "bool" => value.and_then(|v| match v.kind() {
                Kind::Bool(b) => Some(CompiledLiteral::Boolean(*b)),
                _ => {
                    self.bad(*v.pos(), &vp, "expected true or false");
                    None
                }
            }),
            "text" => value
                .and_then(|v| self.str_(v, &vp))
                .map(|s| CompiledLiteral::Text(s.to_owned())),
            "date" => value.and_then(|v| {
                let s = self.str_(v, &vp)?;
                match Date::parse(s) {
                    Ok(d) => Some(CompiledLiteral::Date(d)),
                    Err(e) => {
                        self.bad(*v.pos(), &vp, &e.to_string());
                        None
                    }
                }
            }),
            "enum" => {
                let e = get(m, "enum").and_then(|v| self.u32_(v, &join(path, "enum")));
                let mem = get(m, "member").and_then(|v| self.u32_(v, &join(path, "member")));
                Some(CompiledLiteral::Enum {
                    enumeration: EnumIdx::new(e?),
                    member: MemberIdx::new(mem?),
                })
            }
            other => {
                self.push(
                    Code::IrUnknownField,
                    *whole.pos(),
                    &join(path, "node"),
                    &format!("literal kind `{other}` is not defined by the compiled format"),
                );
                None
            }
        }
    }

    fn node_idx(&mut self, v: &Json, path: &str) -> Option<NodeIdx> {
        self.u32_(v, path).map(NodeIdx::new)
    }

    fn rounding(&mut self, v: &Json, path: &str) -> Option<Option<Rounding>> {
        if matches!(v.kind(), Kind::Null) {
            return Some(None);
        }
        let m = self.obj(v, path, &["mode", "places"])?;
        let mode = get(m, "mode").and_then(|v| {
            let s = self.str_(v, &join(path, "mode"))?;
            let mode = RoundingMode::from_name(s);
            if mode.is_none() {
                self.bad(*v.pos(), &join(path, "mode"), "unknown rounding mode");
            }
            mode
        });
        let places = get(m, "places").and_then(|v| {
            let n = self.u32_(v, &join(path, "places"))?;
            let p = Places::new(u64::from(n)).ok();
            if p.is_none() {
                self.bad(*v.pos(), &join(path, "places"), "places outside 0 to 28");
            }
            p
        });
        Some(Some(Rounding {
            mode: mode?,
            places: places?,
        }))
    }

    fn node(&mut self, v: &Json, path: &str, position: usize) -> Option<CompiledNode> {
        let kind = self.peek(v, "node", path)?;
        let extra: &[&str] = match kind {
            "num" | "bool" | "text" | "date" => &["value"],
            "enum" => &["enum", "member"],
            "field" => &["input"],
            "ref" | "rank_reference" => &["target"],
            "binary" | "cmp" => &["op", "left", "right"],
            "and" | "or" => &["args"],
            "not" => &["arg"],
            "in" => &["value", "set"],
            "agg" => &["fn", "collection", "field"],
            "round" => &["arg", "places", "mode"],
            other => {
                self.push(
                    Code::IrUnknownField,
                    *v.pos(),
                    &join(path, "node"),
                    &format!("node kind `{other}` is not defined by the compiled format"),
                );
                return None;
            }
        };
        let mut keys = vec!["idx", "node", "t"];
        keys.extend_from_slice(extra);
        let m = self.obj(v, path, &keys)?;
        let at = get(m, "idx").and_then(|v| {
            let n = self.u32_(v, &join(path, "idx"))?;
            if usize::try_from(n).ok() != Some(position) {
                self.bad(
                    *v.pos(),
                    &join(path, "idx"),
                    "idx must equal the position in nodes",
                );
                return None;
            }
            Some(n)
        });
        let ty = get(m, "t").and_then(|v| self.ctype(v, &join(path, "t")));
        let p = |key: &str| join(path, key);
        let kind_value = match kind {
            "num" | "bool" | "text" | "date" | "enum" => self
                .literal_payload(m, kind, v, path)
                .map(NodeKind::Literal),
            "field" => get(m, "input")
                .and_then(|v| self.u32_(v, &p("input")))
                .map(|n| NodeKind::Field(InputIdx::new(n))),
            "ref" => get(m, "target")
                .and_then(|v| self.target(v, &p("target")))
                .map(NodeKind::Ref),
            "rank_reference" => get(m, "target")
                .and_then(|v| self.target(v, &p("target")))
                .map(NodeKind::RankReference),
            "binary" => {
                let op = get(m, "op").and_then(|v| self.op(v, &p("op"), BinaryOp::from_name));
                let l = get(m, "left").and_then(|v| self.node_idx(v, &p("left")));
                let r = get(m, "right").and_then(|v| self.node_idx(v, &p("right")));
                Some(NodeKind::Binary {
                    op: op?,
                    left: l?,
                    right: r?,
                })
            }
            "cmp" => {
                let op = get(m, "op").and_then(|v| self.op(v, &p("op"), CmpOp::from_name));
                let l = get(m, "left").and_then(|v| self.node_idx(v, &p("left")));
                let r = get(m, "right").and_then(|v| self.node_idx(v, &p("right")));
                Some(NodeKind::Cmp {
                    op: op?,
                    left: l?,
                    right: r?,
                })
            }
            "and" | "or" => {
                let args = get(m, "args")
                    .and_then(|v| self.list(v, &p("args"), |d, v, p| d.node_idx(v, p)))?;
                Some(if kind == "and" {
                    NodeKind::And(args)
                } else {
                    NodeKind::Or(args)
                })
            }
            "not" => get(m, "arg")
                .and_then(|v| self.node_idx(v, &p("arg")))
                .map(NodeKind::Not),
            "in" => {
                let value = get(m, "value").and_then(|v| self.node_idx(v, &p("value")));
                let set = get(m, "set").and_then(|v| self.u32_(v, &p("set")));
                Some(NodeKind::In {
                    value: value?,
                    set: SetIdx::new(set?),
                })
            }
            "agg" => {
                let func = get(m, "fn").and_then(|v| self.op(v, &p("fn"), AggFn::from_name));
                let collection = get(m, "collection").and_then(|v| self.u32_(v, &p("collection")));
                let field = get(m, "field").and_then(|v| {
                    if matches!(v.kind(), Kind::Null) {
                        Some(None)
                    } else {
                        self.u32_(v, &p("field")).map(|n| Some(FieldIdx::new(n)))
                    }
                });
                Some(NodeKind::Agg {
                    func: func?,
                    collection: CollectionIdx::new(collection?),
                    field: field?,
                })
            }
            _ => {
                // "round": the other names were handled above.
                let arg = get(m, "arg").and_then(|v| self.node_idx(v, &p("arg")));
                let places = get(m, "places").and_then(|v| {
                    let n = self.u32_(v, &p("places"))?;
                    let pl = Places::new(u64::from(n)).ok();
                    if pl.is_none() {
                        self.bad(*v.pos(), &p("places"), "places outside 0 to 28");
                    }
                    pl
                });
                let mode =
                    get(m, "mode").and_then(|v| self.op(v, &p("mode"), RoundingMode::from_name));
                Some(NodeKind::Round {
                    arg: arg?,
                    rounding: Rounding {
                        mode: mode?,
                        places: places?,
                    },
                })
            }
        };
        let _ = at?;
        Some(CompiledNode {
            ty: ty?,
            kind: kind_value?,
        })
    }

    /// A name from a closed set, such as an operator.
    fn op<T>(&mut self, v: &Json, path: &str, from: impl Fn(&str) -> Option<T>) -> Option<T> {
        let s = self.str_(v, path)?;
        let found = from(s);
        if found.is_none() {
            self.bad(
                *v.pos(),
                path,
                &format!("`{s}` is not defined by the compiled format"),
            );
        }
        found
    }

    fn rule(&mut self, v: &Json, path: &str) -> Option<CompiledRule> {
        let kind = self.peek(v, "kind", path)?;
        let extra: &[&str] = match kind {
            "eligibility" | "derived" => &["expr"],
            "weighted_score" => &["terms", "rounding"],
            "ranking" => &["key", "direction", "tie_breakers", "ties"],
            "default" => &["value"],
            other => {
                self.push(
                    Code::IrUnknownField,
                    *v.pos(),
                    &join(path, "kind"),
                    &format!("rule kind `{other}` is not defined by the compiled format"),
                );
                return None;
            }
        };
        let mut keys = RULE_BASE.to_vec();
        keys.extend_from_slice(extra);
        let m = self.obj(v, path, &keys)?;
        let p = |key: &str| join(path, key);
        let id = get(m, "id").and_then(|v| self.rule_id(v, &p("id")));
        let revision = get(m, "revision").and_then(|v| {
            let n = self.u32_(v, &p("revision"))?;
            match RevisionNumber::new(n) {
                Ok(r) => Some(r),
                Err(e) => {
                    self.bad(*v.pos(), &p("revision"), &e.to_string());
                    None
                }
            }
        });
        let target = get(m, "target").and_then(|v| self.name(v, &p("target")));
        let ty = get(m, "type").and_then(|v| self.ctype(v, &p("type")));
        let spans = get(m, "spans").and_then(|v| self.spans(v, &p("spans")));
        let nodes = get(m, "nodes").and_then(|v| {
            let mut position = 0usize;
            self.list(v, &p("nodes"), |d, v, np| {
                let n = d.node(v, np, position);
                position = position.saturating_add(1);
                n
            })
        });
        let body = match kind {
            "eligibility" => get(m, "expr")
                .and_then(|v| self.node_idx(v, &p("expr")))
                .map(|expr| CompiledBody::Eligibility { expr }),
            "derived" => get(m, "expr")
                .and_then(|v| self.node_idx(v, &p("expr")))
                .map(|expr| CompiledBody::Derived { expr }),
            "weighted_score" => {
                let terms = get(m, "terms").and_then(|v| self.list(v, &p("terms"), Self::term));
                let rounding = get(m, "rounding").and_then(|v| self.rounding(v, &p("rounding")));
                Some(CompiledBody::WeightedScore {
                    terms: terms?,
                    rounding: rounding?,
                })
            }
            "ranking" => {
                let key = get(m, "key").and_then(|v| self.node_idx(v, &p("key")));
                let direction =
                    get(m, "direction").and_then(|v| self.direction(v, &p("direction")));
                let tie_breakers = get(m, "tie_breakers")
                    .and_then(|v| self.list(v, &p("tie_breakers"), Self::tie_breaker));
                let ties = get(m, "ties").and_then(|v| {
                    if v.as_str() == Some("group") {
                        Some(Ties::Group)
                    } else {
                        self.bad(*v.pos(), &p("ties"), "expected \"group\"");
                        None
                    }
                });
                Some(CompiledBody::Ranking {
                    key: key?,
                    direction: direction?,
                    tie_breakers: tie_breakers?,
                    ties: ties?,
                })
            }
            _ => get(m, "value")
                .and_then(|v| self.literal(v, &p("value")))
                .map(|value| CompiledBody::Default { value }),
        };
        Some(CompiledRule {
            id: id?,
            revision: revision?,
            target: target?,
            ty: ty?,
            spans: spans?,
            nodes: nodes?,
            body: body?,
        })
    }

    fn term(&mut self, v: &Json, path: &str) -> Option<CompiledTerm> {
        let m = self.obj(v, path, &["weight", "value"])?;
        let weight = get(m, "weight").and_then(|v| self.node_idx(v, &join(path, "weight")));
        let value = get(m, "value").and_then(|v| self.node_idx(v, &join(path, "value")));
        Some(CompiledTerm {
            weight: weight?,
            value: value?,
        })
    }

    fn tie_breaker(&mut self, v: &Json, path: &str) -> Option<CompiledTieBreaker> {
        let m = self.obj(v, path, &["expr", "direction"])?;
        let expr = get(m, "expr").and_then(|v| self.node_idx(v, &join(path, "expr")));
        let direction =
            get(m, "direction").and_then(|v| self.direction(v, &join(path, "direction")));
        Some(CompiledTieBreaker {
            expr: expr?,
            direction: direction?,
        })
    }
}
