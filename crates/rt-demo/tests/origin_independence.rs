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
//! A second test states the agreement explicitly, field by field: the source
//! document bytes and hash, the Source_Model and Candidate_Clauses, the rule
//! identifiers, Revision_Numbers, Rule_Content and spans of the two Rulebooks
//! (they differ in `origin` and `name` only), the ledger records, the compiled
//! artifact and the evaluation results over both data files.
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
use rt_foundation::InputId;
use rt_hash::sha256;
use rt_json::{parse, Kind, ParseLimits, Position, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Json = Value<Position>;

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

/// Ingests the `ai` document, approves the three rules of the hand-authored
/// Rulebook, validates, compiles and evaluates over both data files.
fn run_hand_flow(bin: &Path, work: &Path) {
    ingest(bin, work);
    for (index, (rule, hash)) in APPROVALS.iter().enumerate() {
        let words = approve("rulebook/hand-authored.json", rule, hash, index == 0);
        let c = run_owned("approve", &words, bin, work);
        assert_eq!(c.exit, 0, "{rule}: {}", text(&c));
    }
    let evaluate = |data: &'static str, out: &'static str| -> Vec<&'static str> {
        vec![
            "ruletrace",
            "evaluate",
            "--compiled",
            "work/compiled.json",
            "--data",
            data,
            "--questions",
            "data/canonical/questions.json",
            "--published",
            "data/canonical/published.json",
            "--source-model",
            "work/source-model.json",
            "--out",
            out,
        ]
    };
    let steps: [(&str, Vec<&str>, i32); 4] = [
        (
            "validate",
            vec![
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
            vec![
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
            evaluate("data/canonical/data.json", "work/results.json"),
            4,
        ),
        (
            "evaluate-complete",
            evaluate(
                "data/canonical/data-complete.json",
                "work/results-complete.json",
            ),
            0,
        ),
    ];
    for (id, words, exit) in steps {
        let c = run(id, &words, bin, work);
        assert_eq!(c.exit, exit, "{id}: {}", text(&c));
    }
}

#[test]
fn hand_authored_rulebook_over_the_ai_document_gives_the_ai_bytes() -> TestResult {
    let demo = demo_dir();
    let bin = bin_dir()?;
    let temp = TempDir::new("flow");
    let work = &temp.0;
    copy_demo(&demo, work)?;
    run_hand_flow(&bin, work);
    let golden = demo.join("golden/artifacts");
    for name in ["compiled.json", "results.json", "results-complete.json"] {
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

fn str_of(v: &Json) -> &str {
    v.as_str().expect("a string")
}

fn items(v: &Json) -> &[Json] {
    v.as_array().expect("an array")
}

/// Structural equality that ignores positions.
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

/// The rule object with `id` in a Rulebook.
fn rule<'a>(rulebook: &'a Json, id: &str) -> &'a Json {
    items(at(rulebook, "rules"))
        .iter()
        .find(|r| str_of(at(r, "id")) == id)
        .unwrap_or_else(|| panic!("no rule {id}"))
}

/// Every member of a rule object except `origin`.
fn without_origin(rule: &Json) -> Vec<(&str, &Json)> {
    let Kind::Object(members) = rule.kind() else {
        panic!("a rule is an object")
    };
    members
        .iter()
        .filter(|m| m.key != "origin")
        .map(|m| (m.key.as_str(), &m.value))
        .collect()
}

#[test]
fn hand_authored_and_ai_origin_rulebooks_agree_in_everything_but_origin() -> TestResult {
    let demo = demo_dir();
    let bin = bin_dir()?;
    let temp = TempDir::new("explicit");
    let work = &temp.0;
    copy_demo(&demo, work)?;
    run_hand_flow(&bin, work);
    let golden = demo.join("golden/artifacts");
    let ids = ["elig.entry", "score.weighted", "rank.standing"];

    // Source document: the same bytes, the same hash everywhere it is bound.
    let document = fs::read(demo.join("documents/stand-in-rules.md"))?;
    let hash = sha256(&document).to_string();
    let manifest = json("manifest.json", &fs::read(demo.join("manifest.json"))?);
    assert_eq!(
        str_of(at(&items(at(&manifest, "documents"))[0], "sha256")),
        hash
    );
    for name in ["source-model.json", "candidates.json"] {
        assert!(
            fs::read(work.join("work").join(name))? == fs::read(golden.join(name))?,
            "{name} differs between the two flows"
        );
    }
    let model = json(
        "source-model.json",
        &fs::read(work.join("work/source-model.json"))?,
    );
    assert_eq!(str_of(at(&model, "sha256")), hash);
    assert_eq!(
        str_of(at(&model, "content")).as_bytes(),
        document.as_slice()
    );

    // Rulebooks: same documents, types, identifiers, Revision_Numbers, Rule
    // Content and spans. Only `origin` differs, and it differs for real.
    let hand = json(
        "hand-authored.json",
        &fs::read(work.join("rulebook/hand-authored.json"))?,
    );
    let adopted = json("rulebook.json", &fs::read(golden.join("rulebook.json"))?);
    for key in ["format", "ir_version", "documents", "types"] {
        assert!(same(at(&hand, key), at(&adopted, key)), "rulebook '{key}'");
    }
    assert_eq!(
        str_of(at(&items(at(&hand, "documents"))[0], "sha256")),
        hash
    );
    let hand_ids: Vec<&str> = items(at(&hand, "rules"))
        .iter()
        .map(|r| str_of(at(r, "id")))
        .collect();
    assert_eq!(hand_ids, ids);
    for id in ids {
        let (h, a) = (rule(&hand, id), rule(&adopted, id));
        assert_eq!(at(h, "revision").as_u64(), Some(1), "{id}");
        assert_eq!(at(a, "revision").as_u64(), Some(1), "{id}");
        assert_eq!(str_of(at(at(h, "origin"), "kind")), "manual", "{id}");
        assert_eq!(str_of(at(at(a, "origin"), "kind")), "ai_proposal", "{id}");
        let (hm, am) = (without_origin(h), without_origin(a));
        assert_eq!(
            hm.iter().map(|m| m.0).collect::<Vec<_>>(),
            am.iter().map(|m| m.0).collect::<Vec<_>>(),
            "{id}: member names"
        );
        for ((key, hv), (_, av)) in hm.iter().zip(&am) {
            assert!(same(hv, av), "{id}: member '{key}' differs");
        }
        assert!(same(at(h, "spans"), at(a, "spans")), "{id}: spans");
    }

    // Spans, derived from the document text and not from either tool: the
    // clause starts at its label and ends before the line break.
    let text = String::from_utf8(document)?;
    let expected_spans: Vec<(u64, u64)> = ["1. An entrant", "2. The score", "3. Entrants are"]
        .iter()
        .map(|label| {
            let start = text.find(label).unwrap();
            let end = start + text[start..].find('\n').unwrap();
            (u64::try_from(start).unwrap(), u64::try_from(end).unwrap())
        })
        .collect();
    assert_eq!(expected_spans, [(161, 241), (242, 320), (321, 426)]);

    // Compiled artifact: identical bytes, and the same rules, Revision_Numbers
    // and spans inside.
    let hand_compiled = fs::read(work.join("work/compiled.json"))?;
    let ai_compiled = fs::read(golden.join("compiled.json"))?;
    assert!(hand_compiled == ai_compiled);
    let compiled = json("compiled.json", &hand_compiled);
    let order: Vec<&str> = items(at(&compiled, "order")).iter().map(str_of).collect();
    assert_eq!(order, ids);
    assert_eq!(
        str_of(at(&items(at(&compiled, "documents"))[0], "sha256")),
        hash
    );
    assert_eq!(items(at(&compiled, "rules")).len(), 3);
    for (id, (start, end)) in ids.iter().zip(expected_spans) {
        let rule = rule(&compiled, id);
        assert_eq!(at(rule, "revision").as_u64(), Some(1));
        let spans = items(at(rule, "spans"));
        assert_eq!(spans.len(), 1);
        assert_eq!(at(&spans[0], "start").as_u64(), Some(start));
        assert_eq!(at(&spans[0], "end").as_u64(), Some(end));
        assert_eq!(str_of(at(&spans[0], "sha256")), hash);
        assert_eq!(str_of(at(&spans[0], "document")), "stand-in-rules.md");
    }

    // Ledger: the same Rule_Content hashes, document binding, reviewer and
    // time for the three approvals.
    let record_lines = |path: &Path| -> Result<Vec<Json>, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        Ok(content
            .lines()
            .skip(1)
            .take(3)
            .map(|l| json("ledger-line", l.as_bytes()))
            .collect())
    };
    let hand_records = record_lines(&work.join("work/ledger.jsonl"))?;
    let ai_records = record_lines(&golden.join("ledger.jsonl"))?;
    assert_eq!(hand_records.len(), 3);
    for (index, (h, a)) in hand_records.iter().zip(&ai_records).enumerate() {
        let (rule_id, content_hash) = APPROVALS[index];
        for record in [h, a] {
            assert_eq!(str_of(at(record, "rule")), rule_id);
            assert_eq!(at(record, "revision").as_u64(), Some(1));
            assert_eq!(str_of(at(record, "content_hash")), content_hash);
            assert_eq!(str_of(at(record, "kind")), "approval");
            let binding = items(at(record, "binding"));
            assert_eq!(binding.len(), 1);
            assert_eq!(str_of(at(&binding[0], "sha256")), hash);
        }
        for key in ["reviewer", "time", "ir_version", "prev", "hash"] {
            assert!(
                same(at(h, key), at(a, key)),
                "ledger record {index} '{key}'"
            );
        }
    }

    // Evaluation: the same Outcomes, decided or Indeterminate, over both data
    // files. The values over the complete data were worked out by hand.
    let outcomes = |bytes: &[u8]| -> Vec<String> {
        let results = json("results.json", bytes);
        items(at(&results, "outcomes"))
            .iter()
            .map(|o| {
                if str_of(at(o, "state")) != "decided" {
                    return str_of(at(o, "state")).to_owned();
                }
                let v = at(o, "value");
                if str_of(at(v, "type")) == "standings" {
                    return items(at(v, "places"))
                        .iter()
                        .map(|p| {
                            let m: Vec<&str> = items(at(p, "members")).iter().map(str_of).collect();
                            format!("{}:{}", at(p, "position").as_u64().unwrap(), m.join("="))
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                }
                match at(v, "value").kind() {
                    Kind::Bool(b) => b.to_string(),
                    Kind::String(s) => s.clone(),
                    other => panic!("unexpected value {other:?}"),
                }
            })
            .collect()
    };
    let complete = outcomes(&fs::read(work.join("work/results-complete.json"))?);
    assert_eq!(
        complete,
        [
            "true",
            "true",
            "false",
            "true",
            "true",
            "7.6",
            "7.8",
            "6",
            "7.8",
            "6",
            "1:p2 2:p4 3:p1 4:p3=p5"
        ]
    );
    let canonical = outcomes(&fs::read(work.join("work/results.json"))?);
    assert_eq!(canonical[1], "indeterminate");
    assert_eq!(
        canonical.iter().filter(|o| *o == "indeterminate").count(),
        1
    );
    for name in ["results.json", "results-complete.json"] {
        assert_eq!(
            outcomes(&fs::read(work.join("work").join(name))?),
            outcomes(&fs::read(golden.join(name))?),
            "{name}"
        );
    }
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
