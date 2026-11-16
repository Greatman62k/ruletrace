//! The subprocess runner and the runbook driver (design §8.11.12 steps 3 and
//! 4).

use crate::error::{DemoError, Refusal};
use crate::golden::{check_golden, Mode};
use crate::guard::{authorize_live, classify, Class};
use crate::manifest::Manifest;
use crate::runbook::{Runbook, Step};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The captured result of one process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Captured {
    /// The exit code.
    pub exit: i32,
    /// Standard output bytes.
    pub stdout: Vec<u8>,
    /// Standard error bytes.
    pub stderr: Vec<u8>,
}

fn io_error(path: &Path, e: &std::io::Error) -> DemoError {
    DemoError::Io {
        path: path.display().to_string(),
        problem: e.to_string(),
    }
}

/// Maps a logical executable name to the built binary in `bin_dir` (the value
/// of `RT_BIN_DIR`).
#[must_use]
pub fn binary_path(bin_dir: &Path, logical: &str) -> PathBuf {
    bin_dir.join(format!("{logical}{}", std::env::consts::EXE_SUFFIX))
}

/// Runs one offline step in `work_dir` with an empty environment and no
/// standard input.
///
/// The guard classifies the step first. A `ruletrace-ai` step whose
/// configuration is not provably the `mock-replay` provider is live, and this
/// function refuses it before any process starts. Live steps run only through
/// [`run_live_step`].
///
/// # Errors
/// [`DemoError::LiveRefused`] for a live step, [`DemoError::Step`] when the
/// process cannot start or ends without an exit code.
pub fn run_step(step: &Step, bin_dir: &Path, work_dir: &Path) -> Result<Captured, DemoError> {
    if classify(step, work_dir) == Class::Live {
        return Err(DemoError::LiveRefused {
            step: step.id.clone(),
            refusal: Refusal::LiveStepInOfflineRunner,
        });
    }
    spawn(step, bin_dir, work_dir, &[])
}

/// What the live runner needs besides the step.
#[derive(Clone, Copy, Debug)]
pub struct LiveContext<'a> {
    /// The checked manifest of the demonstration.
    pub manifest: &'a Manifest,
    /// The committed demonstration directory, with its `golden/` and
    /// `provider/`.
    pub demo_dir: &'a Path,
    /// Environment variables passed to the process, supplied by the caller
    /// because the harness reads no environment. Only the credential variable
    /// of the step's configuration is accepted.
    pub environment: &'a [(String, String)],
}

/// Runs one live step, the explicit opt-in of the live flow (Req 33.10).
///
/// [`authorize_live`] decides first. Nothing is started, written or read from
/// the environment when it refuses. The output is never compared with a
/// golden file.
///
/// # Errors
/// The refusal of [`authorize_live`], [`DemoError::LiveRefused`] for an
/// environment variable that is not the credential variable, or
/// [`DemoError::Step`] when the process cannot start.
pub fn run_live_step(
    step: &Step,
    context: &LiveContext<'_>,
    bin_dir: &Path,
    work_dir: &Path,
) -> Result<Captured, DemoError> {
    let allowed = authorize_live(step, context.manifest, context.demo_dir, work_dir)?;
    for (name, _) in context.environment {
        if allowed.credential_env_var.as_deref() != Some(name.as_str()) {
            return Err(DemoError::LiveRefused {
                step: step.id.clone(),
                refusal: Refusal::EnvironmentNotAllowed(name.clone()),
            });
        }
    }
    spawn(step, bin_dir, work_dir, context.environment)
}

fn spawn(
    step: &Step,
    bin_dir: &Path,
    work_dir: &Path,
    environment: &[(String, String)],
) -> Result<Captured, DemoError> {
    let fail = |problem: String| DemoError::Step {
        step: step.id.clone(),
        problem,
    };
    let program = step
        .argv
        .first()
        .ok_or_else(|| fail("argv is empty".to_owned()))?;
    let binary = binary_path(bin_dir, program);
    let output = Command::new(&binary)
        .args(step.argv.iter().skip(1))
        .current_dir(work_dir.join(&step.cwd))
        .env_clear()
        .envs(environment.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| fail(format!("cannot run '{}': {e}", binary.display())))?;
    let exit = output
        .status
        .code()
        .ok_or_else(|| fail("the process ended without an exit code".to_owned()))?;
    Ok(Captured {
        exit,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

/// What one step of a runbook printed, kept for the demonstration summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepRecord {
    /// The step identifier.
    pub id: String,
    /// What the process printed and its exit code.
    pub captured: Captured,
}

/// Makes a copied AI_Configuration readable by its owner only.
///
/// A checkout gives the file the mode of the user's umask, and `ruletrace-ai`
/// warns with RT-AI-0009 about a group or other read bit, so without this the
/// golden standard error would depend on the umask. Platforms without
/// permission bits report no such warning and need no change.
#[cfg(unix)]
fn make_private(path: &Path) -> Result<(), DemoError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| io_error(path, &e))
}

#[cfg(not(unix))]
fn make_private(_path: &Path) -> Result<(), DemoError> {
    Ok(())
}

/// Copies the demonstration into `work_dir`, leaving out the `golden`
/// directory.
///
/// # Errors
/// [`DemoError::Io`] on the first failed file operation.
pub fn copy_demo(demo_dir: &Path, work_dir: &Path) -> Result<(), DemoError> {
    fs::create_dir_all(work_dir).map_err(|e| io_error(work_dir, &e))?;
    let mut entries: Vec<_> = fs::read_dir(demo_dir)
        .map_err(|e| io_error(demo_dir, &e))?
        .collect::<Result<_, _>>()
        .map_err(|e| io_error(demo_dir, &e))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name();
        let from = entry.path();
        let to = work_dir.join(&name);
        let kind = entry.file_type().map_err(|e| io_error(&from, &e))?;
        if kind.is_dir() {
            if name != "golden" {
                copy_demo(&from, &to)?;
            }
        } else if kind.is_file() {
            fs::copy(&from, &to).map_err(|e| io_error(&from, &e))?;
            if to.extension().is_some_and(|e| e == "toml") {
                make_private(&to)?;
            }
        }
    }
    Ok(())
}

/// Runs the steps of `profile` in order and compares each with its golden
/// files. Stops at the first failure. See [`run_runbook_recorded`].
///
/// # Errors
/// The first failure, as for [`run_runbook_recorded`].
pub fn run_runbook(
    runbook: &Runbook,
    profile: &str,
    demo_dir: &Path,
    work_dir: &Path,
    bin_dir: &Path,
    mode: Mode,
) -> Result<(), DemoError> {
    run_runbook_recorded(runbook, profile, demo_dir, work_dir, bin_dir, mode).map(|_| ())
}

/// Runs the steps of `profile` in order and compares each with its golden
/// files. Stops at the first failure. Returns what each step printed.
///
/// Per step the order is: exit code, standard output, standard error, then each
/// listed output file. `demo_dir` holds the committed demonstration and its
/// golden files. `work_dir` receives a copy and must not exist or must be
/// empty.
///
/// # Errors
/// The first failure: [`DemoError::Step`] for a wrong exit code or a missing
/// output file, [`DemoError::Golden`] for a difference.
pub fn run_runbook_recorded(
    runbook: &Runbook,
    profile: &str,
    demo_dir: &Path,
    work_dir: &Path,
    bin_dir: &Path,
    mode: Mode,
) -> Result<Vec<StepRecord>, DemoError> {
    let steps = runbook.steps_for(profile)?;
    copy_demo(demo_dir, work_dir)?;
    let mut records = Vec::new();
    for step in steps {
        let captured = run_step(step, bin_dir, work_dir)?;
        if captured.exit != step.expect_exit {
            return Err(DemoError::Step {
                step: step.id.clone(),
                problem: format!(
                    "exit code {} where {} was declared",
                    captured.exit, step.expect_exit
                ),
            });
        }
        check_golden(
            mode,
            &step.id,
            demo_dir,
            &step.stdout_golden,
            &captured.stdout,
        )?;
        check_golden(
            mode,
            &step.id,
            demo_dir,
            &step.stderr_golden,
            &captured.stderr,
        )?;
        for output in &step.outputs {
            let path = work_dir.join(&output.path);
            let bytes = fs::read(&path).map_err(|e| DemoError::Step {
                step: step.id.clone(),
                problem: format!("output file '{}' cannot be read: {e}", output.path),
            })?;
            check_golden(mode, &step.id, demo_dir, &output.golden, &bytes)?;
        }
        records.push(StepRecord {
            id: step.id.clone(),
            captured,
        });
    }
    Ok(records)
}
