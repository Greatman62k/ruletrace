//! V-1 (UD-45 section 3, design §11.6 and §3.7.5): the real `ruletrace-ai
//! propose` with the offline replay provider is killed at many points of its
//! store write. A later `ruletrace review list` must accept the store, and a
//! repeated `propose` must follow the idempotence rules. No libtest harness;
//! see `kill_harness` for the method.
//!
//! Files of the write protocol (crates/rt-ai-tool/src/store.rs), with `<id>`
//! the Request_Identifier of the batch:
//!   `store/.tmp/<id>.tmp`  created with `create_new`, written, `sync_all`
//!   `store/batches/<id>.json`  a hard link to the temporary file: the final
//!                              name never holds a partial batch
//!   then the directory sync and the removal of `store/.tmp/<id>.tmp`.
//! Slots `<id>.<n>.tmp` are used when `<id>.tmp` is taken; a file left by an
//! interrupted run is never reused or removed.
//! `tx.jsonl` is the Transmission_Log of the run. It is not part of the
//! Proposal_Store and is not classified.
//!
//! "Before the write": no file in `store/.tmp` and none at the final name
//! (the directories may exist). "During": a temporary file exists, with or
//! without the final name (hard link made, temporary file not yet removed).
//! "After": the final file exists and no temporary file.
//!
//! The expected batch bytes come from one uncut run. They are not trusted
//! alone: the Request_Identifier must equal the value pinned by
//! `crates/rt-tests-ai/tests/request_id_pin.rs` for these inputs, and every
//! file found after a kill must be the whole expected file at the final name
//! or a byte prefix of it in `.tmp`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../rt-cli/tests/kill_harness/mod.rs"]
mod kill_harness;
#[path = "../../rt-cli/tests/lifeline_rig/mod.rs"]
mod lifeline_rig;

use kill_harness::{Ended, Observation, Phase, Scenario, Verdict};
use lifeline_rig::{run_to_end, Ran, Scratch, Target};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

const EXE: &str = env!("CARGO_BIN_EXE_ruletrace-ai");
const IMAGE: &str = "ruletrace-ai.exe";
const FIXTURE: &str = "tests/fixtures/live-run1-prompt-2/inputs";
const DEMO: &str = "../../demo/stand-in-ai";
const RECORDING: &str = "provider/recorded-meta-muse-spark-1.3-prompt-2.json";
const CREATED_AT: &str = "2026-11-16T00:00:00Z";
const LIMITS: &str = "[limits]\ntotal_deadline_secs = 120\nattempt_timeout_secs = 90\nmax_retries = 0\nmax_response_bytes = 1048576\n";
// The pinned Request_Identifier of these inputs (request_id_pin.rs).
const PINNED_ID: &str = "1c574c2d1e2658ba060f68b788c2407f9aa712efd697e23c997d8c8c653b54b2";
const PROPOSALS: usize = 5;
const SEED: u64 = 0x5645_5F31_5354_4F52;

type Bytes = Vec<u8>;

struct Store {
    ruletrace: &'static str,
    candidates: Bytes,
    model: Bytes,
    seed_rulebook: Bytes,
    recorded: Bytes,
    batch_name: String,
    batch: Bytes,
    listing: String,
}

fn manifest(rel: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

// The `ruletrace` executable sits next to `ruletrace-ai` when the workspace
// is tested as a whole. A package-only run builds it first, as rt-demo does.
fn ruletrace_exe() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        let name = if cfg!(windows) {
            "ruletrace.exe"
        } else {
            "ruletrace"
        };
        let path = Path::new(EXE).parent().unwrap().join(name);
        if !path.exists() {
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
            let built = std::process::Command::new(cargo)
                .args([
                    "build",
                    "--quiet",
                    "--offline",
                    "-p",
                    "rt-cli",
                    "--bin",
                    "ruletrace",
                ])
                .status()
                .unwrap();
            assert!(built.success(), "building ruletrace failed");
        }
        path.to_string_lossy().into_owned()
    })
}

fn private(dir: &Scratch, rel: &str) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(dir.path(rel), mode).unwrap();
    }
    #[cfg(not(unix))]
    let _ = (dir, rel);
}

fn config(dir: &Scratch) -> String {
    let log = dir.arg("tx.jsonl");
    let script = dir.arg("recorded.json");
    format!(
        "schema_version = 2\nenabled = true\nscript = \"{script}\"\nallow_document_text = true\nai_identities = [\"mock-replay\", \"meta\", \"muse-spark-1.3\"]\ntransmission_log = \"{log}\"\nmax_clauses_per_request = 5\n\n[provider]\nid = \"mock-replay\"\nprotocol = \"mock-replay\"\nmodel = \"muse-spark-1.3\"\nreasoning_effort = \"low\"\n\n{LIMITS}"
    )
}

fn propose_args() -> Vec<String> {
    [
        "propose",
        "--config",
        "config.toml",
        "--rulebook",
        "seed.json",
        "--source-model",
        "source-model.json",
        "--candidates",
        "candidates.json",
        "--store",
        "store",
        "--created-at",
        CREATED_AT,
        "--consent",
        "stand-in-rules.md",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

fn list_args() -> Vec<String> {
    [
        "review",
        "list",
        "--rulebook",
        "seed.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "source-model.json",
        "--proposals",
        "store",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn files_of(dir: &Path) -> Result<BTreeMap<String, Bytes>, String> {
    let mut out = BTreeMap::new();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        out.insert(name, std::fs::read(&path).map_err(|e| e.to_string())?);
    }
    Ok(out)
}

impl Store {
    fn new() -> Self {
        let inputs = manifest(FIXTURE);
        let demo = manifest(DEMO);
        let mut store = Self {
            ruletrace: ruletrace_exe(),
            candidates: std::fs::read(inputs.join("candidates.json")).unwrap(),
            model: std::fs::read(inputs.join("source-model.json")).unwrap(),
            seed_rulebook: std::fs::read(demo.join("rulebook/seed.json")).unwrap(),
            recorded: std::fs::read(demo.join(RECORDING)).unwrap(),
            batch_name: String::new(),
            batch: Vec::new(),
            listing: String::new(),
        };
        let dir = store.prepare(0);
        let ran = store.run(EXE, &dir, propose_args());
        assert_eq!(ran.code, Some(0), "{}", text(&ran.stderr));
        let out = format!("{}{}", text(&ran.stdout), text(&ran.stderr));
        assert!(out.contains(&format!("request {PINNED_ID}:")), "{out}");
        let batches = files_of(&dir.path("store/batches")).unwrap();
        assert_eq!(batches.len(), 1, "one batch for five clauses");
        let (name, bytes) = batches.into_iter().next().unwrap();
        assert_eq!(name, format!("{PINNED_ID}.json"));
        let id_field = format!("\"request_id\":\"{PINNED_ID}\"");
        assert!(text(&bytes).contains(&id_field));
        assert!(files_of(&dir.path("store/.tmp")).unwrap().is_empty());
        let listed = store.run(store.ruletrace, &dir, list_args());
        assert_eq!(listed.code, Some(0), "{}", text(&listed.stderr));
        let listing = text(&listed.stdout);
        assert_eq!(listing.matches("pending proposal '").count(), PROPOSALS);
        store.batch_name = name;
        store.batch = bytes;
        store.listing = listing;
        store
    }

    fn run(&self, exe: &'static str, dir: &Scratch, args: Vec<String>) -> Ran {
        let image = if exe == EXE { IMAGE } else { "ruletrace.exe" };
        let target = Target {
            exe,
            image,
            args,
            cwd: dir.path("."),
            env: vec![(
                "MODEL_API_KEY".to_owned(),
                "kill-test-dummy-value".to_owned(),
            )],
            unread_stdout: false,
        };
        run_to_end(&target, false)
    }

    fn batches(&self, dir: &Scratch) -> Result<BTreeMap<String, Bytes>, String> {
        files_of(&dir.path("store/batches"))
    }

    fn temps(&self, dir: &Scratch) -> Result<BTreeMap<String, Bytes>, String> {
        files_of(&dir.path("store/.tmp"))
    }

    fn is_temp_name(&self, name: &str) -> bool {
        let id = PINNED_ID;
        let Some(rest) = name.strip_prefix(id).and_then(|r| r.strip_suffix(".tmp")) else {
            return false;
        };
        rest.is_empty()
            || rest
                .strip_prefix('.')
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    }
}

impl Scenario for Store {
    fn id(&self) -> &'static str {
        "V-1"
    }

    fn seed(&self) -> u64 {
        SEED
    }

    fn prepare(&self, trial: u32) -> Scratch {
        let dir = Scratch::new(&format!("v1-t{trial}"));
        dir.write("candidates.json", &self.candidates);
        dir.write("source-model.json", &self.model);
        dir.write("seed.json", &self.seed_rulebook);
        dir.write("recorded.json", &self.recorded);
        dir.write("config.toml", config(&dir).as_bytes());
        private(&dir, "config.toml");
        dir
    }

    fn target(&self, dir: &Scratch, _trial: u32) -> Target {
        Target {
            exe: EXE,
            image: IMAGE,
            args: propose_args(),
            cwd: dir.path("."),
            env: vec![(
                "MODEL_API_KEY".to_owned(),
                "kill-test-dummy-value".to_owned(),
            )],
            unread_stdout: false,
        }
    }

    fn touched(&self, dir: &Scratch, _trial: u32) -> bool {
        let tmp = std::fs::read_dir(dir.path("store/.tmp"))
            .map(|mut d| d.next().is_some())
            .unwrap_or(false);
        tmp || dir
            .path(&format!("store/batches/{}", self.batch_name))
            .exists()
    }

    fn observe(&self, dir: &Scratch, _trial: u32) -> Result<Observation, String> {
        for entry in std::fs::read_dir(dir.path(".")).map_err(|e| e.to_string())? {
            let name = entry.map_err(|e| e.to_string())?.file_name();
            let name = name.to_string_lossy().into_owned();
            let known = [
                "candidates.json",
                "config.toml",
                "recorded.json",
                "seed.json",
                "source-model.json",
                "tx.jsonl",
                "store",
            ];
            if !known.contains(&name.as_str()) {
                return Err(format!("unexpected file '{name}'"));
            }
        }
        for entry in std::fs::read_dir(dir.path("store")).into_iter().flatten() {
            let name = entry.map_err(|e| e.to_string())?.file_name();
            let name = name.to_string_lossy().into_owned();
            if name != "batches" && name != ".tmp" {
                return Err(format!("unexpected entry store/{name}"));
            }
        }
        let batches = self.batches(dir)?;
        let temps = self.temps(dir)?;
        let linked = match batches.iter().next() {
            None => false,
            Some((name, bytes)) => {
                if batches.len() != 1 || *name != self.batch_name || *bytes != self.batch {
                    return Err(format!(
                        "a partial or different file at the final name: '{name}', {} bytes of {}",
                        bytes.len(),
                        self.batch.len()
                    ));
                }
                true
            }
        };
        let mut states = Vec::new();
        for (name, bytes) in &temps {
            if !self.is_temp_name(name) {
                return Err(format!("unexpected temporary name '{name}'"));
            }
            if !self.batch.starts_with(bytes) {
                return Err(format!(
                    "temporary file '{name}' is not a prefix of the batch"
                ));
            }
            states.push(if bytes.is_empty() {
                "tmp-empty"
            } else if bytes.len() < self.batch.len() {
                "tmp-partial"
            } else {
                "tmp-whole"
            });
        }
        let link = if linked { "+final-linked" } else { "" };
        if !states.is_empty() {
            return Ok(Observation {
                phase: Phase::During,
                detail: format!("{}{link}", states.join("+")),
            });
        }
        if linked {
            return Ok(Observation {
                phase: Phase::After,
                detail: "final file only".to_owned(),
            });
        }
        let detail = if dir.path("store").exists() {
            "store directories only"
        } else {
            "no store directory"
        };
        Ok(Observation {
            phase: Phase::Before,
            detail: detail.to_owned(),
        })
    }

    fn accept(
        &self,
        dir: &Scratch,
        _trial: u32,
        seen: &Observation,
        _ended: Ended,
    ) -> Result<Verdict, String> {
        let had_final = !self.batches(dir)?.is_empty();
        let temps_before = self.temps(dir)?;
        let state_before = dir.state();

        // The review path's reader accepts the store, with no change.
        let listed = self.run(self.ruletrace, dir, list_args());
        let (out, err) = (text(&listed.stdout), text(&listed.stderr));
        if !dir.path("store/batches").exists() {
            if listed.code != Some(2) || !err.contains("RT-APR-0213") {
                return Err(format!(
                    "no store: expected exit 2 with RT-APR-0213, got {:?}: {err}",
                    listed.code
                ));
            }
        } else {
            if listed.code != Some(0) {
                return Err(format!(
                    "review list rejected the store: {:?}: {err}",
                    listed.code
                ));
            }
            let matches = if had_final {
                out == self.listing
            } else {
                !out.contains("pending proposal '") && out.contains("no revision awaits a decision")
            };
            if !matches {
                return Err(format!("review list shows a different store: {out}"));
            }
        }
        if dir.state() != state_before {
            return Err("review list changed a file".to_owned());
        }

        // A repeated propose: stores the batch, or reports it as stored.
        let again = self.run(EXE, dir, propose_args());
        let (out, err) = (text(&again.stdout), text(&again.stderr));
        if again.code != Some(0) {
            return Err(format!(
                "the repeated propose failed after '{}': {:?}: {err}",
                seen.detail, again.code
            ));
        }
        let reported_stored = format!("{out}{err}").contains("RT-AI-0301");
        if reported_stored != had_final {
            return Err(format!(
                "RT-AI-0301 reported {reported_stored}, batch already stored {had_final}"
            ));
        }
        let batches = self.batches(dir)?;
        if batches.len() != 1 || batches.get(&self.batch_name) != Some(&self.batch) {
            return Err("the store does not hold exactly the expected batch".to_owned());
        }
        // A temporary file of an interrupted run is never reused or removed,
        // and the repeated run leaves none of its own.
        if self.temps(dir)? != temps_before {
            return Err("the repeated propose changed the temporary files".to_owned());
        }
        let listed = self.run(self.ruletrace, dir, list_args());
        if listed.code != Some(0) || text(&listed.stdout) != self.listing {
            return Err(format!(
                "review list after the repeated propose: {:?}: {}",
                listed.code,
                text(&listed.stderr)
            ));
        }
        Ok(Verdict::Accepted)
    }
}

fn v1_kill_during_propose_leaves_no_file_a_later_run_rejects() {
    let store = Store::new();
    let tally = kill_harness::drive(&store, kill_harness::TRIALS);
    assert!(tally.before + tally.during + tally.after + tally.completed == kill_harness::TRIALS);
}

fn main() {
    lifeline_rig::parent_mode_or_return();
    let mut checks = kill_harness::self_checks();
    checks.push((
        "v1_kill_during_propose_leaves_no_file_a_later_run_rejects",
        v1_kill_during_propose_leaves_no_file_a_later_run_rejects,
    ));
    lifeline_rig::run_checks(checks);
}
