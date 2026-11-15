//! Origin independence (Req 16.18, 18.1): a hand-authored Rulebook with the
//! same Rule_Content as the AI-origin Rulebook gives the same artifacts.
//!
//! `demo/stand-in-ai/rulebook/hand-authored.json` is synthetic and hand
//! authored. It holds the three approved rules of the `ai` profile, with
//! origin `manual`, the same Revision_Numbers and the same document, so the
//! content hashes match. The test runs `ingest`, `review approve`, `validate`,
//! `compile` and `evaluate` on it and compares `compiled.json` and
//! `results.json` byte for byte with the committed `ai` goldens, which the
//! `ai` profile acceptance test reproduces from the AI-origin flow. The
//! `hand` profile itself uses another document, so its compiled bytes differ
//! legitimately (the document hash is part of the content hash).
//!
//! The test needs only `ruletrace`, so the `core-only` job runs it too.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

use rt_demo::{copy_demo, run_step, Captured, Step};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const REVIEWER: &str = "Demo Reviewer";
const TIME: &str = "2026-01-15T10:00:00Z";

/// Content hashes of the three approved rules, as recorded in the `ai`
/// ledger golden `golden/artifacts/ledger.jsonl`.
const APPROVALS: [(&str, &str); 3] = [
    (
        "elig.entry",
        "9edaa59e63831bf8d71ccba89eb6025bac65019cbd424ed1348981e784dc368c",
    ),
    (
        "score.weighted",
        "9fa6292ebbd6de150b1021e75b47a0ffca25db2da4c24c5ecd17f2c249136dac",
    ),
    (
        "rank.standing",
        "0bc9dbf21d05814446b9ef0b9466e0f7de03c96b26bdebcb8916a8a328628224",
    ),
];

fn demo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("demo")
        .join("stand-in-ai")
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
            std::env::temp_dir().join(format!("rt-demo-origin-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn step(id: &str, words: &[&str]) -> Step {
    Step {
        id: id.to_owned(),
        profiles: Vec::new(),
        argv: words.iter().map(|w| (*w).to_owned()).collect(),
        expect_exit: 0,
        cwd: ".".to_owned(),
        stdout_golden: String::new(),
        stderr_golden: String::new(),
        outputs: Vec::new(),
    }
}

fn run(id: &str, words: &[&str], bin: &Path, work: &Path) -> Captured {
    run_step(&step(id, words), bin, work).unwrap()
}

fn approve(rulebook: &str, rule: &str, hash: &str, first: bool) -> Vec<String> {
    let mut words = vec![
        "ruletrace",
        "review",
        "approve",
        "--rulebook",
        rulebook,
        "--ledger",
        "work/ledger.jsonl",
        "--source-model",
        "work/source-model.json",
        "--rule",
        rule,
        "--revision",
        "1",
        "--content-hash",
        hash,
        "--reviewer",
        REVIEWER,
        "--time",
        TIME,
    ];
    if first {
        words.push("--create-ledger");
    }
    words.into_iter().map(str::to_owned).collect()
}

fn run_owned(id: &str, words: &[String], bin: &Path, work: &Path) -> Captured {
    let refs: Vec<&str> = words.iter().map(String::as_str).collect();
    run(id, &refs, bin, work)
}

fn text(c: &Captured) -> String {
    format!(
        "exit {}\n{}{}",
        c.exit,
        String::from_utf8_lossy(&c.stdout),
        String::from_utf8_lossy(&c.stderr)
    )
}

fn ingest(bin: &Path, work: &Path) {
    let c = run(
        "ingest",
        &[
            "ruletrace",
            "ingest",
            "--document",
            "documents/stand-in-rules.md",
            "--document-id",
            "stand-in-rules.md",
            "--out",
            "work/source-model.json",
            "--candidates",
            "work/candidates.json",
        ],
        bin,
        work,
    );
    assert_eq!(c.exit, 0, "{}", text(&c));
}

#[test]
fn hand_authored_rulebook_over_the_ai_document_gives_the_ai_bytes() -> TestResult {
    let demo = demo_dir();
    let bin = bin_dir()?;
    let temp = TempDir::new("flow");
    let work = &temp.0;
    copy_demo(&demo, work)?;
    ingest(&bin, work);
    for (index, (rule, hash)) in APPROVALS.iter().enumerate() {
        let words = approve("rulebook/hand-authored.json", rule, hash, index == 0);
        let c = run_owned("approve", &words, &bin, work);
        assert_eq!(c.exit, 0, "{rule}: {}", text(&c));
    }
    let steps: [(&str, &[&str], i32); 3] = [
        (
            "validate",
            &[
                "ruletrace",
                "validate",
                "--rulebook",
                "rulebook/hand-authored.json",
                "--ledger",
                "work/ledger.jsonl",
                "--source-model",
                "work/source-model.json",
                "--candidates",
                "work/candidates.json",
                "--out",
                "work/validation.json",
            ],
            0,
        ),
        (
            "compile",
            &[
                "ruletrace",
                "compile",
                "--validation",
                "work/validation.json",
                "--out",
                "work/compiled.json",
            ],
            0,
        ),
        (
            "evaluate",
            &[
                "ruletrace",
                "evaluate",
                "--compiled",
                "work/compiled.json",
                "--data",
                "data/canonical/data.json",
                "--questions",
                "data/canonical/questions.json",
                "--published",
                "data/canonical/published.json",
                "--source-model",
                "work/source-model.json",
                "--out",
                "work/results.json",
            ],
            4,
        ),
    ];
    for (id, words, exit) in steps {
        let c = run(id, words, &bin, work);
        assert_eq!(c.exit, exit, "{id}: {}", text(&c));
    }
    let golden = demo.join("golden/artifacts");
    for name in ["compiled.json", "results.json"] {
        let hand = fs::read(work.join("work").join(name))?;
        let ai = fs::read(golden.join(name))?;
        assert!(
            hand == ai,
            "{name}: the hand-authored artifact differs from the ai artifact (first difference at byte {:?})",
            rt_demo::first_difference(&hand, &ai)
        );
    }
    // The first three ledger records bind the same Rule_Content hashes,
    // reviewer and time as the AI-origin flow, so they are byte equal too.
    let hand_ledger = fs::read_to_string(work.join("work/ledger.jsonl"))?;
    let ai_ledger = fs::read_to_string(golden.join("ledger.jsonl"))?;
    let first_three = |s: &str| s.lines().take(4).collect::<Vec<_>>().join("\n");
    assert_eq!(first_three(&hand_ledger), first_three(&ai_ledger));
    assert_eq!(hand_ledger.lines().count(), 4);
    Ok(())
}

#[test]
fn a_changed_rule_is_refused_with_the_original_content_hash() -> TestResult {
    let demo = demo_dir();
    let bin = bin_dir()?;
    let temp = TempDir::new("changed");
    let work = &temp.0;
    copy_demo(&demo, work)?;
    ingest(&bin, work);
    let path = work.join("rulebook/hand-authored.json");
    let original = fs::read_to_string(&path)?;
    let changed = original.replacen("\"value\": \"16\"", "\"value\": \"17\"", 1);
    assert_ne!(changed, original, "the fixture has the threshold 16");
    fs::write(&path, changed)?;
    let (rule, hash) = APPROVALS[0];
    let words = approve("rulebook/hand-authored.json", rule, hash, true);
    let c = run_owned("approve-changed", &words, &bin, work);
    assert_eq!(c.exit, 3, "{}", text(&c));
    assert!(text(&c).contains("RT-APR-0203"), "{}", text(&c));
    Ok(())
}
