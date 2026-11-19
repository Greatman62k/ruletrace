//! The `app-api` rule of `check-app-layers` (design §8.10, Req 31.10).
//!
//! The token rules of `app_layers` cannot recognise a call to a public
//! function that computes a decision. This rule instead fixes the public
//! surface: an application layer may use only the external items listed in
//! `tools/app-api-allow.v1`, each with a reason. An external item is any path
//! into another workspace crate. Three forms are read:
//!
//! - `use` declarations, expanded to one path per leaf: simple, grouped
//!   (`use a::{b, c::d}`, nested, with `self`), renamed (`use a::b as c`,
//!   checked as `a::b`) and `extern crate`;
//! - fully qualified paths in expressions and types, such as
//!   `rt_engine::evaluate(..)`, which also reach macro bodies because the
//!   scan reads tokens;
//! - glob imports, which always fail and cannot be listed.
//!
//! Paths that start with `self`, `super`, `crate` or the crate's own name are
//! internal and ignored. `std`, `core` and `alloc` are not covered here (the
//! `app-io` rule covers std I/O).
//!
//! # File format
//!
//! ```text
//! # format: ruletrace-app-api v1
//! allow <crate> <path> -- <reason>
//! ```
//!
//! The path uses the Rust crate identifier (`rt_data::Question`). A `use`
//! leaf must equal an entry exactly. A qualified path in an expression or
//! type matches an entry when it equals it or extends it by whole segments,
//! because associated items (`Type::new`) cannot be told apart from modules
//! by tokens. Entries therefore name concrete items, never modules or
//! crates. An entry that no source uses is stale and fails, so the list stays
//! exact. An entry for a crate that does not exist, or that is not among the
//! checked crates, fails.
//!
//! # Reuse by task 13.4
//!
//! [`check_api`] and [`run_api`] take the crate set and the allow-list path
//! as parameters. Task 13.4 adds an `rt-lab` section to the file. Req 24.13
//! forbids the Laboratory to call an engine evaluation function, or an
//! application function that calls one. That ban is expressed by not listing
//! `prize_check::evaluate`, `bracket_check::evaluate` and
//! `rt_engine::evaluate` for `rt-lab`.
//!
//! # Limits
//!
//! The scan is token based. A path produced by a macro expansion, a module
//! imported by `use` and then reached through a short path (`report::f`), a
//! crate alias used through its alias, and a path split by a macro cannot be
//! seen. Reviewers read the list to see which modules are imported.

use crate::tokens::{production_tokens, Token};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// Rule name printed in front of a finding.
pub const RULE: &str = "app-api";
/// Path of the allow-list, relative to the repository root.
pub const LIST_PATH: &str = "tools/app-api-allow.v1";
const HEADER: &str = "# format: ruletrace-app-api v1";

/// One line of the allow-list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllowEntry {
    /// Crate name as in `tools/allowed-edges.v1`.
    pub krate: String,
    /// Item path with Rust crate identifiers.
    pub path: String,
    /// Why the layer needs the item.
    pub reason: String,
    /// 1-based line in the list.
    pub line: usize,
}

/// Parses the allow-list.
///
/// # Errors
/// One finding per malformed line.
pub fn parse_allow(text: &str, list_path: &str) -> Result<Vec<AllowEntry>, Vec<String>> {
    let mut errors = Vec::new();
    let mut entries: Vec<AllowEntry> = Vec::new();
    let mut lines = text.lines().enumerate();
    if lines.next().map(|(_, l)| l.trim_end()) != Some(HEADER) {
        errors.push(format!(
            "[{RULE}] {list_path}:1: first line must be `{HEADER}`"
        ));
    }
    for (index, raw) in lines {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let bad = |why: &str| format!("[{RULE}] {list_path}:{line}: {why}");
        let Some((head, reason)) = trimmed.split_once(" -- ") else {
            errors.push(bad(
                "malformed entry: expected `allow <crate> <path> -- <reason>`; the reason is missing",
            ));
            continue;
        };
        let parts: Vec<&str> = head.split_whitespace().collect();
        let [keyword, krate, path] = parts.as_slice() else {
            errors.push(bad(
                "malformed entry: expected `allow <crate> <path> -- <reason>`",
            ));
            continue;
        };
        if *keyword != "allow" {
            errors.push(bad("malformed entry: line must start with `allow`"));
            continue;
        }
        if reason.trim().is_empty() {
            errors.push(bad("malformed entry: the reason is empty"));
            continue;
        }
        let segments: Vec<&str> = path.split("::").collect();
        let valid = segments.len() >= 2
            && segments
                .iter()
                .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        if !valid {
            errors.push(bad(
                "malformed path: expected `crate_ident::item` with identifier segments, no glob and no braces",
            ));
            continue;
        }
        if entries.iter().any(|e| e.krate == *krate && e.path == *path) {
            errors.push(bad(&format!("duplicate entry `{path}` for {krate}")));
            continue;
        }
        entries.push(AllowEntry {
            krate: (*krate).to_string(),
            path: (*path).to_string(),
            reason: reason.trim().to_string(),
            line,
        });
    }
    if errors.is_empty() {
        Ok(entries)
    } else {
        Err(errors)
    }
}

/// One use of an external item found in a file.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Found {
    path: String,
    line: usize,
    glob: bool,
    from_use: bool,
}

fn ident_at(tokens: &[Token], i: usize) -> Option<&str> {
    tokens.get(i).and_then(Token::ident)
}

fn punct_at(tokens: &[Token], i: usize, p: &str) -> bool {
    tokens.get(i).is_some_and(|t| t.is_punct(p))
}

/// Parses one use tree starting at `i`. Returns the index after the tree.
fn use_tree(tokens: &[Token], mut i: usize, prefix: &[String], out: &mut Vec<Found>) -> usize {
    let mut path = prefix.to_vec();
    if punct_at(tokens, i, "::") {
        i += 1;
    }
    loop {
        if let Some(name) = ident_at(tokens, i) {
            let is_group_self = name == "self" && !prefix.is_empty() && path.len() == prefix.len();
            if !is_group_self {
                path.push(name.to_string());
            }
            let line = tokens.get(i).map_or(0, |t| t.line);
            i += 1;
            if punct_at(tokens, i, "::") {
                i += 1;
                continue;
            }
            if ident_at(tokens, i) == Some("as") {
                i += 2;
            }
            out.push(Found {
                path: path.join("::"),
                line,
                glob: false,
                from_use: true,
            });
            return i;
        }
        if punct_at(tokens, i, "{") {
            i += 1;
            while i < tokens.len() && !punct_at(tokens, i, "}") {
                let next = use_tree(tokens, i, &path, out);
                i = if next > i { next } else { i + 1 };
                if punct_at(tokens, i, ",") {
                    i += 1;
                }
            }
            return i + 1;
        }
        if punct_at(tokens, i, "*") {
            out.push(Found {
                path: path.join("::"),
                line: tokens.get(i).map_or(0, |t| t.line),
                glob: true,
                from_use: true,
            });
            return i + 1;
        }
        return i;
    }
}

fn idents(names: &BTreeSet<String>) -> BTreeSet<String> {
    names.iter().map(|n| n.replace('-', "_")).collect()
}

/// Finds every path into another workspace crate in one token stream.
fn external_paths(tokens: &[Token], external: &BTreeSet<String>) -> Vec<Found> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let Some(name) = ident_at(tokens, i) else {
            i += 1;
            continue;
        };
        if name == "use" {
            let mut leaves = Vec::new();
            let end = use_tree(tokens, i + 1, &[], &mut leaves);
            found.extend(leaves.into_iter().filter(|f| {
                f.path
                    .split("::")
                    .next()
                    .is_some_and(|c| external.contains(c))
            }));
            i = end.max(i + 1);
            continue;
        }
        if name == "extern" && ident_at(tokens, i + 1) == Some("crate") {
            if let Some(krate) = ident_at(tokens, i + 2).filter(|c| external.contains(*c)) {
                found.push(Found {
                    path: krate.to_string(),
                    line: tokens.get(i + 2).map_or(0, |t| t.line),
                    glob: false,
                    from_use: true,
                });
            }
            i += 3;
            continue;
        }
        // A `::` continues a path only after a segment; otherwise it is the
        // leading `::` of a fully qualified crate path.
        let continues_segment = i >= 2
            && tokens.get(i - 2).is_some_and(|t| {
                t.ident().is_some() || t.is_punct(">") || t.is_punct(")") || t.is_punct("]")
            });
        let after_path_or_field = i > 0
            && tokens
                .get(i - 1)
                .is_some_and(|t| (t.is_punct("::") && continues_segment) || t.is_punct("."));
        if external.contains(name) && !after_path_or_field {
            let mut segments = vec![name.to_string()];
            let mut j = i;
            while punct_at(tokens, j + 1, "::") {
                match ident_at(tokens, j + 2) {
                    Some(next) => {
                        segments.push(next.to_string());
                        j += 2;
                    }
                    None => break,
                }
            }
            if segments.len() >= 2 {
                found.push(Found {
                    path: segments.join("::"),
                    line: tokens.get(j).map_or(0, |t| t.line),
                    glob: false,
                    from_use: false,
                });
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    found
}

fn matches_entry(found: &Found, entry: &str) -> bool {
    if found.path == entry {
        return true;
    }
    !found.from_use
        && found
            .path
            .strip_prefix(entry)
            .is_some_and(|rest| rest.starts_with("::"))
}

/// Scans the production files of the crates in `crates` and checks the
/// allow-list `entries`. `workspace` holds the names of every workspace
/// crate. Returns one line per finding.
#[must_use]
pub fn check_api(
    crates: &BTreeMap<String, Vec<(String, String)>>,
    workspace: &BTreeSet<String>,
    entries: &[AllowEntry],
    list_path: &str,
) -> Vec<String> {
    let mut findings = Vec::new();
    let mut used: BTreeSet<usize> = BTreeSet::new();
    for entry in entries {
        if !workspace.contains(&entry.krate) {
            findings.push(format!(
                "[{RULE}] {}: {list_path}:{}: crate does not exist in the workspace",
                entry.krate, entry.line
            ));
        } else if !crates.contains_key(&entry.krate) {
            findings.push(format!(
                "[{RULE}] {}: {list_path}:{}: crate is not a checked application layer",
                entry.krate, entry.line
            ));
        }
    }
    for (krate, files) in crates {
        let external: BTreeSet<String> = idents(workspace)
            .into_iter()
            .filter(|c| *c != krate.replace('-', "_"))
            .collect();
        let parsed: Vec<(&String, Vec<Token>, BTreeSet<String>)> = files
            .iter()
            .map(|(path, text)| {
                let (tokens, mods) = production_tokens(text);
                (path, tokens, mods)
            })
            .collect();
        let test_mods: BTreeSet<&String> = parsed.iter().flat_map(|(_, _, m)| m.iter()).collect();
        for (path, tokens, _) in &parsed {
            let stem = Path::new(path.as_str())
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if test_mods.iter().any(|m| m.as_str() == stem) {
                continue;
            }
            let mut found = external_paths(tokens, &external);
            found.sort_by(|a, b| (a.line, &a.path).cmp(&(b.line, &b.path)));
            found.dedup();
            for item in found {
                if item.glob {
                    findings.push(format!(
                        "[{RULE}] {krate}: {path}:{}: glob import of `{}::*` is not allowed",
                        item.line, item.path
                    ));
                    continue;
                }
                let mut allowed = false;
                for (n, entry) in entries.iter().enumerate() {
                    if entry.krate == *krate && matches_entry(&item, &entry.path) {
                        used.insert(n);
                        allowed = true;
                    }
                }
                if !allowed {
                    findings.push(format!(
                        "[{RULE}] {krate}: {path}:{}: external item `{}` is not listed in {list_path}",
                        item.line, item.path
                    ));
                }
            }
        }
    }
    for (n, entry) in entries.iter().enumerate() {
        if !used.contains(&n)
            && workspace.contains(&entry.krate)
            && crates.contains_key(&entry.krate)
        {
            findings.push(format!(
                "[{RULE}] {}: {list_path}:{}: stale entry `{}`, no source uses it",
                entry.krate, entry.line, entry.path
            ));
        }
    }
    findings
}

/// Result of the `app-api` rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiScanned {
    /// Crates scanned.
    pub crates: usize,
    /// Allow-list entries, all used.
    pub entries: usize,
}

/// Runs the rule on the repository at `root` for `crate_names`, with the
/// allow-list at `list_path` relative to `root`.
///
/// # Errors
/// One line per finding, or the load failure.
pub fn run_api(
    root: &Path,
    list_path: &str,
    crate_names: &[String],
    workspace: &BTreeSet<String>,
) -> Result<ApiScanned, String> {
    let text = fs::read_to_string(root.join(list_path))
        .map_err(|e| format!("[{RULE}] cannot read {list_path}: {e}"))?;
    let entries = parse_allow(&text, list_path).map_err(|errors| errors.join("\n"))?;
    let mut crates = BTreeMap::new();
    for name in crate_names {
        let dir = root.join("crates").join(name);
        let mut files = crate::scan::read_crate_files(&dir)?;
        if let Ok(build) = fs::read_to_string(dir.join("build.rs")) {
            files.push(("build.rs".to_string(), build));
        }
        crates.insert(name.clone(), files);
    }
    let findings = check_api(&crates, workspace, &entries, list_path);
    if findings.is_empty() {
        Ok(ApiScanned {
            crates: crates.len(),
            entries: entries.len(),
        })
    } else {
        Err(findings.join("\n"))
    }
}

#[cfg(test)]
#[path = "app_api_tests.rs"]
mod tests;
