//! Development binary for repository checks. Not a delivered executable
//! (RN-25).
#![forbid(unsafe_code)]

use rt_xtask::GraphCheck;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("rt-xtask: {message}");
            ExitCode::from(1)
        }
    }
}

const USAGE: &str = "usage: rt-xtask [--root <dir>] gen-catalogue | check-graph | check-tree | scan-sources | scan-api | check-decisions | check-catalogue | check-justification [--release-tags <git tag --list output>] ...";

fn run(args: &[String]) -> Result<(), String> {
    let mut root = PathBuf::from(".");
    let mut commands: Vec<&str> = Vec::new();
    let mut release_tags: Option<String> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--root" => {
                root = iter
                    .next()
                    .map(PathBuf::from)
                    .ok_or("--root needs a directory")?
            }
            "--release-tags" => {
                release_tags = Some(iter.next().cloned().ok_or("--release-tags needs a value")?)
            }
            "check-catalogue"
            | "check-justification"
            | "gen-catalogue"
            | "check-graph"
            | "check-tree"
            | "scan-sources"
            | "scan-api"
            | "check-decisions" => commands.push(arg),
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    if commands.is_empty() {
        return Err(USAGE.to_string());
    }
    let mut metadata: Option<Vec<u8>> = None;
    let mut failures = Vec::new();
    for command in commands {
        let result = match command {
            "gen-catalogue" => rt_xtask::catalogue::generate(&root),
            "check-catalogue" => {
                let tags = match &release_tags {
                    Some(t) => t.clone(),
                    None => git_tags(&root)?,
                };
                rt_xtask::catalogue::check(&root, &tags).map(|report| println!("{report}"))
            }
            "check-justification" => {
                let bytes = match &metadata {
                    Some(bytes) => bytes,
                    None => metadata.insert(cargo_metadata(&root)?),
                };
                let closures = shipped_closures(&root)?;
                rt_xtask::justification::run(&root, bytes, &closures)
                    .map(|n| println!("check-justification: ok ({n} external crates)"))
            }
            "scan-sources" => rt_xtask::scan::run(&root)
                .map(|(c, f)| println!("scan-sources: ok ({c} crates, {f} files)")),
            "scan-api" => rt_xtask::api::run(&root)
                .map(|(c, n)| println!("scan-api: ok ({c} crates, {n} contracts)")),
            "check-decisions" => {
                let bytes = match &metadata {
                    Some(bytes) => bytes,
                    None => metadata.insert(cargo_metadata(&root)?),
                };
                rt_xtask::run_check_decisions(&root, bytes)
                    .map(|n| println!("check-decisions: ok ({n} gated crates are members)"))
            }
            _ => {
                let bytes = match &metadata {
                    Some(bytes) => bytes,
                    None => metadata.insert(cargo_metadata(&root)?),
                };
                let which = if command == "check-graph" {
                    GraphCheck::Graph
                } else {
                    GraphCheck::Tree
                };
                rt_xtask::run_graph_check(&root, bytes, which)
                    .map(|n| println!("{command}: ok ({n} workspace crates)"))
            }
        };
        if let Err(message) = result {
            failures.push(format!("{command} failed:\n{message}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// Output of `git tag --list`. A directory that is not a git work tree has no
/// release tag.
fn git_tags(root: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["tag", "--list"])
        .output()
        .map_err(|e| format!("cannot run git tag --list (pass --release-tags instead): {e}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn cargo_metadata(root: &Path) -> Result<Vec<u8>, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--all-features",
            "--manifest-path",
        ])
        .arg(root.join("Cargo.toml"))
        .output()
        .map_err(|e| format!("cannot run cargo metadata: {e}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Normal-dependency closure of each shipped executable, for every target.
fn shipped_closures(
    root: &Path,
) -> Result<std::collections::BTreeMap<String, std::collections::BTreeSet<String>>, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut out = std::collections::BTreeMap::new();
    for binary in rt_xtask::justification::SHIPPED_BINARIES {
        let output = Command::new(&cargo)
            .args(["tree", "-p", binary, "-e", "normal", "--target", "all"])
            .args(["--prefix", "none", "--manifest-path"])
            .arg(root.join("Cargo.toml"))
            .output()
            .map_err(|e| format!("cannot run cargo tree: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "cargo tree -p {binary} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        out.insert(
            binary.to_string(),
            rt_xtask::justification::tree_crates(&String::from_utf8_lossy(&output.stdout)),
        );
    }
    Ok(out)
}
