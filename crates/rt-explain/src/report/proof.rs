//! The Proof_Report builder (design §8.11.9, Req 33.14, 13.11, 13.19, 13.21,
//! 31.13).
//!
//! One section per decision, in the order of Req 33.14: the Question and
//! Outcome; each applied Rule with identifier and Revision_Number; the
//! Approval_Record of each Rule; each Source_Span with the exact text the
//! Explainer resolved; each input value read; each calculation step with
//! operation, operands and exact result; each tie-breaking fact; each missing
//! input; each considered Rule that did not apply; the verification status; the
//! published-result comparison.
//!
//! The report is built only from the Decision_Trace, the [`ApprovalFacts`]
//! derived from the ledger by the caller, and the Explainer's [`Verification`].
//! It computes no value and contains no text from an AI provider. It has no
//! timestamp, path or environment value, so equal inputs give equal bytes in
//! both forms.
//!
//! The header statements of Req 34 are added by task 11.9 through
//! [`ProofHeader::statements`]. The tie-breaking section lists, in trace order,
//! each `compare` entry with its criteria and both compared values in declared
//! order (Req 12.7), and each `tie_group` entry. A trace without such entries
//! falls back to the Tie_Group of the Outcome.

use crate::results::{diagnostics, obj, severity_name, status_name, text};
use crate::verify::{SpanResolution, Verification, VerificationStatus};
use rt_foundation::{escape_controls, RevisionNumber, RuleId};
use rt_json::Value;
use rt_numeric::to_canonical;
use rt_trace::{
    CompareEntry, CriterionResult, DecisionTrace, DecisionValue, Entry, NotAppliedReason, Outcome,
    Place, QuestionKind, Reason, StepEntry, StepResult, TraceValue,
};
use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

/// Format identifier of the machine form.
pub const PROOF_REPORT_FORMAT: &str = "ruletrace-proof-report";
/// Version of the Proof_Report format.
pub const PROOF_REPORT_VERSION: u32 = 1;

/// The Approval_Record facts of one Rule revision, as the caller derived them
/// from the ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalView {
    /// Reviewer identity.
    pub reviewer: String,
    /// Approval time, as the supplied text of the record.
    pub time: String,
    /// Rule_Content_Hash bound by the record.
    pub content_hash: String,
}

/// Approval facts keyed by (Rule identifier, Revision_Number). `rt-explain`
/// never reads a ledger.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApprovalFacts(BTreeMap<(RuleId, RevisionNumber), ApprovalView>);

impl ApprovalFacts {
    /// No facts.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces the facts of one revision.
    pub fn insert(&mut self, rule: RuleId, revision: RevisionNumber, view: ApprovalView) {
        self.0.insert((rule, revision), view);
    }

    /// The facts of one revision, if supplied.
    #[must_use]
    pub fn get(&self, rule: &RuleId, revision: RevisionNumber) -> Option<&ApprovalView> {
        self.0.get(&(rule.clone(), revision))
    }
}

/// The result of comparing one Outcome with a Published_Result, already
/// rendered by the caller as canonical text. The report copies it and compares
/// nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublishedFact {
    /// No published value was supplied for this question.
    NotCompared,
    /// Computed and published values are equal.
    Equal,
    /// They differ.
    Differs {
        /// Published value, canonical text.
        published: String,
        /// Computed value, canonical text.
        computed: String,
    },
}

/// Everything the builder reads. `traces[i]` and `verifications[i]` answer the
/// same question; `published` is empty (nothing compared) or has one entry per
/// trace.
#[derive(Clone, Copy, Debug)]
pub struct ProofInputs<'a> {
    /// Decision_Traces in Question order.
    pub traces: &'a [DecisionTrace],
    /// Explainer output per trace.
    pub verifications: &'a [Verification],
    /// Approval facts of the Rules.
    pub approvals: &'a ApprovalFacts,
    /// Published-result comparison per trace, or empty.
    pub published: &'a [PublishedFact],
    /// Rulebook content hash of the Compiled_Rulebook, as text.
    pub rulebook_hash: &'a str,
    /// Whether the Competition_Data is synthetic, when the caller states it. A
    /// label only.
    pub synthetic_data: Option<bool>,
}

/// Why a report cannot be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofError {
    /// The slices disagree in length.
    LengthMismatch {
        /// Number of traces.
        traces: usize,
        /// Number of verifications.
        verifications: usize,
        /// Number of published facts.
        published: usize,
    },
}

impl fmt::Display for ProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ProofError::LengthMismatch {
            traces,
            verifications,
            published,
        } = self;
        write!(
            f,
            "proof report input has {traces} traces, {verifications} verifications and {published} published facts"
        )
    }
}

impl std::error::Error for ProofError {}

/// The report header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofHeader {
    /// Rulebook content hash text.
    pub rulebook_hash: String,
    /// Synthetic-data label, when stated.
    pub synthetic_data: Option<bool>,
    /// Statement lines of Req 34, added by task 11.9. Empty here.
    pub statements: Vec<String>,
    /// Number of decisions.
    pub decisions: usize,
    /// Number of traces the Explainer verified. Derived from the verifications.
    pub verified_traces: usize,
}

/// One decision section. Holds the trace itself so a reader can replay it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionSection {
    /// Zero-based index of the Question in the results file.
    pub index: usize,
    /// The Decision_Trace.
    pub trace: DecisionTrace,
    /// Approval facts per applied Rule, aligned with `trace.rules()`; `None`
    /// when not supplied.
    pub approvals: Vec<Option<ApprovalView>>,
    /// Explainer output for the trace.
    pub verification: Verification,
    /// Published-result comparison.
    pub published: PublishedFact,
}

/// The Proof_Report: a header and one section per decision, in Question order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofReport {
    /// Header.
    pub header: ProofHeader,
    /// Sections.
    pub decisions: Vec<DecisionSection>,
}

/// Builds the report. Pure.
///
/// # Errors
/// [`ProofError`] when the input slices disagree in length.
pub fn build_proof_report(i: &ProofInputs<'_>) -> Result<ProofReport, ProofError> {
    let n = i.traces.len();
    if i.verifications.len() != n || !(i.published.is_empty() || i.published.len() == n) {
        return Err(ProofError::LengthMismatch {
            traces: n,
            verifications: i.verifications.len(),
            published: i.published.len(),
        });
    }
    let decisions: Vec<DecisionSection> = i
        .traces
        .iter()
        .zip(i.verifications)
        .enumerate()
        .map(|(index, (trace, verification))| DecisionSection {
            index,
            trace: trace.clone(),
            approvals: trace
                .rules()
                .iter()
                .map(|r| i.approvals.get(&r.rule, r.revision).cloned())
                .collect(),
            verification: verification.clone(),
            published: i
                .published
                .get(index)
                .cloned()
                .unwrap_or(PublishedFact::NotCompared),
        })
        .collect();
    let verified_traces = i
        .verifications
        .iter()
        .filter(|v| v.status() == VerificationStatus::Verified)
        .count();
    Ok(ProofReport {
        header: ProofHeader {
            rulebook_hash: i.rulebook_hash.to_owned(),
            synthetic_data: i.synthetic_data,
            statements: Vec::new(),
            decisions: n,
            verified_traces,
        },
        decisions,
    })
}

// ----- human form -----

fn esc(s: &str) -> String {
    escape_controls(s)
}

fn fmt_value(v: &TraceValue) -> String {
    match v {
        TraceValue::Number(n) => to_canonical(n),
        TraceValue::Bool(b) => b.to_string(),
        TraceValue::Text(s) => format!("\"{}\"", esc(s)),
        TraceValue::Date(s) | TraceValue::Enum(s) => esc(s),
        TraceValue::Standings(places) => format!(
            "standings [{}]",
            places.iter().map(fmt_place).collect::<Vec<_>>().join("; ")
        ),
        TraceValue::Rank(r) => {
            if r.group().is_empty() {
                format!("rank {}", r.position())
            } else {
                format!(
                    "rank {} (tie group {{{}}})",
                    r.position(),
                    r.group()
                        .iter()
                        .map(|m| esc(m))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}

fn fmt_place(p: &Place) -> String {
    format!(
        "{}: {}",
        p.position(),
        p.members()
            .iter()
            .map(|m| esc(m))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn fmt_compare(seq: usize, c: &CompareEntry) -> String {
    let verdict = if c.first_ahead {
        format!("{} ahead of {}", esc(&c.between.0), esc(&c.between.1))
    } else {
        format!("{} tied with {}", esc(&c.between.0), esc(&c.between.1))
    };
    let criteria: Vec<String> = c
        .criteria
        .iter()
        .map(|k| {
            let result = match k.result {
                CriterionResult::First => format!("{} ahead", esc(&c.between.0)),
                CriterionResult::Second => format!("{} ahead", esc(&c.between.1)),
                CriterionResult::Equal => "equal".to_owned(),
            };
            format!(
                "criterion {} ({}, rule {}): {} vs {} -> {result}",
                k.index,
                k.direction.name(),
                esc(k.rule.as_str()),
                fmt_value(&k.left),
                fmt_value(&k.right)
            )
        })
        .collect();
    format!("{seq}. compare {verdict}: {}", criteria.join("; "))
}

fn fmt_reason(r: &Reason) -> String {
    match r {
        Reason::MissingInput { participant, field } => {
            format!("missing input {} ({})", esc(field), esc(participant))
        }
        Reason::EmptyAggregate { field } => format!("min or max of empty {}", esc(field)),
        Reason::Diagnosed(code) => format!("diagnostic {}", code.id()),
    }
}

fn fmt_outcome(o: &Outcome) -> String {
    match o {
        Outcome::Decided(v) => format!("decided {}", fmt_value(v)),
        Outcome::Indeterminate(reasons) => format!(
            "indeterminate [{}]",
            reasons
                .iter()
                .map(fmt_reason)
                .collect::<Vec<_>>()
                .join("; ")
        ),
    }
}

fn fmt_values(vs: &[TraceValue]) -> String {
    vs.iter().map(fmt_value).collect::<Vec<_>>().join(", ")
}

fn fmt_step(seq: usize, s: &StepEntry) -> String {
    let op = match s.op {
        rt_trace::StepOp::Round { places, mode } => {
            format!("round[places={places}, mode={}]", mode.name())
        }
        other => other.name().to_owned(),
    };
    let result = match &s.result {
        StepResult::Value(v) => fmt_value(v),
        StepResult::Undefined => "undefined".to_owned(),
        StepResult::Fault(f) => format!("fault {}", f.code().id()),
    };
    format!(
        "{seq}. {op}({}) -> {result}   [rule {}]",
        fmt_values(&s.operands),
        esc(s.rule.as_str())
    )
}

fn question_line(t: &DecisionTrace) -> String {
    let q = t.question();
    let kind = match q.kind {
        QuestionKind::Value => "value",
        QuestionKind::Rank => "rank",
    };
    format!(
        "{kind} of {} for {}",
        esc(&q.target),
        q.participant
            .as_deref()
            .map_or_else(|| "(no participant)".to_owned(), esc)
    )
}

fn tie_group(t: &DecisionTrace) -> Option<(u32, Vec<String>)> {
    match t.outcome() {
        Outcome::Decided(TraceValue::Rank(r)) if !r.group().is_empty() => {
            Some((r.position(), r.group().to_vec()))
        }
        _ => None,
    }
}

fn pad(label: &str) -> String {
    format!("  {label:<19}")
}

// Fixed by a user decision: states how a ranking relates to eligibility.
pub(crate) const RANKING_SCOPE_LABEL: &str = "Score ranking \u{2014} all submitted participants; eligibility is evaluated independently and is not applied as a ranking filter.";

fn render_section(out: &mut String, d: &DecisionSection) {
    let t = &d.trace;
    let _ = writeln!(
        out,
        "DECISION {}  question: {}   outcome: {}",
        d.index + 1,
        question_line(t),
        fmt_outcome(t.outcome())
    );
    if matches!(t.outcome(), Outcome::Decided(TraceValue::Standings(_))) {
        let _ = writeln!(out, "{}{RANKING_SCOPE_LABEL}", pad("ranking"));
    }
    let rules: Vec<String> = t
        .rules()
        .iter()
        .map(|r| format!("{} @ revision {}", esc(r.rule.as_str()), r.revision.get()))
        .collect();
    let _ = writeln!(
        out,
        "{}{}",
        pad("rules applied"),
        if rules.is_empty() {
            "none".to_owned()
        } else {
            rules.join("; ")
        }
    );
    if t.rules().is_empty() {
        let _ = writeln!(out, "{}none", pad("approval"));
    }
    for (r, a) in t.rules().iter().zip(&d.approvals) {
        let _ = match a {
            Some(a) => writeln!(
                out,
                "{}{} @ {}: reviewer \"{}\", time {}, content hash {}",
                pad("approval"),
                esc(r.rule.as_str()),
                r.revision.get(),
                esc(&a.reviewer),
                esc(&a.time),
                esc(&a.content_hash)
            ),
            None => writeln!(
                out,
                "{}{} @ {}: no approval record supplied",
                pad("approval"),
                esc(r.rule.as_str()),
                r.revision.get()
            ),
        };
    }
    if d.verification.spans().is_empty() {
        let _ = writeln!(out, "{}none", pad("source"));
    }
    for s in d.verification.spans() {
        let rev = s
            .revision
            .map_or_else(String::new, |r| format!(" @ {}", r.get()));
        let _ = writeln!(
            out,
            "{}rule {}{rev}, {} bytes {}..{}, line {} column {}, sha256 {}",
            pad("source"),
            esc(s.rule.as_str()),
            esc(s.span.document.as_str()),
            s.span.start,
            s.span.end,
            s.span.line,
            s.span.column,
            s.span.sha256.as_str()
        );
        match &s.resolution {
            SpanResolution::Resolved(txt) => {
                let _ = writeln!(out, "{}\"{}\"", pad(""), esc(txt));
            }
            SpanResolution::Unresolved(code) => {
                let _ = writeln!(out, "{}unresolved: {}", pad(""), code.id());
            }
        }
    }
    let mut any_read = false;
    let mut any_step = false;
    let mut any_tie = false;
    for (index, e) in t.entries().iter().enumerate() {
        let seq = index + 1;
        match e {
            Entry::Read(r) => {
                any_read = true;
                let who = r
                    .participant
                    .as_deref()
                    .map_or_else(String::new, |p| format!(" ({})", esc(p)));
                let default = if r.default_spans.is_empty() {
                    ""
                } else {
                    " [declared default]"
                };
                let _ = writeln!(
                    out,
                    "{}{seq}. {}{who} = {}{default}   [rule {}]",
                    pad("inputs read"),
                    esc(&r.name),
                    fmt_value(&r.value),
                    esc(r.rule.as_str())
                );
            }
            Entry::Step(s) => {
                any_step = true;
                let _ = writeln!(out, "{}{}", pad("calculation steps"), fmt_step(seq, s));
            }
            Entry::Decision(dec) => {
                any_step = true;
                let value = match &dec.value {
                    DecisionValue::Known(v) => fmt_value(v),
                    DecisionValue::Unknown => "unknown".to_owned(),
                };
                let from: Vec<String> = dec.from.iter().map(u32::to_string).collect();
                let _ = writeln!(
                    out,
                    "{}{seq}. decision {} from [{}] -> {value}   [rule {}]",
                    pad("calculation steps"),
                    dec.op.name(),
                    from.join(", "),
                    esc(dec.rule.as_str())
                );
            }
            Entry::Compare(c) => {
                any_tie = true;
                let _ = writeln!(out, "{}{}", pad("tie-breaking"), fmt_compare(seq, c));
            }
            Entry::Place(pl) if pl.is_tie_group() => {
                any_tie = true;
                let _ = writeln!(
                    out,
                    "{}{seq}. tie group at position {}: {{{}}}; members are listed in ascending identifier order, which has no ranking meaning",
                    pad("tie-breaking"),
                    pl.position(),
                    pl.members().iter().map(|m| esc(m)).collect::<Vec<_>>().join(", ")
                );
            }
            Entry::Place(_) => {}
        }
    }
    if !any_read {
        let _ = writeln!(out, "{}none", pad("inputs read"));
    }
    if !any_step {
        let _ = writeln!(out, "{}none", pad("calculation steps"));
    }
    match tie_group(t).filter(|_| !any_tie) {
        Some((position, members)) => {
            let _ = writeln!(
                out,
                "{}tie group at position {position}: {{{}}}; members are listed in ascending identifier order, which has no ranking meaning",
                pad("tie-breaking"),
                members.iter().map(|m| esc(m)).collect::<Vec<_>>().join(", ")
            );
        }
        None if any_tie => {}
        None => {
            let _ = writeln!(out, "{}none recorded", pad("tie-breaking"));
        }
    }
    if t.missing().is_empty() {
        let _ = writeln!(out, "{}none", pad("missing inputs"));
    }
    for m in t.missing() {
        let _ = writeln!(
            out,
            "{}{} ({})",
            pad("missing inputs"),
            esc(&m.field),
            esc(&m.participant)
        );
    }
    if t.not_applied().is_empty() {
        let _ = writeln!(out, "{}none", pad("not applied"));
    }
    for n in t.not_applied() {
        let why = match &n.reason {
            NotAppliedReason::ConditionNotSatisfied { condition } => {
                format!("condition not satisfied: {}", esc(condition))
            }
            NotAppliedReason::MissingInput { input } => format!("missing input {}", esc(input)),
            NotAppliedReason::Precedence { id } => format!("overridden by precedence {}", esc(id)),
        };
        let _ = writeln!(
            out,
            "{}{} @ {}: {why}",
            pad("not applied"),
            esc(n.rule.as_str()),
            n.revision.get()
        );
    }
    let status = match d.verification.status() {
        VerificationStatus::Verified => "verified",
        VerificationStatus::NotVerified => "not verified",
    };
    let _ = writeln!(out, "{}{status}", pad("verification"));
    for dg in d.verification.diagnostics().as_slice() {
        let _ = writeln!(
            out,
            "{}{} ({}): {}",
            pad(""),
            dg.code().id(),
            severity_name(dg.severity()),
            esc(dg.message())
        );
    }
    let published = match &d.published {
        PublishedFact::NotCompared => "not compared for this question".to_owned(),
        PublishedFact::Equal => "equal".to_owned(),
        PublishedFact::Differs {
            published,
            computed,
        } => format!(
            "differs: published {}, computed {}",
            esc(published),
            esc(computed)
        ),
    };
    let _ = writeln!(out, "{}{published}", pad("published"));
}

/// The human form. Every dynamic character passes through `escape_controls`.
#[must_use]
pub fn render_proof_human(report: &ProofReport) -> String {
    let h = &report.header;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "RuleTrace proof report   format {PROOF_REPORT_FORMAT} v{PROOF_REPORT_VERSION}"
    );
    let _ = writeln!(out, "rulebook {}", esc(&h.rulebook_hash));
    let synthetic = match h.synthetic_data {
        Some(true) => "synthetic",
        Some(false) => "not synthetic",
        None => "not stated",
    };
    let _ = writeln!(out, "data: {synthetic}");
    let _ = writeln!(
        out,
        "provenance: {} of {} traces verified",
        h.verified_traces, h.decisions
    );
    for s in &h.statements {
        let _ = writeln!(out, "statement: {}", esc(s));
    }
    for d in &report.decisions {
        out.push('\n');
        render_section(&mut out, d);
    }
    out
}

// ----- machine form -----

fn entries_of(trace: &DecisionTrace, kinds: &[&str]) -> Vec<Value<()>> {
    let v = trace.to_value();
    v.get("entries")
        .and_then(|m| m.value.as_array())
        .map(|items| {
            items
                .iter()
                .filter(|e| {
                    e.get("kind")
                        .and_then(|k| k.value.as_str())
                        .is_some_and(|k| kinds.contains(&k))
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn member(trace: &DecisionTrace, key: &str) -> Value<()> {
    trace
        .to_value()
        .get(key)
        .map_or_else(Value::null, |m| m.value.clone())
}

fn section_value(d: &DecisionSection) -> Value<()> {
    let t = &d.trace;
    let rules = t
        .rules()
        .iter()
        .zip(&d.approvals)
        .map(|(r, a)| {
            let approval = a.as_ref().map_or_else(Value::null, |a| {
                obj(vec![
                    ("reviewer", text(&a.reviewer)),
                    ("time", text(&a.time)),
                    ("content_hash", text(&a.content_hash)),
                ])
            });
            obj(vec![
                ("rule", text(r.rule.as_str())),
                ("revision", Value::integer(r.revision.get())),
                ("approval", approval),
            ])
        })
        .collect();
    let sources = d
        .verification
        .spans()
        .iter()
        .map(|s| {
            let (txt, code) = match &s.resolution {
                SpanResolution::Resolved(x) => (text(x), Value::null()),
                SpanResolution::Unresolved(c) => (Value::null(), text(c.id())),
            };
            obj(vec![
                ("rule", text(s.rule.as_str())),
                (
                    "revision",
                    s.revision
                        .map_or_else(Value::null, |r| Value::integer(r.get())),
                ),
                ("document", text(s.span.document.as_str())),
                ("sha256", text(s.span.sha256.as_str())),
                ("start", Value::integer(s.span.start)),
                ("end", Value::integer(s.span.end)),
                ("line", Value::integer(s.span.line)),
                ("column", Value::integer(s.span.column)),
                ("text", txt),
                ("code", code),
            ])
        })
        .collect();
    let mut tie = entries_of(t, &["compare", "tie_group"]);
    if tie.is_empty() {
        tie = tie_group(t).map_or_else(Vec::new, |(position, members)| {
            vec![obj(vec![
                ("position", Value::integer(position)),
                (
                    "members",
                    Value::array(members.iter().map(|m| text(m)).collect()),
                ),
                ("order_has_ranking_meaning", Value::boolean(false)),
            ])]
        });
    }
    let published = match &d.published {
        PublishedFact::NotCompared => obj(vec![("state", text("not_compared"))]),
        PublishedFact::Equal => obj(vec![("state", text("equal"))]),
        PublishedFact::Differs {
            published,
            computed,
        } => obj(vec![
            ("state", text("differs")),
            ("published", text(published)),
            ("computed", text(computed)),
        ]),
    };
    obj(vec![
        (
            "index",
            Value::integer(u32::try_from(d.index).unwrap_or(u32::MAX)),
        ),
        ("question", member(t, "question")),
        ("outcome", member(t, "outcome")),
        ("rules", Value::array(rules)),
        ("sources", Value::array(sources)),
        ("inputs_read", Value::array(entries_of(t, &["read"]))),
        (
            "calculation_steps",
            Value::array(entries_of(t, &["step", "decision"])),
        ),
        ("tie_breaking", Value::array(tie)),
        ("missing_inputs", member(t, "missing")),
        ("not_applied", member(t, "not_applied")),
        (
            "verification",
            obj(vec![
                ("status", text(status_name(&d.verification))),
                ("diagnostics", diagnostics(d.verification.diagnostics())),
            ]),
        ),
        ("published", published),
    ])
}

/// The machine form: canonical compact RT-JSON, fixed key order, no float,
/// trailing line feed. `calculation_steps` repeats the trace entries (`step`
/// and `decision`) with operands and exact results, so a reader can recompute
/// each one.
#[must_use]
pub fn render_proof_json(report: &ProofReport) -> Vec<u8> {
    let h = &report.header;
    let header = obj(vec![
        ("format", text(PROOF_REPORT_FORMAT)),
        ("version", Value::integer(PROOF_REPORT_VERSION)),
        ("rulebook_hash", text(&h.rulebook_hash)),
        (
            "synthetic_data",
            h.synthetic_data.map_or_else(Value::null, Value::boolean),
        ),
        (
            "statements",
            Value::array(h.statements.iter().map(|s| text(s)).collect()),
        ),
        (
            "decisions",
            Value::integer(u32::try_from(h.decisions).unwrap_or(u32::MAX)),
        ),
        (
            "verified_traces",
            Value::integer(u32::try_from(h.verified_traces).unwrap_or(u32::MAX)),
        ),
    ]);
    let doc = obj(vec![
        ("header", header),
        (
            "decisions",
            Value::array(report.decisions.iter().map(section_value).collect()),
        ),
    ]);
    let mut s = rt_json::write_compact(&doc);
    s.push('\n');
    s.into_bytes()
}
