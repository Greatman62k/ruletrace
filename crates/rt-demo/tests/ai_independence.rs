//! Property 41: `ruletrace` does not depend on the AI tool (Req 17.8, 18.1,
//! 20.16, 27.9).
//!
//! The three conditions of Req 27.9 give an exhaustive 2x2x2 matrix: the
//! `ruletrace-ai` executable beside `ruletrace` and on `PATH`, an
//! AI_Configuration file in the working directory and the home directory, and
//! a canary value in the credential variables. This is not a generated
//! property: the whole domain is eight cells, so every cell runs and no case
//! count applies.
//!
//! Each cell runs the whole `hand` runbook of `demo/stand-in-hand` and four
//! failing commands. Every step is compared byte for byte with the committed
//! golden files, which were not produced by this test, and every cell is
//! compared with the first cell. Because the golden files are fixed, the
//! `core-only` CI job, which runs this test against a build without the AI
//! crates, shows that both builds produce the same bytes (Req 20.16).
//!
//! When `ruletrace-ai` is not built (core-only build) the "present" cells use
//! an inert file of that name. `ruletrace` must never start it (Req 27.6), so
//! the cells still show that its presence changes nothing; the test prints
//! which kind of file it used.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

use rt_demo::{binary_path, copy_demo, parse_runbook, Step};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const CANARY: &str = "rt-canary-credential-7f3a91c2-not-a-secret";
const CREDENTIAL_VARIABLES: [&str; 3] = ["OPENAI_API_KEY", "RULETRACE_AI_KEY", "RT_CANARY_KEY"];
const PLANTED_CONFIG: &str = "ai.toml";
const AI_CONFIGURATION: &str = "# canary AI_Configuration\nschema_version = 1\nenabled = true\n\
provider = \"openai\"\nmodel = \"canary-model\"\nendpoint = \"https://127.0.0.1:1/v1\"\n\
credential_env = \"OPENAI_API_KEY\"\nallow_document_text = true\n";

#[derive(Clone, Copy, Debug)]
struct Cell {
    tool: bool,
    config: bool,
    credential: bool,
}

impl Cell {
    fn label(self) -> String {
        format!(
            "tool={} config={} credential={}",
            u8::from(self.tool),
            u8::from(self.config),
            u8::from(self.credential)
        )
    }
}

fn cells() -> Vec<Cell> {
    let mut all = Vec::new();
    for tool in [false, true] {
        for config in [false, true] {
            for credential in [false, true] {
                all.push(Cell {
                    tool,
                    config,
                    credential,
                });
            }
        }
    }
    all
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Run {
    exit: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
struct CellResult {
    runs: Vec<(String, Run)>,
    files: BTreeMap<String, Vec<u8>>,
}

fn demo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("demo")
        .join("stand-in-hand")
}

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

/// Removes its directory when dropped, also after a failed assertion.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("rt-demo-indep-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Copies `ruletrace` alone into `dir`, so that a sibling `ruletrace-ai` of
/// the build directory cannot be found next to it. Returns whether the AI
/// file placed beside it is the real executable.
fn prepare_bin(source: &Path, dir: &Path, with_tool: bool) -> bool {
    fs::create_dir_all(dir).unwrap();
    fs::copy(
        binary_path(source, "ruletrace"),
        binary_path(dir, "ruletrace"),
    )
    .unwrap();
    if !with_tool {
        return false;
    }
    let real = binary_path(source, "ruletrace-ai");
    let target = binary_path(dir, "ruletrace-ai");
    if real.is_file() {
        fs::copy(real, target).unwrap();
        true
    } else {
        fs::write(target, b"inert stand-in for ruletrace-ai\n").unwrap();
        false
    }
}

struct Setup {
    root: PathBuf,
    bin: PathBuf,
    path_value: OsString,
    home: PathBuf,
    config_path: Option<PathBuf>,
}

fn setup(cell: Cell, index: usize, base: &Path, source_bin: &Path) -> (Setup, bool) {
    let cell_dir = base.join(format!("cell{index}"));
    let root = cell_dir.join("root");
    copy_demo(&demo_dir(), &root).unwrap();
    let bin = cell_dir.join("bin");
    let real = prepare_bin(source_bin, &bin, cell.tool);
    let home = cell_dir.join("home");
    fs::create_dir_all(&home).unwrap();
    let mut config_path = None;
    if cell.config {
        fs::write(root.join(PLANTED_CONFIG), AI_CONFIGURATION).unwrap();
        fs::write(home.join(PLANTED_CONFIG), AI_CONFIGURATION).unwrap();
        config_path = Some(root.join(PLANTED_CONFIG));
    }
    if cell.tool {
        fs::copy(
            binary_path(&bin, "ruletrace-ai"),
            binary_path(&root, "ruletrace-ai"),
        )
        .unwrap();
    }
    let empty = cell_dir.join("empty");
    fs::create_dir_all(&empty).unwrap();
    let dirs = if cell.tool {
        vec![bin.clone(), empty]
    } else {
        vec![empty]
    };
    let path_value = std::env::join_paths(dirs).unwrap();
    (
        Setup {
            root,
            bin,
            path_value,
            home,
            config_path,
        },
        real,
    )
}

fn run_command(setup: &Setup, cell: Cell, argv: &[String]) -> Run {
    let mut command = Command::new(binary_path(&setup.bin, "ruletrace"));
    command
        .args(argv)
        .current_dir(&setup.root)
        .env_clear()
        .env("PATH", &setup.path_value)
        .env("HOME", &setup.home)
        .env("USERPROFILE", &setup.home)
        .stdin(Stdio::null());
    if cell.credential {
        for name in CREDENTIAL_VARIABLES {
            command.env(name, CANARY);
        }
    }
    if let Some(path) = &setup.config_path {
        command.env("RULETRACE_AI_CONFIG", path);
    }
    let output = command.output().unwrap();
    Run {
        exit: output
            .status
            .code()
            .expect("ruletrace ended without a code"),
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

fn snapshot(root: &Path, dir: &Path, into: &mut BTreeMap<String, Vec<u8>>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            snapshot(root, &path, into);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let planted = relative == PLANTED_CONFIG
            || relative == binary_path(Path::new(""), "ruletrace-ai").to_string_lossy();
        if !planted {
            into.insert(relative, fs::read(&path).unwrap());
        }
    }
}

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

/// Commands outside the runbook, with the exit code Req 16 prescribes and a
/// diagnostic code that must appear on standard error.
fn failing_commands() -> Vec<(Vec<String>, i32, &'static str)> {
    vec![
        (argv(&["frobnicate"]), 2, "RT-CLI-0001"),
        (
            argv(&[
                "validate",
                "--rulebook",
                "rulebook/missing.json",
                "--ledger",
                "work/ledger.jsonl",
                "--source-model",
                "work/source-model.json",
                "--candidates",
                "work/candidates.json",
                "--out",
                "work/never-written.json",
            ]),
            2,
            "RT-CLI-0003",
        ),
        (
            argv(&[
                "review",
                "list",
                "--rulebook",
                "rulebook/hand-authored.json",
                "--ledger",
                "work/no-ledger.jsonl",
                "--source-model",
                "work/source-model.json",
            ]),
            2,
            "RT-CLI-0003",
        ),
        (
            argv(&[
                "validate",
                "--rulebook",
                "rulebook/seed.json",
                "--ledger",
                "work/ledger.jsonl",
                "--source-model",
                "work/source-model.json",
                "--candidates",
                "work/candidates.json",
                "--out",
                "work/validation-no-rules.json",
            ]),
            3,
            "RT-APR-0002",
        ),
    ]
}

fn run_cell(
    cell: Cell,
    index: usize,
    base: &Path,
    source_bin: &Path,
    steps: &[&Step],
) -> CellResult {
    let (setup, _real) = setup(cell, index, base, source_bin);
    let demo = demo_dir();
    let mut runs = Vec::new();
    for step in steps {
        let run = run_command(&setup, cell, &step.argv[1..]);
        assert_eq!(
            run.exit,
            step.expect_exit,
            "{} step {}",
            cell.label(),
            step.id
        );
        assert_eq!(
            run.stdout,
            fs::read(demo.join(&step.stdout_golden)).unwrap(),
            "{} step {} stdout differs from its golden file",
            cell.label(),
            step.id
        );
        assert_eq!(
            run.stderr,
            fs::read(demo.join(&step.stderr_golden)).unwrap(),
            "{} step {} stderr differs from its golden file",
            cell.label(),
            step.id
        );
        for output in &step.outputs {
            assert_eq!(
                fs::read(setup.root.join(&output.path)).unwrap(),
                fs::read(demo.join(&output.golden)).unwrap(),
                "{} step {} output {} differs from its golden file",
                cell.label(),
                step.id,
                output.path
            );
        }
        runs.push((step.id.clone(), run));
    }
    for (position, (words, exit, code)) in failing_commands().into_iter().enumerate() {
        let run = run_command(&setup, cell, &words);
        assert_eq!(
            run.exit,
            exit,
            "{} failing command {position}",
            cell.label()
        );
        assert!(
            String::from_utf8_lossy(&run.stderr).contains(code),
            "{} failing command {position}: stderr lacks {code}",
            cell.label()
        );
        runs.push((format!("failing-{position}"), run));
    }
    assert!(
        !setup.root.join("work/never-written.json").exists(),
        "{} a failed validate wrote its output",
        cell.label()
    );
    let mut files = BTreeMap::new();
    snapshot(&setup.root, &setup.root, &mut files);
    CellResult { runs, files }
}

#[test]
fn ruletrace_output_is_identical_in_all_eight_combinations() -> TestResult {
    let demo = demo_dir();
    let runbook = parse_runbook(&fs::read_to_string(demo.join("RUNBOOK.md"))?)?;
    let steps = runbook.steps_for("hand")?;
    assert!(
        steps.len() >= 8,
        "the hand runbook has {} steps",
        steps.len()
    );
    let source_bin = bin_dir()?;
    let base = TempDir::new("matrix");
    let all = cells();
    assert_eq!(all.len(), 8);
    let reference = run_cell(all[0], 0, &base.0, &source_bin, &steps);
    assert!(
        reference.runs.len() == steps.len() + failing_commands().len(),
        "every step and failing command ran"
    );
    for (index, cell) in all.iter().enumerate().skip(1) {
        let result = run_cell(*cell, index, &base.0, &source_bin, &steps);
        for ((id, expected), (_, actual)) in reference.runs.iter().zip(result.runs.iter()) {
            assert_eq!(actual, expected, "{} command {id}", cell.label());
        }
        assert_eq!(
            result.files.keys().collect::<Vec<_>>(),
            reference.files.keys().collect::<Vec<_>>(),
            "{} wrote a different set of files",
            cell.label()
        );
        for (name, bytes) in &reference.files {
            assert_eq!(&result.files[name], bytes, "{} file {name}", cell.label());
        }
    }
    let canary = CANARY.as_bytes();
    let leaked = |bytes: &[u8]| bytes.windows(canary.len()).any(|w| w == canary);
    assert!(
        !reference
            .runs
            .iter()
            .any(|(_, r)| leaked(&r.stdout) || leaked(&r.stderr)),
        "the canary reached an output"
    );
    assert!(!reference.files.values().any(|b| leaked(b)));
    let real = binary_path(&source_bin, "ruletrace-ai").is_file();
    println!(
        "8 cells equal; {} step(s) and {} failing command(s) each; present cells use the {} ruletrace-ai",
        steps.len(),
        failing_commands().len(),
        if real { "real" } else { "inert stand-in" }
    );
    Ok(())
}

#[test]
fn the_cells_differ_in_what_is_visible_to_ruletrace() -> TestResult {
    let source_bin = bin_dir()?;
    let base = TempDir::new("visible");
    let all = cells();
    for (index, cell) in all.iter().enumerate() {
        let (setup, _) = setup(*cell, index, &base.0, &source_bin);
        let ai = binary_path(&setup.bin, "ruletrace-ai").is_file();
        assert_eq!(ai, cell.tool, "{}", cell.label());
        assert_eq!(
            setup.root.join(PLANTED_CONFIG).is_file(),
            cell.config,
            "{}",
            cell.label()
        );
        assert_eq!(setup.config_path.is_some(), cell.config);
        let found_on_path = std::env::split_paths(&setup.path_value)
            .any(|dir| binary_path(&dir, "ruletrace-ai").is_file());
        assert_eq!(found_on_path, cell.tool, "{}", cell.label());
    }
    Ok(())
}
