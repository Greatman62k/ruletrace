//! Runs the stand-in demonstration, `hand` profile, against its golden files.
//!
//! The binaries come from `RT_BIN_DIR` or, when it is unset, from a
//! `cargo build` of `rt-cli` into the profile directory of this test. Bless
//! with `RT_DEMO_BLESS=1` (refused when `CI` is set).

use rt_demo::{check_manifest, parse_runbook, run_runbook, Mode};
use std::path::{Path, PathBuf};
use std::process::Command;

fn demo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo/stand-in-hand")
}

type TestResult = Result<(), Box<dyn std::error::Error>>;

// Parallel tests of one binary each ran a nested `cargo build`, which can
// relink the executable while another test starts it (macOS NotFound in run
// 43892679845). The build runs once per test process.
fn bin_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    static BUILT: std::sync::OnceLock<Result<PathBuf, String>> = std::sync::OnceLock::new();
    BUILT
        .get_or_init(|| build_bin_dir().map_err(|e| e.to_string()))
        .clone()
        .map_err(Into::into)
}

static BUILD_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn build_bin_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    BUILD_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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

// However many tests of this binary ask for the executables, the guard
// keeps the nested build to one call per test process.
#[test]
fn the_executables_are_built_once_per_test_process() -> TestResult {
    let first = bin_dir()?;
    let second = bin_dir()?;
    assert_eq!(first, second);
    assert_eq!(BUILD_CALLS.load(std::sync::atomic::Ordering::SeqCst), 1);
    Ok(())
}
