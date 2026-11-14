//! Reader for `tools/allowed-edges.v1` (design §3.3).
//!
//! The file is parsed strictly: an unknown line, a missing field, a duplicate crate or a
//! dependency on a crate that has no `crate` line makes the whole list malformed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Path of the list, relative to the repository root.
pub const EDGES_PATH: &str = "tools/allowed-edges.v1";
const HEADER: &str = "# format: ruletrace-allowed-edges v1";

/// Crate class (design §3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// Core crate.
    Core,
    /// Auxiliary (AI) crate.
    Aux,
    /// Application layer.
    App,
    /// Development tooling.
    Tool,
    /// Test-only crate.
    Test,
}

impl Class {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "core" => Some(Self::Core),
            "aux" => Some(Self::Aux),
            "app" => Some(Self::App),
            "tool" => Some(Self::Tool),
            "test" => Some(Self::Test),
            _ => None,
        }
    }

    /// The word used in the list.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Aux => "aux",
            Self::App => "app",
            Self::Tool => "tool",
            Self::Test => "test",
        }
    }
}

/// One `crate` line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrateEntry {
    /// Class of the crate.
    pub class: Class,
    /// Allowed internal dependencies.
    pub deps: BTreeSet<String>,
}

/// A `forbid-*` rule with name patterns (a trailing `*` matches any suffix).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameRule {
    /// Dependent crate patterns.
    pub from: Vec<String>,
    /// Dependency crate patterns.
    pub to: Vec<String>,
}

/// The `forbid-external` line.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ForbidExternal {
    /// Network and AI crate names.
    pub names: BTreeSet<String>,
    /// Crates under which the names are allowed.
    pub except_under: BTreeSet<String>,
}

/// The parsed list.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct EdgeList {
    /// Crates by name.
    pub crates: BTreeMap<String, CrateEntry>,
    /// Pipeline stage numbers by crate name.
    pub stages: BTreeMap<String, u32>,
    /// `forbid-class` pairs.
    pub forbid_class: Vec<(Class, Class)>,
    /// `forbid-transitive` rules.
    pub forbid_transitive: Vec<NameRule>,
    /// `forbid-name` rules.
    pub forbid_name: Vec<NameRule>,
    /// `forbid-external` entry.
    pub forbid_external: ForbidExternal,
    /// `external <crate> allowed-in <crates>` entries.
    pub external: BTreeMap<String, BTreeSet<String>>,
}

/// Whether `name` matches `pattern` (exact, or prefix when the pattern ends in `*`).
#[must_use]
pub fn matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

/// Whether `name` matches any of the patterns.
#[must_use]
pub fn matches_any(patterns: &[String], name: &str) -> bool {
    patterns.iter().any(|p| matches(p, name))
}

impl EdgeList {
    /// Names of the crates of one class, in order.
    pub fn of_class(&self, class: Class) -> impl Iterator<Item = &String> {
        self.crates
            .iter()
            .filter(move |(_, e)| e.class == class)
            .map(|(n, _)| n)
    }
}

/// Reads and parses the list at `path`. Errors say whether the file is missing or malformed.
///
/// # Errors
/// A message naming the file when it cannot be read or does not parse.
pub fn load(path: &Path) -> Result<EdgeList, EdgeListError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| EdgeListError::Missing(format!("cannot read '{}': {e}", path.display())))?;
    parse(&text).map_err(|m| EdgeListError::Malformed(format!("'{}': {m}", path.display())))
}

/// Why the list could not be used (its own failure, Req 24.9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EdgeListError {
    /// The file is missing or unreadable.
    Missing(String),
    /// The file does not follow the format.
    Malformed(String),
}

fn arrow_split<'a>(rest: &'a [&'a str]) -> Option<(&'a [&'a str], &'a [&'a str])> {
    let at = rest.iter().position(|t| *t == "->")?;
    Some((rest.get(..at)?, rest.get(at + 1..)?))
}

fn owned(tokens: &[&str]) -> Vec<String> {
    tokens.iter().map(|t| (*t).to_string()).collect()
}

/// Parses the text of the list.
///
/// # Errors
/// A message with the line number of the first problem.
pub fn parse(text: &str) -> Result<EdgeList, String> {
    if text.lines().next().map(str::trim) != Some(HEADER) {
        return Err(format!("first line must be '{HEADER}'"));
    }
    let mut list = EdgeList::default();
    for (index, raw) in text.lines().enumerate() {
        let n = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let err = |what: &str| format!("line {n}: {what}");
        let (head, rest) = tokens.split_first().ok_or_else(|| err("empty"))?;
        match *head {
            "crate" => {
                let [name, "class", class, "deps", deps @ ..] = rest else {
                    return Err(err("expected 'crate <name> class <class> deps <names|->'"));
                };
                let class = Class::parse(class).ok_or_else(|| err("unknown class"))?;
                let deps: BTreeSet<String> = if deps == ["-"] {
                    BTreeSet::new()
                } else if deps.is_empty() || deps.contains(&"-") {
                    return Err(err("deps must be '-' or a list of crate names"));
                } else {
                    deps.iter().map(|d| (*d).to_string()).collect()
                };
                let entry = CrateEntry { class, deps };
                if list.crates.insert((*name).to_string(), entry).is_some() {
                    return Err(err(&format!("duplicate crate '{name}'")));
                }
            }
            "stage" => {
                let [name, number] = rest else {
                    return Err(err("expected 'stage <name> <number>'"));
                };
                let number: u32 = number.parse().map_err(|_| err("stage is not a number"))?;
                list.stages.insert((*name).to_string(), number);
            }
            "forbid-class" => {
                let Some(([from], [to])) = arrow_split(rest) else {
                    return Err(err("expected 'forbid-class <class> -> <class>'"));
                };
                let from = Class::parse(from).ok_or_else(|| err("unknown class"))?;
                let to = Class::parse(to).ok_or_else(|| err("unknown class"))?;
                list.forbid_class.push((from, to));
            }
            "forbid-transitive" | "forbid-name" => {
                let Some((from, to)) = arrow_split(rest) else {
                    return Err(err("expected '<from...> -> <to...>'"));
                };
                if from.is_empty() || to.is_empty() {
                    return Err(err("both sides of '->' need at least one name"));
                }
                let rule = NameRule {
                    from: owned(from),
                    to: owned(to),
                };
                if *head == "forbid-name" {
                    list.forbid_name.push(rule);
                } else {
                    list.forbid_transitive.push(rule);
                }
            }
            "forbid-external" => {
                let split = rest.iter().position(|t| *t == "except-under");
                let (names, except) = match split {
                    Some(at) => (rest.get(..at), rest.get(at + 1..)),
                    None => (Some(rest), Some(&[][..])),
                };
                let names = names.unwrap_or_default();
                if names.is_empty() {
                    return Err(err("forbid-external needs at least one name"));
                }
                list.forbid_external
                    .names
                    .extend(names.iter().map(|s| (*s).to_string()));
                list.forbid_external
                    .except_under
                    .extend(except.unwrap_or_default().iter().map(|s| (*s).to_string()));
            }
            "external" => {
                let [name, "allowed-in", users @ ..] = rest else {
                    return Err(err("expected 'external <crate> allowed-in <crates>'"));
                };
                if users.is_empty() {
                    return Err(err("allowed-in needs at least one crate"));
                }
                list.external.insert(
                    (*name).to_string(),
                    users.iter().map(|s| (*s).to_string()).collect(),
                );
            }
            other => return Err(err(&format!("unknown directive '{other}'"))),
        }
    }
    if list.crates.is_empty() {
        return Err("no crate lines".to_string());
    }
    for (name, entry) in &list.crates {
        for dep in &entry.deps {
            if !list.crates.contains_key(dep) {
                return Err(format!("crate '{name}' lists unknown dependency '{dep}'"));
            }
        }
    }
    for name in list.stages.keys() {
        if !list.crates.contains_key(name) {
            return Err(format!("stage names unknown crate '{name}'"));
        }
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_header_is_malformed() {
        assert!(parse("crate a class core deps -\n").is_err());
    }

    #[test]
    fn unknown_dependency_is_malformed() {
        let text = format!("{HEADER}\ncrate a class core deps b\n");
        assert!(parse(&text).unwrap_err().contains("unknown dependency 'b'"));
    }

    #[test]
    fn glob_matches_prefix() {
        assert!(matches("rt-ai-*", "rt-ai-tool"));
        assert!(!matches("rt-ai-*", "rt-json"));
    }

    #[test]
    fn missing_file_is_missing() {
        let e = load(Path::new("/nonexistent/allowed-edges.v1")).unwrap_err();
        assert!(matches!(e, EdgeListError::Missing(_)));
    }
}
