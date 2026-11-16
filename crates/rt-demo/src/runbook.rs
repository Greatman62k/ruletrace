//! The `ruletrace-runbook` block of `RUNBOOK.md` (design §8.11.12, Req 33.12).

use crate::error::DemoError;
use crate::json::{array, boolean, check_relative, integer, parse_strict, string};
use rt_json::{Kind, Position, Value};
use std::collections::BTreeSet;

/// The info string of the machine-readable block.
pub const BLOCK_LABEL: &str = "ruletrace-runbook";

/// The logical executables a step may name as `argv[0]`.
pub const LOGICAL_EXECUTABLES: [&str; 4] =
    ["ruletrace", "ruletrace-ai", "prize-check", "bracket-check"];

/// A file a step produces and its golden file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepOutput {
    /// Path relative to the work directory.
    pub path: String,
    /// Golden path relative to the demonstration directory.
    pub golden: String,
}

/// One executable step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// Step identifier.
    pub id: String,
    /// Profiles that run the step. Empty means every profile.
    pub profiles: Vec<String>,
    /// Logical `argv[0]` followed by relative arguments.
    pub argv: Vec<String>,
    /// The declared exit code.
    pub expect_exit: i32,
    /// Working directory relative to the work directory.
    pub cwd: String,
    /// Golden file of standard output.
    pub stdout_golden: String,
    /// Golden file of standard error.
    pub stderr_golden: String,
    /// Output files compared with golden files.
    pub outputs: Vec<StepOutput>,
}

/// A parsed runbook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runbook {
    /// Declared profile names.
    pub profiles: BTreeSet<String>,
    /// Steps in execution order, live steps included.
    pub steps: Vec<Step>,
    /// Identifiers of the steps declared `"live": true`. The offline runner
    /// never runs them; only `run_live_step` does, and only when asked.
    pub live: BTreeSet<String>,
}

impl Runbook {
    /// The offline steps of `profile` in order. Steps declared live are not
    /// included.
    ///
    /// # Errors
    /// [`DemoError::Runbook`] when the profile is not declared.
    pub fn steps_for(&self, profile: &str) -> Result<Vec<&Step>, DemoError> {
        if !self.profiles.contains(profile) {
            return Err(DemoError::Runbook(format!(
                "profile '{profile}' is not declared"
            )));
        }
        Ok(self
            .steps
            .iter()
            .filter(|s| !self.live.contains(&s.id))
            .filter(|s| s.profiles.is_empty() || s.profiles.iter().any(|p| p == profile))
            .collect())
    }

    /// The steps of `profile` declared live, in order.
    ///
    /// # Errors
    /// [`DemoError::Runbook`] when the profile is not declared.
    pub fn live_steps_for(&self, profile: &str) -> Result<Vec<&Step>, DemoError> {
        if !self.profiles.contains(profile) {
            return Err(DemoError::Runbook(format!(
                "profile '{profile}' is not declared"
            )));
        }
        Ok(self
            .steps
            .iter()
            .filter(|s| self.live.contains(&s.id))
            .filter(|s| s.profiles.is_empty() || s.profiles.iter().any(|p| p == profile))
            .collect())
    }
}

struct Fence {
    info: String,
    lines: Vec<String>,
    start_line: usize,
}

/// Splits Markdown into its fenced blocks.
fn fences(markdown: &str) -> Result<Vec<Fence>, DemoError> {
    let mut out = Vec::new();
    let mut open: Option<(char, usize, Fence)> = None;
    for (index, line) in markdown.lines().enumerate() {
        let trimmed = line.trim_start();
        let run = |c: char| trimmed.chars().take_while(|x| *x == c).count();
        if let Some((c, len, mut fence)) = open.take() {
            let n = run(c);
            if n >= len && trimmed.trim_end().chars().all(|x| x == c) {
                out.push(fence);
            } else {
                fence.lines.push(line.to_owned());
                open = Some((c, len, fence));
            }
            continue;
        }
        for c in ['`', '~'] {
            let n = run(c);
            if n >= 3 {
                let info = trimmed
                    .chars()
                    .skip(n)
                    .collect::<String>()
                    .trim()
                    .to_owned();
                open = Some((
                    c,
                    n,
                    Fence {
                        info,
                        lines: Vec::new(),
                        start_line: index + 1,
                    },
                ));
                break;
            }
        }
    }
    match open {
        Some((_, _, f)) => Err(DemoError::Runbook(format!(
            "the fenced block that starts at line {} is not closed",
            f.start_line
        ))),
        None => Ok(out),
    }
}

fn is_live(info: &str) -> bool {
    info.split_whitespace().any(|w| w == "live")
}

fn command_name(line: &str) -> Option<&'static str> {
    let rest = line.trim_start();
    let rest = rest.strip_prefix("$ ").unwrap_or(rest);
    let first = rest.split_whitespace().next()?;
    LOGICAL_EXECUTABLES.iter().copied().find(|n| *n == first)
}

fn bad(step: &str, field: &str, problem: &str) -> DemoError {
    DemoError::Runbook(format!("step '{step}' field '{field}': {problem}"))
}

fn parse_step(node: &Value<Position>) -> Result<Step, DemoError> {
    let id = string(node, "id").map_err(|p| DemoError::Runbook(format!("step id: {p}")))?;
    let f = |name: &'static str| move |p: String| bad(id, name, &p);
    let argv: Vec<String> = array(node, "argv")
        .map_err(f("argv"))?
        .iter()
        .map(|v| v.as_str().map(str::to_owned))
        .collect::<Option<_>>()
        .ok_or_else(|| bad(id, "argv", "every element must be a string"))?;
    let program = argv
        .first()
        .ok_or_else(|| bad(id, "argv", "is empty"))?
        .as_str();
    if !LOGICAL_EXECUTABLES.contains(&program) {
        return Err(bad(
            id,
            "argv",
            &format!("'{program}' is not a logical executable"),
        ));
    }
    for arg in argv.iter().skip(1) {
        if arg.starts_with('/') || arg.starts_with('\\') || arg.as_bytes().get(1) == Some(&b':') {
            return Err(bad(
                id,
                "argv",
                &format!("argument '{arg}' is not relative"),
            ));
        }
        if arg.split(['/', '\\']).any(|s| s == "..") {
            return Err(bad(
                id,
                "argv",
                &format!("argument '{arg}' leaves the work directory"),
            ));
        }
    }
    let code = integer(node, "expect_exit").map_err(f("expect_exit"))?;
    let expect_exit = i32::try_from(code).map_err(|_| bad(id, "expect_exit", "is too large"))?;
    let cwd = match node.get("cwd") {
        None => ".".to_owned(),
        Some(_) => string(node, "cwd").map_err(f("cwd"))?.to_owned(),
    };
    check_relative(&cwd).map_err(|p| bad(id, "cwd", &p))?;
    let stdout_golden = string(node, "stdout_golden").map_err(f("stdout_golden"))?;
    let stderr_golden = string(node, "stderr_golden").map_err(f("stderr_golden"))?;
    check_relative(stdout_golden).map_err(|p| bad(id, "stdout_golden", &p))?;
    check_relative(stderr_golden).map_err(|p| bad(id, "stderr_golden", &p))?;
    let mut profiles = Vec::new();
    if node.get("profile").is_some() {
        for p in array(node, "profile").map_err(f("profile"))? {
            let name = p
                .as_str()
                .ok_or_else(|| bad(id, "profile", "every element must be a string"))?;
            profiles.push(name.to_owned());
        }
    }
    let mut outputs = Vec::new();
    if node.get("outputs").is_some() {
        for o in array(node, "outputs").map_err(f("outputs"))? {
            let path = string(o, "path").map_err(f("outputs.path"))?;
            let golden = string(o, "golden").map_err(f("outputs.golden"))?;
            check_relative(path).map_err(|p| bad(id, "outputs.path", &p))?;
            check_relative(golden).map_err(|p| bad(id, "outputs.golden", &p))?;
            outputs.push(StepOutput {
                path: path.to_owned(),
                golden: golden.to_owned(),
            });
        }
    }
    Ok(Step {
        id: id.to_owned(),
        profiles,
        argv,
        expect_exit,
        cwd,
        stdout_golden: stdout_golden.to_owned(),
        stderr_golden: stderr_golden.to_owned(),
        outputs,
    })
}

/// Parses `RUNBOOK.md`.
///
/// The Markdown must hold exactly one block labelled `ruletrace-runbook`. Any
/// other fenced block with a line that starts with `ruletrace`, `ruletrace-ai`,
/// `prize-check` or `bracket-check` must be marked `live` in its info string,
/// so no unexecuted offline command can exist.
///
/// # Errors
/// [`DemoError::Runbook`] naming the line, step or field.
pub fn parse_runbook(markdown: &str) -> Result<Runbook, DemoError> {
    let blocks = fences(markdown)?;
    let mut machine = Vec::new();
    for block in &blocks {
        if block.info == BLOCK_LABEL {
            machine.push(block);
            continue;
        }
        if is_live(&block.info) {
            continue;
        }
        for (i, line) in block.lines.iter().enumerate() {
            if let Some(name) = command_name(line) {
                return Err(DemoError::Runbook(format!(
                    "unmarked command '{name}' at line {} in a fenced block; mark the block \
                     'live' or move the command into the {BLOCK_LABEL} block",
                    block.start_line + 1 + i
                )));
            }
        }
    }
    let block = match machine.as_slice() {
        [one] => *one,
        [] => return Err(DemoError::Runbook(format!("no {BLOCK_LABEL} block"))),
        _ => {
            return Err(DemoError::Runbook(format!(
                "more than one {BLOCK_LABEL} block"
            )))
        }
    };
    let text = block.lines.join("\n");
    let doc = parse_strict("RUNBOOK.md", text.as_bytes()).map_err(DemoError::Runbook)?;
    let top = |n: &'static str| move |p: String| DemoError::Runbook(format!("field '{n}': {p}"));
    if string(&doc, "format").map_err(top("format"))? != BLOCK_LABEL {
        return Err(DemoError::Runbook(format!(
            "field 'format' is not '{BLOCK_LABEL}'"
        )));
    }
    if integer(&doc, "version").map_err(top("version"))? != 1 {
        return Err(DemoError::Runbook("field 'version' is not 1".to_owned()));
    }
    let profiles: BTreeSet<String> = match doc.get("profiles").map(|m| m.value.kind()) {
        Some(Kind::Object(members)) => members.iter().map(|m| m.key.clone()).collect(),
        _ => {
            return Err(DemoError::Runbook(
                "field 'profiles' is missing or not an object".to_owned(),
            ))
        }
    };
    let mut steps: Vec<Step> = Vec::new();
    let mut live = BTreeSet::new();
    for node in array(&doc, "steps").map_err(top("steps"))? {
        let step = parse_step(node)?;
        if steps.iter().any(|s| s.id == step.id) {
            return Err(DemoError::Runbook(format!(
                "duplicate step id '{}'",
                step.id
            )));
        }
        for p in &step.profiles {
            if !profiles.contains(p) {
                return Err(bad(&step.id, "profile", &format!("'{p}' is not declared")));
            }
        }
        if node.get("live").is_some()
            && boolean(node, "live").map_err(|p| bad(&step.id, "live", &p))?
        {
            if step.argv.first().map(String::as_str) != Some("ruletrace-ai") {
                return Err(bad(
                    &step.id,
                    "live",
                    "only a ruletrace-ai step may be live",
                ));
            }
            if !step.outputs.is_empty() {
                return Err(bad(
                    &step.id,
                    "outputs",
                    "a live step has no golden outputs; its output never feeds a golden file",
                ));
            }
            live.insert(step.id.clone());
        }
        steps.push(step);
    }
    Ok(Runbook {
        profiles,
        steps,
        live,
    })
}
