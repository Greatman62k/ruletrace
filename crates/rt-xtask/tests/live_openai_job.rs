//! Structure and gate behaviour of the optional `live-openai` CI job (task
//! 10.5, UD-15 section 6, design 7.4, Req 21.14). The job must never be
//! required, must skip cleanly without its secret or model variable, and
//! must never expose the secret. No test here makes a network call.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};

const JOB: &str = "live-openai";
const SECRET: &str = "RULETRACE_OPENAI_API_KEY";
const CONDITION: &str = "steps.gate.outputs.configured == 'true'";

fn ci_text() -> String {
    let path: PathBuf =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/ci.yml");
    std::fs::read_to_string(path).unwrap()
}

// Lines of the job `name`: from its key to the next line at indent 2 or
// less, which is the following job or a comment that introduces it.
fn job_lines<'a>(text: &'a str, name: &str) -> Vec<&'a str> {
    let key = format!("  {name}:");
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line == key {
            inside = true;
            out.push(line);
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if inside && !line.trim().is_empty() && indent <= 2 {
            break;
        }
        if inside {
            out.push(line);
        }
    }
    out
}

// Steps of a job body: chunks that start at a line `      - `.
fn steps<'a>(job: &[&'a str]) -> Vec<Vec<&'a str>> {
    let mut out: Vec<Vec<&str>> = Vec::new();
    let mut in_steps = false;
    for line in job {
        if *line == "    steps:" {
            in_steps = true;
            continue;
        }
        if !in_steps {
            continue;
        }
        if line.starts_with("      - ") {
            out.push(Vec::new());
        }
        if let Some(last) = out.last_mut() {
            last.push(line);
        }
    }
    out
}

fn has_condition(step: &[&str]) -> bool {
    step.iter().any(|l| {
        let t = l.trim_start();
        let t = t.strip_prefix("- ").unwrap_or(t);
        t.starts_with("if:") && t.contains(CONDITION)
    })
}

// Every structural rule of the job. Returns one message per violation.
fn violations(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let job = job_lines(text, JOB);
    if job.is_empty() {
        return vec![format!("job {JOB} is missing")];
    }

    for (n, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("needs:") && line.contains(JOB) {
            found.push(format!("line {}: {JOB} is in a needs list", n + 1));
        }
    }

    let secret_uses: Vec<usize> = text
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains("secrets."))
        .map(|(n, _)| n)
        .collect();
    let env_line = format!("      {SECRET}: ${{{{ secrets.{SECRET} }}}}");
    let env_ok = secret_uses.len() == 1
        && text.lines().nth(secret_uses[0]) == Some(env_line.as_str())
        && job.contains(&env_line.as_str());
    if !env_ok {
        found.push("the secret is referenced outside the job env".to_owned());
    }
    let before_steps = job.iter().position(|l| *l == "    steps:");
    let env_pos = job.iter().position(|l| *l == env_line);
    if !matches!((env_pos, before_steps), (Some(e), Some(s)) if e < s) {
        found.push("the secret env line is not in the job-level env".to_owned());
    }

    // The secret may be expanded only by the emptiness test of the gate.
    for line in &job {
        let t = line.trim();
        let expands = t.contains(&format!("${SECRET}")) || t.contains(&format!("${{{SECRET}"));
        let allowed = t.starts_with("if [ -z \"${RULETRACE_OPENAI_API_KEY:-}\" ]");
        if expands && !allowed {
            found.push(format!("unexpected expansion of the secret: {t}"));
        }
        let dumps = t == "set -x" || t.contains("printenv") || t.starts_with("env ") || t == "env";
        if dumps {
            found.push(format!("a step can dump the environment: {t}"));
        }
    }

    for line in &job {
        let t = line.trim();
        if t.starts_with("permissions:") {
            found.push("the job overrides permissions".to_owned());
        }
        if t.contains("upload-artifact") {
            found.push("the live job uploads an artifact".to_owned());
        }
    }

    let all = steps(&job);
    match all.first() {
        Some(first) if first.iter().any(|l| l.trim() == "id: gate") => {
            if has_condition(first) {
                found.push("the gate step must not be conditional".to_owned());
            }
        }
        _ => found.push("the first step is not the gate step".to_owned()),
    }
    for step in all.iter().skip(1) {
        if !has_condition(step) {
            found.push(format!(
                "a live step lacks the gate condition: {}",
                step[0].trim()
            ));
        }
    }
    found
}

#[test]
fn the_committed_workflow_satisfies_every_live_job_rule() {
    assert_eq!(violations(&ci_text()), Vec::<String>::new());
}

#[test]
fn the_job_has_the_gate_plus_the_documented_live_steps() {
    let text = ci_text();
    let all = steps(&job_lines(&text, JOB));
    assert!(all.len() >= 5, "found {} steps", all.len());
    let models: Vec<_> = text
        .lines()
        .filter(|l| l.contains("ruletrace-ai models --config"))
        .collect();
    assert_eq!(models.len(), 1);
    let propose: Vec<_> = text
        .lines()
        .filter(|l| l.contains("\"$bin/ruletrace-ai\" propose"))
        .collect();
    assert_eq!(propose.len(), 1);
    let step = all
        .iter()
        .find(|s| {
            s.iter()
                .any(|l| l.contains("\"$bin/ruletrace-ai\" propose"))
        })
        .unwrap();
    let cond = step
        .iter()
        .find(|l| l.trim_start().starts_with("if:"))
        .unwrap();
    assert!(cond.contains("github.event_name == 'workflow_dispatch'"));
    assert!(cond.contains("inputs.propose == true"));
}

#[test]
fn the_workflow_dispatch_input_is_a_boolean_defaulting_to_false() {
    let text = ci_text();
    let lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .position(|l| *l == "  workflow_dispatch:")
        .unwrap();
    let block = lines[at..at + 8].join("\n");
    assert!(block.contains("      propose:"));
    assert!(block.contains("        type: boolean"));
    assert!(block.contains("        default: false"));
}

#[test]
fn the_ci_gate_platform_count_check_still_holds() {
    let text = ci_text();
    let n = text
        .matches("os: [ubuntu-24.04, macos-15, windows-2022]")
        .count();
    assert_eq!(n, 6);
}

// Mutants: each edit of the real workflow must be reported.

fn mutate(from: &str, to: &str) -> String {
    let text = ci_text();
    assert!(text.contains(from), "mutation anchor missing: {from}");
    text.replacen(from, to, 1)
}

#[test]
fn mutant_a_live_step_without_the_condition_is_reported() {
    let text = mutate(
        "        if: steps.gate.outputs.configured == 'true'\n        run: cargo build",
        "        run: cargo build",
    );
    let found = violations(&text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("Build the executables"), "{found:?}");
}

#[test]
fn mutant_the_checkout_step_without_the_condition_is_reported() {
    let text = mutate(
        "      - uses: actions/checkout@v5\n        if: steps.gate.outputs.configured == 'true'\n",
        "      - uses: actions/checkout@v5\n",
    );
    let found = violations(&text);
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn mutant_the_job_in_the_ci_gate_needs_is_reported() {
    let text = mutate(
        "core-only, demo-offline]",
        "core-only, demo-offline, live-openai]",
    );
    let found = violations(&text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("needs list"));
}

#[test]
fn mutant_the_secret_in_a_step_is_reported() {
    let text = mutate(
        "        run: cargo build --locked --bins -p rt-ai-tool -p rt-cli",
        "        env:\n          LEAK: ${{ secrets.RULETRACE_OPENAI_API_KEY }}\n        run: cargo build --locked --bins -p rt-ai-tool -p rt-cli",
    );
    assert!(!violations(&text).is_empty());
}

#[test]
fn mutant_the_secret_echoed_by_a_step_is_reported() {
    let text = mutate(
        "        run: cargo build --locked --bins -p rt-ai-tool -p rt-cli",
        "        run: echo \"$RULETRACE_OPENAI_API_KEY\"",
    );
    let found = violations(&text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("unexpected expansion of the secret"));
}

#[test]
fn mutant_another_secret_reference_elsewhere_is_reported() {
    let text = mutate(
        "      - name: cargo fmt --check\n",
        "      - name: cargo fmt --check\n        env:\n          X: ${{ secrets.RULETRACE_OPENAI_API_KEY }}\n",
    );
    assert!(!violations(&text).is_empty());
}

#[test]
fn mutant_an_artifact_upload_in_the_live_job_is_reported() {
    let text = mutate(
        "        run: cargo build --locked --bins -p rt-ai-tool -p rt-cli",
        "        uses: actions/upload-artifact@v6\n        run: cargo build --locked --bins -p rt-ai-tool -p rt-cli",
    );
    assert!(violations(&text)
        .iter()
        .any(|v| v.contains("uploads an artifact")));
}

#[test]
fn mutant_a_conditional_gate_step_is_reported() {
    let text = mutate(
        "        id: gate\n",
        "        id: gate\n        if: steps.gate.outputs.configured == 'true'\n",
    );
    assert!(violations(&text)
        .iter()
        .any(|v| v.contains("gate step must not")));
}

#[cfg(unix)]
mod gate_script {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // The `run` block of the gate step with its YAML indentation removed.
    fn script_of(text: &str) -> String {
        script_of_step(text, 0)
    }

    fn script_of_step(text: &str, index: usize) -> String {
        let job = job_lines(text, JOB);
        let all = steps(&job);
        let gate = &all[index];
        let start = gate.iter().position(|l| l.trim() == "run: |").unwrap();
        gate[start + 1..]
            .iter()
            .map(|l| l.strip_prefix("          ").unwrap_or(l))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    struct Outcome {
        stdout: String,
        stderr: String,
        output_file: String,
    }

    fn run(script: &str, key: Option<&str>, model: Option<&str>) -> Outcome {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("rt-live-gate-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("github-output");
        std::fs::write(&out, "").unwrap();
        let mut cmd = Command::new("bash");
        cmd.arg("-e").arg("-c").arg(script);
        cmd.env_clear().env("PATH", "/usr/bin:/bin");
        cmd.env("GITHUB_OUTPUT", &out);
        if let Some(k) = key {
            cmd.env(SECRET, k);
        }
        if let Some(m) = model {
            cmd.env("RULETRACE_OPENAI_MODEL", m);
        }
        let o = cmd.output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let result = Outcome {
            stdout: String::from_utf8(o.stdout).unwrap(),
            stderr: String::from_utf8(o.stderr).unwrap(),
            output_file: std::fs::read_to_string(&out).unwrap(),
        };
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    const CANARY: &str = "CANARY-sk-live-9f3a1c7e5b2d4a60";

    fn clean(o: &Outcome) {
        assert!(!o.stdout.contains(CANARY));
        assert!(!o.stderr.contains(CANARY));
        assert!(!o.output_file.contains(CANARY));
    }

    #[test]
    fn secret_unset_skips_and_names_the_secret() {
        let script = script_of(&ci_text());
        let o = run(&script, None, Some("some-model"));
        assert_eq!(o.output_file, "configured=false\n");
        assert_eq!(
            o.stdout,
            "live-openai skipped: the repository secret RULETRACE_OPENAI_API_KEY is not set.\n"
        );
        assert_eq!(o.stderr, "");
    }

    #[test]
    fn secret_empty_skips_like_unset() {
        let script = script_of(&ci_text());
        let o = run(&script, Some(""), Some("some-model"));
        assert_eq!(o.output_file, "configured=false\n");
        assert!(o
            .stdout
            .contains("secret RULETRACE_OPENAI_API_KEY is not set"));
    }

    #[test]
    fn secret_set_and_model_unset_skips_and_names_the_variable() {
        let script = script_of(&ci_text());
        let o = run(&script, Some(CANARY), None);
        assert_eq!(o.output_file, "configured=false\n");
        assert_eq!(
            o.stdout,
            "live-openai skipped: the secret is set but the repository variable RULETRACE_OPENAI_MODEL is empty or absent.\n"
        );
        clean(&o);
    }

    #[test]
    fn secret_set_and_model_empty_skips() {
        let script = script_of(&ci_text());
        let o = run(&script, Some(CANARY), Some(""));
        assert_eq!(o.output_file, "configured=false\n");
        clean(&o);
    }

    #[test]
    fn both_set_configures_without_printing_the_secret() {
        let script = script_of(&ci_text());
        let o = run(&script, Some(CANARY), Some("some-model"));
        assert_eq!(o.output_file, "configured=true\n");
        assert_eq!(
            o.stdout,
            "live-openai configured: the secret and the model variable are set.\n"
        );
        clean(&o);
    }

    #[test]
    fn mutant_a_gate_that_prints_the_secret_is_detected() {
        let script = script_of(&ci_text()).replace(
            "echo \"live-openai configured: the secret and the model variable are set.\"",
            "echo \"key=$RULETRACE_OPENAI_API_KEY\"",
        );
        let o = run(&script, Some(CANARY), Some("some-model"));
        assert!(o.stdout.contains(CANARY));
    }

    #[test]
    fn mutant_a_gate_that_ignores_the_model_variable_is_detected() {
        let script = script_of(&ci_text()).replace(
            "elif [ -z \"${RULETRACE_OPENAI_MODEL:-}\" ]; then",
            "elif false; then",
        );
        let o = run(&script, Some(CANARY), None);
        assert_eq!(o.output_file, "configured=true\n");
    }

    // Runs the configuration step in a scratch RUNNER_TEMP. Returns the exit
    // success flag, the stderr text and the files written.
    fn write_config(model: &str) -> (bool, String, Vec<(String, u32, String)>) {
        use std::os::unix::fs::PermissionsExt;
        static NEXT: AtomicUsize = AtomicUsize::new(1000);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let temp = std::env::temp_dir().join(format!("rt-live-cfg-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        let text = ci_text();
        let job = job_lines(&text, JOB);
        let all = steps(&job);
        let index = all
            .iter()
            .position(|s| s[0].contains("Write the configuration"))
            .unwrap();
        let script = script_of_step(&text, index);
        let o = Command::new("bash")
            .arg("-e")
            .arg("-c")
            .arg(&script)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("RUNNER_TEMP", &temp)
            .env("RULETRACE_OPENAI_MODEL", model)
            .env(SECRET, CANARY)
            .output()
            .unwrap();
        let mut files = Vec::new();
        let live = temp.join("live-openai");
        if let Ok(read) = std::fs::read_dir(&live) {
            for entry in read {
                let entry = entry.unwrap();
                let mode = entry.metadata().unwrap().permissions().mode() & 0o777;
                let body = std::fs::read_to_string(entry.path()).unwrap();
                files.push((entry.file_name().to_string_lossy().into_owned(), mode, body));
            }
        }
        files.sort();
        let stdout = String::from_utf8(o.stdout).unwrap();
        let stderr = String::from_utf8(o.stderr).unwrap();
        assert!(!stdout.contains(CANARY) && !stderr.contains(CANARY));
        let _ = std::fs::remove_dir_all(&temp);
        (o.status.success(), stderr, files)
    }

    #[test]
    fn a_plain_model_identifier_writes_two_private_configurations() {
        let (ok, _, files) = write_config("gpt-test-1:x");
        assert!(ok);
        let names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(names, ["models.toml", "propose.toml"]);
        for (name, mode, body) in &files {
            assert_eq!(*mode, 0o600, "{name}");
            assert!(body.contains("model = \"gpt-test-1:x\"\n"));
            assert!(body.contains("credential_env_var = \"RULETRACE_OPENAI_API_KEY\"\n"));
            assert!(body.contains("max_retries = 0\n"));
            assert!(!body.contains(CANARY));
        }
        assert!(files[0].2.contains("allow_document_text = false\n"));
        assert!(files[1].2.contains("allow_document_text = true\n"));
    }

    #[test]
    fn a_model_text_outside_the_charset_is_refused_and_nothing_is_written() {
        let hostile = [
            "x\"; touch /tmp/rt-live-pwn; \"",
            "a b",
            "ok\nbad",
            "ok\n",
            "",
            "-lead",
            "a\"b",
            "a\\b",
            "a$(id)",
            &"m".repeat(101),
        ];
        for model in hostile {
            let (ok, stderr, files) = write_config(model);
            assert!(!ok, "accepted {model:?}");
            assert!(files.is_empty(), "wrote files for {model:?}");
            assert!(
                stderr.contains("is not a plain model identifier"),
                "{model:?}"
            );
        }
        assert!(!Path::new("/tmp/rt-live-pwn").exists());
    }
}
