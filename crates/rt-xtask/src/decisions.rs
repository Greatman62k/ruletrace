//! `check-decisions` (tasks.md 2.10, decision gates of group 1).
//!
//! `tools/decision-gates.v1` ties crates to decision records. A gated crate may be a workspace
//! member only when `tools/decisions/<id>.md` says `status: Confirmed`. A missing record counts
//! as Pending.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// Path of the gate list, relative to the repository root.
pub const GATES_PATH: &str = "tools/decision-gates.v1";
/// Directory of decision records, relative to the repository root.
pub const DECISIONS_DIR: &str = "tools/decisions";
const HEADER: &str = "# format: ruletrace-decision-gates v1";

/// State of a decision record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Not yet confirmed by the user.
    Pending,
    /// Confirmed by the user.
    Confirmed,
}

/// One `gate` line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gate {
    /// Decision identifier, also the record file name.
    pub decision: String,
    /// Crates that need the decision.
    pub crates: Vec<String>,
}

/// Parses `tools/decision-gates.v1`.
///
/// # Errors
/// A message naming the line when the header is missing or a line is malformed.
pub fn parse_gates(text: &str) -> Result<Vec<Gate>, String> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    match lines.next() {
        Some((_, l)) if l.trim() == HEADER => {}
        _ => return Err(format!("first line must be `{HEADER}`")),
    }
    let mut out = Vec::new();
    for (n, raw) in lines {
        let line = raw.trim();
        if line.starts_with('#') {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["gate", id, "crates", crates @ ..] if !crates.is_empty() => out.push(Gate {
                decision: (*id).to_string(),
                crates: crates.iter().map(|c| (*c).to_string()).collect(),
            }),
            _ => {
                return Err(format!(
                    "line {}: expected `gate <id> crates <crate...>`",
                    n + 1
                ))
            }
        }
    }
    Ok(out)
}

/// Reads the status of a decision record: the first line that starts with `status:`
/// (case-insensitive, markdown emphasis ignored). `None` when there is no such line or the value
/// is neither `Confirmed` nor `Pending`.
#[must_use]
pub fn parse_status(record: &str) -> Option<Status> {
    let value = record.lines().find_map(|l| {
        let l = l.trim();
        let (key, value) = l.split_once(':')?;
        key.trim().eq_ignore_ascii_case("status").then_some(value)
    })?;
    let value = value
        .trim()
        .trim_matches(|c| c == '*' || c == '"' || c == '\'');
    let word = value.split_whitespace().next().unwrap_or("");
    let word = word.trim_matches(|c: char| !c.is_alphanumeric());
    if word.eq_ignore_ascii_case("confirmed") {
        Some(Status::Confirmed)
    } else if word.eq_ignore_ascii_case("pending") {
        Some(Status::Pending)
    } else {
        None
    }
}

/// Findings for gated crates that are workspace members. `records` maps a decision id to the text
/// of its record when the file exists.
#[must_use]
pub fn check(
    gates: &[Gate],
    records: &BTreeMap<String, String>,
    members: &BTreeSet<String>,
) -> Vec<String> {
    let mut findings = Vec::new();
    for gate in gates {
        let status = records
            .get(&gate.decision)
            .map_or(Some(Status::Pending), |t| parse_status(t));
        for krate in gate.crates.iter().filter(|c| members.contains(*c)) {
            let why = match (records.contains_key(&gate.decision), status) {
                (_, Some(Status::Confirmed)) => continue,
                (false, _) => "has no record".to_string(),
                (true, Some(Status::Pending)) => "is Pending".to_string(),
                (true, None) => "has a record with no valid `status:` line".to_string(),
            };
            findings.push(format!(
                "[decision-pending] {krate}: workspace member gated by {}, which {why} ({DECISIONS_DIR}/{}.md)",
                gate.decision, gate.decision
            ));
        }
    }
    findings
}

/// Runs `check-decisions` on the repository at `root` given workspace member names.
/// Returns the number of gated crates that are members.
///
/// # Errors
/// One line per finding, or the load failure.
pub fn run(root: &Path, members: &BTreeSet<String>) -> Result<usize, String> {
    let text = fs::read_to_string(root.join(GATES_PATH))
        .map_err(|e| format!("[gates] cannot read {GATES_PATH}: {e}"))?;
    let gates = parse_gates(&text).map_err(|m| format!("[gates] {GATES_PATH}: {m}"))?;
    let mut records = BTreeMap::new();
    for gate in &gates {
        let path = root
            .join(DECISIONS_DIR)
            .join(format!("{}.md", gate.decision));
        if let Ok(text) = fs::read_to_string(&path) {
            records.insert(gate.decision.clone(), text);
        }
    }
    let findings = check(&gates, &records, members);
    if findings.is_empty() {
        Ok(gates
            .iter()
            .flat_map(|g| &g.crates)
            .filter(|c| members.contains(*c))
            .count())
    } else {
        Err(findings.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gates() -> Vec<Gate> {
        parse_gates(&format!(
            "{HEADER}\ngate UD-3 crates rt-numeric\ngate UD-27 crates rt-ledger rt-review\n"
        ))
        .unwrap()
    }

    fn members(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn status_formats_in_the_repository_parse() {
        assert_eq!(
            parse_status("---\nstatus: Confirmed\n---"),
            Some(Status::Confirmed)
        );
        assert_eq!(
            parse_status("# T\n\nStatus: **Confirmed**\n"),
            Some(Status::Confirmed)
        );
        assert_eq!(parse_status("status: Pending"), Some(Status::Pending));
        assert_eq!(parse_status("status: maybe"), None);
        assert_eq!(parse_status("no status"), None);
    }

    #[test]
    fn pending_decision_with_member_crate_fails_naming_crate() {
        let mut records = BTreeMap::new();
        records.insert("UD-3".to_string(), "status: Pending\n".to_string());
        let f = check(&gates(), &records, &members(&["rt-numeric"]));
        assert_eq!(f.len(), 1);
        assert!(f[0].contains("rt-numeric") && f[0].contains("UD-3") && f[0].contains("Pending"));
    }

    #[test]
    fn missing_record_counts_as_pending() {
        let f = check(&gates(), &BTreeMap::new(), &members(&["rt-ledger"]));
        assert_eq!(f.len(), 1);
        assert!(f[0].contains("rt-ledger") && f[0].contains("no record"));
    }

    #[test]
    fn confirmed_or_absent_crates_pass() {
        let mut records = BTreeMap::new();
        records.insert("UD-3".to_string(), "status: Confirmed".to_string());
        assert!(check(&gates(), &records, &members(&["rt-numeric", "rt-hash"])).is_empty());
        assert!(check(&gates(), &BTreeMap::new(), &members(&["rt-hash"])).is_empty());
    }

    #[test]
    fn malformed_gate_list_is_rejected() {
        assert!(parse_gates("gate UD-1 crates a").is_err());
        assert!(parse_gates(&format!("{HEADER}\ngate UD-1 crates")).is_err());
        assert!(parse_gates(&format!("{HEADER}\nwhat")).is_err());
    }

    #[test]
    fn real_gate_list_parses() {
        let g = parse_gates(include_str!("../../../tools/decision-gates.v1")).unwrap();
        assert!(g
            .iter()
            .any(|g| g.decision == "UD-2" && g.crates == ["rt-json"]));
    }
}
