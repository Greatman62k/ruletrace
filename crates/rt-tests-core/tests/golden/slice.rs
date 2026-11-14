//! Slice 0 golden fixtures (task 5.27; Req 16.2 to 16.7, 20.4 to 20.6, design §7.3).
//!
//! Each fixture runs the real `ruletrace` binary (`ingest`, `validate`, `compile`, `evaluate`)
//! in a scrubbed environment inside a temporary copy of the fixture, and compares the bytes of
//! every output with the files under `expected/`. The first failing command decides
//! `expected/stderr.txt` and `expected/exit-code`; a fixture whose pipeline stops before an
//! artifact exists has no golden file for it, and the test asserts that the file is absent.
//!
//! Blessing: `RULETRACE_BLESS=1 cargo test -p rt-tests-core --test golden golden_slice` rewrites
//! the expected files. It is refused when `CI` or `GITHUB_ACTIONS` is set, so goldens are only
//! ever blessed locally and reviewed in the diff.

use crate::harness::{compare_bytes, fixtures_root, platform};
use crate::ingest::ruletrace_binary;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What one fixture must produce.
struct Expect {
    /// The fixture directory name.
    name: &'static str,
    /// The exit code of the first failing command, or 0.
    exit: i32,
    /// The artifacts the pipeline writes before it stops.
    artifacts: &'static [&'static str],
}

const ALL: [&str; 3] = ["validation.json", "compiled.json", "results.json"];

const FIXTURES: [Expect; 5] = [
    Expect {
        name: "slice-basic",
        exit: 0,
        artifacts: &ALL,
    },
    Expect {
        name: "slice-missing",
        exit: 4,
        artifacts: &ALL,
    },
    Expect {
        name: "slice-tie",
        exit: 0,
        artifacts: &ALL,
    },
    Expect {
        name: "slice-unsupported",
        exit: 3,
        artifacts: &["validation.json"],
    },
    Expect {
        name: "slice-unresolved",
        exit: 3,
        artifacts: &["validation.json"],
    },
];

struct Run {
    /// Directory holding every output, named as under `expected/`.
    dir: PathBuf,
    exit: i32,
    stderr: Vec<u8>,
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

fn command(work: &Path, args: &[&str]) -> (i32, Vec<u8>) {
    let output = Command::new(ruletrace_binary())
        .env_clear()
        .current_dir(work)
        .args(args)
        .output()
        .unwrap();
    (output.status.code().unwrap(), output.stderr)
}

/// Runs the pipeline in a temporary copy of the fixture (without `expected/`), stopping at the
/// first failing command.
fn run_pipeline(name: &str) -> Run {
    let work = std::env::temp_dir().join(format!("rt-golden-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let fixture = fixtures_root().join(name);
    for part in ["source", "rulebook.json", "ledger.jsonl", "data.json"] {
        let from = fixture.join(part);
        if from.is_dir() {
            copy_tree(&from, &work.join(part));
        } else {
            std::fs::create_dir_all(&work).unwrap();
            std::fs::copy(&from, work.join(part)).unwrap();
        }
    }
    let steps: [(&[&str], &str); 4] = [
        (
            &[
                "ingest",
                "--document",
                "source/rules.md",
                "--document-id",
                "rules.md",
                "--out",
                "source-model.json",
            ],
            "source-model.json",
        ),
        (
            &[
                "validate",
                "--rulebook",
                "rulebook.json",
                "--ledger",
                "ledger.jsonl",
                "--source-model",
                "source-model.json",
                "--out",
                "validation.json",
            ],
            "validation.json",
        ),
        (
            &[
                "compile",
                "--validation",
                "validation.json",
                "--out",
                "compiled.json",
            ],
            "compiled.json",
        ),
        (
            &[
                "evaluate",
                "--compiled",
                "compiled.json",
                "--data",
                "data.json",
                "--source-model",
                "source-model.json",
                "--out",
                "results.json",
            ],
            "results.json",
        ),
    ];
    let mut last = (0, Vec::new());
    for (args, _) in steps {
        last = command(&work, args);
        if last.0 != 0 {
            break;
        }
    }
    Run {
        dir: work,
        exit: last.0,
        stderr: last.1,
    }
}

fn blessing() -> bool {
    let wanted = std::env::var_os("RULETRACE_BLESS").is_some_and(|v| v == "1");
    if wanted {
        assert!(
            std::env::var_os("CI").is_none() && std::env::var_os("GITHUB_ACTIONS").is_none(),
            "RULETRACE_BLESS is refused in CI: goldens are blessed locally only"
        );
    }
    wanted
}

fn check(expect: &Expect) {
    let run = run_pipeline(expect.name);
    let expected = fixtures_root().join(expect.name).join("expected");
    let exit_text = format!("{}\n", run.exit);
    let mut produced: Vec<(&str, Vec<u8>)> = vec![
        (
            "source-model.json",
            std::fs::read(run.dir.join("source-model.json")).unwrap(),
        ),
        ("stderr.txt", run.stderr.clone()),
        ("exit-code", exit_text.into_bytes()),
    ];
    for artifact in ALL {
        let path = run.dir.join(artifact);
        if expect.artifacts.contains(&artifact) {
            produced.push((artifact, std::fs::read(&path).unwrap()));
        } else {
            assert!(
                !path.exists(),
                "{}: {artifact} must not be written when the pipeline stops (platform {})",
                expect.name,
                platform()
            );
        }
    }
    assert_eq!(run.exit, expect.exit, "{} on {}", expect.name, platform());
    let mut failures = Vec::new();
    for (file, bytes) in &produced {
        if blessing() {
            std::fs::create_dir_all(&expected).unwrap();
            std::fs::write(expected.join(file), bytes).unwrap();
        } else if let Err(message) = compare_bytes(&expected.join(file), bytes) {
            failures.push(message);
        }
    }
    for artifact in ALL {
        if !expect.artifacts.contains(&artifact) {
            assert!(
                !expected.join(artifact).exists(),
                "{}: stale golden {artifact}",
                expect.name
            );
        }
    }
    let _ = std::fs::remove_dir_all(&run.dir);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn golden_slice_basic() {
    check(&FIXTURES[0]);
}

#[test]
fn golden_slice_missing() {
    check(&FIXTURES[1]);
}

#[test]
fn golden_slice_tie() {
    check(&FIXTURES[2]);
}

#[test]
fn golden_slice_unsupported() {
    check(&FIXTURES[3]);
}

#[test]
fn golden_slice_unresolved() {
    check(&FIXTURES[4]);
}

fn expected_text(name: &str, file: &str) -> String {
    String::from_utf8(
        std::fs::read(fixtures_root().join(name).join("expected").join(file)).unwrap(),
    )
    .unwrap()
}

/// The observable facts each fixture exists to show, read from the blessed goldens so a wrong
/// bless is caught (Req 16.2 to 16.7).
#[test]
fn golden_slice_facts() {
    let basic = expected_text("slice-basic", "results.json");
    assert!(basic.contains(r#""value":"7.6""#) && basic.contains(r#""value":"7.8""#));
    assert!(basic
        .contains(r#""places":[{"position":1,"members":["p4"]},{"position":2,"members":["p2"]}"#));
    assert!(basic.contains(r#""kind":"compare","between":["p4","p2"]"#));
    assert_eq!(expected_text("slice-basic", "exit-code"), "0\n");

    let missing = expected_text("slice-missing", "results.json");
    assert!(missing.contains(r#""state":"indeterminate""#));
    assert!(missing.contains(r#""kind":"missing_input","participant":"p2","field":"age""#));
    assert_eq!(expected_text("slice-missing", "exit-code"), "4\n");
    let stderr = expected_text("slice-missing", "stderr.txt");
    assert!(stderr.contains("RT-EXE-0001") && stderr.contains("'age'"));
    assert!(stderr.contains("rules.md:21-91 (line 3, column 1)"));

    let tie = expected_text("slice-tie", "results.json");
    assert!(tie.contains(r#"{"position":1,"members":["p1","p2"]}"#));
    assert!(!tie.contains(r#""position":2"#));
    assert!(tie.contains(r#"{"position":3,"members":["p3"]}"#));

    let unsupported = expected_text("slice-unsupported", "stderr.txt");
    assert_eq!(unsupported.matches("unsupported RT-VAL-0003:").count(), 1);
    assert_eq!(expected_text("slice-unsupported", "exit-code"), "3\n");

    let unresolved = expected_text("slice-unresolved", "stderr.txt");
    assert_eq!(unresolved.matches("RT-APR-0001").count(), 1);
    assert_eq!(unresolved.matches("RT-APR-").count(), 1);
    assert_eq!(expected_text("slice-unresolved", "exit-code"), "3\n");
}
