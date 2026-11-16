//! The configuration that the optional `live-openai` CI job writes at run
//! time is accepted by the real `ruletrace-ai` parser (review wave 8,
//! finding 5). The step script is taken from the workflow file and run in a
//! scratch directory. The dry run sends nothing and reads no credential.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use support::{Ran, Work};

fn workflow() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/ci.yml");
    std::fs::read_to_string(path).unwrap()
}

// The `run` block of the step whose name starts with "Write the
// configuration", with its YAML indentation removed.
fn config_script(text: &str) -> String {
    let mut lines = text
        .lines()
        .skip_while(|l| !l.starts_with("      - name: Write the configuration"))
        .skip_while(|l| *l != "        run: |")
        .skip(1);
    let body: Vec<&str> = lines
        .by_ref()
        .take_while(|l| l.is_empty() || l.starts_with("          "))
        .map(|l| l.strip_prefix("          ").unwrap_or(l))
        .collect();
    assert!(!body.is_empty());
    body.join("\n") + "\n"
}

fn written_by(text: &str, work: &Work) {
    let o = Command::new("bash")
        .arg("-e")
        .arg("-c")
        .arg(config_script(text))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("RUNNER_TEMP", work.path("runner-temp"))
        .env("RULETRACE_OPENAI_MODEL", "gpt-test-1")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

fn dry_run(work: &Work) -> Ran {
    let stand_in = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo/stand-in-ai");
    let inputs = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rt-ai-tool/tests/fixtures/live-run1-prompt-2");
    work.copy_from(&inputs, "inputs/candidates.json", "candidates.json");
    work.copy_from(&inputs, "inputs/source-model.json", "source-model.json");
    work.copy_from(&stand_in.join("rulebook"), "seed.json", "seed.json");
    let config = work.path("runner-temp/live-openai/propose.toml");
    assert_eq!(
        std::fs::metadata(&config).unwrap().permissions().mode() & 0o777,
        0o600
    );
    work.ruletrace_ai(&[
        "propose",
        "--dry-run",
        "--config",
        &config.to_string_lossy(),
        "--rulebook",
        &work.arg("seed.json"),
        "--source-model",
        &work.arg("source-model.json"),
        "--candidates",
        &work.arg("candidates.json"),
        "--store",
        &work.arg("store"),
        "--created-at",
        "2026-11-16T00:00:00Z",
        "--consent",
        "stand-in-rules.md",
    ])
}

#[test]
fn the_real_parser_accepts_the_configuration_the_ci_step_writes() {
    let work = Work::new("live-ci-config");
    written_by(&workflow(), &work);
    let ran = dry_run(&work);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert!(
        ran.stderr
            .contains("provider openai, endpoint host api.openai.com, documents ['stand-in-rules.md'], clauses 5"),
        "{}",
        ran.stderr
    );
    let id = ran.stdout.trim_end();
    assert_eq!(id.len(), 64, "{id}");
    assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(!work.exists("store"), "a dry run writes no store");
}

#[test]
fn mutant_an_http_endpoint_in_the_step_is_refused_by_the_real_parser() {
    let text = workflow();
    let from = "endpoint = \"https://api.openai.com\"";
    assert!(text.contains(from));
    let mutated = text.replacen(from, "endpoint = \"http://api.openai.com\"", 1);
    let work = Work::new("live-ci-config-http");
    written_by(&mutated, &work);
    let ran = dry_run(&work);
    assert_eq!(ran.code, 2, "{}", ran.stdout);
    assert!(
        ran.stderr
            .contains("error RT-AI-0004: field `provider.endpoint` must use https"),
        "{}",
        ran.stderr
    );
}

#[test]
fn mutant_a_stale_schema_version_in_the_step_is_refused_by_the_real_parser() {
    let text = workflow();
    let from = "schema_version = 2";
    assert!(text.contains(from));
    let mutated = text.replacen(from, "schema_version = 1", 1);
    let work = Work::new("live-ci-config-schema");
    written_by(&mutated, &work);
    let ran = dry_run(&work);
    assert_eq!(ran.code, 2, "{}", ran.stdout);
    assert!(
        ran.stderr
            .contains("error RT-AI-0002: required field `model` is missing"),
        "{}",
        ran.stderr
    );
}
