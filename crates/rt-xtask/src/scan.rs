//! `scan-sources` (design §3.3 item 3, Req 15.1, 19.7, 20.17, 24.6).
//!
//! A token scan of the source of core crates (and of application crates, whose library files
//! follow the same rules). Test-only items are not scanned. A finding names the crate, the file,
//! the line and the token. Allowlist entries come from `tools/scan-allow.v1`; each needs a reason.

use crate::edges::{self, Class, EdgeList};
use crate::tokens::{production_tokens, Token};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Path of the allowlist, relative to the repository root.
pub const ALLOW_PATH: &str = "tools/scan-allow.v1";
const ALLOW_HEADER: &str = "# format: ruletrace-scan-allow v1";
/// The only crate whose source is scanned for application terms.
pub const ENGINE_CRATE: &str = "rt-engine";

/// Identifiers that are findings on their own, with the name reported.
const FORBIDDEN_IDENTS: &[(&str, &str)] = &[
    ("thread_local", "thread_local!"),
    ("lazy_static", "lazy_static"),
    ("OnceLock", "OnceLock"),
    ("OnceCell", "OnceCell"),
    ("SystemTime", "SystemTime"),
    ("Instant", "Instant"),
    ("rand", "rand"),
    ("f32", "f32"),
    ("f64", "f64"),
    ("unsafe", "unsafe"),
    ("libloading", "libloading"),
    ("HashMap", "HashMap"),
    ("HashSet", "HashSet"),
];
/// Names that follow `std::`.
const FORBIDDEN_STD: &[&str] = &["env", "fs", "net", "process"];
const ENGINE_TERMS: &[&str] = &["prize", "bracket"];

/// Every token name an allowlist entry may use.
fn known_token(token: &str) -> bool {
    token == "static mut"
        || token == "Command"
        || FORBIDDEN_IDENTS.iter().any(|(_, name)| *name == token)
        || FORBIDDEN_STD
            .iter()
            .any(|s| token.strip_prefix("std::") == Some(s))
}

/// One flagged token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    /// Name of the token, as an allowlist entry spells it.
    pub token: String,
    /// 1-based line.
    pub line: usize,
}

/// Scans production tokens. `engine` adds the application-term check (Req 15.1).
#[must_use]
pub fn scan_tokens(tokens: &[Token], engine: bool) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut push = |token: &str, line: usize| {
        hits.push(Hit {
            token: token.to_string(),
            line,
        })
    };
    for (i, t) in tokens.iter().enumerate() {
        let next = |k: usize| tokens.get(i + k);
        if let Some(name) = t.ident() {
            if let Some((_, reported)) = FORBIDDEN_IDENTS.iter().find(|(id, _)| *id == name) {
                push(reported, t.line);
            }
            // `Command` is flagged as a process builder (`process::Command`, `Command::new`), not
            // when it is a name of an enum variant such as `Location::Command`.
            let prev = i.checked_sub(1).and_then(|p| tokens.get(p));
            let before_prev = i.checked_sub(2).and_then(|p| tokens.get(p));
            let after_path = prev.is_some_and(|p| p.is_punct("::"));
            let process_path = after_path && before_prev.is_some_and(|b| b.is_ident("process"));
            let called = !after_path && next(1).is_some_and(|n| n.is_punct("::"));
            if name == "Command" && (process_path || called) {
                push("Command", t.line);
            }
            if name == "static" && next(1).is_some_and(|n| n.is_ident("mut")) {
                push("static mut", t.line);
            }
            if name == "std" && next(1).is_some_and(|n| n.is_punct("::")) {
                match next(2) {
                    Some(n) if n.is_punct("{") => {
                        let end = crate::tokens::skip_group(tokens, i + 2);
                        for g in tokens.get(i + 2..end).unwrap_or(&[]) {
                            if let Some(id) = g.ident().filter(|id| FORBIDDEN_STD.contains(id)) {
                                push(&format!("std::{id}"), g.line);
                            }
                        }
                    }
                    Some(n) => {
                        if let Some(id) = n.ident().filter(|id| FORBIDDEN_STD.contains(id)) {
                            push(&format!("std::{id}"), n.line);
                        }
                    }
                    None => {}
                }
            }
        }
        if engine {
            let text = match &t.kind {
                crate::tokens::Kind::Ident(s) | crate::tokens::Kind::Str(s) => Some(s),
                _ => None,
            };
            if let Some(lower) = text.map(|s| s.to_lowercase()) {
                for term in ENGINE_TERMS {
                    if lower.contains(term) {
                        push(&format!("application term `{term}`"), t.line);
                    }
                }
            }
        }
    }
    hits
}

/// One `allow` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Allow {
    /// Crate name.
    pub crate_name: String,
    /// File relative to the crate directory.
    pub file: String,
    /// Optional line the entry is limited to.
    pub line: Option<usize>,
    /// Token name.
    pub token: String,
    /// Why the use is acceptable.
    pub reason: String,
}

/// Parses `tools/scan-allow.v1`. The first field after `allow` is `<crate>/<file>`, optionally
/// followed by `:<line>`.
///
/// # Errors
/// A message naming the line when the header is missing or an entry is malformed.
pub fn parse_allow(text: &str) -> Result<Vec<Allow>, String> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    match lines.next() {
        Some((_, l)) if l.trim() == ALLOW_HEADER => {}
        _ => return Err(format!("first line must be `{ALLOW_HEADER}`")),
    }
    let mut out = Vec::new();
    for (n, raw) in lines {
        let line = raw.trim();
        if line.starts_with('#') {
            continue;
        }
        let at = n + 1;
        let words: Vec<&str> = line.split_whitespace().collect();
        let [verb, target, rest @ ..] = words.as_slice() else {
            return Err(format!("line {at}: malformed entry"));
        };
        if *verb != "allow" {
            return Err(format!("line {at}: unknown entry `{verb}`"));
        }
        let Some(pos) = rest.iter().position(|w| *w == "reason") else {
            return Err(format!("line {at}: an entry needs `reason <text>`"));
        };
        let token = rest.get(..pos).unwrap_or(&[]).join(" ");
        let reason = rest.get(pos + 1..).unwrap_or(&[]).join(" ");
        if token.is_empty() || reason.is_empty() {
            return Err(format!(
                "line {at}: token and a non-empty reason are required"
            ));
        }
        if !known_token(&token) {
            return Err(format!("line {at}: unknown token `{token}`"));
        }
        let (path, line_no) = match target.rsplit_once(':') {
            Some((p, l)) => (
                p,
                Some(
                    l.parse::<usize>()
                        .map_err(|_| format!("line {at}: bad line number `{l}`"))?,
                ),
            ),
            None => (*target, None),
        };
        let Some((crate_name, file)) = path.split_once('/') else {
            return Err(format!(
                "line {at}: expected `<crate>/<file>`, got `{path}`"
            ));
        };
        out.push(Allow {
            crate_name: crate_name.to_string(),
            file: file.to_string(),
            line: line_no,
            token,
            reason,
        });
    }
    Ok(out)
}

/// Checks that each entry names a crate that may carry one: `rt-cli`, an auxiliary crate, or an
/// application crate in `src/io.rs` or `src/main.rs`.
///
/// # Errors
/// One line per entry that is not allowed.
pub fn validate_allow(allow: &[Allow], list: &EdgeList) -> Result<(), String> {
    let mut bad = Vec::new();
    for a in allow {
        let ok = match list.crates.get(&a.crate_name).map(|c| c.class) {
            Some(Class::Aux) => true,
            Some(Class::Core) => a.crate_name == "rt-cli",
            Some(Class::App) => a.file == "src/io.rs" || a.file == "src/main.rs",
            _ => false,
        };
        if !ok {
            bad.push(format!(
                "[allow-not-permitted] {}/{}: only rt-cli, auxiliary crates and application io.rs or main.rs may be listed",
                a.crate_name, a.file
            ));
        }
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(bad.join("\n"))
    }
}

fn allowed(allow: &[Allow], krate: &str, file: &str, hit: &Hit) -> bool {
    allow.iter().any(|a| {
        a.crate_name == krate
            && a.file == file
            && a.token == hit.token
            && a.line.is_none_or(|l| l == hit.line)
    })
}

fn is_crate_root(file: &str) -> bool {
    file == "src/lib.rs"
        || file == "src/main.rs"
        || file
            .strip_prefix("src/bin/")
            .is_some_and(|rest| rest.ends_with(".rs") && !rest.contains('/'))
}

fn forbids_unsafe(tokens: &[Token]) -> bool {
    tokens.windows(7).any(|w| {
        let p = |k: usize, s: &str| w.get(k).is_some_and(|t| t.is_punct(s));
        let id = |k: usize, s: &str| w.get(k).is_some_and(|t| t.is_ident(s));
        p(0, "#") && p(1, "!") && p(2, "[") && id(3, "forbid") && p(4, "(") && id(5, "unsafe_code")
    })
}

/// Scans the files of one crate. `files` are `(path relative to the crate directory, text)`.
/// Returns one line per finding.
#[must_use]
pub fn scan_crate(krate: &str, files: &[(String, String)], allow: &[Allow]) -> Vec<String> {
    let parsed: Vec<(&String, Vec<Token>, BTreeSet<String>)> = files
        .iter()
        .map(|(path, text)| {
            let (tokens, mods) = production_tokens(text);
            (path, tokens, mods)
        })
        .collect();
    let test_mods: BTreeSet<&String> = parsed.iter().flat_map(|(_, _, m)| m.iter()).collect();
    let mut findings = Vec::new();
    let mut roots = 0usize;
    for (path, tokens, _) in &parsed {
        let stem = Path::new(path.as_str())
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if test_mods.iter().any(|m| m.as_str() == stem) {
            continue;
        }
        if is_crate_root(path) {
            roots += 1;
            // The attribute is checked on the raw file, because it is not test code.
            let raw = crate::tokens::tokenize(
                files
                    .iter()
                    .find(|(p, _)| p == *path)
                    .map_or("", |(_, t)| t.as_str()),
            );
            if !forbids_unsafe(&raw) {
                findings.push(format!(
                    "[forbid-unsafe-code] {krate}: {path} lacks #![forbid(unsafe_code)] (Req 19.7)"
                ));
            }
        }
        for hit in scan_tokens(tokens, krate == ENGINE_CRATE) {
            if !allowed(allow, krate, path, &hit) {
                findings.push(format!(
                    "[forbidden-token] {krate}: {path}:{}: {}",
                    hit.line, hit.token
                ));
            }
        }
    }
    if roots == 0 {
        findings.push(format!(
            "[forbid-unsafe-code] {krate}: no crate root (src/lib.rs or src/main.rs) to carry #![forbid(unsafe_code)]"
        ));
    }
    findings
}

/// Reads every `.rs` file under `dir/src`, sorted, with paths relative to `dir`.
///
/// # Errors
/// A message when a directory or file cannot be read.
pub fn read_crate_files(dir: &Path) -> Result<Vec<(String, String)>, String> {
    fn walk(base: &Path, rel: &str, out: &mut Vec<(String, String)>) -> Result<(), String> {
        let here = base.join(rel);
        let mut names: Vec<_> = fs::read_dir(&here)
            .map_err(|e| format!("cannot read {}: {e}", here.display()))?
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for name in names {
            let child = format!("{rel}/{name}");
            let path = base.join(&child);
            if path.is_dir() {
                walk(base, &child, out)?;
            } else if name.ends_with(".rs") {
                let text = fs::read_to_string(&path)
                    .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
                out.push((child, text));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, "src", &mut out)?;
    Ok(out)
}

/// Names of the crates whose source the scans read: classes core and app that exist on disk.
#[must_use]
pub fn scanned_crates(root: &Path, list: &EdgeList) -> Vec<String> {
    list.crates
        .iter()
        .filter(|(_, e)| matches!(e.class, Class::Core | Class::App))
        .map(|(n, _)| n.clone())
        .filter(|n| root.join("crates").join(n).join("src").is_dir())
        .collect()
}

/// Loads the edge list as a message.
///
/// # Errors
/// The failure text of a missing or malformed list.
pub fn load_edges(root: &Path) -> Result<EdgeList, String> {
    edges::load(&root.join(edges::EDGES_PATH)).map_err(|e| match e {
        edges::EdgeListError::Missing(m) | edges::EdgeListError::Malformed(m) => {
            format!("[edge-list] {m}")
        }
    })
}

/// Runs `scan-sources` on the repository at `root`. Returns `(crates, files)` scanned.
///
/// # Errors
/// One line per finding, or the load failure.
pub fn run(root: &Path) -> Result<(usize, usize), String> {
    let list = load_edges(root)?;
    let text = fs::read_to_string(root.join(ALLOW_PATH))
        .map_err(|e| format!("[allow-list] cannot read {ALLOW_PATH}: {e}"))?;
    let allow = parse_allow(&text).map_err(|m| format!("[allow-list] {ALLOW_PATH}: {m}"))?;
    validate_allow(&allow, &list)?;
    let crates = scanned_crates(root, &list);
    let mut findings = Vec::new();
    let mut file_count = 0usize;
    for name in &crates {
        let files = read_crate_files(&root.join("crates").join(name))?;
        file_count += files.len();
        findings.extend(scan_crate(name, &files, &allow));
    }
    if findings.is_empty() {
        Ok((crates.len(), file_count))
    } else {
        Err(findings.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "#![forbid(unsafe_code)]\n";

    fn scan(krate: &str, body: &str) -> Vec<String> {
        scan_with(krate, body, &[])
    }

    fn scan_with(krate: &str, body: &str, allow: &[Allow]) -> Vec<String> {
        let files = vec![("src/lib.rs".to_string(), format!("{ROOT}{body}"))];
        scan_crate(krate, &files, allow)
    }

    fn fails_naming(body: &str, token: &str) {
        let f = scan("rt-json", body);
        assert!(
            f.iter()
                .any(|l| l.contains("rt-json") && l.contains("src/lib.rs:2") && l.ends_with(token)),
            "{token}: {f:?}"
        );
    }

    #[test]
    fn static_mut_fails() {
        fails_naming("static mut N: u8 = 0;", "static mut");
    }

    #[test]
    fn thread_local_fails() {
        fails_naming("thread_local! { static X: u8 = 0; }", "thread_local!");
    }

    #[test]
    fn std_env_var_fails() {
        fails_naming("fn f() { let _ = std::env::var(\"X\"); }", "std::env");
        fails_naming("use std::{fs, io};", "std::fs");
    }

    #[test]
    fn float_fails() {
        fails_naming("pub fn f(x: f64) {}", "f64");
        fails_naming("const X: f32 = 1.0f32;", "f32");
    }

    #[test]
    fn hashmap_fails() {
        fails_naming("use std::collections::HashMap;", "HashMap");
    }

    #[test]
    fn command_is_flagged_as_a_process_builder_only() {
        fails_naming("fn f() { let _ = Command::new(\"x\"); }", "Command");
        fails_naming(
            "fn f() { let _ = std::process::Command::new(\"x\"); }",
            "Command",
        );
        assert!(scan(
            "rt-json",
            "enum L { Command(String) }\nfn f() { let _ = L::Command(String::new()); }"
        )
        .is_empty());
    }

    #[test]
    fn clean_crate_passes_and_test_code_is_ignored() {
        let body = "use std::collections::BTreeMap;\n// f64 in a comment\n#[cfg(test)]\nmod tests { use std::collections::HashMap; }\n";
        assert!(scan("rt-json", body).is_empty());
    }

    #[test]
    fn missing_forbid_unsafe_code_fails() {
        let files = vec![("src/lib.rs".to_string(), "fn f() {}".to_string())];
        let f = scan_crate("rt-hash", &files, &[]);
        assert_eq!(f.len(), 1);
        assert!(f[0].contains("rt-hash") && f[0].contains("src/lib.rs"));
    }

    #[test]
    fn engine_terms_fail_only_in_the_engine() {
        let body = "pub struct PrizeRule; const S: &str = \"Bracket\";";
        let f = scan("rt-engine", body);
        assert!(f.iter().any(|l| l.ends_with("application term `prize`")));
        assert!(f.iter().any(|l| l.ends_with("application term `bracket`")));
        assert!(scan("rt-data", body).is_empty());
    }

    #[test]
    fn allowlist_entry_with_reason_is_honoured() {
        let allow = parse_allow(&format!(
            "{ALLOW_HEADER}\nallow rt-cli/src/lib.rs std::fs reason reads input files\n"
        ))
        .unwrap();
        assert!(scan_with("rt-cli", "use std::fs;", &allow).is_empty());
        assert!(!scan_with("rt-cli", "use std::env;", &allow).is_empty());
        let limited = parse_allow(&format!(
            "{ALLOW_HEADER}\nallow rt-cli/src/lib.rs:9 std::fs reason only here\n"
        ))
        .unwrap();
        assert!(!scan_with("rt-cli", "use std::fs;", &limited).is_empty());
    }

    #[test]
    fn allowlist_needs_header_reason_and_known_token() {
        assert!(parse_allow("allow rt-cli/src/a.rs std::fs reason x").is_err());
        let h = |l: &str| parse_allow(&format!("{ALLOW_HEADER}\n{l}\n"));
        assert!(h("allow rt-cli/src/a.rs std::fs").is_err());
        assert!(h("allow rt-cli/src/a.rs std::fs reason").is_err());
        assert!(h("allow rt-cli/src/a.rs std::bogus reason r").is_err());
        assert!(h("allow rt-cli/src/a.rs static mut reason r").is_ok());
    }

    #[test]
    fn allowlist_is_limited_to_io_crates() {
        let list = edges::parse(include_str!("../../../tools/allowed-edges.v1")).unwrap();
        let ok = parse_allow(&format!(
            "{ALLOW_HEADER}\nallow rt-cli/src/io.rs std::fs reason r\nallow prize-check/src/io.rs std::fs reason r\n"
        ))
        .unwrap();
        assert!(validate_allow(&ok, &list).is_ok());
        let bad = parse_allow(&format!(
            "{ALLOW_HEADER}\nallow rt-engine/src/lib.rs std::fs reason r\nallow prize-check/src/lib.rs std::fs reason r\n"
        ))
        .unwrap();
        let e = validate_allow(&bad, &list).unwrap_err();
        assert!(e.contains("rt-engine") && e.contains("prize-check"));
    }
}
