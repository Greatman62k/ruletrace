//! The demonstration acceptance test, `ai` profile (design §8.11.12, §7.5
//! Slice 1; Req 33.6 to 33.9, 33.11, 33.12, 23.25).
//!
//! In order: manifest and authored-script checks, the Request_Identifiers that
//! `ruletrace-ai propose --dry-run` prints against the script keys, the runbook
//! step by step against the golden files, the stored batches and the
//! Transmission_Log, the ledger records, the Coverage_Statement, the traces,
//! the dependency tree of `ruletrace`, and negative runs (dependency order,
//! a request the script does not hold, a tampered script, a clause that no
//! class covers).
//!
//! The binaries come from `RT_BIN_DIR` or from a `cargo build` of `rt-cli` and
//! `rt-ai-tool`. Bless the NEW golden files of this demonstration with
//! `RT_DEMO_BLESS=1` (refused when `CI` is set). The test uses no network and
//! no credential: each process runs with an empty environment.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

use rt_demo::coverage::{
    classify, parse_notes, print_json, print_markdown, CoverageClass, CoverageError, StoreBatches,
    StoreFinding,
};
use rt_demo::{
    check_golden, check_manifest, copy_demo, parse_runbook, run_runbook, run_step, Captured, Mode,
    Runbook, Step,
};
use rt_foundation::InputId;
use rt_hash::sha256;
use rt_ir::parse_rulebook;
use rt_json::{parse, Kind, ParseLimits, Position, Value};
use rt_ledger::{parse_ledger, RecordKind};
use rt_proposals::{parse_batch, Batch};
use rt_srcmodel::{
    default_load_limits, parse_candidates, CandidateClause, SourceModel, SourceModels,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Json = Value<Position>;

const PROFILE: &str = "ai";
const REJECTION_REASON: &str = "The clause says under 18; the proposal tests age at most 18, \
                                which makes an entrant aged exactly 18 a junior.";

fn demo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("demo")
        .join("stand-in-ai")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn bin_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(dir) = std::env::var_os("RT_BIN_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    for (package, bin) in [("rt-cli", "ruletrace"), ("rt-ai-tool", "ruletrace-ai")] {
        let status = Command::new(&cargo)
            .args(["build", "--quiet", "-p", package, "--bin", bin])
            .status()?;
        assert!(status.success(), "building {bin} failed");
    }
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
        let dir = std::env::temp_dir().join(format!("rt-demo-ai-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn json(label: &str, bytes: &[u8]) -> Json {
    let input = InputId::new(label).unwrap();
    parse(&input, bytes, ParseLimits::new(8 * 1024 * 1024, 64))
        .unwrap_or_else(|d| panic!("{label}: {}", d.message()))
}

fn at<'a>(v: &'a Json, key: &str) -> &'a Json {
    v.get(key)
        .map(|m| &m.value)
        .unwrap_or_else(|| panic!("missing key '{key}'"))
}

fn text(v: &Json) -> &str {
    v.as_str().expect("a string")
}

fn items(v: &Json) -> &[Json] {
    v.as_array().expect("an array")
}

fn same(a: &Json, b: &Json) -> bool {
    match (a.kind(), b.kind()) {
        (Kind::Null, Kind::Null) => true,
        (Kind::Bool(x), Kind::Bool(y)) => x == y,
        (Kind::Integer(x), Kind::Integer(y)) => x == y,
        (Kind::Number(x), Kind::Number(y)) | (Kind::String(x), Kind::String(y)) => x == y,
        (Kind::Array(x), Kind::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Kind::Object(x), Kind::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|(p, q)| p.key == q.key && same(&p.value, &q.value))
        }
        _ => false,
    }
}

fn output_text(c: &Captured) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&c.stdout),
        String::from_utf8_lossy(&c.stderr)
    )
}

/// The catalogue identifiers on standard error, sorted.
fn codes(c: &Captured) -> Vec<String> {
    let known = ["error", "warning", "ambiguity", "unsupported"];
    let mut found: Vec<String> = String::from_utf8_lossy(&c.stderr)
        .lines()
        .filter_map(|line| {
            let (severity, rest) = line.split_once(' ')?;
            let id = rest.split_once(':')?.0;
            (known.contains(&severity) && id.starts_with("RT-")).then(|| id.to_owned())
        })
        .collect();
    found.sort();
    found
}

/// Adds RT-AI-0309 where the platform skips the directory sync of a stored
/// batch (Req 7.17, 29.19). The expectation is written per platform.
fn with_platform_skip(mut expected: Vec<&str>) -> Vec<String> {
    if cfg!(windows) {
        expected.push("RT-AI-0309");
    }
    expected.sort_unstable();
    expected.into_iter().map(str::to_owned).collect()
}

fn step_by_id<'a>(runbook: &'a Runbook, id: &str) -> &'a Step {
    runbook
        .steps_for(PROFILE)
        .unwrap()
        .into_iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("no step {id}"))
}

fn run_expecting(step: &Step, exit: i32, bin: &Path, dir: &Path) -> Captured {
    let c = run_step(step, bin, dir).unwrap();
    assert_eq!(c.exit, exit, "step {}: {}", step.id, output_text(&c));
    c
}

/// A command that is not part of the runbook.
fn adhoc(id: &str, argv: &[&str]) -> Step {
    Step {
        id: id.to_owned(),
        profiles: Vec::new(),
        argv: argv.iter().map(|a| (*a).to_owned()).collect(),
        expect_exit: 0,
        cwd: ".".to_owned(),
        stdout_golden: String::new(),
        stderr_golden: String::new(),
        outputs: Vec::new(),
    }
}

fn dry_run_args() -> Vec<&'static str> {
    vec![
        "ruletrace-ai",
        "propose",
        "--config",
        "ai/mock-replay.toml",
        "--rulebook",
        "rulebook/seed.json",
        "--source-model",
        "work/source-model.json",
        "--candidates",
        "work/candidates.json",
        "--store",
        "work/store",
        "--created-at",
        "2026-01-15T09:00:00Z",
    ]
}

/// What the script holds: `(request_id, origin, response_sha256, payload)`.
fn script_entries(bytes: &[u8]) -> Result<Vec<(String, String, String, String)>, String> {
    let input = InputId::new("authored-script.json").map_err(|e| e.to_string())?;
    let doc = parse(&input, bytes, ParseLimits::new(8 * 1024 * 1024, 64))
        .map_err(|d| d.message().to_owned())?;
    let mut found = Vec::new();
    for entry in items(at(&doc, "entries")) {
        let id = text(at(entry, "request_id")).to_owned();
        let payload = text(at(entry, "payload")).to_owned();
        let recorded = text(at(entry, "response_sha256")).to_owned();
        let actual = sha256(payload.as_bytes());
        if actual.as_str() != recorded {
            return Err(format!(
                "request {id}: recorded hash {recorded} but the payload hashes to {actual}"
            ));
        }
        found.push((id, text(at(entry, "origin")).to_owned(), recorded, payload));
    }
    Ok(found)
}

/// Patterns of Req 28.4 that mark a credential-looking string.
fn credential_looking(content: &str) -> Option<&'static str> {
    let prefixes = ["sk-", "sk_", "Bearer ", "ghp_", "AKIA"];
    let words = ["api_key", "apikey", "authorization:", "-----BEGIN"];
    prefixes
        .iter()
        .chain(words.iter())
        .find(|p| content.contains(**p))
        .copied()
}

fn clauses_and_models(work: &Path) -> (Vec<CandidateClause>, SourceModels) {
    let limits = ParseLimits::new(8 * 1024 * 1024, 64);
    let model = SourceModel::load(
        &InputId::new("source-model.json").unwrap(),
        &std::fs::read(work.join("work/source-model.json")).unwrap(),
        default_load_limits(),
    )
    .map_err(|d| d.message().to_owned())
    .unwrap();
    let clauses = parse_candidates(
        &InputId::new("candidates.json").unwrap(),
        &std::fs::read(work.join("work/candidates.json")).unwrap(),
        limits,
        &model,
    )
    .unwrap();
    let mut models = SourceModels::new();
    models.insert(model).map_err(|_| "duplicate").unwrap();
    (clauses, models)
}

fn stored_batches(work: &Path) -> Vec<Batch> {
    let dir = work.join("work").join("store").join("batches");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    names.sort();
    names
        .iter()
        .map(|p| {
            let input = InputId::new("batch.json").unwrap();
            parse_batch(&input, &std::fs::read(p).unwrap())
                .map_err(|d| format!("{p:?}: {}", d[0].message()))
                .unwrap()
        })
        .collect()
}

#[test]
fn ai_profile_acceptance() -> TestResult {
    let demo = demo_dir();
    let bin = bin_dir()?;

    // 1. Manifest checks (Req 33.2, 33.13) and the authored-script checks
    // (Req 33.8).
    let manifest_bytes = std::fs::read(demo.join("manifest.json"))?;
    let manifest = check_manifest(&manifest_bytes, &|p| std::fs::read(demo.join(p)).ok())?;
    assert_eq!(manifest.name, "stand-in-ai");
    manifest.require_transmission("stand-in-rules.md")?;
    let manifest_json = json("manifest.json", &manifest_bytes);
    assert!(matches!(
        at(&manifest_json, "stand_in").kind(),
        Kind::Bool(true)
    ));
    let script_bytes = std::fs::read(demo.join("provider/authored-script.json"))?;
    let entries = script_entries(&script_bytes)?;
    assert_eq!(entries.len(), 2);
    let origins = at(at(&manifest_json, "provider_script"), "origins");
    assert_eq!(at(origins, "authored").as_u64(), Some(2));
    assert_eq!(at(origins, "recorded").as_u64(), Some(0));
    assert!(
        entries.iter().all(|(_, origin, _, _)| origin == "authored"),
        "every script entry is labelled authored"
    );
    let script_json = json("authored-script.json", &script_bytes);
    let captured = text(at(&script_json, "captured"));
    assert!(
        captured.contains("authored") && captured.contains("not provider output"),
        "{captured}"
    );
    for (name, content) in [
        ("script", String::from_utf8(script_bytes.clone())?),
        (
            "config",
            std::fs::read_to_string(demo.join("ai/mock-replay.toml"))?,
        ),
        ("runbook", std::fs::read_to_string(demo.join("RUNBOOK.md"))?),
    ] {
        assert_eq!(credential_looking(&content), None, "{name}");
    }
    let runbook_text = std::fs::read_to_string(demo.join("RUNBOOK.md"))?;
    assert!(
        !runbook_text.contains("recorded from") && !runbook_text.contains("provider output.\n"),
        "an authored entry is never described as provider output"
    );

    // 2 to 4. The runbook, step by step: exit codes and every output byte
    // against its golden file.
    let runbook = parse_runbook(&runbook_text)?;
    let bless = std::env::var("RT_DEMO_BLESS").ok();
    let ci = std::env::var("CI").ok();
    let mode = Mode::from_env_values(bless.as_deref(), ci.as_deref())?;
    let base = TempDir::new("acceptance");
    run_runbook(&runbook, PROFILE, &demo, &base.0, &bin, mode)?;
    let work = &base.0;

    // 5a. A script entry is keyed by the identifier that `--dry-run` prints
    // (Slice 1 exit criterion). The dry run uses the seed Rulebook before any
    // adoption, because the request carries the declarations of the Rulebook.
    let pre = TempDir::new("dry-run");
    copy_demo(&demo, &pre.0)?;
    run_expecting(step_by_id(&runbook, "s01"), 0, &bin, &pre.0);
    let mut dry = dry_run_args();
    dry.push("--dry-run");
    let printed = run_expecting(&adhoc("dry-run", &dry), 0, &bin, &pre.0);
    let printed_ids: BTreeSet<String> = String::from_utf8(printed.stdout)?
        .lines()
        .map(str::to_owned)
        .collect();
    let keys: BTreeSet<String> = entries.iter().map(|e| e.0.clone()).collect();
    assert_eq!(printed_ids.len(), 2);
    assert_eq!(printed_ids, keys);

    // 5b. The accepted response bytes sit in the stored batch, one batch per
    // script entry, and the Transmission_Log names each request without text.
    let batches = stored_batches(work);
    assert_eq!(batches.len(), 2);
    for batch in &batches {
        let entry = entries
            .iter()
            .find(|e| e.0 == batch.request_id.as_str())
            .unwrap_or_else(|| panic!("no script entry for {}", batch.request_id));
        assert_eq!(batch.response.payload, entry.3);
        assert_eq!(batch.response_sha256.as_str(), entry.2);
        assert_eq!(batch.created_at, "2026-01-15T09:00:00Z");
    }
    assert_eq!(
        std::fs::read_dir(work.join("work/store/batches"))?.count(),
        2
    );
    let log = std::fs::read_to_string(work.join("work/transmission.log.jsonl"))?;
    assert_eq!(log.lines().count(), 2);
    for key in &keys {
        assert!(log.contains(key.as_str()), "{key} is not logged");
    }
    assert!(
        !log.contains("entrant") && !log.contains("prize"),
        "document text in the Transmission_Log"
    );

    // 5c. The ledger holds the recorded decisions: three approvals in
    // dependency order, then one rejection with its reason (Req 33.11).
    let limits = ParseLimits::new(8 * 1024 * 1024, 64);
    let ledger = parse_ledger(
        &InputId::new("ledger.jsonl")?,
        &std::fs::read(work.join("work/ledger.jsonl"))?,
        limits,
    )
    .map_err(|_| "ledger rejected")?;
    let decisions: Vec<(RecordKind, String)> = ledger
        .records()
        .iter()
        .map(|r| (r.kind(), r.rule().as_str().to_owned()))
        .collect();
    assert_eq!(
        decisions,
        [
            (RecordKind::Approval, "elig.entry".to_owned()),
            (RecordKind::Approval, "score.weighted".to_owned()),
            (RecordKind::Approval, "rank.standing".to_owned()),
            (RecordKind::Rejection, "junior.flag".to_owned()),
        ]
    );
    assert!(ledger
        .records()
        .iter()
        .all(|r| r.reviewer().as_str() == "Demo Reviewer"));
    assert_eq!(ledger.records()[3].reason(), Some(REJECTION_REASON));
    for batch in &batches {
        assert!(
            batch
                .ai_identities
                .iter()
                .all(|i| i.as_str() != "Demo Reviewer"),
            "the Reviewer is not an AI identity"
        );
    }

    // 5d. The Coverage_Statement (Req 33.6), with independently derived
    // expectations: offsets come from the committed document, not from the
    // tools.
    let (clauses, models) = clauses_and_models(work);
    let staged = parse_rulebook(
        &std::fs::read(work.join("rulebook/seed.json"))?,
        &InputId::new("seed.json")?,
        8 * 1024 * 1024,
        64,
    )
    .map_err(|_| "rulebook rejected")?;
    let (rulebook, _) = staged.into_parts();
    let notes = parse_notes(&std::fs::read(demo.join("coverage-notes.json"))?)?;
    let store = StoreBatches::from_batches(&batches, &rulebook);
    assert_eq!(
        store.pending.len(),
        1,
        "only the unsupported proposal is still pending"
    );
    let validation = rt_validate::validate(&rulebook, &ledger, &models, Some(&clauses));
    let statement = classify(&clauses, &rulebook, &ledger, &store, &validation, &notes)?;
    let document = std::fs::read_to_string(demo.join("documents/stand-in-rules.md"))?;
    let expected_starts: Vec<u32> = [
        "1. An entrant is eligible",
        "2. The score is",
        "3. Entrants are ranked",
        "4. The top three entrants",
        "5. An entrant is a junior",
    ]
    .iter()
    .map(|p| u32::try_from(document.find(p).unwrap()).unwrap())
    .collect();
    let starts: Vec<u32> = statement
        .entries
        .iter()
        .map(|e| e.clause.span().start().get())
        .collect();
    assert_eq!(starts, expected_starts);
    let classes: Vec<&CoverageClass> = statement.entries.iter().map(|e| &e.class).collect();
    let approved = |id: &str| CoverageClass::RuleApproved {
        rules: vec![(id.to_owned(), 1)],
    };
    assert_eq!(classes[0], &approved("elig.entry"));
    assert_eq!(classes[1], &approved("score.weighted"));
    assert_eq!(classes[2], &approved("rank.standing"));
    assert_eq!(
        classes[3],
        &CoverageClass::Unsupported {
            capability: "prize_allocation".to_owned()
        }
    );
    assert_eq!(
        classes[4],
        &CoverageClass::ProposalRejected {
            reason: Some(REJECTION_REASON.to_owned()),
            rules: vec![("junior.flag".to_owned(), 1)],
        }
    );
    // The class agrees with the validation run: only the unsupported clause is
    // reported unreferenced by RT-ING-0006.
    let validate_stderr = std::fs::read_to_string(demo.join("golden/steps/s04.stderr"))?;
    assert_eq!(validate_stderr.matches("RT-ING-0006").count(), 1);
    assert!(validate_stderr.contains("427..500"));
    let candidates = json(
        "candidates.json",
        &std::fs::read(work.join("work/candidates.json"))?,
    );
    assert_eq!(
        items(at(&candidates, "clauses")).len(),
        statement.entries.len()
    );
    let dir_mode = Mode::from_env_values(bless.as_deref(), ci.as_deref())?;
    for (name, content) in [
        ("coverage-statement.json", print_json(&statement)),
        ("coverage-statement.md", print_markdown(&statement)),
    ] {
        check_golden(
            dir_mode,
            "coverage",
            &demo,
            &format!("golden/artifacts/{name}"),
            content.as_bytes(),
        )?;
    }

    // 5e. Every Decision_Trace is verified, the proof options do not change
    // the results (Req 13.19), and the Outcomes equal those of the `hand`
    // profile, whose goldens were reviewed separately.
    let results_bytes = std::fs::read(work.join("work/results.json"))?;
    let results = json("results.json", &results_bytes);
    let verification = items(at(&results, "verification"));
    assert_eq!(verification.len(), items(at(&results, "traces")).len());
    assert!(!verification.is_empty());
    assert!(verification
        .iter()
        .all(|v| text(at(v, "status")) == "verified"));
    assert_eq!(
        results_bytes,
        std::fs::read(work.join("work/results.proof-run.json"))?
    );
    let proof = json("proof.json", &std::fs::read(work.join("work/proof.json"))?);
    let sections = items(at(&proof, "decisions"));
    assert_eq!(sections.len(), verification.len());
    assert!(sections
        .iter()
        .all(|s| text(at(at(s, "verification"), "status")) == "verified"));
    let hand_results = json(
        "hand-results.json",
        &std::fs::read(
            repo_root()
                .join("demo")
                .join("stand-in-hand")
                .join("golden")
                .join("artifacts")
                .join("results.json"),
        )?,
    );
    assert!(
        same(at(&results, "outcomes"), at(&hand_results, "outcomes")),
        "the ai and hand profiles decide alike"
    );
    let s06b = std::fs::read_to_string(demo.join("golden/steps/s06b.stderr"))?;
    assert!(s06b.contains("warning RT-EXE-0011"));

    // 5f. The dependency tree of `ruletrace` has no network, provider or AI
    // crate.
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let metadata = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--all-features",
            "--offline",
        ])
        .current_dir(repo_root())
        .output()?;
    assert!(
        metadata.status.success(),
        "cargo metadata failed ({}): {}",
        metadata.status,
        String::from_utf8_lossy(&metadata.stderr)
    );
    let members = rt_xtask::run_graph_check(
        &repo_root().canonicalize()?,
        &metadata.stdout,
        rt_xtask::GraphCheck::Tree,
    )?;
    assert!(members > 0);

    println!("\nstep  exit  command");
    for s in runbook.steps_for(PROFILE)? {
        let words: Vec<&str> = s.argv.iter().take(3).map(String::as_str).collect();
        println!("{:<5} {:<5} {}", s.id, s.expect_exit, words.join(" "));
    }
    println!("\nCoverage_Statement:\n{}", print_markdown(&statement));
    println!("script entries are authored (not provider output); the document is a stand-in\n");
    Ok(())
}

#[test]
fn ai_profile_a_rule_that_reads_an_unadopted_target_is_refused_and_nothing_changes() -> TestResult {
    let demo = demo_dir();
    let bin = bin_dir()?;
    let runbook = parse_runbook(&std::fs::read_to_string(demo.join("RUNBOOK.md"))?)?;
    let dir = TempDir::new("order");
    copy_demo(&demo, &dir.0)?;
    for id in ["s01", "s02", "s03a"] {
        run_expecting(step_by_id(&runbook, id), 0, &bin, &dir.0);
    }
    let ledger_before = std::fs::read(dir.0.join("work/ledger.jsonl"))?;
    let rulebook_before = std::fs::read(dir.0.join("rulebook/seed.json"))?;
    // s03d approves rank.standing, which reads the target of score.weighted.
    let refused = run_expecting(step_by_id(&runbook, "s03d"), 3, &bin, &dir.0);
    assert_eq!(codes(&refused), ["RT-APR-0205"]);
    assert_eq!(
        std::fs::read(dir.0.join("work/ledger.jsonl"))?,
        ledger_before
    );
    assert_eq!(
        std::fs::read(dir.0.join("rulebook/seed.json"))?,
        rulebook_before
    );
    Ok(())
}

#[test]
fn ai_profile_a_request_the_script_does_not_hold_is_a_provider_error_and_keeps_earlier_batches(
) -> TestResult {
    let demo = demo_dir();
    let bin = bin_dir()?;
    let runbook = parse_runbook(&std::fs::read_to_string(demo.join("RUNBOOK.md"))?)?;
    let dir = TempDir::new("missing-entry");
    copy_demo(&demo, &dir.0)?;
    run_expecting(step_by_id(&runbook, "s01"), 0, &bin, &dir.0);
    // Keep only the first entry: the second request has no answer.
    let script_path = dir.0.join("provider/authored-script.json");
    let entries = script_entries(&std::fs::read(&script_path)?)?;
    let (kept, dropped) = (entries[0].clone(), entries[1].0.clone());
    let entry = Value::object(vec![
        ("request_id".to_owned(), Value::string(&kept.0)),
        ("origin".to_owned(), Value::string(&kept.1)),
        ("completion".to_owned(), Value::string("complete")),
        ("response_sha256".to_owned(), Value::string(&kept.2)),
        ("payload".to_owned(), Value::string(&kept.3)),
    ])?;
    let original = json("script.json", &std::fs::read(&script_path)?);
    let script = Value::object(vec![
        (
            "format".to_owned(),
            Value::string("ruletrace-recorded-script"),
        ),
        ("version".to_owned(), Value::integer(1)),
        (
            "provider".to_owned(),
            Value::object(vec![
                ("kind".to_owned(), Value::string("mock-replay")),
                ("model".to_owned(), Value::string("authored-script")),
                ("adapter_version".to_owned(), Value::string("mock-1")),
            ])?,
        ),
        (
            "captured".to_owned(),
            Value::string(text(at(&original, "captured"))),
        ),
        ("entries".to_owned(), Value::array(vec![entry])),
    ])?;
    std::fs::write(&script_path, rt_json::write_compact(&script))?;
    assert_eq!(script_entries(&std::fs::read(&script_path)?)?.len(), 1);

    let ran = run_step(step_by_id(&runbook, "s02"), &bin, &dir.0)?;
    assert_eq!(ran.exit, 5, "{}", output_text(&ran));
    assert_eq!(
        codes(&ran),
        with_platform_skip(vec!["RT-AI-0105", "RT-AI-0305", "RT-AI-0306"])
    );
    let stdout = String::from_utf8(ran.stdout.clone())?;
    assert!(stdout.starts_with("run summary: incomplete\n"), "{stdout}");
    let stderr = String::from_utf8(ran.stderr.clone())?;
    assert!(stderr.contains(&dropped), "the missing request is named");
    let stored: Vec<String> = std::fs::read_dir(dir.0.join("work/store/batches"))?
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(stored, [format!("{}.json", kept.0)]);
    Ok(())
}

#[test]
fn ai_profile_a_script_entry_whose_hash_differs_is_reported_with_its_request_identifier(
) -> TestResult {
    let bytes = std::fs::read(demo_dir().join("provider/authored-script.json"))?;
    let entries = script_entries(&bytes)?;
    let victim = &entries[0];
    let original = String::from_utf8(bytes)?;
    let tampered = original.replacen("Authored entry: clause 1", "Authored entry: clause 9", 1);
    assert_ne!(tampered, original);
    let error = script_entries(tampered.as_bytes()).unwrap_err();
    assert!(error.contains(&victim.0), "{error}");
    assert!(error.contains(&victim.2), "{error}");
    Ok(())
}

#[test]
fn ai_profile_a_credential_looking_string_is_found() {
    assert_eq!(credential_looking("key sk-abcdef"), Some("sk-"));
    assert_eq!(credential_looking("Authorization: x"), None);
    assert_eq!(
        credential_looking("authorization: x"),
        Some("authorization:")
    );
    assert_eq!(credential_looking("entrants share prize money"), None);
}

#[test]
fn ai_profile_coverage_names_the_unclassified_clause_without_the_store_or_the_rejection(
) -> TestResult {
    let demo = demo_dir();
    let bin = bin_dir()?;
    let runbook = parse_runbook(&std::fs::read_to_string(demo.join("RUNBOOK.md"))?)?;
    let base = TempDir::new("coverage-negative");
    run_runbook(&runbook, PROFILE, &demo, &base.0, &bin, Mode::Compare)?;
    let work = &base.0;
    let (clauses, models) = clauses_and_models(work);
    let limits = ParseLimits::new(8 * 1024 * 1024, 64);
    let (rulebook, _) = parse_rulebook(
        &std::fs::read(work.join("rulebook/seed.json"))?,
        &InputId::new("seed.json")?,
        8 * 1024 * 1024,
        64,
    )
    .map_err(|_| "rulebook rejected")?
    .into_parts();
    let ledger = parse_ledger(
        &InputId::new("ledger.jsonl")?,
        &std::fs::read(work.join("work/ledger.jsonl"))?,
        limits,
    )
    .map_err(|_| "ledger rejected")?;
    let notes = parse_notes(&std::fs::read(demo.join("coverage-notes.json"))?)?;
    let validation = rt_validate::validate(&rulebook, &ledger, &models, Some(&clauses));

    // Without the store, the unsupported clause is the first one no row covers.
    let err = classify(
        &clauses,
        &rulebook,
        &ledger,
        &StoreBatches::default(),
        &validation,
        &notes,
    )
    .unwrap_err();
    match err {
        CoverageError::Unclassified {
            document,
            start,
            line,
            label,
        } => {
            assert_eq!(document, "stand-in-rules.md");
            assert_eq!(start, 427);
            assert_eq!(line, 8);
            assert_eq!(label.as_deref(), Some("4."));
        }
        other => panic!("{other:?}"),
    }

    // The finding of the unsupported proposal names the capability of the
    // draft, not the generic kind of the stored message.
    let batches = stored_batches(work);
    let store = StoreBatches::from_batches(&batches, &rulebook);
    assert_eq!(
        store.pending[0].finding,
        Some(StoreFinding::Unsupported {
            capability: "prize_allocation".to_owned()
        })
    );
    Ok(())
}

#[test]
fn ai_profile_windows_goldens_differ_from_the_others_only_by_the_skipped_directory_sync(
) -> TestResult {
    let steps = demo_dir().join("golden").join("steps");
    let clauses = "  stand-in-rules.md 161 241\n  stand-in-rules.md 242 320\n  \
                   stand-in-rules.md 321 426\n  stand-in-rules.md 427 500\n  \
                   stand-in-rules.md 501 546\n";
    let processed = format!("processed:\n{clauses}");
    let listed = clauses;
    let other_stdout =
        format!("run summary: complete\n{processed}stored:\n{listed}unprocessed:\n  (none)\n");
    let windows_stdout = format!(
        "run summary: complete, directory durability not confirmed\n{processed}stored:\n  \
         (none)\nstored, directory durability not confirmed (skipped by platform):\n{listed}\
         unprocessed:\n  (none)\n"
    );
    assert_eq!(
        std::fs::read_to_string(steps.join("s02.stdout"))?,
        other_stdout
    );
    assert_eq!(
        std::fs::read_to_string(steps.join("s02.stdout.windows"))?,
        windows_stdout
    );
    let stderr = std::fs::read_to_string(steps.join("s02.stderr"))?;
    assert!(!stderr.contains("RT-AI-0309"));
    let skip = "warning RT-AI-0309: the platform skipped the directory sync of a stored batch; \
                the durability of its directory entry is not confirmed\n  at command 'propose'\n";
    assert_eq!(
        std::fs::read_to_string(steps.join("s02.stderr.windows"))?,
        format!("{stderr}{skip}")
    );
    Ok(())
}
