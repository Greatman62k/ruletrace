//! `scan-api` (design §3.4.3, Req 22.1, 24.2, 24.7).
//!
//! Reads the public function signatures of core and application crates with the token reader,
//! compares the ones named in `tools/contracts.v1` with their contract, and fails when any public
//! item takes or stores `has_errors` or `no_errors`.
//!
//! Contract line: `contract <crate> <name> (<param types>) [-> <result type>]`, where `<name>` is
//! `function` or `Type::method`. A receiver is written `&self`, `&mut self` or `self`. Types are
//! compared after dropping lifetimes and whitespace between symbols.

use crate::scan;
use crate::tokens::{normalize, production_tokens, skip_group, tokenize, Token};
use std::fs;
use std::path::Path;

/// Path of the contract list, relative to the repository root.
pub const CONTRACTS_PATH: &str = "tools/contracts.v1";
const HEADER: &str = "# format: ruletrace-contracts v1";
const FLAG_NAMES: [&str; 2] = ["has_errors", "no_errors"];

/// A function signature, as type names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    /// Parameter types in order (a receiver is `&self`, `&mut self` or `self`).
    pub params: Vec<String>,
    /// Result type, empty for `()`.
    pub result: String,
}

impl Signature {
    fn show(&self) -> String {
        let r = if self.result.is_empty() {
            String::new()
        } else {
            format!(" -> {}", self.result)
        };
        format!("({}){r}", self.params.join(", "))
    }
}

/// A function found in source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundFn {
    /// `name` or `Type::name`.
    pub qualified: String,
    /// Whether it is declared `pub` without a restriction.
    pub public: bool,
    /// Line of the `fn` keyword.
    pub line: usize,
    /// Its signature.
    pub signature: Signature,
    /// Identifiers bound by its parameter patterns, except `self`.
    pub param_names: Vec<String>,
}

/// A struct field found in source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundField {
    /// Struct name.
    pub owner: String,
    /// Field name.
    pub name: String,
    /// Line.
    pub line: usize,
}

/// Everything `scan-api` reads from one file.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FileItems {
    /// Functions.
    pub fns: Vec<FoundFn>,
    /// Struct fields.
    pub fields: Vec<FoundField>,
}

/// Index after the angle-bracket group opening at `open`.
fn skip_angles(tokens: &[Token], open: usize) -> usize {
    let mut depth = 0usize;
    let mut i = open;
    while let Some(t) = tokens.get(i) {
        if t.is_punct("<") {
            depth += 1;
        } else if t.is_punct(">") {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return i + 1;
            }
        }
        i += 1;
    }
    tokens.len()
}

/// Splits `tokens` at commas that are not inside brackets or angle brackets.
fn split_top(tokens: &[Token]) -> Vec<&[Token]> {
    let mut parts = Vec::new();
    let (mut depth, mut angle, mut start) = (0usize, 0usize, 0usize);
    for (i, t) in tokens.iter().enumerate() {
        if t.is_punct("(") || t.is_punct("[") || t.is_punct("{") {
            depth += 1;
        } else if t.is_punct(")") || t.is_punct("]") || t.is_punct("}") {
            depth = depth.saturating_sub(1);
        } else if t.is_punct("<") {
            angle += 1;
        } else if t.is_punct(">") {
            angle = angle.saturating_sub(1);
        } else if t.is_punct(",") && depth == 0 && angle == 0 {
            parts.push(tokens.get(start..i).unwrap_or(&[]));
            start = i + 1;
        }
    }
    let last = tokens.get(start..).unwrap_or(&[]);
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

/// Parses `( params ) [-> result]` starting at the `(` at `open`. With `named`, each parameter
/// is `pattern: Type`; otherwise it is only a type. Returns the signature, the identifiers bound
/// by patterns and the index after the result type.
fn parse_signature(tokens: &[Token], open: usize, named: bool) -> (Signature, Vec<String>, usize) {
    let close = skip_group(tokens, open);
    let inner = tokens.get(open + 1..close.saturating_sub(1)).unwrap_or(&[]);
    let mut params = Vec::new();
    let mut names = Vec::new();
    for part in split_top(inner) {
        let mut p = part;
        while p.first().is_some_and(|t| t.is_punct("#")) {
            let end = skip_group(p, 1);
            p = p.get(end..).unwrap_or(&[]);
        }
        let colon = p.iter().position(|t| t.is_punct(":"));
        let is_receiver = named && p.iter().take(4).any(|t| t.is_ident("self"));
        match (named, colon) {
            (true, _) if is_receiver && colon.is_none() => params.push(normalize(p)),
            (true, Some(c)) if is_receiver => {
                params.push(normalize(p.get(c + 1..).unwrap_or(&[])));
            }
            (true, Some(c)) => {
                names.extend(
                    p.get(..c)
                        .unwrap_or(&[])
                        .iter()
                        .filter_map(Token::ident)
                        .filter(|id| *id != "mut" && *id != "ref")
                        .map(str::to_string),
                );
                params.push(normalize(p.get(c + 1..).unwrap_or(&[])));
            }
            (true, None) => params.push(normalize(p)),
            (false, _) => params.push(normalize(p)),
        }
    }
    let mut result = String::new();
    let mut end = close;
    if tokens.get(close).is_some_and(|t| t.is_punct("->")) {
        let (mut depth, mut angle) = (0usize, 0usize);
        let mut j = close + 1;
        while let Some(t) = tokens.get(j) {
            if depth == 0
                && angle == 0
                && (t.is_punct("{") || t.is_punct(";") || t.is_ident("where"))
            {
                break;
            }
            if t.is_punct("(") || t.is_punct("[") {
                depth += 1;
            } else if t.is_punct(")") || t.is_punct("]") {
                depth = depth.saturating_sub(1);
            } else if t.is_punct("<") {
                angle += 1;
            } else if t.is_punct(">") {
                angle = angle.saturating_sub(1);
            }
            j += 1;
        }
        result = normalize(tokens.get(close + 1..j).unwrap_or(&[]));
        end = j;
    }
    if result == "()" {
        result.clear();
    }
    (Signature { params, result }, names, end)
}

/// Whether the `fn` at `i` is declared with a bare `pub`.
fn is_public(tokens: &[Token], i: usize) -> bool {
    let mut k = i;
    while k > 0 {
        let Some(prev) = tokens.get(k - 1) else {
            return false;
        };
        let qualifier = prev
            .ident()
            .is_some_and(|s| matches!(s, "const" | "async" | "unsafe" | "extern"))
            || matches!(prev.kind, crate::tokens::Kind::Str(_));
        if qualifier {
            k -= 1;
        } else {
            return prev.is_ident("pub");
        }
    }
    false
}

/// The type an `impl` block at `i` is for.
fn impl_type(tokens: &[Token], i: usize) -> Option<String> {
    let mut j = i + 1;
    if tokens.get(j).is_some_and(|t| t.is_punct("<")) {
        j = skip_angles(tokens, j);
    }
    let end = (j..tokens.len()).find(|k| tokens.get(*k).is_some_and(|t| t.is_punct("{")))?;
    let for_at = (j..end)
        .rev()
        .find(|k| tokens.get(*k).is_some_and(|t| t.is_ident("for")));
    let mut k = for_at.map_or(j, |f| f + 1);
    let mut name = None;
    while k < end {
        let t = tokens.get(k)?;
        if let Some(id) = t.ident() {
            if id == "where" {
                break;
            }
            if id != "dyn" && id != "mut" && id != "const" {
                name = Some(id.to_string());
                if tokens.get(k + 1).is_some_and(|n| n.is_punct("::")) {
                    k += 2;
                    continue;
                }
                break;
            }
        }
        k += 1;
    }
    name
}

/// Reads the functions and struct fields of one file, test code excluded.
#[must_use]
pub fn read_items(src: &str) -> FileItems {
    let (tokens, _) = production_tokens(src);
    let mut items = FileItems::default();
    let mut depth = 0usize;
    let mut impls: Vec<(String, usize)> = Vec::new();
    let mut pending: Option<String> = None;
    let mut i = 0usize;
    while let Some(t) = tokens.get(i) {
        if t.is_punct("{") {
            depth += 1;
            if let Some(name) = pending.take() {
                impls.push((name, depth));
            }
        } else if t.is_punct("}") {
            depth = depth.saturating_sub(1);
            while impls.last().is_some_and(|(_, d)| *d > depth) {
                impls.pop();
            }
        } else if t.is_ident("impl") && !tokens.get(i + 1).is_some_and(|n| n.is_punct("(")) {
            pending = impl_type(&tokens, i);
        } else if t.is_ident("struct") {
            read_struct(&tokens, i, &mut items);
        } else if t.is_ident("fn") {
            if let Some(name) = tokens.get(i + 1).and_then(Token::ident) {
                let mut j = i + 2;
                if tokens.get(j).is_some_and(|n| n.is_punct("<")) {
                    j = skip_angles(&tokens, j);
                }
                if tokens.get(j).is_some_and(|n| n.is_punct("(")) {
                    let (signature, param_names, end) = parse_signature(&tokens, j, true);
                    let qualified = match impls.last() {
                        Some((owner, _)) => format!("{owner}::{name}"),
                        None => name.to_string(),
                    };
                    items.fns.push(FoundFn {
                        qualified,
                        public: is_public(&tokens, i),
                        line: t.line,
                        signature,
                        param_names,
                    });
                    i = end;
                    continue;
                }
            }
        }
        i += 1;
    }
    items
}

fn read_struct(tokens: &[Token], at: usize, items: &mut FileItems) {
    let Some(owner) = tokens.get(at + 1).and_then(Token::ident) else {
        return;
    };
    let mut j = at + 2;
    if tokens.get(j).is_some_and(|t| t.is_punct("<")) {
        j = skip_angles(tokens, j);
    }
    while let Some(t) = tokens.get(j) {
        if t.is_punct(";") || t.is_punct("(") {
            return;
        }
        if t.is_punct("{") {
            break;
        }
        j += 1;
    }
    let end = skip_group(tokens, j);
    let mut depth = 0usize;
    for k in j..end {
        let Some(t) = tokens.get(k) else { return };
        if t.is_punct("{") || t.is_punct("(") || t.is_punct("[") {
            depth += 1;
        } else if t.is_punct("}") || t.is_punct(")") || t.is_punct("]") {
            depth = depth.saturating_sub(1);
        } else if depth == 1 && tokens.get(k + 1).is_some_and(|n| n.is_punct(":")) {
            if let Some(name) = t.ident() {
                items.fields.push(FoundField {
                    owner: owner.to_string(),
                    name: name.to_string(),
                    line: t.line,
                });
            }
        }
    }
}

/// One `contract` line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contract {
    /// Crate name.
    pub crate_name: String,
    /// `function` or `Type::method`.
    pub name: String,
    /// Expected signature.
    pub signature: Signature,
    /// Line number in the contract file.
    pub line: usize,
}

fn next_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.split_once(char::is_whitespace) {
        Some((w, rest)) => (w, rest.trim_start()),
        None => (s, ""),
    }
}

/// Parses `tools/contracts.v1`.
///
/// # Errors
/// A message naming the line when the header is missing or an entry is malformed.
pub fn parse_contracts(text: &str) -> Result<Vec<Contract>, String> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    match lines.next() {
        Some((_, l)) if l.trim() == HEADER => {}
        _ => return Err(format!("first line must be `{HEADER}`")),
    }
    let mut out = Vec::new();
    for (n, raw) in lines {
        let line = raw.trim();
        if line.starts_with('#') {
            continue;
        }
        let at = n + 1;
        let (verb, rest) = next_word(line);
        let (krate, rest) = next_word(rest);
        let (name, rest) = next_word(rest);
        if verb != "contract" || krate.is_empty() || name.is_empty() || !rest.starts_with('(') {
            return Err(format!(
                "line {at}: expected `contract <crate> <name> (<types>) [-> <type>]`"
            ));
        }
        let tokens = tokenize(rest);
        let (signature, _, end) = parse_signature(&tokens, 0, false);
        if end != tokens.len() {
            return Err(format!("line {at}: unexpected text after the result type"));
        }
        out.push(Contract {
            crate_name: krate.to_string(),
            name: name.to_string(),
            signature,
            line: at,
        });
    }
    Ok(out)
}

/// Findings for the contracts of one crate against its functions.
#[must_use]
pub fn check_contracts(
    krate: &str,
    files: &[(String, FileItems)],
    contracts: &[Contract],
) -> Vec<String> {
    let mut findings = Vec::new();
    for c in contracts.iter().filter(|c| c.crate_name == krate) {
        let found: Vec<(&String, &FoundFn)> = files
            .iter()
            .flat_map(|(path, items)| items.fns.iter().map(move |f| (path, f)))
            .filter(|(_, f)| f.public && f.qualified == c.name)
            .collect();
        match found.as_slice() {
            [] => findings.push(format!(
                "[contract-missing] {krate}: no public fn `{}` for contract line {}",
                c.name, c.line
            )),
            [(path, f)] => {
                if f.signature != c.signature {
                    findings.push(format!(
                        "[contract-mismatch] {krate}: {path}:{}: `{}` is {} but the contract says {}",
                        f.line,
                        c.name,
                        f.signature.show(),
                        c.signature.show()
                    ));
                }
            }
            _ => findings.push(format!(
                "[contract-ambiguous] {krate}: `{}` is public in {} places",
                c.name,
                found.len()
            )),
        }
    }
    findings
}

/// Findings for any item that takes or stores `has_errors` or `no_errors` (Req 22.1).
#[must_use]
pub fn check_flags(krate: &str, files: &[(String, FileItems)]) -> Vec<String> {
    let mut findings = Vec::new();
    for (path, items) in files {
        for f in &items.fns {
            let name = f.qualified.rsplit("::").next().unwrap_or("");
            let flag_fn = FLAG_NAMES.contains(&name)
                && f.signature.params.iter().any(|p| !p.ends_with("self"));
            let flag_param = f
                .param_names
                .iter()
                .any(|p| FLAG_NAMES.contains(&p.as_str()));
            if f.public && (flag_fn || flag_param) {
                findings.push(format!(
                    "[flag-in-api] {krate}: {path}:{}: `{}` takes a validity flag (has_errors or no_errors)",
                    f.line, f.qualified
                ));
            }
        }
        for field in &items.fields {
            if FLAG_NAMES.contains(&field.name.as_str()) {
                findings.push(format!(
                    "[flag-stored] {krate}: {path}:{}: `{}.{}` stores a validity flag",
                    field.line, field.owner, field.name
                ));
            }
        }
    }
    findings
}

/// Runs `scan-api` on the repository at `root`. Returns `(crates, contracts)` checked.
///
/// # Errors
/// One line per finding, or the load failure.
pub fn run(root: &Path) -> Result<(usize, usize), String> {
    let list = scan::load_edges(root)?;
    let text = fs::read_to_string(root.join(CONTRACTS_PATH))
        .map_err(|e| format!("[contracts] cannot read {CONTRACTS_PATH}: {e}"))?;
    let contracts =
        parse_contracts(&text).map_err(|m| format!("[contracts] {CONTRACTS_PATH}: {m}"))?;
    let crates = scan::scanned_crates(root, &list);
    let mut findings = Vec::new();
    for c in &contracts {
        if !crates.contains(&c.crate_name) {
            findings.push(format!(
                "[contract-missing] {}: contract line {} names a crate that is not a scanned crate",
                c.crate_name, c.line
            ));
        }
    }
    for name in &crates {
        let files: Vec<(String, FileItems)> =
            scan::read_crate_files(&root.join("crates").join(name))?
                .into_iter()
                .map(|(path, text)| (path, read_items(&text)))
                .collect();
        findings.extend(check_flags(name, &files));
        findings.extend(check_contracts(name, &files, &contracts));
    }
    if findings.is_empty() {
        Ok((crates.len(), contracts.len()))
    } else {
        Err(findings.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(src: &str) -> Vec<(String, FileItems)> {
        vec![("src/lib.rs".to_string(), read_items(src))]
    }

    const SRC: &str = "pub struct S;\nimpl S {\n    pub fn run<'a>(&self, d: &'a [Item], n: u32) -> Result<Vec<Out>, Diag> { todo() }\n}\npub fn free(x: &mut Vec<A>) {}\nfn private(a: u8) {}\npub(crate) fn limited() {}\n";

    #[test]
    fn reads_signatures_with_receivers_and_visibility() {
        let items = read_items(SRC);
        let run = items.fns.iter().find(|f| f.qualified == "S::run").unwrap();
        assert!(run.public);
        assert_eq!(
            run.signature,
            Signature {
                params: vec!["&self".into(), "&[Item]".into(), "u32".into()],
                result: "Result<Vec<Out>,Diag>".into()
            }
        );
        let by = |n: &str| items.fns.iter().find(|f| f.qualified == n).unwrap();
        assert!(by("free").public && by("free").signature.result.is_empty());
        assert!(!by("private").public && !by("limited").public);
    }

    #[test]
    fn matching_contract_passes_and_mismatch_names_both() {
        let ok = parse_contracts(&format!(
            "{HEADER}\ncontract rt-x S::run (&self, &[Item], u32) -> Result<Vec<Out>, Diag>\ncontract rt-x free (&mut Vec<A>)\n"
        ))
        .unwrap();
        assert!(check_contracts("rt-x", &files(SRC), &ok).is_empty());
        let bad = parse_contracts(&format!(
            "{HEADER}\ncontract rt-x free (&Vec<A>)\ncontract rt-x gone ()\ncontract rt-x private (u8)\n"
        ))
        .unwrap();
        let f = check_contracts("rt-x", &files(SRC), &bad);
        assert_eq!(f.len(), 3, "{f:?}");
        assert!(f[0].contains("contract-mismatch") && f[0].contains("&mut Vec<A>"));
        assert!(f[1].contains("`gone`") && f[2].contains("`private`"));
    }

    #[test]
    fn flag_parameter_function_and_field_fail() {
        let src = "pub fn build(has_errors: bool) {}\npub struct D { pub no_errors: bool }\nimpl D { pub fn has_errors(&self) -> bool { true } pub fn set(&mut self, no_errors: bool) {} pub fn no_errors(x: u8) {} }\n";
        let f = check_flags("rt-x", &files(src));
        assert_eq!(f.len(), 4, "{f:?}");
        assert!(f.iter().any(|l| l.contains("`build`")));
        assert!(f.iter().any(|l| l.contains("D.no_errors")));
        assert!(f.iter().any(|l| l.contains("`D::set`")));
        assert!(f.iter().any(|l| l.contains("`D::no_errors`")));
    }

    #[test]
    fn derived_accessors_are_allowed() {
        let src = "pub struct D { items: Vec<u8> }\nimpl D { pub fn has_errors(&self) -> bool { true } pub fn no_errors(&self) -> bool { false } }\n";
        assert!(check_flags("rt-x", &files(src)).is_empty());
    }

    #[test]
    fn malformed_contracts_are_rejected() {
        assert!(parse_contracts("contract a b ()").is_err());
        assert!(parse_contracts(&format!("{HEADER}\ncontract a b")).is_err());
        assert!(parse_contracts(&format!("{HEADER}\ncontract a b (u8) junk")).is_err());
    }
}
