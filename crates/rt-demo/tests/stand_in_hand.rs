//! Runs the stand-in demonstration, `hand` profile, against its golden files (task 6.6).
//!
//! The binaries come from `RT_BIN_DIR` or, when it is unset, from a `cargo build` of `rt-cli`
//! into the profile directory of this test. Bless with `RT_DEMO_BLESS=1` (refused when `CI` is set).

use rt_demo::{check_manifest, parse_runbook, run_runbook, Mode};
use std::path::{Path, PathBuf};
use std::process::Command;

fn demo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo/stand-in-hand")
}

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn bin_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(dir) = std::env::var_os("RT_BIN_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(["build", "--quiet", "-p", "rt-cli", "--bin", "ruletrace"])
        .status()?;
    assert!(status.success(), "building ruletrace failed");
    let exe = std::env::current_exe()?;
    let profile_dir = exe
        .parent()
        .and_then(Path::parent)
        .ok_or("no profile directory")?;
    Ok(profile_dir.to_owned())
}

#[test]
fn stand_in_manifest_is_complete_and_labelled() -> TestResult {
    let dir = demo_dir();
    let bytes = std::fs::read(dir.join("manifest.json"))?;
    let manifest = check_manifest(&bytes, &|p| std::fs::read(dir.join(p)).ok())?;
    assert_eq!(manifest.name, "stand-in-hand");
    assert!(String::from_utf8(bytes)?.contains("\"stand_in\": true"));
    Ok(())
}

#[test]
fn stand_in_hand_profile_matches_its_golden_files() -> TestResult {
    let dir = demo_dir();
    let runbook = parse_runbook(&std::fs::read_to_string(dir.join("RUNBOOK.md"))?)?;
    let bless = std::env::var("RT_DEMO_BLESS").ok();
    let ci = std::env::var("CI").ok();
    let mode = Mode::from_env_values(bless.as_deref(), ci.as_deref())?;
    let work = std::env::temp_dir().join(format!("rt-demo-stand-in-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let result = run_runbook(&runbook, "hand", &dir, &work, &bin_dir()?, mode);
    let _ = std::fs::remove_dir_all(&work);
    Ok(result?)
}
