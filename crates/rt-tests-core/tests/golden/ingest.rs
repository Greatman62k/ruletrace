//! The `ruletrace ingest` golden test: the real binary, a scrubbed environment,
//! byte comparison.

use crate::harness::{compare_bytes, fixtures_root, platform};
use std::path::PathBuf;
use std::process::Command;

/// Builds `ruletrace` and returns its path. The binary lives in the same
/// profile directory as this test executable.
pub fn ruletrace_binary() -> PathBuf {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(["build", "--quiet", "-p", "rt-cli", "--bin", "ruletrace"])
        .status()
        .unwrap();
    assert!(status.success(), "building ruletrace failed");
    let exe = std::env::current_exe().unwrap();
    let profile_dir = exe.parent().unwrap().parent().unwrap();
    profile_dir.join(format!("ruletrace{}", std::env::consts::EXE_SUFFIX))
}

#[test]
fn golden_slice_basic_ingest_source_model() {
    let fixture = fixtures_root().join("slice-basic");
    let out_dir = std::env::temp_dir().join(format!("rt-golden-ingest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).unwrap();
    let out = out_dir.join("source-model.json");

    let output = Command::new(ruletrace_binary())
        .env_clear()
        .current_dir(&fixture)
        .args([
            "ingest",
            "--document",
            "source/rules.md",
            "--document-id",
            "rules.md",
            "--out",
        ])
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ingest failed on {}: {}",
        platform(),
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = std::fs::read(&out).unwrap();
    let result = compare_bytes(&fixture.join("expected/source-model.json"), &actual);
    std::fs::remove_dir_all(&out_dir).unwrap();
    if let Err(msg) = result {
        panic!("{msg}");
    }
}

#[test]
fn golden_slice_basic_source_is_the_worked_example() {
    let bytes = std::fs::read(fixtures_root().join("slice-basic/source/rules.md")).unwrap();
    assert_eq!(bytes.len(), 171);
}
