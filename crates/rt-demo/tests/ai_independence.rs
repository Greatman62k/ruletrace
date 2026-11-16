//! Property 41: `ruletrace` does not depend on the AI tool (Req 17.8, 18.1,
//! 20.16, 27.9).
//!
//! The three conditions of Req 27.9 give an exhaustive 2x2x2 matrix, which the
//! user approved in place of 256 generated cases: the `ruletrace-ai`
//! executable beside `ruletrace` and on `PATH`, an AI_Configuration file in
//! the working directory and the home directory (named by
//! `RULETRACE_AI_CONFIG`), and a canary value in the credential variables.
//! The whole domain is eight cells, so every cell runs and no case count
//! applies.
//!
//! Each cell runs the whole `hand` runbook of `demo/stand-in-hand` and five
//! more commands. Every step is compared byte for byte with the committed
//! golden files, which were not produced by this test, and every cell is
//! compared with the first cell: exit codes, standard output, standard error
//! and the files the run produced. Because the golden files are fixed, the
//! `core-only` CI job, which runs this test against a build without the AI
//! crates, shows that both builds produce the same bytes (Req 20.16).
//!
//! The planted files (the configuration files, the `ruletrace-ai` copies and
//! the demonstration inputs) are part of the snapshot taken before the first
//! command. After the last command the test asserts that none of them
//! changed, so a file that `ruletrace` rewrites or removes is a failure.
//!
//! The canary is a string, never a credential. In every cell the test scans
//! standard output, standard error, every file below the cell directory and
//! every path name for the whole canary and for any fragment of eight bytes
//! of it. Tests of the check itself inject a leak into a
//! file, into standard error and into a path, and tamper with a planted file.
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

use rt_demo::leak::first_leak;
use rt_demo::{binary_path, copy_demo, parse_runbook, Step};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A string, not a credential. Every eight-byte window of it is unlikely in
/// ordinary output, so a fragment match is a real leak and not a word.
const CANARY: &str = "rtc-Zq7x93LmN-Rt5vK2pWd8Hc4Jf1Bs6Ug0Ya3Xe9";
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

/// The files and directories below a cell directory, keyed by the path
/// relative to it with `/` separators.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Tree {
    files: BTreeMap<String, Vec<u8>>,
    dirs: BTreeSet<String>,
}

/// Everything a cell produced that the canary check scans.
struct Surfaces {
    runs: Vec<(String, Run)>,
    after: Tree,
}

#[derive(Debug, PartialEq, Eq)]
struct CellResult {
    runs: Vec<(String, Run)>,
    produced: Tree,
}

fn demo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("demo")
        .join("stand-in-hand")
}

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

fn build_bin_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
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
        fs::write(&target, b"inert stand-in for ruletrace-ai\n").unwrap();
        // Executable, so an attempt to start it fails on the file content
        // and not on a permission error a caller could swallow.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
        }
        false
    }
}

struct Setup {
    dir: PathBuf,
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
            dir: cell_dir,
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

fn read_tree_into(base: &Path, dir: &Path, into: &mut Tree) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(base)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            into.dirs.insert(relative);
            read_tree_into(base, &path, into);
        } else {
            into.files.insert(relative, fs::read(&path).unwrap());
        }
    }
}

/// Every file and directory of a cell: the working directory, the home
/// directory, the `bin` directory and the empty `PATH` directory.
fn read_tree(cell_dir: &Path) -> Tree {
    let mut tree = Tree::default();
    read_tree_into(cell_dir, cell_dir, &mut tree);
    tree
}

/// What `ruletrace` did to the files that existed before its first command:
/// a changed or removed file or a removed directory is a problem. New files
/// are the run's output and are not reported here.
fn changed_planted(before: &Tree, after: &Tree) -> Vec<String> {
    let mut problems = Vec::new();
    for (name, bytes) in &before.files {
        match after.files.get(name) {
            None => problems.push(format!("planted file {name} was removed")),
            Some(now) if now != bytes => problems.push(format!("planted file {name} changed")),
            Some(_) => {}
        }
    }
    for name in &before.dirs {
        if !after.dirs.contains(name) {
            problems.push(format!("planted directory {name} was removed"));
        }
    }
    problems
}

/// The files and directories that appeared during the run.
fn produced(before: &Tree, after: &Tree) -> Tree {
    Tree {
        files: after
            .files
            .iter()
            .filter(|(name, _)| !before.files.contains_key(*name))
            .map(|(n, b)| (n.clone(), b.clone()))
            .collect(),
        dirs: after.dirs.difference(&before.dirs).cloned().collect(),
    }
}

/// Every surface that holds the canary, whole or in part: standard output and
/// standard error of each command, the bytes of every file and every path
/// name of the cell directory.
fn find_leaks(surfaces: &Surfaces) -> Vec<String> {
    let secret = CANARY.as_bytes();
    let mut leaks = Vec::new();
    for (id, run) in &surfaces.runs {
        if let Some(at) = first_leak(secret, &run.stdout) {
            leaks.push(format!("stdout of {id} at byte {at}"));
        }
        if let Some(at) = first_leak(secret, &run.stderr) {
            leaks.push(format!("stderr of {id} at byte {at}"));
        }
    }
    for (name, bytes) in &surfaces.after.files {
        if let Some(at) = first_leak(secret, bytes) {
            leaks.push(format!("file {name} at byte {at}"));
        }
        if let Some(at) = first_leak(secret, name.as_bytes()) {
            leaks.push(format!("path of file {name} at byte {at}"));
        }
    }
    for name in &surfaces.after.dirs {
        if let Some(at) = first_leak(secret, name.as_bytes()) {
            leaks.push(format!("path of directory {name} at byte {at}"));
        }
    }
    leaks
}

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

/// A command outside the runbook with its expected exit code and the text that
/// must appear on standard error (a diagnostic code) or on standard output.
struct Extra {
    argv: Vec<String>,
    exit: i32,
    stderr_has: Option<&'static str>,
    stdout_has: Option<&'static str>,
}

fn extra_commands() -> Vec<Extra> {
    let failing = |words: Vec<String>, exit: i32, code: &'static str| Extra {
        argv: words,
        exit,
        stderr_has: Some(code),
        stdout_has: None,
    };
    vec![
        failing(argv(&["frobnicate"]), 2, "RT-CLI-0001"),
        failing(
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
        // `review approve` still needs a ledger: a missing file is refused
        // unless `--create-ledger` is given.
        failing(
            argv(&[
                "review",
                "approve",
                "--rulebook",
                "rulebook/hand-authored.json",
                "--ledger",
                "work/no-ledger.jsonl",
                "--source-model",
                "work/source-model.json",
                "--rule",
                "elig.entry",
                "--revision",
                "1",
                "--content-hash",
                "0b86ad7b084217b9ed8f0156f0b5c7a8b4f92269dd9fcdb1e637d77be9c15bd9",
                "--reviewer",
                "Demo Reviewer",
                "--time",
                "2026-01-15T10:00:00Z",
            ]),
            2,
            "RT-CLI-0003",
        ),
        failing(
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
        // `review list` on a missing ledger is read-only and succeeds (Req
        // 26.6 as amended).
        Extra {
            argv: argv(&[
                "review",
                "list",
                "--rulebook",
                "rulebook/hand-authored.json",
                "--ledger",
                "work/no-ledger-list.jsonl",
                "--source-model",
                "work/source-model.json",
            ]),
            exit: 0,
            stderr_has: None,
            stdout_has: Some("status Proposed"),
        },
    ]
}

fn run_cell(
    cell: Cell,
    index: usize,
    base: &Path,
    source_bin: &Path,
    steps: &[&Step],
) -> (CellResult, Surfaces) {
    let (setup, _real) = setup(cell, index, base, source_bin);
    let demo = demo_dir();
    let before = read_tree(&setup.dir);
    let exe = |dir: &str, name: &str| {
        format!(
            "{dir}/{}",
            binary_path(Path::new(""), name).to_string_lossy()
        )
    };
    for (name, present) in [
        ("root/ai.toml".to_owned(), cell.config),
        ("home/ai.toml".to_owned(), cell.config),
        (exe("root", "ruletrace-ai"), cell.tool),
        (exe("bin", "ruletrace-ai"), cell.tool),
        (exe("bin", "ruletrace"), true),
        ("root/rulebook/hand-authored.json".to_owned(), true),
    ] {
        assert_eq!(
            before.files.contains_key(&name),
            present,
            "{} planted file {name}",
            cell.label()
        );
    }
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
    for (position, extra) in extra_commands().into_iter().enumerate() {
        let run = run_command(&setup, cell, &extra.argv);
        assert_eq!(
            run.exit,
            extra.exit,
            "{} extra command {position}",
            cell.label()
        );
        if let Some(code) = extra.stderr_has {
            assert!(
                String::from_utf8_lossy(&run.stderr).contains(code),
                "{} extra command {position}: stderr lacks {code}",
                cell.label()
            );
        } else {
            assert!(
                run.stderr.is_empty(),
                "{} extra command {position}: unexpected stderr",
                cell.label()
            );
        }
        if let Some(text) = extra.stdout_has {
            assert!(
                String::from_utf8_lossy(&run.stdout).contains(text),
                "{} extra command {position}: stdout lacks {text}",
                cell.label()
            );
        }
        runs.push((format!("extra-{position}"), run));
    }
    assert!(
        !setup.root.join("work/never-written.json").exists(),
        "{} a failed validate wrote its output",
        cell.label()
    );
    assert!(
        !setup.root.join("work/no-ledger-list.jsonl").exists(),
        "{} `review list` created a ledger",
        cell.label()
    );
    let after = read_tree(&setup.dir);
    assert_eq!(
        changed_planted(&before, &after),
        Vec::<String>::new(),
        "{} ruletrace changed a file that was there before",
        cell.label()
    );
    let new = produced(&before, &after);
    for name in new.files.keys().chain(new.dirs.iter()) {
        assert!(
            name.starts_with("root/work/"),
            "{} wrote outside the work directory: {name}",
            cell.label()
        );
    }
    let result = CellResult {
        runs: runs.clone(),
        produced: new,
    };
    (result, Surfaces { runs, after })
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
    let (reference, surfaces) = run_cell(all[0], 0, &base.0, &source_bin, &steps);
    assert!(
        reference.runs.len() == steps.len() + extra_commands().len(),
        "every step and extra command ran"
    );
    assert_eq!(find_leaks(&surfaces), Vec::<String>::new(), "cell 0");
    drop(surfaces);
    for (index, cell) in all.iter().enumerate().skip(1) {
        let (result, surfaces) = run_cell(*cell, index, &base.0, &source_bin, &steps);
        assert_eq!(
            find_leaks(&surfaces),
            Vec::<String>::new(),
            "{} the canary reached a surface",
            cell.label()
        );
        drop(surfaces);
        for ((id, expected), (_, actual)) in reference.runs.iter().zip(result.runs.iter()) {
            assert_eq!(actual, expected, "{} command {id}", cell.label());
        }
        assert_eq!(
            result.produced.files.keys().collect::<Vec<_>>(),
            reference.produced.files.keys().collect::<Vec<_>>(),
            "{} wrote a different set of files",
            cell.label()
        );
        assert_eq!(
            result.produced.dirs,
            reference.produced.dirs,
            "{} created a different set of directories",
            cell.label()
        );
        for (name, bytes) in &reference.produced.files {
            assert_eq!(
                &result.produced.files[name],
                bytes,
                "{} file {name}",
                cell.label()
            );
        }
    }
    let real = binary_path(&source_bin, "ruletrace-ai").is_file();
    println!(
        "8 cells equal; {} step(s) and {} extra command(s) each; {} produced file(s) per cell; \
         present cells use the {} ruletrace-ai",
        steps.len(),
        extra_commands().len(),
        reference.produced.files.len(),
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

/// A real run of the cell with a credential, as the starting point of the
/// tests of the checks themselves.
fn credential_cell(
    label: &str,
) -> Result<(TempDir, Setup, Tree, Surfaces), Box<dyn std::error::Error>> {
    let demo = demo_dir();
    let runbook = parse_runbook(&fs::read_to_string(demo.join("RUNBOOK.md"))?)?;
    let steps = runbook.steps_for("hand")?;
    let source_bin = bin_dir()?;
    let base = TempDir::new(label);
    let cell = Cell {
        tool: false,
        config: true,
        credential: true,
    };
    let (setup, _) = setup(cell, 0, &base.0, &source_bin);
    let before = read_tree(&setup.dir);
    let mut runs = Vec::new();
    for step in steps {
        runs.push((step.id.clone(), run_command(&setup, cell, &step.argv[1..])));
    }
    let after = read_tree(&setup.dir);
    Ok((base, setup, before, Surfaces { runs, after }))
}

#[test]
fn the_canary_check_detects_a_leak_into_standard_error_a_file_and_a_path() -> TestResult {
    let (_base, _setup, _before, clean) = credential_cell("mutant-canary")?;
    assert_eq!(find_leaks(&clean), Vec::<String>::new(), "a clean run");

    let mut stderr_leak = Surfaces {
        runs: clean.runs.clone(),
        after: clean.after.clone(),
    };
    let id = stderr_leak.runs[3].0.clone();
    let offset = stderr_leak.runs[3].1.stderr.len() + "error: key ".len();
    stderr_leak.runs[3]
        .1
        .stderr
        .extend_from_slice(format!("error: key {CANARY}\n").as_bytes());
    assert_eq!(
        find_leaks(&stderr_leak),
        [format!("stderr of {id} at byte {offset}")]
    );

    let mut file_leak = Surfaces {
        runs: clean.runs.clone(),
        after: clean.after.clone(),
    };
    file_leak
        .after
        .files
        .get_mut("root/work/ledger.jsonl")
        .expect("the ledger exists")
        .extend_from_slice(CANARY.as_bytes());
    let found = find_leaks(&file_leak);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("file root/work/ledger.jsonl at byte "),
        "{found:?}"
    );

    // Only a fragment of eight bytes, in a file name.
    let mut path_leak = Surfaces {
        runs: clean.runs.clone(),
        after: clean.after.clone(),
    };
    path_leak
        .after
        .files
        .insert(format!("root/work/{}.txt", &CANARY[12..20]), Vec::new());
    let found = find_leaks(&path_leak);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("path of file root/work/"), "{found:?}");

    // Seven bytes of the canary are not a fragment.
    let mut short = Surfaces {
        runs: clean.runs,
        after: clean.after,
    };
    short.after.files.insert(
        "root/work/short.txt".to_owned(),
        CANARY.as_bytes()[12..19].to_vec(),
    );
    assert_eq!(find_leaks(&short), Vec::<String>::new());
    Ok(())
}

#[test]
fn the_planted_file_check_detects_a_changed_a_removed_and_a_new_file() -> TestResult {
    let (_base, setup, before, run) = credential_cell("mutant-planted")?;
    assert_eq!(changed_planted(&before, &run.after), Vec::<String>::new());
    let new = produced(&before, &run.after);
    assert!(new.files.contains_key("root/work/ledger.jsonl"));
    assert!(new.files.keys().all(|n| n.starts_with("root/work/")));

    let planted = "root/ai.toml";
    assert!(before.files.contains_key(planted));
    let mut changed = run.after.clone();
    changed.files.get_mut(planted).unwrap().push(b'#');
    assert_eq!(
        changed_planted(&before, &changed),
        [format!("planted file {planted} changed")]
    );
    let mut removed = run.after.clone();
    removed.files.remove("home/ai.toml");
    assert_eq!(
        changed_planted(&before, &removed),
        ["planted file home/ai.toml was removed".to_owned()]
    );
    // A real rewrite on disk is seen by `read_tree`.
    fs::write(setup.root.join(PLANTED_CONFIG), b"rewritten")?;
    assert_eq!(
        changed_planted(&before, &read_tree(&setup.dir)),
        [format!("planted file {planted} changed")]
    );
    // A new file is output, not a change of a planted file.
    let mut extra = run.after.clone();
    extra
        .files
        .insert("root/work/new.txt".to_owned(), Vec::new());
    assert_eq!(changed_planted(&before, &extra), Vec::<String>::new());
    Ok(())
}
