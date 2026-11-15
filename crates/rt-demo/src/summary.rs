//! Plain-text descriptions of produced files for the demonstration summary.
//!
//! The acceptance tests print these descriptions for the transcript. They read
//! the produced `results.json`, the ledger and the captured standard error of a
//! step. No assertion depends on this text: the tests assert on the parsed
//! files themselves.

use crate::error::DemoError;
use crate::json::{array, field, parse_strict, string};
use rt_json::{Kind, Position, Value};

type Json = Value<Position>;

fn bad(step: &str, problem: impl Into<String>) -> DemoError {
    DemoError::Step {
        step: step.to_owned(),
        problem: problem.into(),
    }
}

fn get<'a>(step: &str, v: &'a Json, key: &str) -> Result<&'a Json, DemoError> {
    field(v, key).map_err(|e| bad(step, format!("'{key}': {e}")))
}

fn text<'a>(step: &str, v: &'a Json, key: &str) -> Result<&'a str, DemoError> {
    string(v, key).map_err(|e| bad(step, format!("'{key}': {e}")))
}

fn list<'a>(step: &str, v: &'a Json, key: &str) -> Result<&'a [Json], DemoError> {
    array(v, key).map_err(|e| bad(step, format!("'{key}': {e}")))
}

fn scalar(v: &Json) -> String {
    match v.kind() {
        Kind::String(s) | Kind::Number(s) => s.clone(),
        Kind::Integer(i) => i.to_string(),
        Kind::Bool(b) => b.to_string(),
        _ => "?".to_owned(),
    }
}

fn describe_value(step: &str, value: &Json) -> Result<String, DemoError> {
    if text(step, value, "type")? != "standings" {
        return Ok(scalar(get(step, value, "value")?));
    }
    let mut places = Vec::new();
    for place in list(step, value, "places")? {
        let members: Vec<&str> = list(step, place, "members")?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        places.push(format!(
            "{}:{}",
            scalar(get(step, place, "position")?),
            members.join("=")
        ));
    }
    Ok(places.join(" "))
}

fn describe_reasons(step: &str, outcome: &Json) -> Result<String, DemoError> {
    let mut reasons = Vec::new();
    for reason in list(step, outcome, "reasons")? {
        let Kind::Object(members) = reason.kind() else {
            return Err(bad(step, "an Indeterminate reason is not an object"));
        };
        let kind = text(step, reason, "kind")?;
        let rest: Vec<String> = members
            .iter()
            .filter(|m| m.key != "kind")
            .map(|m| format!("{} {}", m.key, scalar(&m.value)))
            .collect();
        reasons.push(format!("{kind}: {}", rest.join(", ")));
    }
    Ok(reasons.join("; "))
}

fn question_label(step: &str, question: &Json) -> Result<String, DemoError> {
    let target = text(step, question, "target")?;
    Ok(
        match question.get("participant").and_then(|m| m.value.as_str()) {
            Some(p) => format!("{target} {p}"),
            None => target.to_owned(),
        },
    )
}

/// The diagnostic codes on standard error, in order, each with its severity.
///
/// A diagnostic line is `<severity> RT-XXX-NNNN: <message>`; continuation
/// lines start with a space and are skipped.
#[must_use]
pub fn diagnostic_codes(stderr: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stderr)
        .lines()
        .filter_map(|line| {
            let (severity, rest) = line.split_once(' ')?;
            let code = rest.split(':').next()?;
            let shaped = code.starts_with("RT-") && code.split('-').count() == 3;
            let known = !severity.is_empty() && severity.bytes().all(|b| b.is_ascii_lowercase());
            (shaped && known).then(|| format!("{code} ({severity})"))
        })
        .collect()
}

/// Describes one `evaluate` step: every question with its Outcome, the reason
/// of each Indeterminate Outcome, the diagnostic codes on standard error and
/// the verification status of the traces.
///
/// # Errors
/// [`DemoError::Step`] when `results` is not a results file.
pub fn describe_evaluate(
    step_id: &str,
    exit: i32,
    results: &[u8],
    stderr: &[u8],
) -> Result<String, DemoError> {
    let doc = parse_strict("results.json", results).map_err(|e| bad(step_id, e))?;
    let outcomes = list(step_id, &doc, "outcomes")?;
    let traces = list(step_id, &doc, "traces")?;
    let verification = list(step_id, &doc, "verification")?;
    if outcomes.len() != traces.len() || outcomes.len() != verification.len() {
        return Err(bad(
            step_id,
            "outcomes, traces and verification differ in length",
        ));
    }
    let mut out = format!("evaluate {step_id}: exit {exit}\n");
    out.push_str("  question                outcome\n");
    let mut decided = 0;
    for (trace, outcome) in traces.iter().zip(outcomes) {
        let label = question_label(step_id, get(step_id, trace, "question")?)?;
        let described = if text(step_id, outcome, "state")? == "decided" {
            decided += 1;
            describe_value(step_id, get(step_id, outcome, "value")?)?
        } else {
            let state = text(step_id, outcome, "state")?;
            format!("{state} ({})", describe_reasons(step_id, outcome)?)
        };
        out.push_str(&format!("  {label:<23} {described}\n"));
    }
    out.push_str(&format!("  decided {decided} of {}\n", outcomes.len()));
    let codes = diagnostic_codes(stderr);
    if codes.is_empty() {
        out.push_str("  diagnostics: none\n");
    } else {
        out.push_str(&format!("  diagnostics: {}\n", codes.join(", ")));
    }
    let verified = verification
        .iter()
        .filter(|v| text(step_id, v, "status").is_ok_and(|s| s == "verified"))
        .count();
    out.push_str(&format!(
        "  verification: {verified} of {} traces verified\n",
        verification.len()
    ));
    Ok(out)
}

/// Describes each decision record of a ledger file, one line per record.
///
/// # Errors
/// [`DemoError::Step`] when a line is not a ledger record.
pub fn describe_ledger(ledger: &[u8]) -> Result<Vec<String>, DemoError> {
    let mut lines = Vec::new();
    for (index, line) in String::from_utf8_lossy(ledger).lines().enumerate().skip(1) {
        let step = "ledger";
        let record = parse_strict("ledger.jsonl", line.as_bytes())
            .map_err(|e| bad(step, format!("line {}: {e}", index + 1)))?;
        let kind = text(step, &record, "kind")?;
        let mut described = format!(
            "{kind} of {} @ {} by \"{}\" at {}, content hash {}",
            text(step, &record, "rule")?,
            scalar(get(step, &record, "revision")?),
            text(step, &record, "reviewer")?,
            text(step, &record, "time")?,
            text(step, &record, "content_hash")?
        );
        if let Some(reason) = record.get("reason").and_then(|m| m.value.as_str()) {
            described.push_str(&format!(", reason: {reason}"));
        }
        lines.push(described);
    }
    Ok(lines)
}
