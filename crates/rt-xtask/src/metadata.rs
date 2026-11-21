//! The part of `cargo metadata --format-version 1 --all-features` that the
//! graph checks use, parsed with `rt-json`.

use rt_foundation::InputId;
use rt_json::{parse, Kind, ParseLimits, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Kind of a declared dependency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DepKind {
    /// Normal dependency.
    Normal,
    /// Build dependency.
    Build,
    /// Development dependency.
    Dev,
}

/// One declared dependency of a workspace member.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DepDecl {
    /// Package name of the dependency.
    pub name: String,
    /// Kind of the dependency.
    pub kind: DepKind,
}

/// The workspace as `cargo metadata` describes it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Metadata {
    /// Declared dependencies of each workspace member, by package name.
    pub members: BTreeMap<String, Vec<DepDecl>>,
    /// Names of the binary targets (target kind `bin`) of each workspace
    /// member. A member whose package has no `targets` array has no entry,
    /// which `check-executables` reports instead of reading it as "no binary".
    pub bins: BTreeMap<String, BTreeSet<String>>,
    /// Resolved graph of all packages by name (all kinds, all features). `None`
    /// when the metadata has no `resolve` section.
    pub resolved: Option<BTreeMap<String, BTreeSet<String>>>,
}

type Json = Value<rt_json::Position>;

fn field<'a>(v: &'a Json, key: &str) -> Result<&'a Json, String> {
    v.get(key)
        .map(|m| &m.value)
        .ok_or_else(|| format!("metadata is missing '{key}'"))
}

fn text<'a>(v: &'a Json, key: &str) -> Result<&'a str, String> {
    field(v, key)?
        .as_str()
        .ok_or_else(|| format!("metadata field '{key}' is not a string"))
}

fn items<'a>(v: &'a Json, key: &str) -> Result<&'a [Json], String> {
    field(v, key)?
        .as_array()
        .ok_or_else(|| format!("metadata field '{key}' is not an array"))
}

/// Parses the bytes of `cargo metadata` output.
///
/// # Errors
/// A message when the bytes are not RT-JSON or lack a field the checks need.
pub fn parse_metadata(bytes: &[u8]) -> Result<Metadata, String> {
    let input = InputId::new("cargo-metadata").map_err(|e| e.to_string())?;
    let limits = ParseLimits::new(512 * 1024 * 1024, 64);
    let doc = parse(&input, bytes, limits)
        .map_err(|d| format!("cargo metadata output does not parse: {}", d.message()))?;
    let mut names_by_id: BTreeMap<&str, &str> = BTreeMap::new();
    let mut declared: BTreeMap<&str, Vec<DepDecl>> = BTreeMap::new();
    let mut bins_by_name: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for package in items(&doc, "packages")? {
        let name = text(package, "name")?;
        names_by_id.insert(text(package, "id")?, name);
        let mut deps = Vec::new();
        for dep in items(package, "dependencies")? {
            let kind = match field(dep, "kind")?.kind() {
                Kind::Null => DepKind::Normal,
                Kind::String(k) if k == "dev" => DepKind::Dev,
                Kind::String(k) if k == "build" => DepKind::Build,
                _ => return Err(format!("package '{name}' has an unknown dependency kind")),
            };
            deps.push(DepDecl {
                name: text(dep, "name")?.to_string(),
                kind,
            });
        }
        deps.sort();
        declared.insert(name, deps);
        if package.get("targets").is_some() {
            let mut bins = BTreeSet::new();
            for target in items(package, "targets")? {
                let is_bin = items(target, "kind")?
                    .iter()
                    .any(|k| k.as_str() == Some("bin"));
                if is_bin {
                    bins.insert(text(target, "name")?.to_string());
                }
            }
            bins_by_name.insert(name, bins);
        }
    }
    let mut members = BTreeMap::new();
    let mut bins = BTreeMap::new();
    for id in items(&doc, "workspace_members")? {
        let id = id.as_str().ok_or("workspace member id is not a string")?;
        let name = names_by_id
            .get(id)
            .ok_or_else(|| format!("workspace member '{id}' is not in packages"))?;
        members.insert(
            (*name).to_string(),
            declared.get(name).cloned().unwrap_or_default(),
        );
        if let Some(found) = bins_by_name.get(name) {
            bins.insert((*name).to_string(), found.clone());
        }
    }
    let resolved = match doc.get("resolve").map(|m| &m.value) {
        Some(resolve) if resolve.as_object().is_some() => {
            let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for node in items(resolve, "nodes")? {
                let id = text(node, "id")?;
                let name = names_by_id
                    .get(id)
                    .ok_or_else(|| format!("resolve node '{id}' is not in packages"))?;
                let entry = graph.entry((*name).to_string()).or_default();
                for dep in items(node, "deps")? {
                    let pkg = text(dep, "pkg")?;
                    let dep_name = names_by_id
                        .get(pkg)
                        .ok_or_else(|| format!("resolved dependency '{pkg}' is not in packages"))?;
                    entry.insert((*dep_name).to_string());
                }
            }
            Some(graph)
        }
        _ => None,
    };
    Ok(Metadata {
        members,
        bins,
        resolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_is_rejected() {
        assert!(parse_metadata(b"not json").is_err());
        assert!(parse_metadata(b"{}").unwrap_err().contains("packages"));
    }

    #[test]
    fn only_bin_kind_targets_are_binaries() {
        let json = br#"{"packages":[{"name":"a","id":"a1","dependencies":[],
            "targets":[{"name":"x","kind":["bin"]},{"name":"y","kind":["lib"]},
            {"name":"z","kind":["example"]},{"name":"t","kind":["test"]}]},
            {"name":"b","id":"b1","dependencies":[]}],
            "workspace_members":["a1","b1"]}"#;
        let md = parse_metadata(json).unwrap();
        assert_eq!(md.bins["a"], BTreeSet::from(["x".to_string()]));
        assert!(!md.bins.contains_key("b"));
    }
}
