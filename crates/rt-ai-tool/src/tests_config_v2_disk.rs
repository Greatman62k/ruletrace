//! Real-disk tests of task 10.13: loading a configuration, of either
//! version, never creates, changes, renames or deletes a file (Req 35.9),
//! and a version 2 mock-replay configuration behaves like its version 1
//! equivalent end to end.

use crate::tests_cli_rig::{run_std, Fixture};
use crate::tests_config_v2::V2;
use crate::tests_gate_rig::{skip_shapes, COMPLETE_ONE};
use crate::tests_rig::payload;
use rt_foundation::ExitCode;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::SystemTime;

type Snapshot = BTreeMap<String, (Vec<u8>, Option<SystemTime>)>;

// Every file and directory under `root`: name, bytes and modification time.
fn snapshot(root: &Path) -> Snapshot {
    fn walk(dir: &Path, root: &Path, out: &mut Snapshot) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let modified = std::fs::metadata(&path).unwrap().modified().ok();
            if path.is_dir() {
                out.insert(format!("{rel}/"), (Vec::new(), modified));
                walk(&path, root, out);
            } else {
                out.insert(rel, (std::fs::read(&path).unwrap(), modified));
            }
        }
    }
    let mut out = Snapshot::new();
    walk(root, root, &mut out);
    out
}

fn v2_meta_text(fixture: &Fixture, endpoint: &str) -> String {
    let log = crate::tests_cli_rig::toml_path(&fixture.path("work/tx.jsonl"));
    V2.replace(
        "transmission_log = \"tx.jsonl\"",
        &format!("transmission_log = \"{log}\""),
    )
    .replace("https://api.meta.ai", endpoint)
}

fn v2_mock_text(fixture: &Fixture) -> String {
    format!(
        "schema_version = 2\nenabled = true\nscript = \"{}\"\nallow_document_text = true\n\
         ai_identities = [\"openai-responses\", \"model-1\"]\ntransmission_log = \"{}\"\n\n\
         [provider]\nid = \"mock-replay\"\nprotocol = \"mock-replay\"\nmodel = \"model-1\"\n",
        crate::tests_cli_rig::toml_path(&fixture.path("script.json")),
        crate::tests_cli_rig::toml_path(&fixture.path("work/tx.jsonl")),
    )
}

struct Run {
    name: &'static str,
    config: String,
    extra: Vec<&'static str>,
    exit: ExitCode,
    codes: Vec<&'static str>,
}

fn run_case(
    name: &'static str,
    config: String,
    extra: &[&'static str],
    exit: ExitCode,
    codes: &[&'static str],
) -> Run {
    Run {
        name,
        config,
        extra: extra.to_vec(),
        exit,
        codes: codes.to_vec(),
    }
}

const CONSENT: [&str; 2] = ["--consent", "rules.md"];

fn runs(fixture: &Fixture) -> Vec<Run> {
    let ok = ExitCode::Success;
    let bad = ExitCode::InvalidInput;
    let openai = fixture.real_config("");
    vec![
        run_case(
            "v1 mock dry run",
            fixture.mock_config(""),
            &["--dry-run"],
            ok,
            &[],
        ),
        run_case(
            "v2 mock dry run",
            v2_mock_text(fixture),
            &["--dry-run"],
            ok,
            &[],
        ),
        run_case(
            "v1 openai, credential unset",
            openai.clone(),
            &CONSENT,
            bad,
            &["RT-AI-0007"],
        ),
        run_case(
            "v2 meta, credential unset",
            v2_meta_text(fixture, "https://api.meta.ai"),
            &CONSENT,
            bad,
            &["RT-AI-0007"],
        ),
        run_case(
            "v2 meta, host not permitted",
            v2_meta_text(fixture, "https://evil.example"),
            &CONSENT,
            bad,
            &["RT-AI-0013"],
        ),
        run_case(
            "v1 openai, host not permitted",
            openai.replace("api.openai.com", "evil.example"),
            &CONSENT,
            bad,
            &["RT-AI-0013"],
        ),
    ]
}

#[test]
fn loading_never_writes_renames_or_deletes_any_file_and_output_is_repeatable() {
    let fixture = Fixture::new("v2-nowrite");
    for case in runs(&fixture) {
        let Run {
            name,
            config,
            extra,
            exit,
            codes,
        } = case;
        fixture.write_config(&config);
        let before = snapshot(fixture.path("").as_path());
        let first = run_std(&fixture.args(&extra));
        let second = run_std(&fixture.args(&extra));
        let after = snapshot(fixture.path("").as_path());
        assert_eq!(first.code, exit, "{name}: {}", first.stderr);
        assert_eq!(first.codes(), codes, "{name}: {}", first.stderr);
        assert_eq!(after, before, "{name}: a file or directory changed");
        assert_eq!(
            (first.code, &first.stdout, &first.stderr),
            (second.code, &second.stdout, &second.stderr),
            "{name}"
        );
    }
}

#[test]
fn the_configuration_file_bytes_and_modification_time_survive_a_successful_dry_run() {
    let fixture = Fixture::new("v2-config-bytes");
    for text in [fixture.mock_config(""), v2_mock_text(&fixture)] {
        fixture.write_config(&text);
        let path = fixture.path("ai.toml");
        let meta = std::fs::metadata(&path).unwrap();
        let ran = run_std(&fixture.args(&["--dry-run"]));
        assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            meta.modified().unwrap()
        );
        assert_eq!(std::fs::metadata(&path).unwrap().len(), meta.len());
    }
}

#[test]
fn a_version_2_mock_replay_run_equals_the_version_1_run_byte_for_byte() {
    let fixture = Fixture::new("v2-e2e");
    std::fs::create_dir_all(fixture.path("work")).unwrap();
    fixture.write_script(&payload(0));
    let id = fixture.request_id();
    let batch = format!("store/batches/{id}.json");

    let first = run_std(&fixture.consented());
    assert_eq!(first.code, ExitCode::Success, "{}", first.stderr);
    assert_eq!(first.stdout, COMPLETE_ONE);
    assert_eq!(first.shapes(), skip_shapes());
    let batch_v1 = std::fs::read(fixture.path(&batch)).unwrap();
    let log_v1 = std::fs::read(fixture.path("work/tx.jsonl")).unwrap();

    std::fs::remove_dir_all(fixture.path("store")).unwrap();
    std::fs::remove_dir_all(fixture.path("work")).unwrap();
    std::fs::create_dir_all(fixture.path("work")).unwrap();
    fixture.write_config(&v2_mock_text(&fixture));
    assert_eq!(fixture.request_id(), id);

    let second = run_std(&fixture.consented());
    assert_eq!(second.code, ExitCode::Success, "{}", second.stderr);
    assert_eq!(second.stdout, first.stdout);
    assert_eq!(second.stderr, first.stderr);
    assert_eq!(std::fs::read(fixture.path(&batch)).unwrap(), batch_v1);
    assert_eq!(
        std::fs::read(fixture.path("work/tx.jsonl")).unwrap(),
        log_v1
    );
}
