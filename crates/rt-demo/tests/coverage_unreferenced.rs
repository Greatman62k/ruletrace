//! The `coverage-unreferenced` fixture: a clause that no rule references.
//!
//! The real `ruletrace` binary ingests the document and validates with
//! `--candidates`: exit 0 and the Warning RT-ING-0006 naming the unreferenced
//! clause with its span. The Coverage_Statement generator then classes that
//! clause by design §8.11.8 and fails by name when its note is removed. Bless
//! the goldens with `RT_DEMO_BLESS=1` (refused when `CI` is set).

use rt_demo::coverage::{
    classify, parse_notes, print_json, print_markdown, CoverageClass, CoverageNotes, StoreBatches,
};
use rt_demo::{check_golden, Mode};
use rt_foundation::InputId;
use rt_ir::parse_rulebook;
use rt_json::ParseLimits;
use rt_ledger::parse_ledger;
use rt_srcmodel::{default_load_limits, parse_candidates, SourceModel, SourceModels};
use std::path::{Path, PathBuf};
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const UNREFERENCED_SPAN: &str = "rules.md:72-117 (line 4, column 1)";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/coverage-unreferenced")
}

fn binary() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(["build", "--quiet", "-p", "rt-cli", "--bin", "ruletrace"])
        .status()?;
    assert!(status.success(), "building ruletrace failed");
    let exe = std::env::current_exe()?;
    let profile = exe
        .parent()
        .and_then(Path::parent)
        .ok_or("no profile directory")?;
    Ok(profile.join(format!("ruletrace{}", std::env::consts::EXE_SUFFIX)))
}

fn mode() -> Result<Mode, Box<dyn std::error::Error>> {
    let bless = std::env::var("RT_DEMO_BLESS").ok();
    let ci = std::env::var("CI").ok();
    Ok(Mode::from_env_values(bless.as_deref(), ci.as_deref())?)
}

/// Runs the binary in a scrubbed environment inside `work`.
fn step(
    bin: &Path,
    work: &Path,
    args: &[&str],
) -> Result<(i32, Vec<u8>), Box<dyn std::error::Error>> {
    let out = Command::new(bin)
        .env_clear()
        .current_dir(work)
        .args(args)
        .output()?;
    Ok((out.status.code().ok_or("no exit code")?, out.stderr))
}

struct Run {
    work: PathBuf,
    exit: i32,
    stderr: Vec<u8>,
}

fn pipeline() -> Result<Run, Box<dyn std::error::Error>> {
    let bin = binary()?;
    let work = std::env::temp_dir().join(format!("rt-cov-unref-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(work.join("source"))?;
    for part in ["source/rules.md", "rulebook.json", "ledger.jsonl"] {
        std::fs::copy(fixture().join(part), work.join(part))?;
    }
    let (code, _) = step(
        &bin,
        &work,
        &[
            "ingest",
            "--document",
            "source/rules.md",
            "--document-id",
            "rules.md",
            "--out",
            "source-model.json",
            "--candidates",
            "candidates.json",
        ],
    )?;
    assert_eq!(code, 0);
    let (exit, stderr) = step(
        &bin,
        &work,
        &[
            "validate",
            "--rulebook",
            "rulebook.json",
            "--ledger",
            "ledger.jsonl",
            "--source-model",
            "source-model.json",
            "--candidates",
            "candidates.json",
            "--out",
            "validation.json",
        ],
    )?;
    Ok(Run { work, exit, stderr })
}

#[test]
fn validate_warns_about_the_unreferenced_clause_and_exits_0() -> TestResult {
    let run = pipeline()?;
    let text = String::from_utf8(run.stderr.clone())?;
    assert_eq!(run.exit, 0, "{text}");
    assert_eq!(text.matches("RT-ING-0006").count(), 1, "{text}");
    assert!(text.contains("warning RT-ING-0006"), "{text}");
    assert!(text.contains(UNREFERENCED_SPAN), "{text}");
    assert!(!text.contains("error"), "{text}");
    let mode = mode()?;
    let dir = fixture();
    for name in ["source-model.json", "candidates.json", "validation.json"] {
        let bytes = std::fs::read(run.work.join(name))?;
        check_golden(
            mode,
            "coverage-unreferenced",
            &dir,
            &format!("expected/{name}"),
            &bytes,
        )?;
    }
    check_golden(
        mode,
        "coverage-unreferenced",
        &dir,
        "expected/stderr.txt",
        &run.stderr,
    )?;
    check_golden(
        mode,
        "coverage-unreferenced",
        &dir,
        "expected/exit-code",
        format!("{}\n", run.exit).as_bytes(),
    )?;
    let _ = std::fs::remove_dir_all(&run.work);
    Ok(())
}

#[test]
fn generator_classes_the_unreferenced_clause_and_fails_without_its_note() -> TestResult {
    let dir = fixture();
    let read = |p: &str| std::fs::read(dir.join(p));
    let limits = ParseLimits::new(8 * 1024 * 1024, 64);
    let input = |n: &str| InputId::new(n);
    let model = SourceModel::load(
        &input("source-model.json")?,
        &read("expected/source-model.json")?,
        default_load_limits(),
    )
    .map_err(|d| d.message().to_owned())?;
    let clauses = parse_candidates(
        &input("candidates.json")?,
        &read("expected/candidates.json")?,
        limits,
        &model,
    )?;
    let (rulebook, _) = parse_rulebook(
        &read("rulebook.json")?,
        &input("rulebook.json")?,
        8 * 1024 * 1024,
        64,
    )
    .map_err(|_| "rulebook rejected")?
    .into_parts();
    let ledger = parse_ledger(&input("ledger.jsonl")?, &read("ledger.jsonl")?, limits)
        .map_err(|_| "ledger rejected")?;
    let mut models = SourceModels::new();
    models.insert(model).map_err(|_| "duplicate document")?;
    let validation = rt_validate::validate(&rulebook, &ledger, &models, Some(&clauses));
    let notes = parse_notes(&read("coverage-notes.json")?)?;
    let statement = classify(
        &clauses,
        &rulebook,
        &ledger,
        &StoreBatches::default(),
        &validation,
        &notes,
    )?;
    assert!(matches!(
        &statement.entries[0].class,
        CoverageClass::RuleApproved { rules } if rules == &[("elig.age".to_owned(), 1)]
    ));
    assert!(matches!(
        &statement.entries[1].class,
        CoverageClass::NotARule { .. }
    ));
    let mode = mode()?;
    check_golden(
        mode,
        "coverage-unreferenced",
        &dir,
        "expected/coverage-statement.json",
        print_json(&statement).as_bytes(),
    )?;
    check_golden(
        mode,
        "coverage-unreferenced",
        &dir,
        "expected/coverage-statement.md",
        print_markdown(&statement).as_bytes(),
    )?;

    let removed = classify(
        &clauses,
        &rulebook,
        &ledger,
        &StoreBatches::default(),
        &validation,
        &CoverageNotes::default(),
    );
    let text = removed.err().ok_or("expected a failure")?.to_string();
    assert!(text.contains("'2.'") && text.contains("byte 72"), "{text}");
    Ok(())
}
