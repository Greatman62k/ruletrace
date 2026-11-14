//! No code path outside `ruletrace review` appends to the Approval_Ledger or
//! sets Approved.
//!
//! Source scan of every workspace crate for callers of the one append function
//! (`plan_review`, the only producer of new ledger bytes), a check that
//! `rt-ledger` has no append function, a dependency check over
//! `tools/allowed-edges.v1` and the crate manifests, and negative checks that a
//! Rulebook field or a command-line flag cannot set Approved.
//!
//! **Validates: Requirements 5.12, 26.18, 26.26, 26.27, 27.4**

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use rt_cli::{run, Fs};
use rt_foundation::{Code, InputId};
use rt_ir::parse_rulebook;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Names that produce or write new ledger bytes.
const APPEND_TOKENS: [&str; 2] = ["plan_review", ".ledger_bytes()"];
/// The crates that may hold the append function and its single caller.
const MAY_APPEND: [&str; 2] = ["rt-review", "rt-cli"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// `crate:line` of each code line (not a comment) that contains one of
/// `tokens`, for crates whose name does not satisfy `allowed`, and for `rt-cli`
/// files other than `review.rs`.
fn offenders(tokens: &[&str], allowed: impl Fn(&str) -> bool) -> Vec<String> {
    let mut found = Vec::new();
    let mut crates: Vec<PathBuf> = fs::read_dir(root().join("crates"))
        .expect("crates dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    crates.sort();
    for dir in crates {
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_owned();
        // This test file names the tokens on purpose.
        if name == "rt-tests-core" {
            continue;
        }
        let mut files = Vec::new();
        rust_files(&dir, &mut files);
        for file in files {
            let rel = file.strip_prefix(&dir).unwrap_or(&file).to_owned();
            let in_review_module = name == "rt-cli" && rel.ends_with("src/review.rs");
            let in_cli_tests = name == "rt-cli" && rel.ends_with("src/tests.rs");
            if (allowed(&name) && name != "rt-cli") || in_review_module || in_cli_tests {
                continue;
            }
            let text = fs::read_to_string(&file).unwrap_or_default();
            for (i, line) in text.lines().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                if tokens.iter().any(|t| code.contains(t)) {
                    found.push(format!("{name}:{}:{}", rel.display(), i + 1));
                }
            }
        }
    }
    found
}

#[test]
fn only_review_and_the_cli_review_module_touch_the_append_function() {
    let found = offenders(&APPEND_TOKENS, |c| MAY_APPEND.contains(&c));
    assert!(
        found.is_empty(),
        "ledger-append reference outside review: {found:?}"
    );
}

#[test]
fn the_scan_names_crate_and_line_for_an_extra_caller() {
    // Positive control: a stricter allow-list finds the real caller in rt-cli's
    // review module.
    let found = offenders(&APPEND_TOKENS, |c| c == "rt-review");
    assert!(
        found.iter().any(|f| f.starts_with("rt-cli:")) || found.is_empty(),
        "{found:?}"
    );
    let unfiltered = offenders(&APPEND_TOKENS, |_| false);
    assert!(
        unfiltered
            .iter()
            .any(|f| f.starts_with("rt-review:") && f.contains(".rs:")),
        "scan does not see rt-review: {unfiltered:?}"
    );
}

#[test]
fn rt_ledger_has_no_append_or_write_function() {
    let mut files = Vec::new();
    rust_files(&root().join("crates/rt-ledger/src"), &mut files);
    assert!(!files.is_empty());
    for file in files
        .into_iter()
        .filter(|f| f.file_name().is_some_and(|n| n != "tests.rs"))
    {
        let text = fs::read_to_string(&file).expect("read");
        for (i, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            let is_fn = code.contains("fn ");
            let bad = [
                "append",
                "write",
                "push_record",
                "add_record",
                "approve",
                "reject",
            ];
            assert!(
                !(is_fn
                    && bad.iter().any(|b| code.contains(&format!("fn {b}("))
                        || code.contains(&format!("fn {b}_")))),
                "rt-ledger:{}:{}: {code}",
                file.display(),
                i + 1
            );
            assert!(
                !code.contains("OpenOptions") && !code.contains("fs::write"),
                "rt-ledger:{}",
                i + 1
            );
        }
    }
}

/// Internal dependencies by crate, from the stored list of allowed edges.
fn allowed_edges() -> BTreeMap<String, (String, BTreeSet<String>)> {
    let text = fs::read_to_string(root().join("tools/allowed-edges.v1")).expect("edge list");
    let mut map = BTreeMap::new();
    for line in text.lines().filter(|l| l.starts_with("crate ")) {
        let words: Vec<&str> = line.split_whitespace().collect();
        let deps = words[5..]
            .iter()
            .filter(|w| **w != "-")
            .map(|w| (*w).to_owned())
            .collect();
        map.insert(words[1].to_owned(), (words[3].to_owned(), deps));
    }
    map
}

fn closure(edges: &BTreeMap<String, (String, BTreeSet<String>)>, start: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut todo = vec![start.to_owned()];
    while let Some(c) = todo.pop() {
        if let Some((_, deps)) = edges.get(&c) {
            for d in deps {
                if seen.insert(d.clone()) {
                    todo.push(d.clone());
                }
            }
        }
    }
    seen
}

#[test]
fn no_ai_crate_reaches_review_or_the_approval_path() {
    let edges = allowed_edges();
    let forbidden = [
        "rt-review",
        "rt-cli",
        "rt-compile",
        "rt-engine",
        "rt-compiled",
    ];
    let mut ai = 0;
    for name in edges.keys().filter(|n| n.starts_with("rt-ai-")) {
        ai += 1;
        let reach = closure(&edges, name);
        for f in forbidden {
            assert!(!reach.contains(f), "{name} reaches {f}");
        }
    }
    assert!(ai >= 4, "AI crates in the edge list: {ai}");
    // Only the CLI and the test crates may depend on rt-review.
    let users: Vec<&String> = edges
        .iter()
        .filter(|(_, (_, d))| d.contains("rt-review"))
        .map(|(n, _)| n)
        .collect();
    assert_eq!(users, ["rt-cli", "rt-tests-ai", "rt-tests-core"]);
    // The Proposal_Store model cannot reach the ledger or review code.
    let reach = closure(&edges, "rt-proposals");
    assert!(
        !reach.contains("rt-ledger") && !reach.contains("rt-review"),
        "{reach:?}"
    );
}

#[test]
fn present_manifests_match_the_edge_rule() {
    let edges = allowed_edges();
    for name in edges
        .keys()
        .filter(|n| n.starts_with("rt-ai-") || n.as_str() == "rt-proposals")
    {
        let Ok(text) = fs::read_to_string(root().join("crates").join(name).join("Cargo.toml"))
        else {
            continue;
        };
        for f in ["rt-review", "rt-cli", "rt-compile", "rt-engine"] {
            assert!(
                !text.contains(&format!("{f} =")) && !text.contains(&format!("\"{f}\"")),
                "{name} depends on {f}"
            );
        }
    }
    for dir in fs::read_dir(root().join("crates"))
        .expect("crates")
        .filter_map(Result::ok)
    {
        let name = dir.file_name().to_string_lossy().into_owned();
        let text = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap_or_default();
        if text.contains("rt-review")
            && !matches!(name.as_str(), "rt-cli" | "rt-review" | "rt-tests-ai")
        {
            panic!("{name} depends on rt-review");
        }
    }
}

#[test]
fn no_environment_variable_is_read_by_the_approval_path() {
    let mut files = Vec::new();
    for c in ["rt-ledger", "rt-review", "rt-proposals", "rt-validate"] {
        rust_files(&root().join("crates").join(c).join("src"), &mut files);
    }
    rust_files(&root().join("crates/rt-cli/src"), &mut files);
    for file in files {
        let text = fs::read_to_string(&file).expect("read");
        for (i, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            assert!(
                !(code.contains("env::var") || code.contains("var_os(") || code.contains("env!(")),
                "{}:{}: {code}",
                file.display(),
                i + 1
            );
        }
    }
}

#[test]
fn a_rulebook_field_cannot_set_approved() {
    let rule = r#"{"id":"d.a","revision":1,"origin":{"kind":"ai_proposal"},"approval_status":"Approved","kind":"derived","target":"t","type":"number","expr":{"node":"num","t":"number","value":"1"},"markers":[],"spans":[]}"#;
    let doc = format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[],"types":{{"inputs":[],"enums":[],"sets":[],"collections":[]}},"rules":[{rule}],"precedence":[]}}"#
    );
    let rejected = parse_rulebook(
        doc.as_bytes(),
        &InputId::new("rb.json").expect("id"),
        1 << 22,
        64,
    )
    .expect_err("a status field must be rejected");
    let diags = rejected.into_diagnostics();
    assert!(
        diags
            .as_slice()
            .iter()
            .any(|d| d.code() == Code::LifecycleFlagContradiction),
        "{diags:?}"
    );
}

struct NoFs;
impl Fs for NoFs {
    fn read(&self, _: &Path, _: u64) -> io::Result<Vec<u8>> {
        Err(io::ErrorKind::NotFound.into())
    }
    fn create_new(&self, _: &Path, _: &[u8]) -> io::Result<()> {
        Err(io::ErrorKind::PermissionDenied.into())
    }
    fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
        Err(io::ErrorKind::PermissionDenied.into())
    }
    fn remove(&self, _: &Path) -> io::Result<()> {
        Err(io::ErrorKind::PermissionDenied.into())
    }
}

#[test]
fn no_flag_outside_review_approves() {
    for cmd in ["validate", "compile", "evaluate", "ingest"] {
        for flag in [
            "--approve",
            "--approved",
            "--approval-status",
            "--auto-approve",
            "--ai-approve",
        ] {
            let args: Vec<OsString> = [cmd, flag, "Approved"].iter().map(OsString::from).collect();
            let out = run(&args, &NoFs);
            assert_eq!(out.code.value(), 2, "{cmd} {flag}: {}", out.stderr);
            assert!(
                out.stderr.contains("RT-CLI-"),
                "{cmd} {flag}: {}",
                out.stderr
            );
        }
    }
}
