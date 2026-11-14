//! `check-justification`: every external crate in `Cargo.lock` needs a row in
//! `tools/dependencies.v1` with a purpose, the alternatives considered and a
//! maintenance status (Req 21.5, 21.6).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Justification list, relative to the repository root.
pub const DEPENDENCIES_PATH: &str = "tools/dependencies.v1";
/// Lock file, relative to the repository root.
pub const LOCK_PATH: &str = "Cargo.lock";
const HEADER: &str = "# format: ruletrace-dependencies v1";

/// One justification row. Empty text means the field was absent or blank.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// Purpose text.
    pub purpose: String,
    /// Alternatives considered, including the standard library.
    pub alternatives: String,
    /// Maintenance status.
    pub status: String,
}

fn tokens(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut rest = line.trim_start();
    while !rest.is_empty() {
        if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted.find('"').ok_or("unterminated quoted text")?;
            out.push(quoted.get(..end).unwrap_or_default().to_string());
            rest = quoted.get(end + 1..).unwrap_or_default().trim_start();
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            out.push(rest.get(..end).unwrap_or_default().to_string());
            rest = rest.get(end..).unwrap_or_default().trim_start();
        }
    }
    Ok(out)
}

/// Parses `tools/dependencies.v1`. Fields that are absent or blank are kept as
/// empty text so that [`check`] can name them.
///
/// # Errors
/// A missing header, a malformed line, an unknown key or a crate listed twice.
pub fn parse_entries(text: &str) -> Result<BTreeMap<String, Entry>, String> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some(HEADER) {
        return Err(format!(
            "{DEPENDENCIES_PATH}: first line must be `{HEADER}`"
        ));
    }
    let mut map = BTreeMap::new();
    for (n, line) in lines.enumerate() {
        let at = n + 2;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let toks = tokens(line).map_err(|e| format!("{DEPENDENCIES_PATH}:{at}: {e}"))?;
        let mut it = toks.into_iter();
        if it.next().as_deref() != Some("dep") {
            return Err(format!(
                "{DEPENDENCIES_PATH}:{at}: line must start with `dep`"
            ));
        }
        let name = it
            .next()
            .ok_or_else(|| format!("{DEPENDENCIES_PATH}:{at}: missing crate name"))?;
        let mut entry = Entry::default();
        while let Some(key) = it.next() {
            let value = it.next().unwrap_or_default();
            match key.as_str() {
                "used-by" => {}
                "purpose" => entry.purpose = value.trim().to_string(),
                "alternatives" => entry.alternatives = value.trim().to_string(),
                "status" => entry.status = value.trim().to_string(),
                other => {
                    return Err(format!("{DEPENDENCIES_PATH}:{at}: unknown key `{other}`"));
                }
            }
        }
        if map.insert(name.clone(), entry).is_some() {
            return Err(format!(
                "{DEPENDENCIES_PATH}:{at}: crate `{name}` listed twice"
            ));
        }
    }
    Ok(map)
}

/// Names of the external (sourced) packages in `Cargo.lock` text.
#[must_use]
pub fn external_crates(lock: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut name: Option<String> = None;
    let mut external = false;
    let mut flush = |name: &mut Option<String>, external: &mut bool| {
        if let Some(n) = name.take() {
            if *external {
                out.insert(n);
            }
        }
        *external = false;
    };
    for line in lock.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            flush(&mut name, &mut external);
        } else if let Some(v) = line.strip_prefix("name = ") {
            name = Some(v.trim_matches('"').to_string());
        } else if line.starts_with("source = ") {
            external = true;
        }
    }
    flush(&mut name, &mut external);
    out
}

/// Pure part of the check. Returns the number of external crates checked.
///
/// # Errors
/// One line per crate that has no row or a row without purpose, alternatives or
/// status.
pub fn check_texts(lock: &str, dependencies: &str) -> Result<usize, String> {
    let entries = parse_entries(dependencies)?;
    let crates = external_crates(lock);
    let mut findings = Vec::new();
    for name in &crates {
        match entries.get(name) {
            None => findings.push(format!(
                "{name}: external crate in {LOCK_PATH} has no row in {DEPENDENCIES_PATH}"
            )),
            Some(e) => {
                for (field, value) in [
                    ("purpose", &e.purpose),
                    ("alternatives", &e.alternatives),
                    ("status", &e.status),
                ] {
                    if value.is_empty() {
                        findings.push(format!("{name}: row in {DEPENDENCIES_PATH} lacks {field}"));
                    }
                }
            }
        }
    }
    if findings.is_empty() {
        Ok(crates.len())
    } else {
        Err(findings.join("\n"))
    }
}

/// `check-justification` on the repository at `root`.
///
/// # Errors
/// One line per finding, or the file that could not be read.
pub fn run(root: &Path) -> Result<usize, String> {
    let read = |rel: &str| {
        let path = root.join(rel);
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
    };
    check_texts(&read(LOCK_PATH)?, &read(DEPENDENCIES_PATH)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK: &str = "version = 4\n\n[[package]]\nname = \"rt-a\"\nversion = \"0.1.0\"\n\n[[package]]\nname = \"foo\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\nname = \"foo\"\nversion = \"2.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n";
    const GOOD: &str = "# format: ruletrace-dependencies v1\ndep foo used-by rt-a purpose \"p\" alternatives \"a, std\" status \"s\"\n";

    #[test]
    fn workspace_crates_need_no_row_and_versions_share_one() {
        assert_eq!(check_texts(LOCK, GOOD), Ok(1));
    }

    #[test]
    fn crate_without_a_row_is_named() {
        let err = check_texts(LOCK, "# format: ruletrace-dependencies v1\n").unwrap_err();
        assert!(err.starts_with("foo: external crate"), "{err}");
    }

    #[test]
    fn each_missing_field_is_named() {
        for field in ["purpose", "alternatives", "status"] {
            let dropped = match field {
                "purpose" => GOOD.replace("purpose \"p\" ", ""),
                "alternatives" => GOOD.replace("alternatives \"a, std\" ", ""),
                _ => GOOD.replace(" status \"s\"", ""),
            };
            let err = check_texts(LOCK, &dropped).unwrap_err();
            assert!(err.contains(&format!("lacks {field}")), "{field}: {err}");
        }
        let blank = GOOD.replace("status \"s\"", "status \"  \"");
        assert!(check_texts(LOCK, &blank)
            .unwrap_err()
            .contains("lacks status"));
    }

    #[test]
    fn malformed_lists_are_rejected() {
        assert!(parse_entries("dep foo").is_err());
        assert!(parse_entries(&format!("{HEADER}\nwhat")).is_err());
        assert!(parse_entries(&format!("{HEADER}\ndep foo purpose \"x")).is_err());
        assert!(parse_entries(&format!("{HEADER}\ndep foo bogus \"x\"")).is_err());
        assert!(parse_entries(&format!("{HEADER}\ndep foo\ndep foo")).is_err());
    }
}
