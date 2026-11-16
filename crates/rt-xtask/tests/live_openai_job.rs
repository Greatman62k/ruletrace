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

// A step needs the secret when it is the gate or when it runs one of the
// two `ruletrace-ai` commands that call the provider.
fn needs_secret(step: &[&str]) -> bool {
    step.iter().any(|l| {
        let t = l.trim();
        t == "id: gate"
            || t.contains("ruletrace-ai models --config")
            || t.contains("\"$bin/ruletrace-ai\" propose")
    })
}

// Text of every `needs` value in the file, for the inline, multi-line flow
// and block list styles of YAML. One entry per key, with its line number.
fn needs_values(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        let Some(rest) = line.trim_start().strip_prefix("needs:") else {
            continue;
        };
        let mut value = rest.to_owned();
        if rest.contains('[') && !rest.contains(']') {
            for next in &lines[n + 1..] {
                value.push(' ');
                value.push_str(next);
                if next.contains(']') {
                    break;
                }
            }
        } else if rest.trim().is_empty() {
            for next in &lines[n + 1..] {
                let t = next.trim();
                if t.is_empty() || t.starts_with('#') {
                    continue;
                }
                if !t.starts_with('-') {
                    break;
                }
                value.push(' ');
                value.push_str(t);
            }
        }
        out.push((n + 1, value));
    }
    out
}

// Every structural rule of the job. Returns one message per violation.
fn violations(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let job = job_lines(text, JOB);
    if job.is_empty() {
        return vec![format!("job {JOB} is missing")];
    }

    for (n, value) in needs_values(text) {
        if value.contains(JOB) {
            found.push(format!("line {n}: {JOB} is in a needs list"));
        }
    }

    for (n, line) in text.lines().enumerate() {
        if !line.trim_start().starts_with('#') && line.contains("pull_request_target") {
            found.push(format!("line {}: pull_request_target is used", n + 1));
        }
    }

    // The secret may be referenced only as an exact step-level env entry.
    let env_line = format!("          {SECRET}: ${{{{ secrets.{SECRET} }}}}");
    let steps_at = job.iter().position(|l| *l == "    steps:");
    for line in &job[..steps_at.unwrap_or(job.len())] {
        if line.contains("secrets.") {
            found.push("the secret is in the job-level env".to_owned());
        }
    }
    let all_lines: Vec<&str> = text.lines().collect();
    let total = all_lines.iter().filter(|l| l.contains("secrets.")).count();
    let exact = all_lines.iter().filter(|l| **l == env_line).count();
    let wanted = steps(&job).iter().filter(|s| needs_secret(s)).count();
    if total != exact || exact != wanted {
        found.push(format!(
            "the secret is referenced {total} times ({exact} exact step env entries), expected {wanted}"
        ));
    }
    for step in steps(&job) {
        let env_pos = step.iter().position(|l| *l == env_line);
        let key_pos = step.iter().position(|l| *l == "        env:");
        let run_pos = step.iter().position(|l| l.trim_start().starts_with("run:"));
        let name = step[0].trim();
        match (needs_secret(&step), env_pos) {
            (true, None) => found.push(format!("a step that needs the secret lacks it: {name}")),
            (false, Some(_)) => found.push(format!("a step must not see the secret: {name}")),
            (true, Some(e)) => {
                let after_key = matches!(key_pos, Some(k) if k < e);
                let before_run = run_pos.is_none_or(|r| e < r);
                if !after_key || !before_run {
                    found.push(format!("the secret is not in the step env: {name}"));
                }
            }
            (false, None) => {}
        }
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

#[test]
fn mutant_the_secret_at_job_level_is_reported() {
    let text = mutate(
        "      RULETRACE_OPENAI_MODEL: ${{ vars.RULETRACE_OPENAI_MODEL }}\n",
        "      RULETRACE_OPENAI_API_KEY: ${{ secrets.RULETRACE_OPENAI_API_KEY }}\n      RULETRACE_OPENAI_MODEL: ${{ vars.RULETRACE_OPENAI_MODEL }}\n",
    );
    let found = violations(&text);
    assert!(
        found.iter().any(|v| v.contains("job-level env")),
        "{found:?}"
    );
}

#[test]
fn mutant_the_secret_in_the_build_step_env_is_reported() {
    let text = mutate(
        "        run: cargo build --locked --bins -p rt-ai-tool -p rt-cli",
        "        env:\n          RULETRACE_OPENAI_API_KEY: ${{ secrets.RULETRACE_OPENAI_API_KEY }}\n        run: cargo build --locked --bins -p rt-ai-tool -p rt-cli",
    );
    let found = violations(&text);
    assert!(
        found
            .iter()
            .any(|v| v == "a step must not see the secret: - name: Build the executables"),
        "{found:?}"
    );
}

#[test]
fn mutant_the_secret_missing_from_the_models_step_is_reported() {
    let text = ci_text();
    let at = text.find("      - name: Model catalogue").unwrap();
    let tail = text[at..].replacen(
        "        env:\n          RULETRACE_OPENAI_API_KEY: ${{ secrets.RULETRACE_OPENAI_API_KEY }}\n",
        "",
        1,
    );
    let mutated = format!("{}{}", &text[..at], tail);
    assert_ne!(mutated, text);
    let found = violations(&mutated);
    assert!(
        found
            .iter()
            .any(|v| v.starts_with("a step that needs the secret lacks it")),
        "{found:?}"
    );
}

#[test]
fn mutant_the_secret_in_run_text_is_reported() {
    let text = mutate(
        "        run: cargo build --locked --bins -p rt-ai-tool -p rt-cli",
        "        run: |\n          RULETRACE_OPENAI_API_KEY: ${{ secrets.RULETRACE_OPENAI_API_KEY }}",
    );
    assert!(!violations(&text).is_empty());
}

#[test]
fn mutant_pull_request_target_as_a_key_is_reported() {
    let text = mutate(
        "  pull_request:\n",
        "  pull_request:\n  pull_request_target:\n",
    );
    let found = violations(&text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("pull_request_target"));
}

#[test]
fn mutant_pull_request_target_in_flow_style_is_reported() {
    let text = mutate(
        "name: ci\n\non:\n",
        "name: ci\n\non: [pull_request_target]\nxon:\n",
    );
    let found = violations(&text);
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn a_comment_that_names_pull_request_target_is_not_a_violation() {
    let text = mutate(
        "name: ci\n",
        "name: ci\n# never use pull_request_target here\n",
    );
    assert_eq!(violations(&text), Vec::<String>::new());
}

const GATE_NEEDS: &str =
    "    needs: [fmt, clippy, test, golden, toolchain, graph, deny, core-only, demo-offline]\n";

#[test]
fn mutant_the_job_in_a_block_style_needs_list_is_reported() {
    let mut block = String::from("    needs:\n");
    for name in [
        "fmt",
        "clippy",
        "test",
        "golden",
        "toolchain",
        "graph",
        "deny",
        "core-only",
        "demo-offline",
        "live-openai",
    ] {
        block.push_str(&format!("      - {name}\n"));
    }
    let text = mutate(GATE_NEEDS, &block);
    let found = violations(&text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("needs list"));
}

#[test]
fn mutant_the_job_in_a_multi_line_flow_needs_list_is_reported() {
    let text = mutate(
        GATE_NEEDS,
        "    needs: [fmt, clippy, test, golden,\n      toolchain, graph, deny, core-only,\n      demo-offline, live-openai]\n",
    );
    let found = violations(&text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("needs list"));
}

#[test]
fn a_block_style_needs_list_without_the_job_is_accepted() {
    let text = mutate(GATE_NEEDS, "    needs:\n      - fmt\n      - clippy\n");
    assert_eq!(violations(&text), Vec::<String>::new());
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

    type Written = (bool, String, Vec<(String, u32, String)>);

    // Runs the configuration step of `text` in a scratch RUNNER_TEMP, with
    // an optional `LC_ALL`. Returns the success flag, the stderr text and
    // the files written.
    fn write_config_in(text: &str, model: &str, lc_all: Option<&str>) -> Written {
        use std::os::unix::fs::PermissionsExt;
        static NEXT: AtomicUsize = AtomicUsize::new(1000);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let temp = std::env::temp_dir().join(format!("rt-live-cfg-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        let job = job_lines(text, JOB);
        let all = steps(&job);
        let index = all
            .iter()
            .position(|s| s[0].contains("Write the configuration"))
            .unwrap();
        let script = script_of_step(text, index);
        let mut cmd = Command::new("bash");
        cmd.arg("-e").arg("-c").arg(&script);
        cmd.env_clear().env("PATH", "/usr/bin:/bin");
        cmd.env("RUNNER_TEMP", &temp);
        cmd.env("RULETRACE_OPENAI_MODEL", model);
        if let Some(lc) = lc_all {
            cmd.env("LC_ALL", lc);
        }
        let o = cmd.output().unwrap();
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

    fn write_config(model: &str) -> Written {
        write_config_in(&ci_text(), model, None)
    }

    // An installed UTF-8 locale whose collation makes bracket ranges accept
    // letters outside ASCII, if the machine has one.
    fn utf8_locale() -> Option<String> {
        let o = Command::new("locale").arg("-a").output().ok()?;
        let list = String::from_utf8(o.stdout).ok()?;
        list.lines()
            .find(|l| l.eq_ignore_ascii_case("en_US.utf8") || l.eq_ignore_ascii_case("en_US.utf-8"))
            .map(str::to_owned)
    }

    // The configuration script exports `LC_ALL=C` before its first regex
    // match, so the charset class is ASCII whatever the runner locale is.
    fn pin_present(text: &str) -> bool {
        let job = job_lines(text, JOB);
        let all = steps(&job);
        let step = all
            .iter()
            .find(|s| s[0].contains("Write the configuration"))
            .unwrap();
        let pin = step.iter().position(|l| l.trim() == "export LC_ALL=C");
        let matched = step.iter().position(|l| l.contains("=~"));
        matches!((pin, matched), (Some(p), Some(m)) if p < m)
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

    const ENDPOINT_LINE: &str = "endpoint = \"https://api.openai.com\"";

    // The generated files name exactly one endpoint, and it is the pinned one.
    fn endpoint_pinned(files: &[(String, u32, String)]) -> bool {
        !files.is_empty()
            && files.iter().all(|(_, _, body)| {
                let found: Vec<&str> = body
                    .lines()
                    .filter(|l| l.trim_start().starts_with("endpoint"))
                    .collect();
                found == [ENDPOINT_LINE]
            })
    }

    #[test]
    fn the_generated_configurations_pin_the_exact_endpoint() {
        let (ok, _, files) = write_config("gpt-test-1");
        assert!(ok);
        assert_eq!(files.len(), 2);
        assert!(endpoint_pinned(&files));
    }

    #[test]
    fn mutant_a_look_alike_or_http_endpoint_is_detected() {
        for bad in [
            "http://api.openai.com",
            "https://api.openai.com.evil.example",
            "https://api.openai.com@evil.example",
            "http://api.openai.com.evil.example",
        ] {
            let text = ci_text().replacen(
                "endpoint = \"https://api.openai.com\"",
                &format!("endpoint = \"{bad}\""),
                1,
            );
            assert_ne!(text, ci_text());
            let (ok, _, files) = write_config_in(&text, "gpt-test-1", None);
            assert!(ok, "{bad}");
            assert!(!endpoint_pinned(&files), "{bad}");
        }
    }

    #[test]
    fn mutant_a_second_endpoint_line_is_detected() {
        let text = ci_text().replacen(
            "          endpoint = \"https://api.openai.com\"\n",
            "          endpoint = \"https://api.openai.com\"\n          endpoint = \"http://x\"\n",
            1,
        );
        let (_, _, files) = write_config_in(&text, "gpt-test-1", None);
        assert!(!endpoint_pinned(&files));
    }

    const NON_ASCII: [&str; 3] = ["\u{e9}", "a\u{663}", "\u{ff21}bc"];
    const VALID: [&str; 5] = [
        "gpt-5",
        "o3-mini",
        "A1",
        "gpt-4.1-2025-04-14",
        "ft:gpt:org:x_y",
    ];

    #[test]
    fn non_ascii_model_texts_are_refused_in_every_available_locale() {
        let mut locales: Vec<Option<String>> = vec![None];
        locales.push(utf8_locale());
        for lc in locales {
            for model in NON_ASCII {
                let (ok, stderr, files) = write_config_in(&ci_text(), model, lc.as_deref());
                assert!(!ok, "accepted {model:?} under {lc:?}");
                assert!(files.is_empty(), "{model:?} {lc:?}");
                assert!(stderr.contains("is not a plain model identifier"));
            }
        }
    }

    #[test]
    fn valid_model_identifiers_are_accepted_in_every_available_locale() {
        let longest = "m".repeat(100);
        let mut locales: Vec<Option<String>> = vec![None];
        locales.push(utf8_locale());
        for lc in locales {
            for model in VALID.iter().copied().chain([longest.as_str()]) {
                let (ok, stderr, files) = write_config_in(&ci_text(), model, lc.as_deref());
                assert!(ok, "refused {model:?} under {lc:?}: {stderr}");
                assert_eq!(files.len(), 2);
            }
        }
    }

    #[test]
    fn the_committed_step_pins_the_locale_before_the_match() {
        assert!(pin_present(&ci_text()));
    }

    #[test]
    fn mutant_removing_the_locale_pin_is_detected() {
        let text = mutate_text("          export LC_ALL=C\n", "");
        assert!(!pin_present(&text));
        // Where the machine has a UTF-8 locale, show that the defect is
        // real: the unpinned step accepts letters outside ASCII. The job
        // runs on Linux only, and the bash on macOS does not show the
        // defect, so the behavioural half is asserted on Linux alone.
        match utf8_locale().filter(|_| cfg!(target_os = "linux")) {
            Some(lc) => {
                let accepted = NON_ASCII
                    .iter()
                    .filter(|m| write_config_in(&text, m, Some(&lc)).0)
                    .count();
                assert!(accepted > 0, "no non-ASCII text accepted under {lc}");
            }
            None => eprintln!("no Linux UTF-8 locale: only the structural check ran"),
        }
    }

    #[test]
    fn mutant_a_pin_placed_after_the_match_is_detected() {
        let text = mutate_text("          export LC_ALL=C\n", "").replacen(
            "          dir=\"$RUNNER_TEMP/live-openai\"\n",
            "          export LC_ALL=C\n          dir=\"$RUNNER_TEMP/live-openai\"\n",
            1,
        );
        assert!(!pin_present(&text));
    }

    fn mutate_text(from: &str, to: &str) -> String {
        let text = ci_text();
        assert!(text.contains(from), "mutation anchor missing: {from}");
        text.replacen(from, to, 1)
    }
}
