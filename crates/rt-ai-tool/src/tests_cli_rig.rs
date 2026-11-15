//! Fixtures of the `ruletrace-ai propose` tests: input files on the real
//! disk under a [`ScratchDir`], a scripted environment and a runner that
//! captures both output streams.

use crate::fs::StoreFs;
use crate::tests::MapEnv;
use crate::tests_provenance::{doc, model, RULES, RULES_HASH};
use crate::tests_scratch::ScratchDir;
use crate::{run, Out, Redactor, StdFs};
use rt_foundation::ExitCode;
use rt_hash::sha256;
use rt_json::{write_compact, Value};
use rt_srcmodel::{print_candidates, CandidateClause};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(crate) const CREATED_AT: &str = "2026-11-15T00:00:00Z";

/// What one run printed and returned.
pub(crate) struct Ran {
    pub(crate) code: ExitCode,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl Ran {
    /// `(severity, code id)` of each Diagnostic on standard error.
    pub(crate) fn shapes(&self) -> Vec<(String, String)> {
        self.stderr
            .lines()
            .filter_map(|l| {
                let (severity, rest) = l.split_once(' ')?;
                let id = rest.split_once(':')?.0;
                let known = ["error", "warning", "ambiguity", "unsupported"];
                (known.contains(&severity) && id.starts_with("RT-"))
                    .then(|| (severity.to_owned(), id.to_owned()))
            })
            .collect()
    }

    pub(crate) fn codes(&self) -> Vec<String> {
        self.shapes().into_iter().map(|(_, id)| id).collect()
    }
}

pub(crate) struct Fixture {
    dir: ScratchDir,
}

fn rulebook_text() -> String {
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"n","version":"1","documents":[{{"id":"rules.md","sha256":"{RULES_HASH}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[],"precedence":[]}}"#
    )
}

/// A path as the inside of a TOML basic string. Windows paths hold `\`,
/// which a basic string reads as the start of an escape.
pub(crate) fn toml_path(path: &Path) -> String {
    escape_toml_basic(&path.to_string_lossy())
}

pub(crate) fn escape_toml_basic(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, text).unwrap();
}

impl Fixture {
    /// Inputs for one clause (`Alpha beta.`) of `rules.md`, a `mock-replay`
    /// configuration and no recorded script yet.
    pub(crate) fn new(name: &str) -> Self {
        let fixture = Self {
            dir: ScratchDir::new(name),
        };
        let source = model("rules.md", RULES);
        write(&fixture.path("rules.model.json"), &source.print());
        write(&fixture.path("rulebook.json"), &rulebook_text());
        let clause = CandidateClause::new(&source, 0, 11, None).unwrap();
        write(
            &fixture.path("candidates.json"),
            &print_candidates(&doc("rules.md"), &[clause]),
        );
        fixture.write_config(&fixture.mock_config(""));
        fixture
    }

    pub(crate) fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// A file below the `store` directory, joined with the separators of
    /// this platform as the tool joins its own store paths.
    pub(crate) fn store_file(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.path("store"), |path, part| path.join(part))
    }

    pub(crate) fn text(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap()
    }

    pub(crate) fn mock_config(&self, extra: &str) -> String {
        format!(
            "schema_version = 1\nenabled = true\nprovider = \"mock-replay\"\n\
             model = \"model-1\"\nscript = \"{}\"\nallow_document_text = true\n\
             ai_identities = [\"openai-responses\", \"model-1\"]\n\
             transmission_log = \"{}\"\n{extra}",
            toml_path(&self.path("script.json")),
            toml_path(&self.path("work/tx.jsonl"))
        )
    }

    pub(crate) fn real_config(&self, extra: &str) -> String {
        format!(
            "schema_version = 1\nenabled = true\nprovider = \"openai-responses\"\n\
             model = \"model-1\"\nendpoint = \"https://api.example.org\"\n\
             credential_env_var = \"PROVIDER_CREDENTIAL\"\nallow_document_text = true\n\
             ai_identities = [\"openai-responses\", \"model-1\"]\n\
             transmission_log = \"{}\"\n{extra}",
            toml_path(&self.path("work/tx.jsonl"))
        )
    }

    pub(crate) fn write_config(&self, text: &str) {
        let path = self.path("ai.toml");
        write(&path, text);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let private = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&path, private).unwrap();
        }
    }

    /// The arguments of a run over this fixture's files.
    pub(crate) fn args(&self, extra: &[&str]) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec!["propose".into()];
        for (name, file) in [
            ("--config", "ai.toml"),
            ("--rulebook", "rulebook.json"),
            ("--source-model", "rules.model.json"),
            ("--candidates", "candidates.json"),
            ("--store", "store"),
        ] {
            args.push(name.into());
            args.push(self.path(file).into());
        }
        args.push("--created-at".into());
        args.push(CREATED_AT.into());
        args.extend(extra.iter().map(OsString::from));
        args
    }

    pub(crate) fn consented(&self) -> Vec<OsString> {
        self.args(&["--consent", "rules.md"])
    }

    /// The Request_Identifier of the run, from its own `--dry-run`.
    pub(crate) fn request_id(&self) -> String {
        let ran = run_with(&self.args(&["--dry-run"]), &MapEnv::new(&[]), &StdFs);
        assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
        ran.stdout.trim().to_owned()
    }

    /// Writes a recorded script that answers the request with `payload`.
    pub(crate) fn write_script(&self, payload: &str) {
        let id = self.request_id();
        self.write_script_for(&id, payload);
    }

    pub(crate) fn write_script_for(&self, id: &str, payload: &str) {
        let entry = Value::object(vec![
            ("request_id".to_owned(), Value::string(id)),
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
        write(&self.path("script.json"), &write_compact(&script));
    }

    /// Every file under the scratch directory, relative, sorted.
    pub(crate) fn files(&self) -> Vec<String> {
        fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else {
                    let rel = path.strip_prefix(root).unwrap();
                    let parts: Vec<_> = rel.iter().map(|c| c.to_string_lossy()).collect();
                    out.push(parts.join("/"));
                }
            }
        }
        let mut found = Vec::new();
        walk(self.dir.path(), self.dir.path(), &mut found);
        found.sort();
        found
    }
}

pub(crate) fn run_with(args: &[OsString], env: &MapEnv, fs: &dyn StoreFs) -> Ran {
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let code = {
        let mut out = Out::new(&mut stdout, &mut stderr, Redactor::no_credential());
        run(args, env, fs, &mut out)
    };
    Ran {
        code,
        stdout: String::from_utf8(stdout).unwrap(),
        stderr: String::from_utf8(stderr).unwrap(),
    }
}

pub(crate) fn run_std(args: &[OsString]) -> Ran {
    run_with(args, &MapEnv::new(&[]), &StdFs)
}

#[test]
fn a_windows_path_survives_the_toml_basic_string_round_trip() {
    let path = r"D:\a\ruletrace\crates/../target\test-scratch\run\ai.toml";
    let text = format!("v = \"{}\"\n", escape_toml_basic(path));
    let parsed: toml_edit::Document<String> = text.parse().unwrap();
    assert_eq!(parsed["v"].as_str(), Some(path));
}

#[test]
fn an_unescaped_windows_path_is_not_valid_toml() {
    let text = "v = \"D:\\a\\b\"\n";
    assert!(text.parse::<toml_edit::Document<String>>().is_err());
}
