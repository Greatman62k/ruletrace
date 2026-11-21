//! The `app-api` rule on the real workspace and on its real allow-list.
//!
//! **Validates: Requirements 31.10, 31.11** (source-scan part).

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use rt_xtask::app_api::{check_api, parse_allow, run_api, LIST_PATH};
use rt_xtask::{app_layers, scan};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn layer_names() -> (Vec<String>, std::collections::BTreeSet<String>) {
    let root = repo_root();
    let list = scan::load_edges(&root).unwrap();
    let layers = app_layers::layers(&root, &list);
    (layers, list.crates.keys().cloned().collect())
}

#[test]
fn the_workspace_passes_the_app_api_rule() {
    let (layers, workspace) = layer_names();
    let scanned = run_api(&repo_root(), LIST_PATH, &layers, &workspace).unwrap();
    assert_eq!(scanned.crates, 2);
    assert!(scanned.entries > 50, "{scanned:?}");
}

#[test]
fn removing_the_engine_call_from_the_real_list_fails_at_the_call_site() {
    let (layers, workspace) = layer_names();
    let root = repo_root();
    let text = fs::read_to_string(root.join(LIST_PATH)).unwrap();
    let kept: String = text
        .lines()
        .filter(|l| !l.starts_with("allow prize-check rt_engine::evaluate "))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_ne!(kept, text);
    let entries = parse_allow(&kept, LIST_PATH).unwrap();
    let mut crates = BTreeMap::new();
    for name in &layers {
        let files = scan::read_crate_files(&root.join("crates").join(name)).unwrap();
        crates.insert(name.clone(), files);
    }
    let found = check_api(&crates, &workspace, &entries, LIST_PATH);
    assert_eq!(
        found,
        vec![format!(
            "[app-api] prize-check: src/evaluate.rs:93: external item `rt_engine::evaluate` is not listed in {LIST_PATH}"
        )]
    );
}
