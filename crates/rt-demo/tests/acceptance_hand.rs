//! The demonstration acceptance test, `hand` profile (design §8.11.12, Req
//! 33.15 to 33.17, 20.26).
//!
//! In order: manifest checks, the runbook step by step against the golden
//! files, every trace verified, the Proof_Report replayed by an independent
//! decimal reference, the composition of the data (Req 33.13), the
//! Coverage_Statement, the dependency tree of `ruletrace`, and the smoke
//! versions of the mutations M1 to M3 (design §8.11.11). Run with
//! `-- --nocapture` for the summary.
//!
//! The binaries come from `RT_BIN_DIR` or from a `cargo build` of `rt-cli`.
//! Bless the golden files with `RT_DEMO_BLESS=1` (refused when `CI` is set).
//! The test uses no network and no credential: each process runs with an empty
//! environment.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

use rt_demo::{
    check_manifest, describe_evaluate, describe_ledger, parse_runbook, run_runbook_recorded,
    run_step, Captured, Mode, Runbook, Step, StepRecord,
};
use rt_foundation::InputId;
use rt_json::{parse, Kind, ParseLimits, Position, Value};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Json = Value<Position>;

const PROFILE: &str = "hand";

fn demo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo/stand-in-hand")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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
        let dir = std::env::temp_dir().join(format!("rt-demo-{label}-{}", std::process::id()));
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

fn read_json(path: &Path) -> Json {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    json("file.json", &bytes)
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

/// An exact decimal for the replay: `mantissa / 10^scale`. It is an independent
/// reference model; it shares no code with `rt-numeric`.
#[derive(Clone, Copy, Debug)]
struct Dec {
    mantissa: i128,
    scale: u32,
}

impl Dec {
    fn parse(s: &str) -> Dec {
        let (negative, digits) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
        let (whole, frac) = digits.split_once('.').unwrap_or((digits, ""));
        let mantissa: i128 = format!("{whole}{frac}")
            .parse()
            .unwrap_or_else(|_| panic!("'{s}' is not a decimal"));
        Dec {
            mantissa: if negative { -mantissa } else { mantissa },
            scale: u32::try_from(frac.len()).unwrap(),
        }
    }

    fn at_scale(self, scale: u32) -> i128 {
        self.mantissa * 10_i128.pow(scale - self.scale)
    }

    fn compare(self, other: Dec) -> Ordering {
        let s = self.scale.max(other.scale);
        self.at_scale(s).cmp(&other.at_scale(s))
    }

    fn mul(self, other: Dec) -> Dec {
        Dec {
            mantissa: self.mantissa * other.mantissa,
            scale: self.scale + other.scale,
        }
    }

    fn add(self, other: Dec) -> Dec {
        let s = self.scale.max(other.scale);
        Dec {
            mantissa: self.at_scale(s) + other.at_scale(s),
            scale: s,
        }
    }
}

/// A recorded trace value.
#[derive(Clone, Copy, Debug)]
enum Tv {
    Num(Dec),
    Bool(bool),
    Unknown,
    /// A value of another type (standings). Only a `recorded` decision carries
    /// one.
    Other,
}

impl Tv {
    fn parse(v: &Json) -> Tv {
        match text(at(v, "type")) {
            "number" => Tv::Num(Dec::parse(text(at(v, "value")))),
            "bool" => match at(v, "value").kind() {
                Kind::Bool(b) => Tv::Bool(*b),
                other => panic!("bool value expected, found {other:?}"),
            },
            "unknown" => Tv::Unknown,
            _ => Tv::Other,
        }
    }

    fn equals(self, other: Tv) -> bool {
        match (self, other) {
            (Tv::Num(a), Tv::Num(b)) => a.compare(b) == Ordering::Equal,
            (Tv::Bool(a), Tv::Bool(b)) => a == b,
            (Tv::Unknown, Tv::Unknown) | (Tv::Other, Tv::Other) => true,
            _ => false,
        }
    }
}

fn number(v: Tv) -> Dec {
    match v {
        Tv::Num(d) => d,
        other => panic!("a number was expected, found {other:?}"),
    }
}

/// Kleene conjunction.
fn all_of(values: &[Tv]) -> Tv {
    if values.iter().any(|v| matches!(v, Tv::Bool(false))) {
        Tv::Bool(false)
    } else if values.iter().any(|v| matches!(v, Tv::Unknown)) {
        Tv::Unknown
    } else {
        Tv::Bool(true)
    }
}

/// Recomputes every calculation and decision entry of one Proof_Report section
/// from its recorded operation and operands. Returns the number of entries
/// recomputed.
fn replay_section(section: &Json) -> usize {
    let mut recorded: Vec<(u64, Tv)> = Vec::new();
    for read in items(at(section, "inputs_read")) {
        recorded.push((
            at(read, "seq").as_u64().unwrap(),
            Tv::parse(at(read, "value")),
        ));
    }
    let mut checked = 0;
    for entry in items(at(section, "calculation_steps")) {
        let seq = at(entry, "seq").as_u64().unwrap();
        let op = text(at(entry, "op"));
        let value = match text(at(entry, "kind")) {
            "step" => {
                let operands: Vec<Tv> =
                    items(at(entry, "operands")).iter().map(Tv::parse).collect();
                let result = Tv::parse(at(entry, "result"));
                let expected = match op {
                    "mul" => {
                        let [a, b] = operands[..] else {
                            panic!("seq {seq}: mul needs two operands")
                        };
                        Tv::Num(number(a).mul(number(b)))
                    }
                    "sum" => Tv::Num(
                        operands
                            .iter()
                            .fold(Dec::parse("0"), |acc, v| acc.add(number(*v))),
                    ),
                    "ge" => {
                        let [a, b] = operands[..] else {
                            panic!("seq {seq}: ge needs two operands")
                        };
                        Tv::Bool(number(a).compare(number(b)) != Ordering::Less)
                    }
                    other => panic!("seq {seq}: replay does not know the step operation '{other}'"),
                };
                assert!(
                    expected.equals(result),
                    "seq {seq}: {op} recomputes to {expected:?}, the report records {result:?}"
                );
                result
            }
            "decision" => {
                let value = Tv::parse(at(entry, "value"));
                let from: Vec<Tv> = items(at(entry, "from"))
                    .iter()
                    .map(|n| {
                        let n = n.as_u64().unwrap();
                        recorded
                            .iter()
                            .find(|(s, _)| *s == n)
                            .unwrap_or_else(|| panic!("seq {seq} refers to unknown entry {n}"))
                            .1
                    })
                    .collect();
                let expected = match op {
                    "and" => all_of(&from),
                    "pass" => *from.first().expect("pass has one source"),
                    "recorded" => {
                        assert!(from.is_empty(), "seq {seq}: recorded has no source");
                        value
                    }
                    other => panic!("seq {seq}: replay does not know the decision '{other}'"),
                };
                assert!(
                    expected.equals(value),
                    "seq {seq}: {op} recomputes to {expected:?}, the report records {value:?}"
                );
                value
            }
            other => panic!("seq {seq}: unexpected entry kind '{other}'"),
        };
        recorded.push((seq, value));
        checked += 1;
    }
    checked
}

fn step_by_id<'a>(runbook: &'a Runbook, id: &str) -> &'a Step {
    runbook
        .steps_for(PROFILE)
        .unwrap()
        .into_iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("no step {id}"))
}

fn output_text(c: &Captured) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&c.stdout),
        String::from_utf8_lossy(&c.stderr)
    )
}

fn run_expecting(step: &Step, exit: i32, bin: &Path, dir: &Path) -> Captured {
    let c = run_step(step, bin, dir).unwrap();
    assert_eq!(c.exit, exit, "step {}: {}", step.id, output_text(&c));
    c
}

fn replace_once(path: &Path, from: &str, to: &str) {
    let content = std::fs::read_to_string(path).unwrap();
    assert_eq!(content.matches(from).count(), 1, "'{from}' must occur once");
    std::fs::write(path, content.replacen(from, to, 1)).unwrap();
}

/// Index of the traces whose entries read `name` of `participant`.
fn reads(trace: &Json, participant: &str, name: &str) -> bool {
    items(at(trace, "entries")).iter().any(|e| {
        text(at(e, "kind")) == "read"
            && at(e, "participant").as_str() == Some(participant)
            && at(e, "name").as_str() == Some(name)
    })
}

fn describe_outcome(outcome: &Json) -> String {
    match text(at(outcome, "state")) {
        "decided" => {
            let v = at(outcome, "value");
            match text(at(v, "type")) {
                "standings" => items(at(v, "places"))
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
                    .join(" "),
                _ => match at(v, "value").kind() {
                    Kind::Bool(b) => b.to_string(),
                    Kind::String(s) => s.clone(),
                    other => format!("{other:?}"),
                },
            }
        }
        state => state.to_owned(),
    }
}

fn question_label(q: &Json) -> String {
    let target = text(at(q, "target"));
    at(q, "participant")
        .as_str()
        .map_or_else(|| target.to_owned(), |p| format!("{target} {p}"))
}

/// The argument that follows `flag` in a step.
fn option<'a>(step: &'a Step, flag: &str) -> Option<&'a str> {
    step.argv
        .iter()
        .position(|a| a == flag)
        .and_then(|i| step.argv.get(i + 1))
        .map(String::as_str)
}

fn print_summary(runbook: &Runbook, work: &Path, records: &[StepRecord]) {
    println!("\nSLICE 0 SUMMARY, profile `hand`");
    println!(
        "\nTHIS IS A STAND-IN. The document below was written for the RuleTrace project. It is not \
         the rules of any real competition."
    );
    println!(
        "NO AI WAS USED. The runbook has no `ruletrace-ai` step, the manifest names no provider \
         script, and every rule of the rulebook has origin `manual`. Each approval below is a \
         human decision recorded by `ruletrace review approve`."
    );
    let document = std::fs::read_to_string(work.join("documents/stand-in-rules.md")).unwrap();
    println!("\nstand-in document (documents/stand-in-rules.md):\n{document}");
    println!("approvals in the ledger (work/ledger.jsonl):");
    let ledger = std::fs::read(work.join("work/ledger.jsonl")).unwrap();
    for line in describe_ledger(&ledger).unwrap() {
        println!("  {line}");
    }
    println!("\nstep  exit  command");
    for s in runbook.steps_for(PROFILE).unwrap() {
        let words: Vec<&str> = s.argv.iter().take(2).map(String::as_str).collect();
        println!("{:<5} {:<5} {}", s.id, s.expect_exit, words.join(" "));
    }
    println!(
        "\nevaluate steps (exit 4 = at least one Outcome is Indeterminate, exit 0 = all decided):"
    );
    for record in records {
        let step = step_by_id(runbook, &record.id);
        if step.argv.get(1).map(String::as_str) != Some("evaluate") {
            continue;
        }
        let out = option(step, "--out").expect("evaluate has --out");
        let results = std::fs::read(work.join(out)).unwrap();
        let described = describe_evaluate(
            &record.id,
            record.captured.exit,
            &results,
            &record.captured.stderr,
        )
        .unwrap();
        println!("\n{described}");
    }
    let proof = std::fs::read_to_string(work.join("work/proof.txt")).unwrap();
    let first = proof.split("\nDECISION 2 ").next().unwrap_or(&proof);
    println!("first Proof_Report section (s07):\n{first}");
    let ranking: Vec<&str> = proof
        .split("\nDECISION ")
        .filter(|section| section.contains("decided standings"))
        .collect();
    println!("ranking decision of the Proof_Report (s07):");
    for section in ranking {
        println!("DECISION {section}");
    }
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
fn hand_profile_acceptance() -> TestResult {
    let demo = demo_dir();
    let bin = bin_dir()?;

    // 1. Manifest checks (Req 33.2, 33.13).
    let manifest_bytes = std::fs::read(demo.join("manifest.json"))?;
    let manifest = check_manifest(&manifest_bytes, &|p| std::fs::read(demo.join(p)).ok())?;
    assert_eq!(manifest.name, "stand-in-hand");
    let manifest_json = json("manifest.json", &manifest_bytes);
    assert!(matches!(
        at(&manifest_json, "stand_in").kind(),
        Kind::Bool(true)
    ));

    // 2 to 4. The runbook, step by step: exit codes and every output byte
    // against its golden file.
    let runbook = parse_runbook(&std::fs::read_to_string(demo.join("RUNBOOK.md"))?)?;
    let bless = std::env::var("RT_DEMO_BLESS").ok();
    let ci = std::env::var("CI").ok();
    let mode = Mode::from_env_values(bless.as_deref(), ci.as_deref())?;
    let base = TempDir::new("acceptance");
    let records = run_runbook_recorded(&runbook, PROFILE, &demo, &base.0, &bin, mode)?;
    let work = &base.0;

    // 5a. Every Decision_Trace is verified by the Explainer.
    let results = read_json(&work.join("work/results.json"));
    let verification = items(at(&results, "verification"));
    let traces = items(at(&results, "traces"));
    assert_eq!(verification.len(), traces.len());
    assert!(!traces.is_empty());
    assert!(
        verification
            .iter()
            .all(|v| text(at(v, "status")) == "verified"),
        "a Decision_Trace is not verified"
    );

    // 5b. The Proof_Report replays: each calculation recomputed from operation
    // and operands.
    let proof = read_json(&work.join("work/proof.json"));
    let header = at(&proof, "header");
    assert_ranking_label(
        &std::fs::read_to_string(work.join("work/proof.txt"))?,
        &proof,
    );
    let sections = items(at(&proof, "decisions"));
    assert_eq!(
        at(header, "decisions").as_u64(),
        Some(sections.len() as u64)
    );
    assert_eq!(
        at(header, "verified_traces").as_u64(),
        Some(sections.len() as u64)
    );
    assert!(sections
        .iter()
        .all(|s| text(at(at(s, "verification"), "status")) == "verified"));
    let replayed: usize = sections.iter().map(replay_section).sum();
    assert!(replayed >= 40, "only {replayed} entries were replayed");

    // 5c. The data holds the participants and results of Req 33.13.
    let outcome_of = |i: usize| items(at(&results, "outcomes")).get(i).unwrap();
    let eligibility = |wanted: &str| {
        traces.iter().enumerate().any(|(i, t)| {
            let q = at(t, "question");
            text(at(q, "target")) == "eligible"
                && text(at(outcome_of(i), "state")) == wanted
                && (wanted != "decided" || describe_outcome(outcome_of(i)) == "true")
        })
    };
    assert!(eligibility("decided"), "no eligible participant");
    assert!(
        traces.iter().enumerate().any(|(i, t)| {
            text(at(at(t, "question"), "target")) == "eligible"
                && describe_outcome(outcome_of(i)) == "false"
        }),
        "no ineligible participant"
    );
    assert!(
        eligibility("indeterminate"),
        "no Indeterminate eligibility Outcome from a Missing_Value"
    );
    assert!(
        traces.iter().enumerate().any(|(i, _)| {
            let o = outcome_of(i);
            text(at(o, "state")) == "decided"
                && at(at(o, "value"), "type").as_str() == Some("standings")
                && items(at(at(o, "value"), "places"))
                    .iter()
                    .any(|p| items(at(p, "members")).len() > 1)
        }),
        "no Tie_Group in a ranking"
    );
    let published: Vec<&str> = sections
        .iter()
        .map(|s| text(at(at(s, "published"), "state")))
        .collect();
    assert!(
        published.contains(&"equal"),
        "no Published_Result equal to the computed result"
    );
    assert!(published
        .iter()
        .all(|p| ["equal", "not_compared"].contains(p)));
    let seeded = items(at(&manifest_json, "data"))
        .iter()
        .filter(|d| matches!(at(d, "seeded_difference").kind(), Kind::Bool(true)))
        .count();
    assert_eq!(seeded, 1, "exactly one data file is labelled as seeded");
    let s06b = std::fs::read_to_string(demo.join("golden/steps/s06b.stderr"))?;
    assert!(
        s06b.contains("warning RT-EXE-0011"),
        "the seeded difference is not reported"
    );

    // 5d. The Coverage_Statement classifies every clause of the fresh
    // Candidate_Clause list as an approved rule (the generator itself is tested
    // in tests/coverage.rs).
    let candidates = read_json(&work.join("work/candidates.json"));
    let statement = read_json(&demo.join("golden/artifacts/coverage-statement.json"));
    let clauses = items(at(&candidates, "clauses"));
    let entries = items(at(&statement, "entries"));
    assert_eq!(clauses.len(), entries.len());
    for (c, e) in clauses.iter().zip(entries) {
        for key in ["document", "start", "end"] {
            assert!(
                same(at(c, key), at(e, key)),
                "coverage entry differs in '{key}'"
            );
        }
        assert_eq!(text(at(e, "class")), "rule_approved");
    }

    // 5e. The dependency tree of `ruletrace` has no network, provider or AI
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
    // The resolve covers every platform, so a cold cargo cache needs
    // `cargo fetch --locked` first. Offline keeps the check network-free.
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

    // 5f. The happy path (Req 16 criterion 13): over `data-complete.json` every
    // Outcome is decided and `evaluate` exits 0. The expected values were
    // worked out by hand from the rules, not read from the tool:
    //   score = 0.6 * judges + 0.4 * technical
    //   p1 17/8/7: 4.8 + 2.8 = 7.6   p2 20/7/9: 4.2 + 3.6 = 7.8
    //   p3 15/6/6: 3.6 + 2.4 = 6     p4 21/9/6: 5.4 + 2.4 = 7.8
    //   p5 19/6/6: 3.6 + 2.4 = 6
    //   eligible = age >= 16 and licence: only p3 (age 15) is not.
    //   ranking: score descending, then technical descending. p2 and p4 tie on
    //   7.8 and p2 leads on technical 9 against 6; p1 is third on 7.6; p3 and
    //   p5 tie on 6 and on technical 6, so they form one Tie_Group.
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
        assert_eq!(step_by_id(&runbook, step_id).expect_exit, 0);
        let record = records.iter().find(|r| r.id == step_id).unwrap();
        assert_eq!(record.captured.exit, 0, "{step_id}");
        assert!(
            record.captured.stderr.is_empty(),
            "{step_id} has diagnostics"
        );
        let complete = read_json(&work.join(out));
        let found: Vec<(String, String)> = items(at(&complete, "traces"))
            .iter()
            .zip(items(at(&complete, "outcomes")))
            .map(|(t, o)| {
                assert_eq!(
                    text(at(o, "state")),
                    "decided",
                    "{step_id} has an undecided Outcome"
                );
                (question_label(at(t, "question")), describe_outcome(o))
            })
            .collect();
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
                .all(|v| text(at(v, "status")) == "verified"
                    && items(at(v, "spans"))
                        .iter()
                        .all(|s| at(s, "resolved").kind() == &Kind::Bool(true))),
            "{step_id}: a trace is not verified"
        );
    }
    // Render and proof options never change the results (Req 13.19).
    assert_eq!(
        std::fs::read(work.join("work/results-complete.json"))?,
        std::fs::read(work.join("work/results-complete.proof-run.json"))?
    );
    let complete_proof = read_json(&work.join("work/proof-complete.json"));
    assert_ranking_label(
        &std::fs::read_to_string(work.join("work/proof-complete.txt"))?,
        &complete_proof,
    );
    let complete_header = at(&complete_proof, "header");
    assert_eq!(at(complete_header, "decisions").as_u64(), Some(11));
    assert_eq!(at(complete_header, "verified_traces").as_u64(), Some(11));
    assert!(items(at(&complete_proof, "decisions"))
        .iter()
        .all(|s| text(at(at(s, "verification"), "status")) == "verified"));
    // The complete data differ from the canonical data only by the age of p2,
    // so the Indeterminate scenario above stays the only difference.
    let canonical = read_json(&demo.join("data/canonical/data.json"));
    let complete_data = read_json(&demo.join("data/canonical/data-complete.json"));
    let (before, after) = (
        items(at(&canonical, "participants")),
        items(at(&complete_data, "participants")),
    );
    assert_eq!(before.len(), after.len());
    for (b, a) in before.iter().zip(after) {
        let id = text(at(b, "id"));
        assert_eq!(id, text(at(a, "id")));
        let (bf, af) = (at(b, "fields"), at(a, "fields"));
        let Kind::Object(a_fields) = af.kind() else {
            panic!("fields of {id} is not an object")
        };
        for member in a_fields {
            match bf.get(&member.key) {
                Some(old) => assert!(same(&old.value, &member.value), "{id}.{}", member.key),
                None => assert_eq!(
                    (id, member.key.as_str(), text(&member.value)),
                    ("p2", "age", "20"),
                    "the only added field is the age of p2"
                ),
            }
        }
    }
    // The worked-out scores also follow from the data by the independent
    // decimal model, so the literals above cannot drift from the data file.
    for (index, participant) in after.iter().enumerate() {
        let fields = at(participant, "fields");
        let judges = Dec::parse(text(at(fields, "judges")));
        let technical = Dec::parse(text(at(fields, "technical")));
        let score = Dec::parse("0.6")
            .mul(judges)
            .add(Dec::parse("0.4").mul(technical));
        assert_eq!(
            score.compare(Dec::parse(expected_complete[5 + index].1)),
            Ordering::Equal,
            "score of {}",
            text(at(participant, "id"))
        );
    }

    // 5g. No AI was used (printed in the summary): no `ruletrace-ai` step, no
    // provider script in the manifest, and every rule has the origin `manual`.
    assert!(runbook
        .steps_for(PROFILE)?
        .iter()
        .all(|s| s.argv.first().map(String::as_str) != Some("ruletrace-ai")));
    assert!(matches!(
        at(&manifest_json, "provider_script").kind(),
        Kind::Null
    ));
    let rulebook = read_json(&demo.join("rulebook/hand-authored.json"));
    assert!(items(at(&rulebook, "rules"))
        .iter()
        .all(|r| text(at(at(r, "origin"), "kind")) == "manual"));

    print_summary(&runbook, work, &records);

    // 6. Mutations M1 to M3, smoke versions (design §8.11.11).

    // M1: one source byte inside the span of the first approved rule.
    let m1 = TempDir::new("m1");
    rt_demo::copy_demo(work, &m1.0)?;
    let compiled = read_json(&work.join("work/compiled.json"));
    let first_rule = items(at(&compiled, "rules")).first().unwrap();
    let first_span = items(at(first_rule, "spans")).first().unwrap();
    let (start, end) = (
        usize::try_from(at(first_span, "start").as_u64().unwrap())?,
        usize::try_from(at(first_span, "end").as_u64().unwrap())?,
    );
    let doc_path = m1.0.join("documents/stand-in-rules.md");
    let mut doc = std::fs::read(&doc_path)?;
    let at_byte = doc
        .get(start..end)
        .and_then(|s| s.iter().position(u8::is_ascii_lowercase))
        .map(|i| start + i)
        .ok_or("no lowercase byte in the span")?;
    let slot = doc.get_mut(at_byte).ok_or("byte outside the document")?;
    *slot = if *slot == b'x' { b'y' } else { b'x' };
    std::fs::write(&doc_path, &doc)?;
    run_expecting(step_by_id(&runbook, "s01"), 0, &bin, &m1.0);
    // The canonical data holds an Indeterminate Outcome, and `evaluate` ranks
    // stage 6 (exit 4) before the provenance rows of stage 7 (exit 2), see
    // crates/rt-cli/src/evaluate.rs.
    let c = run_expecting(step_by_id(&runbook, "s06"), 4, &bin, &m1.0);
    assert!(
        output_text(&c).contains("RT-PRV-0002"),
        "{}",
        output_text(&c)
    );
    let m1_results = read_json(&m1.0.join("work/results.json"));
    let m1_verification = items(at(&m1_results, "verification"));
    assert_eq!(m1_verification.len(), traces.len(), "Outcomes are complete");
    for v in m1_verification {
        assert_eq!(text(at(v, "status")), "not_verified");
        assert!(items(at(v, "spans"))
            .iter()
            .all(|s| at(s, "code").as_str() == Some("RT-PRV-0002")));
    }
    assert!(same(at(&m1_results, "outcomes"), at(&results, "outcomes")));

    // M2: one data value; only Outcomes that read it may change.
    let m2 = TempDir::new("m2");
    rt_demo::copy_demo(work, &m2.0)?;
    replace_once(
        &m2.0.join("data/canonical/data.json"),
        "\"judges\": \"8\"",
        "\"judges\": \"9\"",
    );
    run_expecting(step_by_id(&runbook, "s06"), 4, &bin, &m2.0);
    let m2_results = read_json(&m2.0.join("work/results.json"));
    let (mut changed, mut unchanged) = (0, 0);
    for (i, t) in traces.iter().enumerate() {
        let after_trace = items(at(&m2_results, "traces")).get(i).unwrap();
        let identical = same(t, after_trace)
            && same(
                outcome_of(i),
                items(at(&m2_results, "outcomes")).get(i).unwrap(),
            );
        if reads(t, "p1", "judges") {
            changed += usize::from(!identical);
        } else {
            assert!(
                identical,
                "Outcome {i} does not read p1 judges and must not change"
            );
            unchanged += 1;
        }
    }
    assert!(changed >= 1, "no dependent Outcome changed");
    assert!(unchanged >= 1, "no independent Outcome was checked");

    // M3: one approved Rule_Content literal edited without a new approval
    // (RN-29, option a).
    let m3 = TempDir::new("m3");
    rt_demo::copy_demo(work, &m3.0)?;
    std::fs::remove_file(m3.0.join("work/validation.json"))?;
    std::fs::remove_file(m3.0.join("work/compiled.json"))?;
    replace_once(
        &m3.0.join("rulebook/hand-authored.json"),
        "\"value\": \"16\"",
        "\"value\": \"17\"",
    );
    let c = run_expecting(step_by_id(&runbook, "s04"), 3, &bin, &m3.0);
    assert!(
        output_text(&c).contains("RT-APR-0001"),
        "{}",
        output_text(&c)
    );
    assert!(
        m3.0.join("work/validation.json").exists(),
        "validate writes the artifact (RN-29)"
    );
    let c = run_expecting(step_by_id(&runbook, "s05"), 3, &bin, &m3.0);
    assert!(
        output_text(&c).contains("RT-APR-0001"),
        "{}",
        output_text(&c)
    );
    assert!(
        !m3.0.join("work/compiled.json").exists(),
        "no Compiled_Rulebook is written"
    );

    println!("\nmutations: M1 source byte, M2 data value, M3 rule literal: all detected");
    Ok(())
}
