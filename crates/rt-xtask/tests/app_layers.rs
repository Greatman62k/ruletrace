//! `check-app-layers` and `check-executables` on the real workspace (positive
//! runs) and on a synthetic repository on disk (a negative run that reports
//! the crate, the file and the line).
//!
//! **Validates: Requirements 31.10, 24.12, 27.1** (the source-scan part of
//! 31.11 only; its clock, working-directory and locale test is task 9.17).

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use rt_xtask::{app_layers, executables, scan};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn cargo_metadata(root: &Path) -> Vec<u8> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--all-features"])
        .args(["--offline", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    output.stdout
}

#[test]
fn the_workspace_passes_the_source_rules() {
    let scanned = app_layers::run_sources(&repo_root()).unwrap();
    assert_eq!(scanned.crates, 2);
    assert!(scanned.files >= 12, "{scanned:?}");
    assert!(scanned.rulebooks >= 1, "{scanned:?}");
}

#[test]
fn the_workspace_passes_the_edge_and_executable_rules() {
    let root = repo_root();
    let metadata = cargo_metadata(&root);
    assert_eq!(app_layers::run_edges(&root, &metadata).unwrap(), 2);
    let list = scan::load_edges(&root).unwrap();
    assert_eq!(executables::run(&root, &list, &metadata).unwrap(), 2);
}

struct TempRepo(PathBuf);

impl TempRepo {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("rt-xtask-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Self(dir)
    }

    fn write(&self, path: &str, text: &str) {
        let full = self.0.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn synthetic(name: &str, lib: &str) -> TempRepo {
    let repo = TempRepo::new(name);
    repo.write(
        "tools/allowed-edges.v1",
        "# format: ruletrace-allowed-edges v1\ncrate prize-check class app deps -\n",
    );
    repo.write(
        "tools/app-literals.v1",
        "# format: ruletrace-app-literals v1\nliteral prize-check \"--score\" reason option name\n",
    );
    repo.write(
        "tests/fixtures/x/rulebook.json",
        r#"{"format":"ruletrace-rulebook","types":{"inputs":[{"name":"goals"}]},"rules":[]}"#,
    );
    repo.write("crates/prize-check/src/lib.rs", lib);
    repo
}

#[test]
fn a_clean_synthetic_crate_passes() {
    let repo = synthetic(
        "a_clean_synthetic_crate_passes",
        "mod inner;\nfn name() -> &'static str {\n    \"--score\"\n}\n",
    );
    repo.write("crates/prize-check/src/inner.rs", "pub fn f() {}\n");
    let scanned = app_layers::run_sources(&repo.0).unwrap();
    assert_eq!(
        (scanned.crates, scanned.files, scanned.rulebooks),
        (1, 2, 1)
    );
}

#[test]
fn a_violation_in_a_nested_file_names_crate_file_and_line() {
    let repo = synthetic(
        "a_violation_in_a_nested_file_names_crate_file_and_line",
        "mod inner;\n",
    );
    repo.write(
        "crates/prize-check/src/inner.rs",
        "pub fn f(a: u32, b: u32) -> u32 {\n    let weight = 3;\n    a + b\n}\n",
    );
    let message = app_layers::run_sources(&repo.0).unwrap_err();
    let lines: Vec<&str> = message.lines().collect();
    assert_eq!(lines.len(), 2, "{message}");
    assert!(lines[0].starts_with("[app-literal] prize-check: src/inner.rs:2: "));
    assert!(lines[1].starts_with("[app-arith] prize-check: src/inner.rs:3: "));
}

#[test]
fn a_field_name_in_the_literal_list_is_refused() {
    let repo = synthetic("a_field_name_in_the_literal_list_is_refused", "fn f() {}\n");
    repo.write(
        "tools/app-literals.v1",
        "# format: ruletrace-app-literals v1\nliteral prize-check \"goals\" reason hiding it\n",
    );
    let message = app_layers::run_sources(&repo.0).unwrap_err();
    assert!(message.contains("goals") && message.contains("declared by a committed rulebook"));
}

#[test]
fn without_a_committed_rulebook_the_check_refuses_to_run() {
    let repo = synthetic(
        "without_a_committed_rulebook_the_check_refuses_to_run",
        "fn f() {}\n",
    );
    fs::remove_dir_all(repo.0.join("tests")).unwrap();
    let message = app_layers::run_sources(&repo.0).unwrap_err();
    assert!(message.starts_with("[app-names]"), "{message}");
}
