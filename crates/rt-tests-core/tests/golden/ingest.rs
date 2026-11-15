//! The `ruletrace ingest` golden test: the real binary, a scrubbed environment,
//! byte comparison.

use crate::harness::{compare_bytes, fixtures_root, platform};
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

/// Builds `ruletrace` once per test process and returns its path. The
/// binary lives in the same profile directory as this test executable.
/// Building once keeps parallel tests from running nested `cargo build`
/// commands that replace the binary while another test launches it.
pub fn ruletrace_binary() -> PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
            let status = Command::new(cargo)
                .args(["build", "--quiet", "-p", "rt-cli", "--bin", "ruletrace"])
                .status()
                .unwrap();
            assert!(status.success(), "building ruletrace failed");
            let exe = std::env::current_exe().unwrap();
            let profile_dir = exe.parent().unwrap().parent().unwrap();
            profile_dir.join(format!("ruletrace{}", std::env::consts::EXE_SUFFIX))
        })
        .clone()
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

/// Launchability smoke check for the macOS `NotFound` flake. It is not a
/// demonstrated regression test: it also passes on the old code on Linux.
/// The once-per-process build is an unverified mitigation until a macOS CI
/// run is green.
#[test]
fn the_binary_stays_launchable_while_many_threads_ask_for_it() {
    let launches: Vec<_> = (0..24)
        .map(|_| {
            std::thread::spawn(|| {
                let binary = ruletrace_binary();
                Command::new(binary).env_clear().arg("--help").output()
            })
        })
        .collect();
    for launch in launches {
        let result = launch.join().unwrap();
        assert!(
            result.is_ok(),
            "launching ruletrace failed: {:?}",
            result.err()
        );
    }
}
