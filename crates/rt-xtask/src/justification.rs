//! `check-justification`: every external crate in `Cargo.lock` needs a row in
//! `tools/dependencies.v1` with a purpose, the alternatives considered and a
//! maintenance status (Req 21.5, 21.6).

use crate::metadata::Metadata;
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
    /// Crates that the row names as direct users.
    pub used_by: Vec<String>,
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
                "used-by" => {
                    entry.used_by = value
                        .split(',')
                        .map(|n| n.trim().to_string())
                        .filter(|n| !n.is_empty())
                        .collect();
                }
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

/// Shipped executables whose normal-dependency closure a row may not
/// describe as development-only.
pub const SHIPPED_BINARIES: [&str; 2] = ["rt-cli", "rt-ai-tool"];

const NOT_SHIPPED_PHRASES: [&str; 5] = [
    "dev-dependency",
    "dev-only",
    "dev only",
    "never in a shipped binary",
    "never a normal dependency",
];

fn reaches(graph: &BTreeMap<String, BTreeSet<String>>, from: &str, to: &str) -> bool {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut stack = vec![from];
    while let Some(next) = stack.pop() {
        for dep in graph.get(next).into_iter().flatten() {
            if dep == to {
                return true;
            }
            if seen.insert(dep.as_str()) {
                stack.push(dep.as_str());
            }
        }
    }
    false
}

/// Truth check of the rows. `closures` maps each shipped executable to the
/// package names in its normal-dependency closure (`cargo tree -e normal
/// --target all`). A row that says the crate is development-only must not be
/// in any closure, and every crate named in `used-by` must reach it through
/// the resolved graph (the pulling crate of a transitive row counts).
#[must_use]
pub fn check_claims(
    entries: &BTreeMap<String, Entry>,
    md: &Metadata,
    closures: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<String> {
    let Some(all) = &md.resolved else {
        return vec![format!(
            "{DEPENDENCIES_PATH}: cargo metadata has no resolve section; row claims cannot be checked"
        )];
    };
    let mut findings = Vec::new();
    for (name, entry) in entries {
        let text = format!("{} {}", entry.purpose, entry.status).to_lowercase();
        let binaries: Vec<&str> = closures
            .iter()
            .filter(|(_, crates)| crates.contains(name))
            .map(|(binary, _)| binary.as_str())
            .collect();
        if let Some(phrase) = NOT_SHIPPED_PHRASES.iter().find(|p| text.contains(**p)) {
            if !binaries.is_empty() {
                findings.push(format!(
                    "{name}: row says \"{phrase}\" but the crate is in the normal closure of {}",
                    binaries.join(", ")
                ));
            }
        }
        for user in &entry.used_by {
            if !reaches(all, user, name) {
                findings.push(format!(
                    "{name}: row names used-by {user}, which does not depend on it"
                ));
            }
        }
    }
    findings
}

/// Package names listed by `cargo tree --prefix none` output.
#[must_use]
pub fn tree_crates(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

/// `check-justification` on the repository at `root`. `metadata_bytes` is the
/// output of `cargo metadata` and `closures` the normal closures of
/// [`SHIPPED_BINARIES`].
///
/// # Errors
/// One line per finding, or the file that could not be read.
pub fn run(
    root: &Path,
    metadata_bytes: &[u8],
    closures: &BTreeMap<String, BTreeSet<String>>,
) -> Result<usize, String> {
    let read = |rel: &str| {
        let path = root.join(rel);
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
    };
    let dependencies = read(DEPENDENCIES_PATH)?;
    let count = check_texts(&read(LOCK_PATH)?, &dependencies)?;
    let md = crate::metadata::parse_metadata(metadata_bytes)?;
    let findings = check_claims(&parse_entries(&dependencies)?, &md, closures);
    if findings.is_empty() {
        Ok(count)
    } else {
        Err(findings.join("\n"))
    }
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

    fn metadata_with(shipped_dep: &str) -> (Metadata, BTreeMap<String, BTreeSet<String>>) {
        let json = format!(
            "{{\"packages\":[{{\"name\":\"rt-cli\",\"id\":\"a\",\"dependencies\":[]}},{{\"name\":\"{shipped_dep}\",\"id\":\"b\",\"dependencies\":[]}},{{\"name\":\"devonly\",\"id\":\"c\",\"dependencies\":[]}}],\"workspace_members\":[\"a\"],\"resolve\":{{\"nodes\":[{{\"id\":\"a\",\"deps\":[{{\"pkg\":\"b\",\"dep_kinds\":[{{\"kind\":null,\"target\":null}}]}},{{\"pkg\":\"c\",\"dep_kinds\":[{{\"kind\":\"dev\",\"target\":null}}]}}]}},{{\"id\":\"b\",\"deps\":[]}},{{\"id\":\"c\",\"deps\":[]}}]}}}}"
        );
        let closure = tree_crates(&format!("rt-cli v0.1.0 (/x)\n{shipped_dep} v1.0.0\n"));
        let closures = BTreeMap::from([("rt-cli".to_string(), closure)]);
        (
            crate::metadata::parse_metadata(json.as_bytes()).unwrap(),
            closures,
        )
    }

    fn rows(text: &str) -> BTreeMap<String, Entry> {
        parse_entries(&format!("{HEADER}\n{text}\n")).unwrap()
    }

    #[test]
    fn dev_only_claim_on_a_shipped_crate_is_a_finding() {
        let (md, closures) = metadata_with("getrandom");
        let bad = rows("dep getrandom used-by rt-cli purpose \"entropy; dev-dependency closure only\" alternatives \"std\" status \"s\"");
        let findings = check_claims(&bad, &md, &closures);
        assert_eq!(
            findings,
            vec![
                "getrandom: row says \"dev-dependency\" but the crate is in the normal closure of rt-cli"
                    .to_string()
            ]
        );
    }

    #[test]
    fn honest_rows_and_dev_only_crates_pass() {
        let (md, closures) = metadata_with("getrandom");
        let ok = rows(
            "dep getrandom used-by rt-cli purpose \"entropy for ring\" alternatives \"std\" status \"s\"\ndep devonly used-by rt-cli purpose \"test oracle; dev-only\" alternatives \"std\" status \"s\"",
        );
        assert_eq!(check_claims(&ok, &md, &closures), Vec::<String>::new());
    }

    #[test]
    fn used_by_naming_a_non_dependent_is_a_finding() {
        let (md, closures) = metadata_with("getrandom");
        let bad = rows(
            "dep getrandom used-by rt-cli,ghost purpose \"p\" alternatives \"a\" status \"s\"",
        );
        assert_eq!(
            check_claims(&bad, &md, &closures),
            vec!["getrandom: row names used-by ghost, which does not depend on it".to_string()]
        );
    }
}
