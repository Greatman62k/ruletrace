//! The `ruletrace` executable on the real file system: `review list` before
//! the first approval creates and changes nothing, and the revision it shows
//! is the one `review approve --create-ledger` then records (Req 26.6).
//!
//! The fixture under `tests/fixtures/review_list_missing` is the one of the
//! in-process tests: Rulebook rule `d.a` revision 1 and a store with one
//! pending proposal of the new rule `d.b`.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PROPOSAL: &str = "5bfe10d8f8b876a6779d29e5a175bdfb25976b41d5b2656aafdd905ed4b303dd";
const HASH: &str = "ce9b83fee1e5a1768565bf77a243c18fc79f0157a41dd7b943962baf282e8f5a";

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Every path under `root`, relative, with its bytes (`None` for a directory).
fn snapshot(root: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    fn walk(dir: &Path, root: &Path, into: &mut BTreeMap<String, Option<Vec<u8>>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.strip_prefix(root).unwrap().display().to_string();
            if path.is_dir() {
                into.insert(name, None);
                walk(&path, root, into);
            } else {
                into.insert(name, Some(std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut into = BTreeMap::new();
    walk(root, root, &mut into);
    into
}

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rt-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/review_list_missing");
    copy_tree(&fixture, &root);
    root
}

fn ruletrace(root: &Path, ledger: &Path, verb_args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ruletrace"))
        .current_dir(root)
        .args(["review"])
        .args(verb_args)
        .args(["--rulebook", "rb.json", "--ledger"])
        .arg(ledger)
        .args(["--source-model", "model.json", "--proposals", "store"])
        .output()
        .unwrap()
}

#[test]
fn list_before_the_first_approval_writes_nothing_and_approve_gives_the_same_revision() {
    let root = scratch("list-missing");
    // One ledger path with an existing parent, one with a missing parent.
    for ledger in [
        PathBuf::from("ledger.jsonl"),
        PathBuf::from("not-yet/l.jsonl"),
    ] {
        let before = snapshot(&root);
        let out = ruletrace(&root, &ledger, &["list"]);
        let after = snapshot(&root);
        let stdout = String::from_utf8(out.stdout).unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        assert!(
            stdout.contains(&format!(
                "pending proposal '{PROPOSAL}' for rule 'd.b' prospective revision 1\n"
            )),
            "{stdout}"
        );
        assert!(after == before, "the tree changed for {ledger:?}");
        assert!(!root.join(&ledger).exists());
    }
    // Without --create-ledger a decision on a missing ledger is still refused.
    // The refusal is unchanged. Approve and reject take the exclusive lock
    // first, which creates the empty lock file, so no tree equality is
    // asserted for them.
    let refused = ruletrace(&root, Path::new("ledger.jsonl"), &decide());
    assert_eq!(refused.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("RT-CLI-0003"));
    assert!(!root.join("ledger.jsonl").exists());
    // With it, the revision is the one that list displayed.
    let mut args = decide();
    args.push("--create-ledger");
    let approved = ruletrace(&root, Path::new("ledger.jsonl"), &args);
    let said = String::from_utf8(approved.stdout).unwrap();
    assert_eq!(approved.status.code(), Some(0), "{said}");
    assert!(
        said.starts_with("approved rule 'd.b' revision 1 (ledger record "),
        "{said}"
    );
    assert!(root.join("ledger.jsonl").exists());
    let _ = std::fs::remove_dir_all(&root);
}

fn decide() -> Vec<&'static str> {
    vec![
        "approve",
        "--rule",
        "d.b",
        "--revision",
        "1",
        "--content-hash",
        HASH,
        "--reviewer",
        "A. Reviewer",
        "--time",
        "2026-11-14T09:00:00Z",
    ]
}
