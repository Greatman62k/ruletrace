//! Tests of `check-app-layers`. Each violation class has a negative source
//! whose finding must name the crate, the file, the exact line and the rule.
//! The expected lines are counted by hand from the sources below.

use super::*;
use crate::edges;
use crate::metadata::parse_metadata;

fn names() -> BTreeSet<String> {
    ["goals", "age", "eligible"].map(String::from).into()
}

fn listed(items: &[&str]) -> Vec<LiteralEntry> {
    items
        .iter()
        .map(|t| LiteralEntry {
            crate_name: "*".to_string(),
            text: (*t).to_string(),
            reason: "test".to_string(),
        })
        .collect()
}

fn scan_with(body: &str, literals: &[LiteralEntry]) -> Vec<String> {
    let files = vec![("src/lib.rs".to_string(), body.to_string())];
    scan_crate("prize-check", &files, literals, &names())
}

fn scan(body: &str) -> Vec<String> {
    scan_with(body, &[])
}

/// The scan reports exactly one finding, of `rule`, on `line` of `src/lib.rs`.
fn fails_once(body: &str, rule: &str, line: usize) {
    let found = scan(body);
    let prefix = format!("[{rule}] prize-check: src/lib.rs:{line}: ");
    assert!(
        found.len() == 1 && found[0].starts_with(&prefix),
        "expected one finding starting `{prefix}`, got {found:?}\nsource:\n{body}"
    );
}

fn passes(body: &str) {
    let found = scan(body);
    assert!(found.is_empty(), "unexpected findings {found:?}\n{body}");
}

#[test]
fn add_fails_on_its_line() {
    fails_once(
        "fn f(a: u32, b: u32) -> u32 {\n    a + b\n}\n",
        "app-arith",
        2,
    );
}

#[test]
fn subtract_fails_on_its_line() {
    fails_once(
        "fn f(a: u32, b: u32) -> u32 {\n    a - b\n}\n",
        "app-arith",
        2,
    );
    fails_once(
        "fn f(v: &[u8]) -> usize {\n    v.len() - 1\n}\n",
        "app-arith",
        2,
    );
}

#[test]
fn multiply_fails_on_its_line() {
    fails_once(
        "fn f(a: u32, b: u32) -> u32 {\n    a * b\n}\n",
        "app-arith",
        2,
    );
}

#[test]
fn divide_and_remainder_fail_on_their_line() {
    fails_once(
        "fn f(a: u32, b: u32) -> u32 {\n    a / b\n}\n",
        "app-arith",
        2,
    );
    fails_once(
        "fn f(a: u32, b: u32) -> u32 {\n    a % b\n}\n",
        "app-arith",
        2,
    );
}

#[test]
fn assignment_forms_fail_on_their_line() {
    for op in ["+=", "-=", "*=", "/=", "%="] {
        let body = format!("fn f(mut a: u32, b: u32) {{\n    a {op} b;\n}}\n");
        let found = scan(&body);
        assert!(
            found
                .iter()
                .any(|l| l.starts_with("[app-arith] prize-check: src/lib.rs:2:")),
            "{op}: {found:?}"
        );
    }
}

#[test]
fn ordering_comparisons_fail_on_their_line() {
    fails_once(
        "fn f(a: u32, b: u32) -> bool {\n    a < b\n}\n",
        "app-compare",
        2,
    );
    fails_once(
        "fn f(a: u32, b: u32) -> bool {\n    a > b\n}\n",
        "app-compare",
        2,
    );
    fails_once(
        "fn f(a: u32, b: u32) -> bool {\n    a >= b\n}\n",
        "app-compare",
        2,
    );
    fails_once(
        "fn f(a: u32, b: u32) -> bool {\n    a <= b\n}\n",
        "app-compare",
        2,
    );
}

#[test]
fn equality_of_exact_number_operands_fails_but_record_equality_passes() {
    fails_once(
        "fn f(a: &X, b: &X) -> bool {\n    to_canonical(a) == to_canonical(b)\n}\n",
        "app-compare",
        2,
    );
    fails_once(
        "fn f(m: &TraceValue) -> bool {\n    TraceValue::Number(m) != m\n}\n",
        "app-compare",
        2,
    );
    // The consistency check of a report: two typed records, no Exact_Number
    // carrier in either operand.
    passes("fn f(outcome: &Outcome, trace: &Trace) -> bool {\n    outcome != trace.outcome()\n}\n");
    passes("fn f(a: &Id, b: &Id) -> bool {\n    a == b\n}\n");
}

#[test]
fn sorting_functions_fail_on_their_line() {
    for call in [
        "v.sort();",
        "v.sort_by(|a, b| a.id());",
        "v.sort_by_key(|a| a.id());",
        "v.sort_unstable();",
        "v.sort_unstable_by_key(|a| a.id());",
        "let _ = v.binary_search(&x);",
        "let _ = v.iter().rev();",
    ] {
        let body = format!("fn f(v: &mut Vec<Id>) {{\n    {call}\n}}\n");
        let found = scan(&body);
        assert!(
            found
                .iter()
                .any(|l| l.starts_with("[app-order] prize-check: src/lib.rs:2:")),
            "{call}: {found:?}"
        );
    }
}

#[test]
fn minimum_maximum_and_ranking_selection_fail_on_their_line() {
    for call in [
        "let _ = v.iter().min();",
        "let _ = v.iter().max();",
        "let _ = v.iter().min_by_key(|s| s.id());",
        "let _ = v.iter().max_by(|a, b| a.id());",
        "let _ = std::cmp::max(a, b);",
        "let _ = a.clamp(b, c);",
    ] {
        let body = format!("fn f(v: &[Id]) {{\n    {call}\n}}\n");
        let found = scan(&body);
        assert!(
            found
                .iter()
                .any(|l| l.starts_with("[app-order] prize-check: src/lib.rs:2:")),
            "{call}: {found:?}"
        );
    }
}

#[test]
fn aggregates_fail_on_their_line() {
    for call in [
        "let _ = v.iter().sum::<u32>();",
        "let _ = v.iter().product::<u32>();",
        "let _ = v.iter().fold(0, f);",
        "let _ = v.iter().reduce(f);",
    ] {
        let body = format!("fn f(v: &[u32]) {{\n    {call}\n}}\n");
        let found = scan(&body);
        assert!(
            found
                .iter()
                .any(|l| l.starts_with("[app-order] prize-check: src/lib.rs:2:")),
            "{call}: {found:?}"
        );
    }
}

#[test]
fn rounding_and_checked_arithmetic_calls_fail_on_their_line() {
    for call in [
        "let _ = x.round();",
        "let _ = x.floor();",
        "let _ = x.ceil();",
        "let _ = x.abs();",
        "let _ = x.checked_add(y);",
        "let _ = x.saturating_sub(y);",
        "let _ = x.is_zero();",
    ] {
        let body = format!("fn f(x: u32, y: u32) {{\n    {call}\n}}\n");
        let found = scan(&body);
        assert!(
            found
                .iter()
                .any(|l| l.starts_with("[app-order] prize-check: src/lib.rs:2:")),
            "{call}: {found:?}"
        );
    }
}

#[test]
fn comparison_types_fail_on_their_line() {
    fails_once("use std::collections::BinaryHeap;\n", "app-order", 1);
    fails_once("fn f() -> Ordering {\n    todo()\n}\n", "app-order", 1);
    fails_once("fn f(a: Reverse<u8>) {}\n", "app-order", 1);
    fails_once(
        "fn f(a: &X, b: &X) {\n    let _ = a.cmp(b);\n}\n",
        "app-order",
        2,
    );
    fails_once(
        "fn f(a: &X, b: &X) {\n    let _ = a.partial_cmp(b);\n}\n",
        "app-order",
        2,
    );
}

#[test]
fn rt_numeric_operations_fail_on_their_line() {
    fails_once("use rt_numeric::add;\n", "app-numeric", 1);
    // A call is also a named operation, so it is reported by both rules.
    let found = scan("fn f(a: &N, b: &N) {\n    let _ = rt_numeric::mul(a, b);\n}\n");
    assert!(
        found.contains(&"[app-numeric] prize-check: src/lib.rs:2: `rt_numeric::mul`".to_string()),
        "{found:?}"
    );
    fails_once("use rt_numeric::*;\n", "app-numeric", 1);
    fails_once("use rt_numeric::{to_canonical, div};\n", "app-numeric", 1);
    fails_once("use rt_numeric::ExactNumber;\n", "app-numeric", 1);
}

#[test]
fn rt_numeric_rendering_is_the_one_allowed_item() {
    passes("use rt_numeric::to_canonical;\nfn f(n: &N) -> String {\n    to_canonical(n)\n}\n");
    passes("use rt_numeric::{to_canonical};\n");
}

#[test]
fn tie_breaking_names_fail_on_their_line() {
    fails_once(
        "fn f() {\n    let tie_breakers = 0;\n}\n",
        "app-tiebreak",
        2,
    );
    fails_once("struct TieBreaker;\n", "app-tiebreak", 1);
    fails_once(
        "fn f() -> &'static str {\n    \"tie-break\"\n}\n",
        "app-tiebreak",
        2,
    );
}

#[test]
fn rounding_mode_names_fail_on_their_line() {
    fails_once("use rt_x::RoundingMode;\n", "app-rounding", 1);
    fails_once(
        "fn f() {\n    let m = Mode::HalfEven;\n}\n",
        "app-rounding",
        2,
    );
    fails_once(
        "fn f() -> &'static str {\n    \"half_even\"\n}\n",
        "app-rounding",
        2,
    );
    let found = scan_with(
        "fn f() -> &'static str {\n    \"toward_zero\"\n}\n",
        &listed(&["toward_zero"]),
    );
    assert!(found.iter().any(|l| l.starts_with("[app-rounding]")));
}

#[test]
fn literal_weight_threshold_and_decimal_fail_on_their_line() {
    fails_once("const WEIGHT: u32 = 3;\n", "app-literal", 1);
    fails_once("fn f() -> u32 {\n    100\n}\n", "app-literal", 2);
    fails_once("fn f() -> u32 {\n    2u32\n}\n", "app-literal", 2);
    fails_once("fn f() {\n    let _ = 0.5;\n}\n", "app-literal", 2);
    fails_once("fn f() {\n    let _ = [1, 2];\n}\n", "app-literal", 2);
}

#[test]
fn zero_one_and_tuple_indices_pass() {
    passes("fn f(v: &[u8], t: (u8, u8, u8)) -> u8 {\n    let _ = v.get(0);\n    let _ = 1u32;\n    t.2\n}\n");
}

#[test]
fn unlisted_string_literal_fails_and_listed_passes() {
    fails_once(
        "fn f() -> &'static str {\n    \"--bonus\"\n}\n",
        "app-literal",
        2,
    );
    let body = "fn f() -> &'static str {\n    \"--bonus\"\n}\n";
    assert!(scan_with(body, &listed(&["--bonus"])).is_empty());
    let other_crate = vec![LiteralEntry {
        crate_name: "bracket-check".to_string(),
        text: "--bonus".to_string(),
        reason: "test".to_string(),
    }];
    assert!(!scan_with(body, &other_crate).is_empty());
}

#[test]
fn competition_field_name_fails_even_when_listed() {
    fails_once(
        "fn f() -> &'static str {\n    \"goals\"\n}\n",
        "app-field-name",
        2,
    );
    let found = scan_with(
        "fn f() -> &'static str {\n    \"goals\"\n}\n",
        &listed(&["goals"]),
    );
    assert_eq!(found.len(), 1);
    assert!(found[0].starts_with("[app-field-name] prize-check: src/lib.rs:2:"));
}

#[test]
fn listing_a_rulebook_name_is_refused() {
    let list = edges::parse(
        "# format: ruletrace-allowed-edges v1\ncrate prize-check class app deps -\ncrate rt-json class core deps -\n",
    )
    .unwrap();
    let ok = [LiteralEntry {
        crate_name: "prize-check".to_string(),
        text: "--score".to_string(),
        reason: "r".to_string(),
    }];
    assert!(validate_literals(&ok, &list, &names()).is_ok());
    let bad = [LiteralEntry {
        crate_name: "prize-check".to_string(),
        text: "goals".to_string(),
        reason: "r".to_string(),
    }];
    assert!(validate_literals(&bad, &list, &names())
        .unwrap_err()
        .contains("goals"));
    let core = [LiteralEntry {
        crate_name: "rt-json".to_string(),
        text: "x".to_string(),
        reason: "r".to_string(),
    }];
    assert!(validate_literals(&core, &list, &names()).is_err());
}

#[test]
fn file_network_clock_random_environment_and_subprocess_fail_on_their_line() {
    let cases = [
        ("use std::fs;", "std::fs"),
        ("use std::net::TcpStream;", "std::net"),
        ("use std::env;", "std::env"),
        ("use std::io::Read;", "std::io"),
        ("use std::time::Duration;", "std::time"),
        ("use std::thread;", "std::thread"),
        ("use std::process::exit;", "std::process"),
        ("use std::{collections::BTreeMap, fs};", "std::fs"),
        ("fn f() {\n    let _ = std::fs::read(\"x\");\n}", "std::fs"),
        ("fn f() {\n    let _ = Instant::now();\n}", "Instant"),
        ("fn f() {\n    let _ = SystemTime::now();\n}", "SystemTime"),
        ("fn f() {\n    let _ = rand::random::<u8>();\n}", "rand"),
        ("fn f() {\n    let _ = Command::new(\"x\");\n}", "Command"),
        (
            "fn f() {\n    let _ = std::process::Command::new(\"x\");\n}",
            "Command",
        ),
        ("fn f() {\n    println!();\n}", "println!"),
        ("fn f() {\n    let _ = env!(\"A\");\n}", "env!"),
        (
            "fn f() {\n    let _ = include_str!(\"a\");\n}",
            "include_str!",
        ),
    ];
    for (body, token) in cases {
        let line = if body.contains('\n') { 2 } else { 1 };
        let found = scan(body);
        let hit = found.iter().any(|l| {
            l.starts_with(&format!("[app-io] prize-check: src/lib.rs:{line}: "))
                && l.contains(&format!("`{token}`"))
        });
        assert!(hit, "{token}: {found:?}");
    }
}

#[test]
fn constructs_that_are_not_logic_pass() {
    let body = "\
use std::collections::{BTreeMap, BTreeSet};
use rt_data::tabular::*;
enum Location {
    Command(String),
}
struct Holder<T: Clone + Send> {
    items: BTreeMap<String, Vec<T>>,
}
fn bounded<T: Clone + Send>(x: &T) -> Box<dyn Fn(&T) -> u8 + 'static> {
    todo()
}
fn deref(x: &u32, y: Option<u32>) -> Option<u32> {
    let z = -1i32;
    let w = *x;
    let v: Vec<Vec<u8>> = Vec::new();
    let s: BTreeSet<u32> = BTreeSet::new();
    let t = Location::Command(String::new());
    let _ = &mut *Box::new(w);
    match y {
        Some(n) => Some(n),
        None => None,
    }
}
";
    passes(body);
}

#[test]
fn test_items_and_comments_are_not_scanned() {
    let body = "\
// a + b and std::fs and v.sort()
fn ok() {}
#[cfg(test)]
mod tests {
    fn t(a: u32, b: u32) -> u32 {
        a + b
    }
}
";
    passes(body);
}

#[test]
fn out_of_line_test_module_files_are_skipped() {
    let files = vec![
        (
            "src/lib.rs".to_string(),
            "#[cfg(test)]\nmod tests_x;\nfn ok() {}\n".to_string(),
        ),
        (
            "src/tests_x.rs".to_string(),
            "fn t(a: u32, b: u32) -> u32 { a + b }\n".to_string(),
        ),
        (
            "src/other.rs".to_string(),
            "fn t(a: u32, b: u32) -> u32 {\n    a + b\n}\n".to_string(),
        ),
    ];
    let found = scan_crate("prize-check", &files, &[], &names());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("[app-arith] prize-check: src/other.rs:2:"));
}

#[test]
fn literal_list_parser_is_strict() {
    let h = |l: &str| parse_literals(&format!("{LITERALS_HEADER}\n{l}\n"));
    assert!(parse_literals("literal * \"a\" reason r\n").is_err());
    assert!(h("literal * \"a\"").is_err());
    assert!(h("literal * \"a\" reason").is_err());
    assert!(h("literal * a reason r").is_err());
    assert!(h("allow * \"a\" reason r").is_err());
    let ok = h("literal bracket-check \"the Target '{shown}' is {}\" reason because").unwrap();
    assert_eq!(ok[0].text, "the Target '{shown}' is {}");
    assert_eq!(ok[0].crate_name, "bracket-check");
    assert_eq!(ok[0].reason, "because");
}

#[test]
fn committed_literal_list_parses_with_a_reason_per_entry() {
    let text = include_str!("../../../tools/app-literals.v1");
    let entries = parse_literals(text).unwrap();
    assert!(!entries.is_empty());
    assert!(entries.iter().all(|e| !e.reason.is_empty()));
}

#[test]
fn rulebook_names_come_from_types_targets_and_rule_ids() {
    let text = r#"{"format":"ruletrace-rulebook","name":"top-level-name",
        "documents":[{"id":"rules.md"}],
        "types":{"inputs":[{"name":"goals","type":"number"}],
                 "collections":[{"name":"rounds","fields":[{"name":"points"}]}]},
        "rules":[{"id":"elig.basic","target":"eligible",
                  "expr":{"node":"field","name":"age"}}]}"#;
    let found = rulebook_names(text).unwrap();
    let expected: BTreeSet<String> = ["goals", "rounds", "points", "elig.basic", "eligible", "age"]
        .map(String::from)
        .into();
    assert_eq!(found, expected);
    assert!(rulebook_names("not json").is_err());
}

type Spec<'a> = &'a [(&'a str, &'a [&'a str])];

const EDGES: &str = "# format: ruletrace-allowed-edges v1
crate rt-foundation class core deps -
crate rt-engine class core deps rt-foundation
crate rt-explain class core deps rt-foundation
crate rt-cli class core deps rt-foundation rt-engine
crate rt-review class core deps rt-foundation
crate rt-ledger class core deps rt-foundation
crate prize-check class app deps rt-foundation rt-engine rt-explain
crate bracket-check class app deps rt-foundation rt-engine
forbid-transitive prize-check bracket-check -> rt-cli rt-review rt-ledger
forbid-name prize-check -> bracket-check
";

fn app_edge_findings(spec: Spec<'_>) -> Vec<String> {
    let list = edges::parse(EDGES).unwrap();
    let externals: BTreeSet<&str> = spec
        .iter()
        .flat_map(|(_, d)| d.iter().copied())
        .filter(|d| !spec.iter().any(|(n, _)| n == d))
        .collect();
    let mut packages: Vec<String> = externals
        .iter()
        .map(|n| format!("{{\"name\":\"{n}\",\"id\":\"id-{n}\",\"dependencies\":[]}}"))
        .collect();
    packages.extend(spec.iter().map(|(n, deps)| {
        let deps: Vec<String> = deps
            .iter()
            .map(|d| format!("{{\"name\":\"{d}\",\"kind\":null}}"))
            .collect();
        format!(
            "{{\"name\":\"{n}\",\"id\":\"id-{n}\",\"dependencies\":[{}]}}",
            deps.join(",")
        )
    }));
    let ids: Vec<String> = spec.iter().map(|(n, _)| format!("\"id-{n}\"")).collect();
    let nodes: Vec<String> = spec
        .iter()
        .map(|(n, deps)| {
            let d: Vec<String> = deps
                .iter()
                .map(|x| format!("{{\"pkg\":\"id-{x}\"}}"))
                .collect();
            format!("{{\"id\":\"id-{n}\",\"deps\":[{}]}}", d.join(","))
        })
        .collect();
    let json = format!(
        "{{\"packages\":[{}],\"workspace_members\":[{}],\"resolve\":{{\"nodes\":[{}]}}}}",
        packages.join(","),
        ids.join(","),
        nodes.join(",")
    );
    let md = parse_metadata(json.as_bytes()).unwrap();
    crate::graph::check_app_edges(&list, &md)
        .iter()
        .map(ToString::to_string)
        .collect()
}

const WORKSPACE: Spec<'static> = &[
    ("rt-foundation", &[]),
    ("rt-engine", &["rt-foundation"]),
    ("rt-explain", &["rt-foundation"]),
    ("rt-cli", &["rt-foundation", "rt-engine"]),
    ("rt-review", &["rt-foundation"]),
    ("rt-ledger", &["rt-foundation"]),
    ("prize-check", &["rt-foundation", "rt-engine", "rt-explain"]),
    ("bracket-check", &["rt-foundation", "rt-engine"]),
];

#[test]
fn conforming_application_edges_pass() {
    assert_eq!(app_edge_findings(WORKSPACE), Vec::<String>::new());
}

fn with(change: impl FnOnce(&mut Vec<(&'static str, Vec<&'static str>)>)) -> Vec<String> {
    let mut owned: Vec<(&'static str, Vec<&'static str>)> =
        WORKSPACE.iter().map(|(n, d)| (*n, d.to_vec())).collect();
    change(&mut owned);
    let spec: Vec<(&str, &[&str])> = owned.iter().map(|(n, d)| (*n, d.as_slice())).collect();
    app_edge_findings(&spec)
}

fn add_dep(owned: &mut [(&'static str, Vec<&'static str>)], from: &str, to: &'static str) {
    for (n, deps) in owned.iter_mut() {
        if *n == from {
            deps.push(to);
        }
    }
}

#[test]
fn direct_edge_to_the_cli_crate_fails() {
    let f = with(|w| add_dep(w, "prize-check", "rt-cli"));
    assert!(f
        .iter()
        .any(|l| l.starts_with("[edge-not-allowed]") && l.contains("prize-check -> rt-cli")));
    assert!(f.iter().any(|l| l.starts_with("[forbid-transitive]")));
}

#[test]
fn edge_to_the_other_application_fails() {
    let f = with(|w| add_dep(w, "prize-check", "bracket-check"));
    assert!(f
        .iter()
        .any(|l| l.starts_with("[forbid-name]") && l.contains("prize-check -> bracket-check")));
}

#[test]
fn transitive_edge_through_an_allowed_crate_fails() {
    let f = with(|w| add_dep(w, "rt-explain", "rt-review"));
    let hit = f.iter().any(|l| {
        l.starts_with("[app-transitive]") && l.contains("prize-check") && l.contains("rt-review")
    });
    assert!(hit, "{f:?}");
    assert!(f.iter().any(|l| l.starts_with("[forbid-transitive]")));
}

#[test]
fn dependency_outside_the_workspace_fails() {
    let f = with(|w| add_dep(w, "bracket-check", "serde"));
    assert!(f.iter().any(|l| l.starts_with("[app-external]")
        && l.contains("bracket-check")
        && l.contains("serde")));
}

// Review 9.8 F4: selecting the largest or smallest entry of an ordered
// collection is a selection over participants or scores.
#[test]
fn selecting_an_end_of_an_ordered_collection_fails_on_its_line() {
    for call in [
        "s.iter().last()",
        "s.iter().next_back()",
        "m.first_key_value()",
        "m.last_key_value()",
        "s.pop_first()",
        "s.pop_last()",
    ] {
        let body = format!("fn f() {{\n    let _ = {call};\n}}\n");
        let found = scan(&body);
        assert!(
            found
                .iter()
                .any(|l| l.starts_with("[app-order] prize-check: src/lib.rs:2:")),
            "{call}: {found:?}"
        );
    }
}

// Review 9.8 F5: a `Path` method that reads the file system is file access.
#[test]
fn a_path_method_that_reads_the_file_system_fails_on_its_line() {
    for call in [
        "p.exists()",
        "p.is_file()",
        "p.is_dir()",
        "p.metadata()",
        "p.read_dir()",
        "p.canonicalize()",
    ] {
        let body = format!("fn f(p: &Path) {{\n    let _ = {call};\n}}\n");
        let found = scan(&body);
        assert!(
            found
                .iter()
                .any(|l| l.starts_with("[app-io] prize-check: src/lib.rs:2:")),
            "{call}: {found:?}"
        );
    }
}

// A field or a free function of the same name is not a `Path` method call.
#[test]
fn a_field_named_like_a_path_method_is_not_file_access() {
    let found = scan("fn f(e: &E) -> bool {\n    e.exists\n}\n");
    assert!(
        !found.iter().any(|l| l.starts_with("[app-io]")),
        "{found:?}"
    );
}
