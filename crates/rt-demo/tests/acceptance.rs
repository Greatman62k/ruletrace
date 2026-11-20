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
    check_golden, check_manifest, copy_demo, describe_evaluate, describe_ledger, parse_runbook,
    run_runbook, run_runbook_recorded, run_step, Captured, Mode, Runbook, Step,
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

/// Every file below `dir` with its bytes, keyed by the relative path.
fn tree(dir: &Path, relative: &Path, into: &mut std::collections::BTreeMap<String, Vec<u8>>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        let name = relative.join(path.file_name().unwrap());
        if path.is_dir() {
            tree(&path, &name, into);
        } else {
            into.insert(
                name.to_string_lossy().replace('\\', "/"),
                std::fs::read(&path).unwrap(),
            );
        }
    }
}

/// The argument that follows `flag` in a step.
fn option<'a>(step: &'a Step, flag: &str) -> Option<&'a str> {
    step.argv
        .iter()
        .position(|a| a == flag)
        .and_then(|i| step.argv.get(i + 1))
        .map(String::as_str)
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

const RANKING_LABEL: &str = "Score ranking \u{2014} all submitted participants; eligibility is evaluated independently and is not applied as a ranking filter.";

/// The report states, once per standings decision, that the ranking covers
/// all submitted participants, and p3 (ineligible in the demo data) is listed.
fn assert_ranking_label(report: &str, proof: &Json) {
    let standings: Vec<&Json> = items(at(proof, "decisions"))
        .iter()
        .filter(|s| {
            at(at(s, "outcome"), "state").as_str() == Some("decided")
                && text(at(at(at(s, "outcome"), "value"), "type")) == "standings"
        })
        .collect();
    assert!(!standings.is_empty(), "no ranking decision in the report");
    assert_eq!(report.matches(RANKING_LABEL).count(), standings.len());
    for s in standings {
        let members: Vec<&str> = items(at(at(at(s, "outcome"), "value"), "places"))
            .iter()
            .flat_map(|p| items(at(p, "members")))
            .map(text)
            .collect();
        assert!(members.contains(&"p3"), "ineligible p3 is not ranked");
    }
    assert!(report.contains("4: p3, p5]"));
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
    let records = run_runbook_recorded(&runbook, PROFILE, &demo, &base.0, &bin, mode)?;
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

    // 5a2. `review list` before the first decision (Req 26.6 as amended): it
    // lists all five pending proposals in script order, comes before the
    // first approval in the runbook, and creates and changes no file.
    let position = |id: &str| records.iter().position(|r| r.id == id).unwrap();
    assert!(position("s02") < position("s02b") && position("s02b") < position("s03a"));
    assert!(records.iter().all(|r| r.id != "s03b"), "s03b is retired");
    let listing = &records[position("s02b")].captured;
    assert!(listing.stderr.is_empty());
    let listed = String::from_utf8(listing.stdout.clone())?;
    let listed_rules: Vec<&str> = listed
        .lines()
        .filter(|l| l.starts_with("pending proposal "))
        .map(|l| l.split('\'').nth(3).unwrap())
        .collect();
    assert_eq!(
        listed_rules,
        [
            "elig.entry",
            "score.weighted",
            "rank.standing",
            "prize.split",
            "junior.flag"
        ]
    );
    let early = TempDir::new("list-first");
    copy_demo(&demo, &early.0)?;
    for id in ["s01", "s02"] {
        run_expecting(step_by_id(&runbook, id), 0, &bin, &early.0);
    }
    let (mut before, mut after) = (Default::default(), Default::default());
    tree(&early.0, Path::new(""), &mut before);
    let again = run_expecting(step_by_id(&runbook, "s02b"), 0, &bin, &early.0);
    tree(&early.0, Path::new(""), &mut after);
    assert!(before == after, "`review list` changed or created a file");
    assert!(!early.0.join("work/ledger.jsonl").exists());
    assert_eq!(again.stdout, listing.stdout);

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
    {
        let report = std::fs::read_to_string(work.join("work/proof.txt"))?;
        let ranking: Vec<&str> = report
            .split("\nDECISION ")
            .filter(|section| section.contains("decided standings"))
            .collect();
        println!("\nranking decision of the Proof_Report (s07):");
        for section in ranking {
            println!("DECISION {section}");
        }
    }
    assert_ranking_label(
        &std::fs::read_to_string(work.join("work/proof.txt"))?,
        &proof,
    );
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

    // 5e2. Origin independence (Req 16.18): the hand-authored Rulebook with
    // the same Rule_Content and Revision_Numbers compiles and evaluates to the
    // bytes this AI-origin flow just produced.
    let authored = TempDir::new("hand-authored");
    rt_demo::copy_demo(&demo, &authored.0)?;
    let hand = &authored.0;
    let rb = "rulebook/hand-authored.json";
    let common = ["--source-model", "work/source-model.json"];
    run_expecting(
        &adhoc(
            "o01",
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
        ),
        0,
        &bin,
        hand,
    );
    let approvals = [
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
    for (i, (rule, hash)) in approvals.iter().enumerate() {
        let mut words = vec![
            "ruletrace",
            "review",
            "approve",
            "--rulebook",
            rb,
            "--ledger",
            "work/ledger.jsonl",
            common[0],
            common[1],
            "--rule",
            rule,
            "--revision",
            "1",
            "--content-hash",
            hash,
            "--reviewer",
            "Demo Reviewer",
            "--time",
            "2026-01-15T10:00:00Z",
        ];
        if i == 0 {
            words.push("--create-ledger");
        }
        run_expecting(&adhoc("o02", &words), 0, &bin, hand);
    }
    for (id, exit, words) in [
        (
            "o03",
            0,
            vec![
                "ruletrace",
                "validate",
                "--rulebook",
                rb,
                "--ledger",
                "work/ledger.jsonl",
                common[0],
                common[1],
                "--candidates",
                "work/candidates.json",
                "--out",
                "work/validation.json",
            ],
        ),
        (
            "o04",
            0,
            vec![
                "ruletrace",
                "compile",
                "--validation",
                "work/validation.json",
                "--out",
                "work/compiled.json",
            ],
        ),
        (
            "o05",
            4,
            vec![
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
                common[0],
                common[1],
                "--out",
                "work/results.json",
            ],
        ),
    ] {
        run_expecting(&adhoc(id, &words), exit, &bin, hand);
    }
    let mut equality = Vec::new();
    for name in ["compiled.json", "results.json"] {
        let hand_bytes = std::fs::read(hand.join("work").join(name))?;
        let ai_bytes = std::fs::read(work.join("work").join(name))?;
        assert!(
            hand_bytes == ai_bytes,
            "{name}: the hand-authored and AI-origin artifacts differ"
        );
        equality.push(format!(
            "{name}: {} bytes, sha256 {}, identical from the AI-origin flow and from the hand-authored flow",
            ai_bytes.len(),
            sha256(&ai_bytes)
        ));
    }

    // 5e3. The happy path (Req 16 criterion 13): over `data-complete.json`
    // every Outcome is decided and `evaluate` exits 0. The expected values were
    // worked out by hand from the rules, not read from the tool:
    //   score = 0.6 * judges + 0.4 * technical
    //   p1 17/8/7: 4.8 + 2.8 = 7.6   p2 20/7/9: 4.2 + 3.6 = 7.8
    //   p3 15/6/6: 3.6 + 2.4 = 6     p4 21/9/6: 5.4 + 2.4 = 7.8
    //   p5 19/6/6: 3.6 + 2.4 = 6
    //   eligible = age >= 16 and licence: only p3 (age 15) is not.
    //   ranking: score descending, then technical descending: p2 (7.8, 9),
    //   p4 (7.8, 6), p1 (7.6), then p3 and p5 tie on 6 and technical 6.
    let expected_complete = [
        ("eligible p1", "true"),
        ("eligible p2", "true"),
        ("eligible p3", "false"),
        ("eligible p4", "true"),
        ("eligible p5", "true"),
        ("score p1", "7.6"),
        ("score p2", "7.8"),
        ("score p3", "6"),
        ("score p4", "7.8"),
        ("score p5", "6"),
        ("standing", "1:p2 2:p4 3:p1 4:p3=p5"),
    ];
    for (step_id, out) in [
        ("s06c", "work/results-complete.json"),
        ("s07b", "work/results-complete.proof-run.json"),
    ] {
        let record = &records[position(step_id)];
        assert_eq!(step_by_id(&runbook, step_id).expect_exit, 0);
        assert_eq!(record.captured.exit, 0, "{step_id}");
        assert!(
            record.captured.stderr.is_empty(),
            "{step_id} has diagnostics"
        );
        let complete = json(out, &std::fs::read(work.join(out))?);
        let mut found = Vec::new();
        for (trace, outcome) in items(at(&complete, "traces"))
            .iter()
            .zip(items(at(&complete, "outcomes")))
        {
            assert_eq!(text(at(outcome, "state")), "decided", "{step_id}");
            let question = at(trace, "question");
            let label = match question.get("participant").and_then(|m| m.value.as_str()) {
                Some(p) => format!("{} {p}", text(at(question, "target"))),
                None => text(at(question, "target")).to_owned(),
            };
            let value = at(outcome, "value");
            let described = if text(at(value, "type")) == "standings" {
                items(at(value, "places"))
                    .iter()
                    .map(|p| {
                        let members: Vec<&str> = items(at(p, "members")).iter().map(text).collect();
                        format!(
                            "{}:{}",
                            at(p, "position").as_u64().unwrap(),
                            members.join("=")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                match at(value, "value").kind() {
                    Kind::Bool(b) => b.to_string(),
                    Kind::String(s) => s.clone(),
                    other => panic!("unexpected value {other:?}"),
                }
            };
            found.push((label, described));
        }
        let wanted: Vec<(String, String)> = expected_complete
            .iter()
            .map(|(q, v)| ((*q).to_owned(), (*v).to_owned()))
            .collect();
        assert_eq!(found, wanted, "{step_id}");
        assert!(items(at(&complete, "diagnostics")).is_empty(), "{step_id}");
        let verification = items(at(&complete, "verification"));
        assert_eq!(verification.len(), 11, "{step_id}");
        assert!(
            verification
                .iter()
                .all(|v| text(at(v, "status")) == "verified"),
            "{step_id}: a trace is not verified"
        );
    }
    assert_eq!(
        std::fs::read(work.join("work/results-complete.json"))?,
        std::fs::read(work.join("work/results-complete.proof-run.json"))?
    );
    let complete_proof = json(
        "proof-complete.json",
        &std::fs::read(work.join("work/proof-complete.json"))?,
    );
    assert_ranking_label(
        &std::fs::read_to_string(work.join("work/proof-complete.txt"))?,
        &complete_proof,
    );
    assert_eq!(
        at(at(&complete_proof, "header"), "verified_traces").as_u64(),
        Some(11)
    );
    assert!(items(at(&complete_proof, "decisions"))
        .iter()
        .all(|s| text(at(at(s, "verification"), "status")) == "verified"));
    // The data differ from the canonical data only by the age of p2.
    let canonical = json(
        "data.json",
        &std::fs::read(demo.join("data/canonical/data.json"))?,
    );
    let complete_data = json(
        "data-complete.json",
        &std::fs::read(demo.join("data/canonical/data-complete.json"))?,
    );
    let (old_p, new_p) = (
        items(at(&canonical, "participants")),
        items(at(&complete_data, "participants")),
    );
    assert_eq!(old_p.len(), new_p.len());
    for (o, n) in old_p.iter().zip(new_p) {
        let id = text(at(o, "id"));
        assert_eq!(id, text(at(n, "id")));
        let Kind::Object(new_fields) = at(n, "fields").kind() else {
            panic!("fields of {id} is not an object")
        };
        for member in new_fields {
            match at(o, "fields").get(&member.key) {
                Some(old) => assert!(same(&old.value, &member.value), "{id}.{}", member.key),
                None => assert_eq!(
                    (id, member.key.as_str(), text(&member.value)),
                    ("p2", "age", "20")
                ),
            }
        }
    }

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

    println!("\nSLICE 1 SUMMARY, profile `ai`");
    println!(
        "\nTHIS IS A STAND-IN. The document below was written for the RuleTrace project. It is not \
         the rules of any real competition."
    );
    println!(
        "THE SCRIPT ENTRIES ARE AUTHORED. No AI provider produced them: a person wrote them by hand, \
         and the mock provider replays them offline (no network, no credential)."
    );
    println!("\nstand-in document (documents/stand-in-rules.md):\n{document}");
    println!("proposals (read from the stored batches in work/store/batches):");
    for batch in &batches {
        let origin = entries
            .iter()
            .find(|e| e.0 == batch.request_id.as_str())
            .map_or("?", |e| e.1.as_str());
        println!(
            "  Request_Identifier {} (script entry origin: {origin}, provider {}, model {})",
            batch.request_id, batch.provider, batch.model
        );
        for proposal in &batch.proposals {
            let rule = proposal
                .rule
                .as_ref()
                .map_or_else(|| "(no rule)".to_owned(), |r| r.0.id.to_string());
            println!(
                "    proposal {} kind {} rule {rule}",
                proposal.proposal_id,
                proposal.kind.as_str()
            );
            for e in &proposal.evidence {
                println!(
                    "      source span {} bytes {}..{} (line {}, column {}): \"{}\"",
                    e.document, e.start, e.end, e.line, e.column, e.quote
                );
            }
        }
    }
    println!("\nreview steps (each a human decision recorded by `ruletrace review`):");
    for s in runbook.steps_for(PROFILE)? {
        if s.argv.get(1).map(String::as_str) != Some("review") {
            continue;
        }
        let verb = s.argv.get(2).map_or("", String::as_str);
        match verb {
            "list" => println!("  {}: review list", s.id),
            _ => {
                let reason =
                    option(s, "--reason").map_or(String::new(), |r| format!(", reason: {r}"));
                println!(
                    "  {}: review {verb} {} @ {} by \"{}\" at {}{reason}",
                    s.id,
                    option(s, "--rule").unwrap_or("?"),
                    option(s, "--revision").unwrap_or("?"),
                    option(s, "--reviewer").unwrap_or("?"),
                    option(s, "--time").unwrap_or("?"),
                );
            }
        }
    }
    println!("\ndecisions in the ledger (work/ledger.jsonl):");
    for line in describe_ledger(&std::fs::read(work.join("work/ledger.jsonl"))?)? {
        println!("  {line}");
    }
    println!("\nstep  exit  command");
    for s in runbook.steps_for(PROFILE)? {
        let words: Vec<&str> = s.argv.iter().take(3).map(String::as_str).collect();
        println!("{:<5} {:<5} {}", s.id, s.expect_exit, words.join(" "));
    }
    println!(
        "\nevaluate steps (exit 4 = at least one Outcome is Indeterminate, exit 0 = all decided):"
    );
    for record in &records {
        let step = step_by_id(&runbook, &record.id);
        if step.argv.get(1).map(String::as_str) != Some("evaluate") {
            continue;
        }
        let out = option(step, "--out").expect("evaluate has --out");
        let described = describe_evaluate(
            &record.id,
            record.captured.exit,
            &std::fs::read(work.join(out))?,
            &record.captured.stderr,
        )?;
        println!("\n{described}");
    }
    println!("Coverage_Statement:\n{}", print_markdown(&statement));
    println!("compiled and evaluated artifacts of the ai flow and of the hand-authored flow:");
    for line in &equality {
        println!("  {line}");
    }
    println!("\nscript entries are authored (not provider output); the document is a stand-in\n");
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

/// A string, not a credential. No eight-byte window of it is likely in
/// ordinary output.
const CANARY: &str = "rtc-Zq7x93LmN-Rt5vK2pWd8Hc4Jf1Bs6Ug0Ya3Xe9";
const CREDENTIAL_VARIABLES: [&str; 3] = ["OPENAI_API_KEY", "RULETRACE_AI_KEY", "RT_CANARY_KEY"];

/// What the canary check scans: each command's output and every file of the
/// working directory.
struct Seen {
    outputs: Vec<(String, Captured)>,
    files: std::collections::BTreeMap<String, Vec<u8>>,
}

/// The surfaces that hold the canary, whole or as a fragment of eight bytes.
fn canary_leaks(seen: &Seen) -> Vec<String> {
    let secret = CANARY.as_bytes();
    let mut leaks = Vec::new();
    for (id, c) in &seen.outputs {
        if rt_demo::leak::first_leak(secret, &c.stdout).is_some() {
            leaks.push(format!("stdout of {id}"));
        }
        if rt_demo::leak::first_leak(secret, &c.stderr).is_some() {
            leaks.push(format!("stderr of {id}"));
        }
    }
    for (name, bytes) in &seen.files {
        if rt_demo::leak::first_leak(secret, bytes).is_some() {
            leaks.push(format!("file {name}"));
        }
        if rt_demo::leak::first_leak(secret, name.as_bytes()).is_some() {
            leaks.push(format!("path {name}"));
        }
    }
    leaks
}

/// Runs a step with an empty environment except for the canary in every
/// credential variable.
fn run_with_canary(step: &Step, bin: &Path, dir: &Path) -> Captured {
    let program = rt_demo::binary_path(bin, &step.argv[0]);
    let mut command = Command::new(program);
    command
        .args(&step.argv[1..])
        .current_dir(dir.join(&step.cwd))
        .env_clear()
        .stdin(std::process::Stdio::null());
    for name in CREDENTIAL_VARIABLES {
        command.env(name, CANARY);
    }
    let output = command.output().unwrap();
    Captured {
        exit: output.status.code().expect("an exit code"),
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

#[test]
fn ai_profile_a_canary_credential_reaches_no_surface_of_the_runs() -> TestResult {
    let demo = demo_dir();
    let bin = bin_dir()?;
    let runbook = parse_runbook(&std::fs::read_to_string(demo.join("RUNBOOK.md"))?)?;
    let base = TempDir::new("canary");
    copy_demo(&demo, &base.0)?;
    let mut outputs = Vec::new();
    for step in runbook.steps_for(PROFILE)? {
        let c = run_with_canary(step, &bin, &base.0);
        assert_eq!(
            c.exit,
            step.expect_exit,
            "step {}: {}",
            step.id,
            output_text(&c)
        );
        // The credential variables change nothing: the mock provider names no
        // credential, so every output equals the golden file of the run
        // without them.
        check_golden(
            Mode::Compare,
            &step.id,
            &demo,
            &step.stdout_golden,
            &c.stdout,
        )?;
        check_golden(
            Mode::Compare,
            &step.id,
            &demo,
            &step.stderr_golden,
            &c.stderr,
        )?;
        for output in &step.outputs {
            check_golden(
                Mode::Compare,
                &step.id,
                &demo,
                &output.golden,
                &std::fs::read(base.0.join(&output.path))?,
            )?;
        }
        outputs.push((step.id.clone(), c));
    }
    let mut files = std::collections::BTreeMap::new();
    tree(&base.0, Path::new(""), &mut files);
    for needed in [
        "work/store/batches",
        "work/transmission.log.jsonl",
        "work/ledger.jsonl",
        "work/compiled.json",
    ] {
        assert!(
            files.keys().any(|k| k.starts_with(needed)),
            "the scan must cover {needed}"
        );
    }
    let seen = Seen { outputs, files };
    assert_eq!(canary_leaks(&seen), Vec::<String>::new());
    // The Run_Summary is the standard output of `propose`.
    let propose = &seen.outputs.iter().find(|(id, _)| id == "s02").unwrap().1;
    assert!(String::from_utf8_lossy(&propose.stdout).starts_with("run summary: complete"));

    // The check finds a leak into standard error, into a proposal-store file
    // and into the ledger, and into a file name. Each mutant is a copy of the
    // clean surfaces with one injected leak.
    let inject_stderr = |id: &str| {
        let mut outputs = seen.outputs.clone();
        let slot = outputs.iter_mut().find(|(i, _)| i == id).unwrap();
        slot.1.stderr.extend_from_slice(CANARY.as_bytes());
        Seen {
            outputs,
            files: seen.files.clone(),
        }
    };
    assert_eq!(canary_leaks(&inject_stderr("s02")), ["stderr of s02"]);
    let inject_file = |name: &str, tail: &[u8]| {
        let mut files = seen.files.clone();
        files.get_mut(name).unwrap().extend_from_slice(tail);
        Seen {
            outputs: seen.outputs.clone(),
            files,
        }
    };
    let batch = seen
        .files
        .keys()
        .find(|k| k.starts_with("work/store/batches/"))
        .unwrap()
        .clone();
    assert_eq!(
        canary_leaks(&inject_file(&batch, CANARY.as_bytes())),
        [format!("file {batch}")]
    );
    assert_eq!(
        canary_leaks(&inject_file("work/ledger.jsonl", &CANARY.as_bytes()[5..13])),
        ["file work/ledger.jsonl"]
    );
    assert_eq!(
        canary_leaks(&inject_file(
            "work/transmission.log.jsonl",
            &CANARY.as_bytes()[5..12]
        )),
        Vec::<String>::new(),
        "seven bytes are not a fragment"
    );
    let mut renamed = seen.files.clone();
    renamed.insert(format!("work/{CANARY}.tmp"), Vec::new());
    let named = Seen {
        outputs: seen.outputs.clone(),
        files: renamed,
    };
    assert_eq!(canary_leaks(&named), [format!("path work/{CANARY}.tmp")]);
    Ok(())
}

#[test]
fn ai_profile_a_configuration_with_an_inline_credential_is_refused_without_echo() -> TestResult {
    let demo = demo_dir();
    let bin = bin_dir()?;
    let runbook = parse_runbook(&std::fs::read_to_string(demo.join("RUNBOOK.md"))?)?;
    let dir = TempDir::new("inline-credential");
    copy_demo(&demo, &dir.0)?;
    run_expecting(step_by_id(&runbook, "s01"), 0, &bin, &dir.0);
    // The canary stands in for an inline credential. The configuration file
    // holds it on purpose, so only the outputs and the stores are scanned.
    let config = std::fs::read_to_string(demo.join("ai/mock-replay.toml"))?;
    let inline = dir.0.join("ai/inline.toml");
    std::fs::write(&inline, format!("{config}api_key = \"{CANARY}\"\n"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&inline, std::fs::Permissions::from_mode(0o600))?;
    }
    let mut words = dry_run_args();
    words.extend(["--consent", "stand-in-rules.md"]);
    let mut step = adhoc("inline", &words);
    step.argv[3] = "ai/inline.toml".to_owned();
    assert_eq!(step.argv[2], "--config");
    let c = run_with_canary(&step, &bin, &dir.0);
    assert_eq!(c.exit, 2, "{}", output_text(&c));
    assert!(
        String::from_utf8_lossy(&c.stderr).contains("RT-AI-0003"),
        "{}",
        output_text(&c)
    );
    let mut files = std::collections::BTreeMap::new();
    tree(&dir.0, Path::new(""), &mut files);
    files.retain(|name, _| name != "ai/inline.toml");
    assert!(
        !files
            .keys()
            .any(|k| k.starts_with("work/store") || k.starts_with("work/transmission")),
        "a refused run stores nothing and logs no transmission"
    );
    let seen = Seen {
        outputs: vec![("inline".to_owned(), c)],
        files,
    };
    assert_eq!(canary_leaks(&seen), Vec::<String>::new());
    Ok(())
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
