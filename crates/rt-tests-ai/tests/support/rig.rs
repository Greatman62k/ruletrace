//! A scratch directory and runners for `ruletrace` and `ruletrace-ai`.
//!
//! Both tools run through their library entry points on the real disk, in a
//! directory under the workspace `target` directory that is removed when the
//! guard drops.

use rt_ai_tool::{EnvReader, Out, Redactor, StdFs as AiFs};
use rt_cli::StdFs;
use rt_hash::sha256;
use rt_json::{write_compact, Value};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const SCRATCH_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/test-scratch/rt-tests-ai"
);

/// The fixed time stamp of every `propose` run.
pub const CREATED_AT: &str = "2026-11-15T00:00:00Z";

/// The fixed time of every review decision, equal to the one in the
/// authored ledger fixture.
pub const REVIEW_TIME: &str = "2026-01-15T10:00:00Z";

/// The reviewer of the authored ledger fixture.
pub const REVIEWER: &str = "A. Reviewer";

/// The directory of the `slice-ai` fixture.
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/slice-ai")
        .join(name)
}

/// What one command printed and returned.
pub struct Ran {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Ran {
    /// The catalogue identifiers on standard error, in order.
    pub fn codes(&self) -> Vec<String> {
        self.stderr
            .lines()
            .filter_map(|line| {
                let (severity, rest) = line.split_once(' ')?;
                let known = ["error", "warning", "ambiguity", "unsupported"];
                let id = rest.split_once(':')?.0;
                (known.contains(&severity) && id.starts_with("RT-")).then(|| id.to_owned())
            })
            .collect()
    }
}

struct NoEnv;

impl EnvReader for NoEnv {
    fn get(&self, _name: &str) -> Option<String> {
        None
    }
}

/// A scratch directory that holds one scenario.
pub struct Work {
    root: PathBuf,
}

impl Work {
    pub fn new(name: &str) -> Self {
        let root = Path::new(SCRATCH_ROOT).join(format!("{}-{name}", std::process::id()));
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// The path as an argument, with forward slashes on every platform.
    pub fn arg(&self, rel: &str) -> String {
        self.path(rel).to_string_lossy().replace('\\', "/")
    }

    pub fn write(&self, rel: &str, bytes: &[u8]) {
        let path = self.path(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    pub fn read(&self, rel: &str) -> Vec<u8> {
        std::fs::read(self.path(rel)).unwrap()
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.path(rel).exists()
    }

    /// Copies `from` (relative to a fixture directory) to `to`.
    pub fn copy_from(&self, fixture_dir: &Path, from: &str, to: &str) {
        let bytes = std::fs::read(fixture_dir.join(from)).unwrap();
        self.write(to, &bytes);
    }

    /// Every file under the directory with its bytes, keyed by relative
    /// path with forward slashes. Lock files are left out: they are control
    /// files that hold no data, and the tests check that separately.
    pub fn state(&self) -> BTreeMap<String, Vec<u8>> {
        fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else if path.extension().is_none_or(|e| e != "lock") {
                    let rel = path.strip_prefix(root).unwrap();
                    let key = rel.to_string_lossy().replace('\\', "/");
                    out.insert(key, std::fs::read(&path).unwrap());
                }
            }
        }
        let mut found = BTreeMap::new();
        walk(&self.root, &self.root, &mut found);
        found
    }

    /// Asserts that every lock file under the directory is empty.
    pub fn assert_lock_files_hold_no_data(&self) {
        fn walk(dir: &Path) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path);
                } else if path.extension().is_some_and(|e| e == "lock") {
                    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0, "{path:?}");
                }
            }
        }
        walk(&self.root);
    }

    /// Runs `ruletrace` with `args`.
    pub fn ruletrace(&self, args: &[&str]) -> Ran {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let done = rt_cli::run(&args, &StdFs);
        Ran {
            code: i32::from(done.code.value()),
            stdout: String::from_utf8(done.stdout).unwrap(),
            stderr: done.stderr,
        }
    }

    /// Runs `ruletrace-ai` with `args`.
    pub fn ruletrace_ai(&self, args: &[&str]) -> Ran {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        let code = {
            let mut out = Out::new(&mut stdout, &mut stderr, Redactor::no_credential());
            rt_ai_tool::run(&args, &NoEnv, &AiFs, &mut out)
        };
        Ran {
            code: i32::from(code.value()),
            stdout: String::from_utf8(stdout).unwrap(),
            stderr: String::from_utf8(stderr).unwrap(),
        }
    }

    /// Writes `ai.toml` for the `mock-replay` provider. The recorded script
    /// and the Transmission_Log sit in this directory.
    pub fn write_config(&self, ai_identities: &[&str]) {
        let identities: Vec<String> = ai_identities.iter().map(|i| format!("\"{i}\"")).collect();
        let text = format!(
            "schema_version = 1\nenabled = true\nprovider = \"mock-replay\"\n\
             model = \"model-1\"\nscript = \"{}\"\nallow_document_text = true\n\
             ai_identities = [{}]\ntransmission_log = \"{}\"\n",
            self.arg("script.json"),
            identities.join(", "),
            self.arg("work/transmission.log.jsonl"),
        );
        std::fs::create_dir_all(self.path("work")).unwrap();
        self.write("ai.toml", text.as_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let private = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(self.path("ai.toml"), private).unwrap();
        }
    }

    fn propose_args(&self, rulebook: &str, extra: &[&str]) -> Vec<String> {
        let mut args = vec!["propose".to_owned()];
        for (name, rel) in [
            ("--config", "ai.toml"),
            ("--rulebook", rulebook),
            ("--source-model", "rules.model.json"),
            ("--candidates", "candidates.json"),
            ("--store", "store"),
        ] {
            args.push(name.to_owned());
            args.push(self.arg(rel));
        }
        args.push("--created-at".to_owned());
        args.push(CREATED_AT.to_owned());
        args.extend(extra.iter().map(|s| (*s).to_owned()));
        args
    }

    /// `propose --dry-run`: the Request_Identifier the run would send.
    pub fn request_id(&self, rulebook: &str) -> String {
        let args = self.propose_args(rulebook, &["--dry-run"]);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let ran = self.ruletrace_ai(&refs);
        assert_eq!(ran.code, 0, "{}", ran.stderr);
        ran.stdout.trim().to_owned()
    }

    /// Writes a recorded script that answers `request_id` with `payload`.
    pub fn write_script(&self, request_id: &str, payload: &str) {
        let entry = Value::object(vec![
            ("request_id".to_owned(), Value::string(request_id)),
            ("origin".to_owned(), Value::string("authored")),
            ("completion".to_owned(), Value::string("complete")),
            (
                "response_sha256".to_owned(),
                Value::string(sha256(payload.as_bytes()).as_str()),
            ),
            ("payload".to_owned(), Value::string(payload)),
        ])
        .unwrap();
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
                    ("model".to_owned(), Value::string("model-1")),
                    ("adapter_version".to_owned(), Value::string("mock-1")),
                ])
                .unwrap(),
            ),
            ("captured".to_owned(), Value::string("2026-11-15")),
            ("entries".to_owned(), Value::array(vec![entry])),
        ])
        .unwrap();
        self.write("script.json", write_compact(&script).as_bytes());
    }

    /// `propose --consent rules.md` against `rulebook`.
    pub fn propose(&self, rulebook: &str) -> Ran {
        let args = self.propose_args(rulebook, &["--consent", "rules.md"]);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.ruletrace_ai(&refs)
    }

    /// `ruletrace ingest` of `source/rules.md` into `rules.model.json`.
    pub fn ingest(&self) -> Ran {
        self.ruletrace(&[
            "ingest",
            "--document",
            &self.arg("source/rules.md"),
            "--document-id",
            "rules.md",
            "--out",
            &self.arg("rules.model.json"),
        ])
    }

    fn review_base(&self, verb: &str, rulebook: &str, ledger: &str) -> Vec<String> {
        vec![
            "review".to_owned(),
            verb.to_owned(),
            "--rulebook".to_owned(),
            self.arg(rulebook),
            "--ledger".to_owned(),
            self.arg(ledger),
            "--source-model".to_owned(),
            self.arg("rules.model.json"),
            "--proposals".to_owned(),
            self.arg("store"),
        ]
    }

    pub fn review_list(&self, rulebook: &str, ledger: &str) -> Ran {
        let args = self.review_base("list", rulebook, ledger);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.ruletrace(&refs)
    }

    /// One `approve` (no reason) or `reject` (with a reason) decision.
    pub fn review_decide(&self, d: &Decision<'_>) -> Ran {
        let verb = if d.reason.is_some() {
            "reject"
        } else {
            "approve"
        };
        let mut args = self.review_base(verb, d.rulebook, d.ledger);
        for (name, value) in [
            ("--rule", d.rule),
            ("--revision", "1"),
            ("--content-hash", d.content_hash),
            ("--reviewer", d.reviewer),
            ("--time", REVIEW_TIME),
        ] {
            args.push(name.to_owned());
            args.push(value.to_owned());
        }
        if let Some(reason) = d.reason {
            args.push("--reason".to_owned());
            args.push(reason.to_owned());
        }
        if d.create_ledger {
            args.push("--create-ledger".to_owned());
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.ruletrace(&refs)
    }

    /// `validate`, `compile` and `evaluate` in order, stopping at the first
    /// non-zero exit. Outputs go to `<prefix>validation.json`,
    /// `<prefix>compiled.json` and `<prefix>results.json`.
    pub fn pipeline(&self, rulebook: &str, ledger: &str, prefix: &str) -> Vec<Ran> {
        let validation = self.arg(&format!("{prefix}validation.json"));
        let compiled = self.arg(&format!("{prefix}compiled.json"));
        let results = self.arg(&format!("{prefix}results.json"));
        let model = self.arg("rules.model.json");
        let rulebook = self.arg(rulebook);
        let ledger = self.arg(ledger);
        let data = self.arg("data.json");
        let steps: [Vec<&str>; 3] = [
            vec![
                "validate",
                "--rulebook",
                &rulebook,
                "--ledger",
                &ledger,
                "--source-model",
                &model,
                "--out",
                &validation,
            ],
            vec!["compile", "--validation", &validation, "--out", &compiled],
            vec![
                "evaluate",
                "--compiled",
                &compiled,
                "--data",
                &data,
                "--source-model",
                &model,
                "--out",
                &results,
            ],
        ];
        let mut ran = Vec::new();
        for step in &steps {
            let r = self.ruletrace(step);
            let stop = r.code != 0;
            ran.push(r);
            if stop {
                break;
            }
        }
        ran
    }
}

/// The arguments of one review decision.
pub struct Decision<'a> {
    pub rulebook: &'a str,
    pub ledger: &'a str,
    pub rule: &'a str,
    pub content_hash: &'a str,
    pub reviewer: &'a str,
    pub reason: Option<&'a str>,
    pub create_ledger: bool,
}

/// `(rule, content hash)` of each approval record of the authored ledger
/// fixture, in record order. The hashes are literals of that reviewed file,
/// so they do not come from the AI path.
pub fn authored_approvals() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(fixture("ledger.authored.jsonl")).unwrap();
    text.lines()
        .skip(1)
        .map(|line| (field(line, "rule"), field(line, "content_hash")))
        .collect()
}

fn field(line: &str, name: &str) -> String {
    let marker = format!("\"{name}\":\"");
    let start = line.find(&marker).unwrap() + marker.len();
    let len = line[start..].find('"').unwrap();
    line[start..start + len].to_owned()
}
