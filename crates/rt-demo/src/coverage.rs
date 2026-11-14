//! The Coverage_Statement generator (design §8.11.8, Req 33.6).
//!
//! [`classify`] is a pure function. It gives every Candidate_Clause exactly one class by the
//! first matching row of the table in §8.11.8 and fails naming the first clause that matches no
//! row. Nothing is read from a file, the environment or a clock here; the caller supplies bytes
//! for [`parse_notes`] and writes the output of [`print_json`] and [`print_markdown`].

use crate::json::{array, integer, parse_strict, string};
use rt_foundation::{Code, DocumentId, Location};
use rt_ir::{ApprovalStatus, RuleKey, Rulebook};
use rt_json::{write_pretty, Value};
use rt_ledger::{derive_statuses, effective_revisions, ApprovalLedger, RecordKind};
use rt_srcmodel::CandidateClause;
use rt_validate::ValidationResult;
use std::fmt;

const NOTES_FORMAT: &str = "ruletrace-coverage-notes";
const STATEMENT_FORMAT: &str = "ruletrace-coverage-statement";

/// One human-authored reason why a clause is not a rule, keyed by the exact span of the clause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverageNote {
    /// The document of the clause.
    pub document: DocumentId,
    /// Start byte offset of the clause.
    pub start: u32,
    /// End byte offset of the clause (exclusive).
    pub end: u32,
    /// The authored reason.
    pub reason: String,
}

/// The parsed `coverage-notes.json`. Every entry is labelled `authored` in the file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoverageNotes {
    notes: Vec<CoverageNote>,
}

impl CoverageNotes {
    /// Builds a notes set from entries.
    #[must_use]
    pub fn new(notes: Vec<CoverageNote>) -> Self {
        Self { notes }
    }

    /// The entries in file order.
    #[must_use]
    pub fn notes(&self) -> &[CoverageNote] {
        &self.notes
    }

    fn find(&self, clause: &CandidateClause) -> Option<&CoverageNote> {
        let s = clause.span();
        self.notes.iter().find(|n| {
            &n.document == s.doc() && n.start == s.start().get() && n.end == s.end().get()
        })
    }
}

/// The finding the Semantic_Outcome of a stored proposal recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreFinding {
    /// RT-VAL-0003 naming the unsupported capability.
    Unsupported {
        /// The capability name.
        capability: String,
    },
    /// RT-VAL-0004 with the recorded interpretations.
    Ambiguity {
        /// The recorded interpretations.
        interpretations: Vec<String>,
    },
}

/// A Pending_Proposal of the Proposal_Store with its evidence span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingEntry {
    /// Document of the evidence.
    pub document: DocumentId,
    /// Start byte offset of the evidence.
    pub start: u32,
    /// End byte offset of the evidence (exclusive).
    pub end: u32,
    /// The finding, or `None` for a proposal that awaits a decision.
    pub finding: Option<StoreFinding>,
}

/// The stored proposal batches, reduced to what coverage needs. Empty in the `hand` profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreBatches {
    /// Pending proposals in store order.
    pub pending: Vec<PendingEntry>,
}

/// The class of one Candidate_Clause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoverageClass {
    /// An Effective revision cites the clause.
    RuleApproved {
        /// Rule identifier and revision number, ascending.
        rules: Vec<(String, u32)>,
    },
    /// A Rejected revision cites the clause.
    ProposalRejected {
        /// The rejection reason of the ledger record, when it has one.
        reason: Option<String>,
        /// Rule identifier and revision number, ascending.
        rules: Vec<(String, u32)>,
    },
    /// A stored proposal needs a capability the Rule_IR does not support.
    Unsupported {
        /// The capability name.
        capability: String,
    },
    /// A stored proposal has several interpretations.
    Ambiguity {
        /// The recorded interpretations.
        interpretations: Vec<String>,
    },
    /// The clause is not a rule, for an authored reason.
    NotARule {
        /// The authored reason.
        reason: String,
    },
}

impl CoverageClass {
    /// The stable class name used in the files.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            CoverageClass::RuleApproved { .. } => "rule_approved",
            CoverageClass::ProposalRejected { .. } => "proposal_rejected",
            CoverageClass::Unsupported { .. } => "unsupported",
            CoverageClass::Ambiguity { .. } => "ambiguity",
            CoverageClass::NotARule { .. } => "not_a_rule",
        }
    }
}

/// One line of the statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverageEntry {
    /// The clause with its verified span and text.
    pub clause: CandidateClause,
    /// Its one class.
    pub class: CoverageClass,
}

/// The Coverage_Statement: every clause once, in ascending start offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverageStatement {
    /// The entries.
    pub entries: Vec<CoverageEntry>,
}

/// Why the statement cannot be generated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoverageError {
    /// No row matched the clause (row 6).
    Unclassified {
        /// Document of the clause.
        document: String,
        /// Start byte offset.
        start: u32,
        /// 1-based line.
        line: u32,
        /// Numbering label, when the clause has one.
        label: Option<String>,
    },
    /// The class contradicts the validation run for the clause.
    ValidationDisagrees {
        /// Document of the clause.
        document: String,
        /// Start byte offset.
        start: u32,
        /// What disagrees.
        problem: String,
    },
    /// The notes file is invalid.
    Notes(String),
}

impl fmt::Display for CoverageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoverageError::Unclassified {
                document,
                start,
                line,
                label,
            } => {
                let label = label.as_deref().unwrap_or("no label");
                write!(
                    f,
                    "coverage: the Candidate_Clause '{label}' at byte {start} (line {line}) of \
                     '{document}' matches no class and has no coverage note"
                )
            }
            CoverageError::ValidationDisagrees {
                document,
                start,
                problem,
            } => write!(
                f,
                "coverage: the clause at byte {start} of '{document}' disagrees with the \
                 validation run: {problem}"
            ),
            CoverageError::Notes(m) => write!(f, "coverage notes: {m}"),
        }
    }
}

impl std::error::Error for CoverageError {}

/// Parses `coverage-notes.json`: `format`, `version`, `notes` of
/// `{document, start, end, origin: "authored", reason}`.
///
/// # Errors
/// [`CoverageError::Notes`] naming the entry and the field.
pub fn parse_notes(bytes: &[u8]) -> Result<CoverageNotes, CoverageError> {
    let bad = |m: String| CoverageError::Notes(m);
    let root = parse_strict("coverage-notes.json", bytes).map_err(bad)?;
    let format = string(&root, "format").map_err(|e| bad(format!("format: {e}")))?;
    if format != NOTES_FORMAT {
        return Err(bad(format!(
            "format is '{format}', expected '{NOTES_FORMAT}'"
        )));
    }
    if integer(&root, "version") != Ok(1) {
        return Err(bad("version must be 1".to_owned()));
    }
    let mut notes = Vec::new();
    for (i, n) in array(&root, "notes")
        .map_err(|e| bad(format!("notes: {e}")))?
        .iter()
        .enumerate()
    {
        let at = |what: &str, e: String| bad(format!("notes[{i}].{what}: {e}"));
        let document = string(n, "document").map_err(|e| at("document", e))?;
        let document = DocumentId::new(document).map_err(|e| at("document", e.to_string()))?;
        let num = |key: &str| -> Result<u32, CoverageError> {
            let v = integer(n, key).map_err(|e| at(key, e))?;
            u32::try_from(v).map_err(|_| at(key, "too large".to_owned()))
        };
        let origin = string(n, "origin").map_err(|e| at("origin", e))?;
        if origin != "authored" {
            return Err(at("origin", format!("is '{origin}', expected 'authored'")));
        }
        notes.push(CoverageNote {
            document,
            start: num("start")?,
            end: num("end")?,
            reason: string(n, "reason").map_err(|e| at("reason", e))?.to_owned(),
        });
    }
    Ok(CoverageNotes { notes })
}

fn contained(doc: &DocumentId, start: u32, end: u32, clause: &CandidateClause) -> bool {
    let s = clause.span();
    doc == s.doc() && start >= s.start().get() && end <= s.end().get()
}

fn keys_in(
    rulebook: &Rulebook,
    clause: &CandidateClause,
    keep: &dyn Fn(&RuleKey) -> bool,
) -> Vec<RuleKey> {
    let mut keys: Vec<RuleKey> = rulebook
        .revisions
        .iter()
        .filter(|r| keep(&r.key))
        .filter(|r| {
            r.content
                .spans
                .iter()
                .any(|s| contained(&s.document, s.start, s.end, clause))
        })
        .map(|r| r.key.clone())
        .collect();
    keys.sort();
    keys
}

fn pairs(keys: &[RuleKey]) -> Vec<(String, u32)> {
    keys.iter()
        .map(|k| (k.id.as_str().to_owned(), k.revision.get()))
        .collect()
}

/// Classifies every clause by the first matching row of design §8.11.8.
///
/// `clauses` are taken in ascending start offset (sorted here, so the caller's order does not
/// matter). The class is then checked against `validation`: a clause classed `rule_approved`
/// must have no `RT-ING-0006` Warning.
///
/// # Errors
/// [`CoverageError::Unclassified`] naming the first clause that matches no row, or
/// [`CoverageError::ValidationDisagrees`].
pub fn classify(
    clauses: &[CandidateClause],
    rulebook: &Rulebook,
    ledger: &ApprovalLedger,
    store: &StoreBatches,
    validation: &ValidationResult,
    notes: &CoverageNotes,
) -> Result<CoverageStatement, CoverageError> {
    let statuses = derive_statuses(rulebook, ledger);
    let effective = effective_revisions(rulebook, &statuses);
    let mut ordered: Vec<&CandidateClause> = clauses.iter().collect();
    ordered.sort_by(|a, b| {
        (a.span().doc(), a.span().start().get()).cmp(&(b.span().doc(), b.span().start().get()))
    });
    let unreferenced: Vec<&rt_foundation::SourceSpan> = validation
        .state()
        .as_slice()
        .iter()
        .filter(|d| d.code() == Code::CandidateUnreferenced)
        .filter_map(|d| match d.primary() {
            Location::Source(s) => Some(s),
            _ => None,
        })
        .collect();
    let mut entries = Vec::with_capacity(ordered.len());
    for clause in ordered {
        let class = classify_one(
            clause, rulebook, ledger, store, notes, &statuses, &effective,
        )
        .ok_or_else(|| CoverageError::Unclassified {
            document: clause.span().doc().to_string(),
            start: clause.span().start().get(),
            line: clause.span().line(),
            label: clause.label().map(str::to_owned),
        })?;
        let flagged = unreferenced.iter().any(|s| *s == clause.span());
        if matches!(class, CoverageClass::RuleApproved { .. }) && flagged {
            return Err(CoverageError::ValidationDisagrees {
                document: clause.span().doc().to_string(),
                start: clause.span().start().get(),
                problem: "classed rule_approved but RT-ING-0006 reports it unreferenced".to_owned(),
            });
        }
        entries.push(CoverageEntry {
            clause: clause.clone(),
            class,
        });
    }
    Ok(CoverageStatement { entries })
}

fn classify_one(
    clause: &CandidateClause,
    rulebook: &Rulebook,
    ledger: &ApprovalLedger,
    store: &StoreBatches,
    notes: &CoverageNotes,
    statuses: &std::collections::BTreeMap<RuleKey, ApprovalStatus>,
    effective: &std::collections::BTreeMap<rt_foundation::RuleId, RuleKey>,
) -> Option<CoverageClass> {
    // Row 1.
    let approved = keys_in(rulebook, clause, &|k| effective.get(&k.id) == Some(k));
    if !approved.is_empty() {
        return Some(CoverageClass::RuleApproved {
            rules: pairs(&approved),
        });
    }
    // Row 2.
    let rejected = keys_in(rulebook, clause, &|k| {
        statuses.get(k) == Some(&ApprovalStatus::Rejected)
    });
    if let Some(first) = rejected.first() {
        let reason = ledger
            .records()
            .iter()
            .find(|r| {
                r.kind() == RecordKind::Rejection
                    && r.rule() == &first.id
                    && r.revision() == first.revision
            })
            .and_then(|r| r.reason().map(str::to_owned));
        return Some(CoverageClass::ProposalRejected {
            reason,
            rules: pairs(&rejected),
        });
    }
    // Rows 3 and 4.
    let inside = |e: &&PendingEntry| contained(&e.document, e.start, e.end, clause);
    for entry in store.pending.iter().filter(inside) {
        if let Some(StoreFinding::Unsupported { capability }) = &entry.finding {
            return Some(CoverageClass::Unsupported {
                capability: capability.clone(),
            });
        }
    }
    for entry in store.pending.iter().filter(inside) {
        if let Some(StoreFinding::Ambiguity { interpretations }) = &entry.finding {
            return Some(CoverageClass::Ambiguity {
                interpretations: interpretations.clone(),
            });
        }
    }
    // Row 5. Row 6 is the caller's `None`.
    notes.find(clause).map(|n| CoverageClass::NotARule {
        reason: n.reason.clone(),
    })
}

fn object(members: Vec<(&str, Value<()>)>) -> Value<()> {
    Value::object(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
    .unwrap_or_else(|_| Value::null())
}

fn strings(items: &[String]) -> Value<()> {
    Value::array(items.iter().map(Value::string).collect())
}

fn rule_values(rules: &[(String, u32)]) -> Value<()> {
    Value::array(
        rules
            .iter()
            .map(|(id, rev)| {
                object(vec![
                    ("rule", Value::string(id)),
                    ("revision", Value::integer(*rev)),
                ])
            })
            .collect(),
    )
}

/// The canonical `coverage-statement.json`: RT-JSON pretty with a fixed key order.
#[must_use]
pub fn print_json(statement: &CoverageStatement) -> String {
    let entries = statement
        .entries
        .iter()
        .map(|e| {
            let s = e.clause.span();
            let mut members = vec![
                ("document", Value::string(s.doc().as_str())),
                ("start", Value::integer(s.start().get())),
                ("end", Value::integer(s.end().get())),
                ("line", Value::integer(s.line())),
                ("column", Value::integer(s.column())),
                (
                    "label",
                    e.clause.label().map_or_else(Value::null, Value::string),
                ),
                ("text", Value::string(e.clause.text())),
                ("class", Value::string(e.class.name())),
            ];
            match &e.class {
                CoverageClass::RuleApproved { rules } => {
                    members.push(("rules", rule_values(rules)))
                }
                CoverageClass::ProposalRejected { reason, rules } => {
                    members.push(("rules", rule_values(rules)));
                    members.push((
                        "reason",
                        reason.as_deref().map_or_else(Value::null, Value::string),
                    ));
                }
                CoverageClass::Unsupported { capability } => {
                    members.push(("code", Value::string("RT-VAL-0003")));
                    members.push(("capability", Value::string(capability)));
                }
                CoverageClass::Ambiguity { interpretations } => {
                    members.push(("code", Value::string("RT-VAL-0004")));
                    members.push(("interpretations", strings(interpretations)));
                }
                CoverageClass::NotARule { reason } => {
                    members.push(("reason", Value::string(reason)));
                    members.push(("reason_origin", Value::string("authored")));
                }
            }
            object(members)
        })
        .collect();
    write_pretty(&object(vec![
        ("format", Value::string(STATEMENT_FORMAT)),
        ("version", Value::integer(1)),
        ("entries", Value::array(entries)),
    ]))
}

fn class_detail(class: &CoverageClass) -> String {
    let list = |rules: &[(String, u32)]| {
        rules
            .iter()
            .map(|(id, rev)| format!("{id} r{rev}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match class {
        CoverageClass::RuleApproved { rules } => list(rules),
        CoverageClass::ProposalRejected { reason, rules } => format!(
            "{}; reason: {}",
            list(rules),
            reason.as_deref().unwrap_or("none recorded")
        ),
        CoverageClass::Unsupported { capability } => format!("RT-VAL-0003, {capability}"),
        CoverageClass::Ambiguity { interpretations } => {
            format!("RT-VAL-0004, {}", interpretations.join(" | "))
        }
        CoverageClass::NotARule { reason } => format!("{reason} (authored)"),
    }
}

/// The `coverage-statement.md` rendering: one row per clause, in the order of the JSON file.
#[must_use]
pub fn print_markdown(statement: &CoverageStatement) -> String {
    let mut out = String::from("# Coverage statement\n\n");
    out.push_str(
        "Every Candidate_Clause of the demonstration documents, once, by start offset.\n\n",
    );
    out.push_str("| Document | Start | Line | Label | Class | Detail | Text |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
    for e in &statement.entries {
        let s = e.clause.span();
        let cell = |t: &str| t.replace('|', "\\|").replace('\n', " ");
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            cell(s.doc().as_str()),
            s.start().get(),
            s.line(),
            cell(e.clause.label().unwrap_or("")),
            e.class.name(),
            cell(&class_detail(&e.class)),
            cell(e.clause.text()),
        ));
    }
    out
}
