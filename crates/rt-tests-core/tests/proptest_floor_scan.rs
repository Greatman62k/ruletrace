//! Source scan: every property test of the workspace goes through
//! `property_tests!` (Req 20.2). A raw `proptest!` block would let
//! `PROPTEST_CASES` lower its case count and still pass.
//!
//! Rules, each with a negative fixture below:
//! 1. no `proptest!` invocation in crate sources;
//! 2. a file that uses `property_tests!` has the shared module in scope;
//! 3. a `below_floor_config` test is listed in
//!    `tools/proptest-below-floor.v1` with its count, and the list names no
//!    test that is not below the floor;
//! 4. a `proptest_config` with a literal count below 256 is rejected.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const FLOOR: u32 = 256;
const MODULE_PATH: &str = "#[path = \"../../../tools/proptest_floor.rs\"]";
const SCANNER: &str = "proptest_floor_scan.rs";

struct Source {
    path: String,
    text: String,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn workspace_sources() -> Vec<Source> {
    let root = root();
    let mut files = Vec::new();
    for part in ["src", "tests"] {
        let crates = fs::read_dir(root.join("crates")).expect("crates directory");
        for krate in crates.filter_map(Result::ok) {
            rust_files(&krate.path().join(part), &mut files);
        }
    }
    files
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|n| n != SCANNER))
        .map(|p| Source {
            path: p
                .strip_prefix(&root)
                .map(|r| list_path(&r.to_string_lossy()))
                .unwrap_or_default(),
            text: fs::read_to_string(&p).expect("readable source"),
        })
        .collect()
}

/// The form of a workspace path that the list and the crate rule use.
fn list_path(relative: &str) -> String {
    relative.replace('\\', "/").replace("../", "")
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// One `property_tests!` block: its kind, literal count and test names.
struct Block {
    below_floor: bool,
    cases: Option<u32>,
    tests: Vec<String>,
}

fn literal_cases(config_line: &str) -> Option<u32> {
    let after = config_line.split("with_cases(").nth(1)?;
    after.split(')').next()?.trim().parse().ok()
}

fn blocks(text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if is_comment(line) || !line.contains("property_tests!") || !line.trim_end().ends_with('{')
        {
            continue;
        }
        let level = indent(line);
        let config = lines[i + 1..]
            .iter()
            .take(3)
            .find(|l| l.contains("proptest_config(") || l.contains("below_floor_config("));
        let mut block = Block {
            below_floor: config.is_some_and(|l| l.contains("below_floor_config(")),
            cases: config.and_then(|l| literal_cases(l)),
            tests: Vec::new(),
        };
        for next in &lines[i + 1..] {
            if indent(next) == level && next.trim() == "}" {
                break;
            }
            let trimmed = next.trim_start();
            if indent(next) == level + 4 {
                if let Some(rest) = trimmed.strip_prefix("fn ") {
                    block
                        .tests
                        .push(rest.split('(').next().unwrap_or("").to_string());
                }
            }
        }
        out.push(block);
    }
    out
}

fn parse_list(list: &str) -> Vec<(String, String, u32)> {
    list.lines()
        .filter_map(|l| l.strip_prefix("below "))
        .filter_map(|l| {
            let parts: Vec<&str> = l.split_whitespace().collect();
            match parts.as_slice() {
                [file, test, "cases", n] => {
                    Some((file.to_string(), test.to_string(), n.parse().ok()?))
                }
                _ => None,
            }
        })
        .collect()
}

fn has_module(text: &str) -> bool {
    text.contains("mod proptest_floor;") && text.contains(MODULE_PATH)
}

/// Returns one message per violation of the four rules.
fn scan(sources: &[Source], list: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let mut seen = BTreeSet::new();
    let listed = parse_list(list);
    for source in sources {
        for line in source.text.lines().filter(|l| !is_comment(l)) {
            if line.contains("proptest!") {
                findings.push(format!(
                    "{}: raw proptest! block; use property_tests!",
                    source.path
                ));
            }
        }
        let found = blocks(&source.text);
        if !found.is_empty() {
            let in_scope = match source.path.split_once("/src/") {
                Some((krate, _)) => sources
                    .iter()
                    .any(|s| s.path == format!("{krate}/src/lib.rs") && has_module(&s.text)),
                None => has_module(&source.text),
            };
            if !in_scope {
                findings.push(format!(
                    "{}: property_tests! without the proptest_floor module",
                    source.path
                ));
            }
        }
        for block in found {
            if block.below_floor {
                for test in &block.tests {
                    match listed
                        .iter()
                        .find(|(f, t, _)| *f == source.path && t == test)
                    {
                        Some((_, _, n)) if Some(*n) == block.cases => {
                            seen.insert((source.path.clone(), test.clone()));
                        }
                        Some(_) => findings.push(format!(
                            "{}: {test} is listed with a different case count",
                            source.path
                        )),
                        None => findings.push(format!(
                            "{}: {test} is below the floor and not in the list",
                            source.path
                        )),
                    }
                }
                if block.cases.is_none_or(|n| n >= FLOOR) {
                    findings.push(format!(
                        "{}: below_floor_config with a count that is not below {FLOOR}",
                        source.path
                    ));
                }
            } else if block.cases.is_some_and(|n| n < FLOOR) {
                findings.push(format!(
                    "{}: proptest_config below {FLOOR} cases",
                    source.path
                ));
            }
        }
    }
    for (file, test, _) in &listed {
        if !seen.contains(&(file.clone(), test.clone())) {
            findings.push(format!("list: {file} {test} is not a below-floor test"));
        }
    }
    findings
}

fn fixture(path: &str, text: &str) -> Source {
    Source {
        path: path.to_string(),
        text: text.to_string(),
    }
}

fn lib() -> Source {
    fixture("crates/x/src/lib.rs", &format!("#[cfg(test)]\n{MODULE}"))
}

const MODULE: &str = "#[path = \"../../../tools/proptest_floor.rs\"]\nmod proptest_floor;\n";

#[test]
fn the_workspace_has_no_unprotected_property_test() {
    let list =
        fs::read_to_string(root().join("tools/proptest-below-floor.v1")).expect("below-floor list");
    let sources = workspace_sources();
    assert!(
        sources
            .iter()
            .filter(|s| s.text.contains("property_tests!"))
            .count()
            > 40,
        "the scan must see the property tests"
    );
    assert_eq!(scan(&sources, &list), Vec::<String>::new());
}

#[test]
fn a_raw_proptest_block_is_reported() {
    let source = fixture(
        "crates/x/src/tests.rs",
        "proptest! {\n    #![proptest_config(ProptestConfig::with_cases(256))]\n    fn a(x in 0u8..1) { }\n}\n",
    );
    assert_eq!(
        scan(&[source, lib()], ""),
        ["crates/x/src/tests.rs: raw proptest! block; use property_tests!"]
    );
}

#[test]
fn a_commented_proptest_mention_is_not_reported() {
    let source = fixture("crates/x/src/tests.rs", "// proptest! is not used here\n");
    assert!(scan(&[source, lib()], "").is_empty());
}

#[test]
fn an_integration_test_without_the_module_is_reported() {
    let text = "property_tests! {\n    fn a(x in 0u8..1) { }\n}\n";
    assert_eq!(
        scan(&[fixture("crates/x/tests/t.rs", text)], ""),
        ["crates/x/tests/t.rs: property_tests! without the proptest_floor module"]
    );
    let with_module = format!("{MODULE}{text}");
    assert!(scan(&[fixture("crates/x/tests/t.rs", &with_module)], "").is_empty());
}

#[test]
fn a_literal_count_below_the_floor_is_reported() {
    let text = "property_tests! {\n    #![proptest_config(ProptestConfig::with_cases(10))]\n    fn a(x in 0u8..1) { }\n}\n";
    assert_eq!(
        scan(&[fixture("crates/x/src/t.rs", text), lib()], ""),
        ["crates/x/src/t.rs: proptest_config below 256 cases"]
    );
}

#[test]
fn an_unlisted_below_floor_test_is_reported() {
    let text = "property_tests! {\n    #![below_floor_config(ProptestConfig::with_cases(64))]\n    fn a(x in 0u8..1) { }\n}\n";
    assert_eq!(
        scan(&[fixture("crates/x/src/t.rs", text), lib()], ""),
        ["crates/x/src/t.rs: a is below the floor and not in the list"]
    );
}

#[test]
fn a_listed_below_floor_test_with_its_count_is_accepted() {
    let text = "property_tests! {\n    #![below_floor_config(ProptestConfig::with_cases(64))]\n    fn a(x in 0u8..1) { }\n}\n";
    let list = "below crates/x/src/t.rs a cases 64\n";
    assert!(scan(&[fixture("crates/x/src/t.rs", text), lib()], list).is_empty());
    let wrong = "below crates/x/src/t.rs a cases 128\n";
    assert_eq!(
        scan(&[fixture("crates/x/src/t.rs", text), lib()], wrong),
        [
            "crates/x/src/t.rs: a is listed with a different case count",
            "list: crates/x/src/t.rs a is not a below-floor test",
        ]
    );
}

#[test]
fn a_stale_list_entry_and_a_below_floor_marker_at_256_are_reported() {
    let text = "property_tests! {\n    #![below_floor_config(ProptestConfig::with_cases(256))]\n    fn a(x in 0u8..1) { }\n}\n";
    let list = "below crates/x/src/t.rs a cases 256\nbelow crates/x/src/t.rs gone cases 64\n";
    let findings = scan(&[fixture("crates/x/src/t.rs", text), lib()], list);
    assert_eq!(
        findings,
        [
            "crates/x/src/t.rs: below_floor_config with a count that is not below 256",
            "list: crates/x/src/t.rs gone is not a below-floor test",
        ]
    );
}

#[test]
fn a_source_file_whose_crate_lacks_the_module_is_reported() {
    let text = "property_tests! {\n    fn a(x in 0u8..1) { }\n}\n";
    let bare_lib = fixture("crates/x/src/lib.rs", "");
    assert_eq!(
        scan(&[fixture("crates/x/src/t.rs", text), bare_lib], ""),
        ["crates/x/src/t.rs: property_tests! without the proptest_floor module"]
    );
}

// Regression: on Windows the relative paths of the workspace scan have `\`
// separators, so neither the crate rule nor the list matched (CI run
// 44716508559, test (windows-2022)).
#[test]
fn windows_separators_match_the_crate_rule_and_the_list() {
    let text = "property_tests! {\n    #![below_floor_config(ProptestConfig::with_cases(64))]\n    fn a(x in 0u8..1) { }\n}\n";
    let list = "below crates/x/src/t.rs a cases 64\n";
    let sources = [
        fixture(&list_path("crates\\x\\src\\t.rs"), text),
        fixture(
            &list_path("crates\\x\\src\\lib.rs"),
            &format!("#[cfg(test)]\n{MODULE}"),
        ),
    ];
    assert_eq!(scan(&sources, list), Vec::<String>::new());
    assert_eq!(list_path("crates\\x\\src\\t.rs"), "crates/x/src/t.rs");
}
