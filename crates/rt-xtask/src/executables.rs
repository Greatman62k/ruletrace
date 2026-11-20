//! `check-executables` (design §3.3 item 7 as amended by UD-38, Req 27.1,
//! 27.15).
//!
//! The binary targets of the workspace packages of class core, aux and lab
//! must equal the entries of `tools/executables.v1`, each in the package the
//! entry names. A package of class app must have no binary target. Classes
//! tool and test are excluded by rule: their binaries (`rt-xtask`, test
//! helpers) are development programs that are not delivered (RN-25).
//!
//! The list names `ruletrace` and `ruletrace-ai` until task 13.4 adds
//! `ruletrace-lab` in the commit that creates `rt-lab`. Req 27.15 names three
//! executables, so this check does not establish it before that commit.

use crate::edges::{Class, EdgeList};
use crate::metadata::Metadata;
use std::collections::{BTreeMap, BTreeSet};

/// Path of the list, relative to the repository root.
pub const LIST_PATH: &str = "tools/executables.v1";
const HEADER: &str = "# format: ruletrace-executables v1";

/// The parsed list: executable name to the package that builds it.
pub type ExecutableList = BTreeMap<String, String>;

/// Parses `tools/executables.v1`. Each entry is
/// `executable <name> package <package>`.
///
/// # Errors
/// A message naming the line when the header is missing, an entry is
/// malformed or a name is listed twice.
pub fn parse_list(text: &str) -> Result<ExecutableList, String> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    match lines.next() {
        Some((_, l)) if l.trim() == HEADER => {}
        _ => return Err(format!("first line must be `{HEADER}`")),
    }
    let mut out = ExecutableList::new();
    for (n, raw) in lines {
        let line = raw.trim();
        if line.starts_with('#') {
            continue;
        }
        let at = n + 1;
        let words: Vec<&str> = line.split_whitespace().collect();
        let ["executable", name, "package", package] = words.as_slice() else {
            return Err(format!(
                "line {at}: expected `executable <name> package <package>`"
            ));
        };
        if out
            .insert((*name).to_string(), (*package).to_string())
            .is_some()
        {
            return Err(format!("line {at}: executable '{name}' is listed twice"));
        }
    }
    if out.is_empty() {
        return Err("the list names no executable".to_string());
    }
    Ok(out)
}

fn delivered(class: Class) -> bool {
    matches!(class, Class::Core | Class::Aux | Class::Lab)
}

/// Runs the check. Each finding is one line that starts with its rule name in
/// brackets.
#[must_use]
pub fn check(list: &EdgeList, wanted: &ExecutableList, md: &Metadata) -> Vec<String> {
    let mut out = Vec::new();
    for (name, package) in wanted {
        let class = list.crates.get(package).map(|e| e.class);
        if !class.is_some_and(delivered) {
            out.push(format!(
                "[executable-list] executable '{name}' names package '{package}', which is not a package of class core, aux or lab"
            ));
        }
    }
    let mut built: BTreeSet<(&str, &str)> = BTreeSet::new();
    for package in md.members.keys() {
        let class = list.crates.get(package).map(|e| e.class);
        if matches!(class, Some(Class::Tool | Class::Test)) {
            continue;
        }
        let Some(bins) = md.bins.get(package) else {
            out.push(format!(
                "[metadata-malformed] workspace package '{package}' has no target list in cargo metadata"
            ));
            continue;
        };
        for bin in bins {
            if class.is_some_and(delivered) {
                built.insert((bin, package));
                if wanted.get(bin).map(String::as_str) != Some(package.as_str()) {
                    out.push(format!(
                        "[executable-extra] package '{package}' builds binary '{bin}', which {LIST_PATH} does not list for it"
                    ));
                }
            } else if class == Some(Class::App) {
                out.push(format!(
                    "[app-binary] application package '{package}' has binary target '{bin}'; an application layer is a library"
                ));
            } else {
                out.push(format!(
                    "[executable-unclassified] package '{package}' has no class in tools/allowed-edges.v1 and builds binary '{bin}'"
                ));
            }
        }
    }
    for (name, package) in wanted {
        if !built.contains(&(name.as_str(), package.as_str())) {
            out.push(format!(
                "[executable-missing] {LIST_PATH} lists '{name}' in package '{package}', which builds no such binary"
            ));
        }
    }
    out.sort();
    out
}

/// Reads the list and runs the check. Returns the number of executables.
///
/// # Errors
/// One line per finding, or the load failure.
pub fn run(
    root: &std::path::Path,
    list: &EdgeList,
    metadata_bytes: &[u8],
) -> Result<usize, String> {
    let text = std::fs::read_to_string(root.join(LIST_PATH))
        .map_err(|e| format!("[executable-list] cannot read {LIST_PATH}: {e}"))?;
    let wanted = parse_list(&text).map_err(|m| format!("[executable-list] {LIST_PATH}: {m}"))?;
    let md = crate::metadata::parse_metadata(metadata_bytes)
        .map_err(|m| format!("[metadata-malformed] {m}"))?;
    let findings = check(list, &wanted, &md);
    if findings.is_empty() {
        Ok(wanted.len())
    } else {
        Err(findings.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edges;
    use crate::metadata::parse_metadata;

    const EDGES: &str = "# format: ruletrace-allowed-edges v1
crate rt-foundation class core deps -
crate rt-cli class core deps rt-foundation
crate rt-ai-tool class aux deps rt-foundation
crate rt-lab class lab deps rt-foundation
crate prize-check class app deps rt-foundation
crate rt-xtask class tool deps rt-foundation
crate rt-tests-core class test deps rt-foundation
";

    const LIST: &str = "# format: ruletrace-executables v1
executable ruletrace package rt-cli
executable ruletrace-ai package rt-ai-tool
";

    type Spec<'a> = &'a [(&'a str, &'a [&'a str])];

    fn metadata(spec: Spec<'_>) -> Metadata {
        let packages: Vec<String> = spec
            .iter()
            .map(|(name, bins)| {
                let mut targets: Vec<String> = bins
                    .iter()
                    .map(|b| format!("{{\"name\":\"{b}\",\"kind\":[\"bin\"]}}"))
                    .collect();
                targets.push(format!("{{\"name\":\"{name}\",\"kind\":[\"lib\"]}}"));
                format!(
                    "{{\"name\":\"{name}\",\"id\":\"id-{name}\",\"dependencies\":[],\"targets\":[{}]}}",
                    targets.join(",")
                )
            })
            .collect();
        let ids: Vec<String> = spec.iter().map(|(n, _)| format!("\"id-{n}\"")).collect();
        let json = format!(
            "{{\"packages\":[{}],\"workspace_members\":[{}]}}",
            packages.join(","),
            ids.join(",")
        );
        parse_metadata(json.as_bytes()).unwrap()
    }

    const CONFORMING: Spec<'static> = &[
        ("rt-foundation", &[]),
        ("rt-cli", &["ruletrace"]),
        ("rt-ai-tool", &["ruletrace-ai"]),
        ("prize-check", &[]),
        ("rt-xtask", &["rt-xtask"]),
        ("rt-tests-core", &["helper"]),
    ];

    fn findings(spec: Spec<'_>) -> Vec<String> {
        let list = edges::parse(EDGES).unwrap();
        check(&list, &parse_list(LIST).unwrap(), &metadata(spec))
    }

    #[test]
    fn conforming_workspace_passes_and_tool_and_test_bins_are_excluded() {
        assert_eq!(findings(CONFORMING), Vec::<String>::new());
    }

    #[test]
    fn extra_binary_in_an_application_fails() {
        let spec: Spec<'_> = &[
            ("rt-cli", &["ruletrace"]),
            ("rt-ai-tool", &["ruletrace-ai"]),
            ("prize-check", &["prize-check"]),
        ];
        let f = findings(spec);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].starts_with("[app-binary]") && f[0].contains("prize-check"));
    }

    #[test]
    fn missing_ruletrace_ai_is_named() {
        let spec: Spec<'_> = &[("rt-cli", &["ruletrace"]), ("rt-ai-tool", &[])];
        let f = findings(spec);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].starts_with("[executable-missing]") && f[0].contains("'ruletrace-ai'"));
    }

    #[test]
    fn unknown_binary_in_a_core_package_fails() {
        let spec: Spec<'_> = &[
            ("rt-foundation", &["debug-dump"]),
            ("rt-cli", &["ruletrace"]),
            ("rt-ai-tool", &["ruletrace-ai"]),
        ];
        let f = findings(spec);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].starts_with("[executable-extra]") && f[0].contains("'debug-dump'"));
    }

    #[test]
    fn a_listed_name_in_the_wrong_package_fails_twice() {
        let spec: Spec<'_> = &[
            ("rt-cli", &["ruletrace", "ruletrace-ai"]),
            ("rt-ai-tool", &[]),
        ];
        let f = findings(spec);
        assert!(
            f.iter().any(|l| l.starts_with("[executable-extra]")),
            "{f:?}"
        );
        assert!(
            f.iter().any(|l| l.starts_with("[executable-missing]")),
            "{f:?}"
        );
    }

    #[test]
    fn a_lab_binary_outside_the_list_fails() {
        let spec: Spec<'_> = &[
            ("rt-cli", &["ruletrace"]),
            ("rt-ai-tool", &["ruletrace-ai"]),
            ("rt-lab", &["ruletrace-lab"]),
        ];
        let f = findings(spec);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].starts_with("[executable-extra]") && f[0].contains("ruletrace-lab"));
    }

    #[test]
    fn a_binary_of_an_unclassified_package_fails() {
        let spec: Spec<'_> = &[
            ("rt-cli", &["ruletrace"]),
            ("rt-ai-tool", &["ruletrace-ai"]),
            ("stray", &["stray"]),
        ];
        let f = findings(spec);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].starts_with("[executable-unclassified]"));
    }

    #[test]
    fn a_package_without_a_target_list_is_malformed_metadata() {
        let json = br#"{"packages":[{"name":"rt-cli","id":"a","dependencies":[]}],
            "workspace_members":["a"]}"#;
        let md = parse_metadata(json).unwrap();
        let list = edges::parse(EDGES).unwrap();
        let f = check(&list, &parse_list(LIST).unwrap(), &md);
        assert!(
            f.iter().any(|l| l.starts_with("[metadata-malformed]")),
            "{f:?}"
        );
    }

    #[test]
    fn list_naming_a_tool_package_is_rejected() {
        let list = edges::parse(EDGES).unwrap();
        let wanted = parse_list(&format!("{LIST}executable x package rt-xtask\n")).unwrap();
        let f = check(&list, &wanted, &metadata(CONFORMING));
        assert!(
            f.iter().any(|l| l.starts_with("[executable-list]")),
            "{f:?}"
        );
    }

    #[test]
    fn list_parser_is_strict() {
        assert!(parse_list("executable a package b\n").is_err());
        let h = |l: &str| parse_list(&format!("{HEADER}\n{l}\n"));
        assert!(h("executable a package").is_err());
        assert!(h("exe a package b").is_err());
        assert!(h("executable a package b\nexecutable a package c").is_err());
        assert!(h("# only a comment").is_err());
        assert_eq!(h("executable a package b").unwrap()["a"], "b");
    }

    #[test]
    fn committed_list_names_exactly_ruletrace_and_ruletrace_ai() {
        let wanted = parse_list(include_str!("../../../tools/executables.v1")).unwrap();
        let expected: ExecutableList = [
            ("ruletrace".to_string(), "rt-cli".to_string()),
            ("ruletrace-ai".to_string(), "rt-ai-tool".to_string()),
        ]
        .into();
        assert_eq!(wanted, expected);
    }
}
