//! V-4 (UD-45 section 3, design §3.8.3): the real `ruletrace review approve`
//! is killed at many points of its journaled commit, and the next review
//! command must recover or refuse exactly as the journal design says.
//! No libtest harness; see `kill_harness` for the method.
//!
//! Files of the commit protocol (crates/rt-cli/src/commit.rs), with
//! `<l>` the ledger path and `<r>` the Rulebook path:
//!   `<l>.journal.tmp`  attempt marker, written and synced first
//!   `<l>.new`, `<r>.new`  new bytes, each written and synced
//!   `<l>.old`, `<r>.old`  hard links to the current files (`.old` only for
//!                         a file that exists)
//!   `<l>.journal`  the marker renamed: the decision is durable from here
//!   then `<l>.new` is renamed over `<l>`, then `<r>.new` over `<r>`, then
//!   the temp and backup files and the journal are removed (journal last).
//! `<l>.lock` is the advisory lock file. It exists from the start of every
//! review command and is not a write file.
//!
//! "Before the write": none of the six files above exists and the pair holds
//! the start bytes. "During": any of the six exists. "After": none exists and
//! the pair holds the final bytes.
//!
//! Two variants alternate: even trials approve `elig.entry` and create the
//! ledger, odd trials approve `score.weighted` on top of an existing ledger.
//! Expected bytes are not taken from the code under test alone: the ledger of
//! each variant must equal the leading lines of the reviewed golden
//! `demo/stand-in-ai/golden/artifacts/ledger.jsonl`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod kill_harness;
mod lifeline_rig;

use kill_harness::{Ended, Observation, Phase, Scenario, Verdict};
use lifeline_rig::{run_to_end, Ran, Scratch, Target};
use std::collections::BTreeSet;
use std::path::Path;

const EXE: &str = env!("CARGO_BIN_EXE_ruletrace");
const IMAGE: &str = "ruletrace.exe";
const DEMO: &str = "../../demo/stand-in-ai";
const TIME: &str = "2026-01-15T10:00:00Z";
const REVIEWER: &str = "Demo Reviewer";
// Hashes of the runbook steps s03a and s03c of the demonstration.
const ELIG: (&str, &str) = (
    "elig.entry",
    "9edaa59e63831bf8d71ccba89eb6025bac65019cbd424ed1348981e784dc368c",
);
const SCORE: (&str, &str) = (
    "score.weighted",
    "9fa6292ebbd6de150b1021e75b47a0ffca25db2da4c24c5ecd17f2c249136dac",
);
const SEED: u64 = 0x5645_5F34_5245_5649;

// Names of the six write files and the label used in reports.
const WRITE_FILES: [(&str, &str); 6] = [
    ("ledger.jsonl.journal.tmp", "journal.tmp"),
    ("ledger.jsonl.journal", "journal"),
    ("ledger.jsonl.new", "ledger.new"),
    ("rb.json.new", "rulebook.new"),
    ("ledger.jsonl.old", "ledger.old"),
    ("rb.json.old", "rulebook.old"),
];
const KNOWN: [&str; 5] = [
    "rb.json",
    "sm.json",
    "store",
    "ledger.jsonl",
    "ledger.jsonl.lock",
];

type Bytes = Vec<u8>;

#[derive(Clone)]
struct Pair {
    ledger: Option<Bytes>,
    rulebook: Bytes,
}

struct Variant {
    start: Pair,
    end: Pair,
    rule: (&'static str, &'static str),
    create_ledger: bool,
}

struct Review {
    model: Bytes,
    batches: Vec<(String, Bytes)>,
    even: Variant,
    odd: Variant,
}

fn demo(rel: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(DEMO).join(rel)
}

fn cli(dir: &Scratch, args: &[String]) -> Ran {
    let target = Target {
        exe: EXE,
        image: IMAGE,
        args: args.to_vec(),
        cwd: dir.path("."),
        env: Vec::new(),
        unread_stdout: false,
    };
    run_to_end(&target, false)
}

fn approve_args(rule: (&str, &str), create_ledger: bool) -> Vec<String> {
    let mut args: Vec<String> = [
        "review",
        "approve",
        "--rulebook",
        "rb.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "sm.json",
        "--proposals",
        "store",
        "--rule",
        rule.0,
        "--revision",
        "1",
        "--content-hash",
        rule.1,
        "--reviewer",
        REVIEWER,
        "--time",
        TIME,
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    if create_ledger {
        args.push("--create-ledger".to_owned());
    }
    args
}

fn list_args() -> Vec<String> {
    [
        "review",
        "list",
        "--rulebook",
        "rb.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "sm.json",
        "--proposals",
        "store",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

fn validate_args() -> Vec<String> {
    [
        "validate",
        "--rulebook",
        "rb.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "sm.json",
        "--out",
        "validation.json",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn lay_out(dir: &Scratch, rulebook: &[u8], model: &[u8], batches: &[(String, Bytes)]) {
    dir.write("rb.json", rulebook);
    dir.write("sm.json", model);
    for (name, bytes) in batches {
        dir.write(&format!("store/batches/{name}"), bytes);
    }
}

fn read_pair(dir: &Scratch) -> Pair {
    Pair {
        ledger: std::fs::read(dir.path("ledger.jsonl")).ok(),
        rulebook: std::fs::read(dir.path("rb.json")).unwrap(),
    }
}

impl Review {
    // Runs the two approvals once, uncut, and checks their ledgers against
    // the reviewed golden before any trial relies on them.
    fn new() -> Self {
        let rulebook = std::fs::read(demo("rulebook/seed.json")).unwrap();
        let model = std::fs::read(demo("golden/artifacts/source-model.json")).unwrap();
        let mut batches = Vec::new();
        let store = demo("golden/artifacts/proposal-store/batches");
        for entry in std::fs::read_dir(store).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            batches.push((name, std::fs::read(&path).unwrap()));
        }
        batches.sort();
        let dir = Scratch::new("v4-reference");
        lay_out(&dir, &rulebook, &model, &batches);
        let first = cli(&dir, &approve_args(ELIG, true));
        assert_eq!(first.code, Some(0), "{}", text(&first.stderr));
        let after_first = read_pair(&dir);
        let second = cli(&dir, &approve_args(SCORE, false));
        assert_eq!(second.code, Some(0), "{}", text(&second.stderr));
        let after_second = read_pair(&dir);
        let golden = std::fs::read(demo("golden/artifacts/ledger.jsonl")).unwrap();
        let golden_lines: Vec<&[u8]> = golden.split_inclusive(|b| *b == b'\n').collect();
        let prefix = |n: usize| golden_lines.get(..n).unwrap().concat();
        assert_eq!(after_first.ledger.as_deref(), Some(prefix(2).as_slice()));
        assert_eq!(after_second.ledger.as_deref(), Some(prefix(3).as_slice()));
        // A second uncut run from the same start gives the same bytes.
        let again = Scratch::new("v4-reference-again");
        lay_out(&again, &rulebook, &model, &batches);
        assert_eq!(cli(&again, &approve_args(ELIG, true)).code, Some(0));
        assert_eq!(read_pair(&again).rulebook, after_first.rulebook);
        Self {
            even: Variant {
                start: Pair {
                    ledger: None,
                    rulebook: rulebook.clone(),
                },
                end: after_first.clone(),
                rule: ELIG,
                create_ledger: true,
            },
            odd: Variant {
                start: after_first,
                end: after_second,
                rule: SCORE,
                create_ledger: false,
            },
            model,
            batches,
        }
    }

    fn variant(&self, trial: u32) -> &Variant {
        if trial.is_multiple_of(2) {
            &self.even
        } else {
            &self.odd
        }
    }

    fn names(&self, dir: &Scratch) -> Result<BTreeSet<String>, String> {
        let mut names = BTreeSet::new();
        for entry in std::fs::read_dir(dir.path(".")).map_err(|e| e.to_string())? {
            let name = entry.map_err(|e| e.to_string())?.file_name();
            names.insert(name.to_string_lossy().into_owned());
        }
        for name in &names {
            let allowed = KNOWN.contains(&name.as_str())
                || WRITE_FILES.iter().any(|(file, _)| file == name)
                || name == "validation.json";
            if !allowed {
                return Err(format!("unexpected file '{name}'"));
            }
        }
        Ok(names)
    }
}

fn leftovers(names: &BTreeSet<String>) -> Vec<&'static str> {
    WRITE_FILES
        .iter()
        .filter(|(file, _)| names.contains(*file))
        .map(|(_, label)| *label)
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tag {
    Start,
    End,
}

fn tag(now: &Option<Bytes>, start: &Option<Bytes>, end: &Bytes) -> Result<Tag, String> {
    if now == start {
        Ok(Tag::Start)
    } else if now.as_ref() == Some(end) {
        Ok(Tag::End)
    } else {
        Err("the file is neither its start bytes nor its final bytes".to_owned())
    }
}

fn tags(v: &Variant, pair: &Pair) -> Result<(Tag, Tag), String> {
    let ledger = tag(
        &pair.ledger,
        &v.start.ledger,
        v.end.ledger.as_ref().unwrap(),
    )
    .map_err(|e| format!("ledger: {e}"))?;
    let rulebook = tag(
        &Some(pair.rulebook.clone()),
        &Some(v.start.rulebook.clone()),
        &v.end.rulebook,
    )
    .map_err(|e| format!("Rulebook: {e}"))?;
    Ok((ledger, rulebook))
}

impl Scenario for Review {
    fn id(&self) -> &'static str {
        "V-4"
    }

    fn seed(&self) -> u64 {
        SEED
    }

    fn prepare(&self, trial: u32) -> Scratch {
        let dir = Scratch::new(&format!("v4-t{trial}"));
        let v = self.variant(trial);
        lay_out(&dir, &v.start.rulebook, &self.model, &self.batches);
        if let Some(ledger) = &v.start.ledger {
            dir.write("ledger.jsonl", ledger);
        }
        dir
    }

    fn target(&self, dir: &Scratch, trial: u32) -> Target {
        let v = self.variant(trial);
        Target {
            exe: EXE,
            image: IMAGE,
            args: approve_args(v.rule, v.create_ledger),
            cwd: dir.path("."),
            env: Vec::new(),
            unread_stdout: false,
        }
    }

    fn touched(&self, dir: &Scratch, _trial: u32) -> bool {
        WRITE_FILES
            .iter()
            .any(|(file, _)| std::fs::symlink_metadata(dir.path(file)).is_ok())
    }

    fn observe(&self, dir: &Scratch, trial: u32) -> Result<Observation, String> {
        let v = self.variant(trial);
        let names = self.names(dir)?;
        let left = leftovers(&names);
        let (ledger, rulebook) = tags(v, &read_pair(dir))?;
        if (ledger, rulebook) == (Tag::Start, Tag::End) {
            return Err("a new Rulebook with the old ledger (Req 26.30)".to_owned());
        }
        if (ledger, rulebook) == (Tag::End, Tag::Start) && !left.contains(&"journal") {
            return Err("a new ledger with the old Rulebook and no journal".to_owned());
        }
        let pair = format!("ledger={ledger:?} rulebook={rulebook:?}");
        if left.is_empty() {
            let phase = if ledger == Tag::Start {
                Phase::Before
            } else {
                Phase::After
            };
            Ok(Observation {
                phase,
                detail: format!("no write file, {pair}"),
            })
        } else {
            Ok(Observation {
                phase: Phase::During,
                detail: format!("{} present, {pair}", left.join("+")),
            })
        }
    }

    fn accept(
        &self,
        dir: &Scratch,
        trial: u32,
        seen: &Observation,
        _ended: Ended,
    ) -> Result<Verdict, String> {
        let v = self.variant(trial);
        let names = self.names(dir)?;
        let left = leftovers(&names);
        let before = dir.state();
        let (ledger, _) = tags(v, &read_pair(dir))?;

        // Readers refuse while a journal exists (RT-APR-0214, exit 2) and
        // accept every other state.
        let listed = cli(dir, &list_args());
        let listed_err = text(&listed.stderr);
        if left.contains(&"journal") {
            if listed.code != Some(2) || !listed_err.contains("RT-APR-0214") {
                return Err(format!(
                    "review list with a journal: exit {:?}, expected 2 with RT-APR-0214: {listed_err}",
                    listed.code
                ));
            }
        } else if listed.code != Some(0) {
            return Err(format!(
                "review list without a journal: exit {:?}: {listed_err}",
                listed.code
            ));
        }
        if dir.state() != before {
            return Err("review list changed a file".to_owned());
        }

        // The next review command: the same approval again.
        let again = approve_args(v.rule, v.create_ledger);
        let mut verdict = Verdict::Accepted;
        let mut ran = cli(dir, &again);
        let torn_marker_only = left == ["journal.tmp"];
        if torn_marker_only && ran.code == Some(70) {
            // Design §3.8.3 decision (h): a marker that is not whole cannot
            // be told from a user's file, so it is refused and kept.
            let err = text(&ran.stderr);
            if !err.contains("RT-APR-0208") || !err.contains("ledger.jsonl.journal.tmp") {
                return Err(format!(
                    "refusal of a torn marker is not RT-APR-0208: {err}"
                ));
            }
            if dir.state() != before {
                return Err("the refusal of a torn marker changed a file".to_owned());
            }
            std::fs::remove_file(dir.path("ledger.jsonl.journal.tmp"))
                .map_err(|e| e.to_string())?;
            verdict = Verdict::RefusedByDesign;
            ran = cli(dir, &again);
        }
        let err = text(&ran.stderr);
        let out = text(&ran.stdout);
        match ledger {
            Tag::Start => {
                if ran.code != Some(0) || !out.contains(&format!("approved rule '{}'", v.rule.0)) {
                    return Err(format!(
                        "state {} was not completed by the next approval: exit {:?}: {out}{err}",
                        seen.detail, ran.code
                    ));
                }
            }
            Tag::End => {
                if ran.code != Some(3) || !err.contains("RT-APR-0204") {
                    return Err(format!(
                        "committed state {} was not recovered to RT-APR-0204 (exit 3): exit {:?}: {err}",
                        seen.detail, ran.code
                    ));
                }
            }
        }

        // The final state is the committed pair with nothing left over.
        let after = read_pair(dir);
        if after.ledger != v.end.ledger || after.rulebook != v.end.rulebook {
            return Err("the pair after recovery differs from the final bytes".to_owned());
        }
        let names = self.names(dir)?;
        if !leftovers(&names).is_empty() {
            return Err(format!(
                "leftover files after recovery: {:?}",
                leftovers(&names)
            ));
        }
        let listed = cli(dir, &list_args());
        let shown = text(&listed.stdout);
        if listed.code != Some(0) || shown.contains(&format!("for rule '{}'", v.rule.0)) {
            return Err(format!(
                "review list after recovery: exit {:?}: {shown}",
                listed.code
            ));
        }
        let validated = cli(dir, &validate_args());
        if validated.code != Some(0) {
            return Err(format!(
                "validate after recovery: exit {:?}: {}",
                validated.code,
                text(&validated.stderr)
            ));
        }
        Ok(verdict)
    }
}

fn v4_kill_during_review_approve_leaves_a_recoverable_state() {
    let review = Review::new();
    let tally = kill_harness::drive(&review, kill_harness::TRIALS);
    assert!(tally.before + tally.during + tally.after + tally.completed == kill_harness::TRIALS);
}

fn main() {
    lifeline_rig::parent_mode_or_return();
    let mut checks = kill_harness::self_checks();
    checks.push((
        "v4_kill_during_review_approve_leaves_a_recoverable_state",
        v4_kill_during_review_approve_leaves_a_recoverable_state,
    ));
    lifeline_rig::run_checks(checks);
}
