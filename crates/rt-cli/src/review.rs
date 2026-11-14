//! `ruletrace review list`, `approve` and `reject` (design §3.8.3, §3.9.1; Req
//! 5.8 to 5.12, 17.11, 23.12 to 23.14, 26.16 to 26.19).
//!
//! The review group reads no environment variable, no AI configuration and no
//! clock. The Reviewer and the time come only from arguments, and the planning
//! is the pure [`rt_review::plan_review`]. This module owns the I/O around it:
//!
//! * `approve` and `reject` take the exclusive lock on `<ledger>.lock`
//!   (`RT-APR-0212`, exit 70), read the Rulebook, the ledger and the
//!   Source_Models, plan, and replace the ledger with one atomic rename of the
//!   complete new file ([`commit_files`] with a single file). A refusal is
//!   exactly one Diagnostic and no file changes. The lock file is a control
//!   file that holds no data and is not part of the review state.
//! * A review of a revision that exists in the Rulebook changes the ledger
//!   only, so one rename is atomic.
//! * `list` takes no lock and writes nothing. Source text comes from the
//!   Explainer ([`rt_explain::verify_trace`]) over a trace that holds the spans
//!   of the revision.

use crate::args::{DecideArgs, ReviewArgs, ReviewCommand};
use crate::io::{acquire_lock, commit_files, input_location, Fs};
use crate::validate::{collect, exit_code_of, input_id, load_sources, read_input};
use rt_foundation::{
    escape_controls, Code, Diagnostic, DiagnosticState, ExitCode, Location, RevisionNumber, RuleId,
    MAX_CONTAINER_DEPTH, MAX_LEDGER_BYTES, MAX_RULEBOOK_BYTES,
};
use rt_ir::{rule_content_hash, ApprovalStatus, Origin, RuleKey, RuleRevision, Rulebook};
use rt_json::ParseLimits;
use rt_ledger::{derive_statuses, parse_ledger, unresolved, AiIdentitySet, ApprovalLedger};
use rt_review::{plan_review, ReviewAction, ReviewDecision};
use rt_srcmodel::SourceModels;
use rt_trace::{
    DecisionTrace, DocumentHash, Outcome, QuestionKind, TraceParts, TraceQuestion, TraceRule,
    TraceSpan,
};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// What the command produced.
pub(crate) struct Done {
    pub(crate) stdout: Vec<u8>,
    pub(crate) state: DiagnosticState,
    pub(crate) code: ExitCode,
}

fn failed(state: DiagnosticState) -> Done {
    let state = state.sorted();
    let code = match exit_code_of(&state) {
        ExitCode::Success => ExitCode::Internal,
        code => code,
    };
    Done {
        stdout: Vec::new(),
        state,
        code,
    }
}

fn failed_one(d: Diagnostic) -> Done {
    let mut state = DiagnosticState::empty();
    state.push(d);
    failed(state)
}

/// Runs the command.
pub(crate) fn run(args: &ReviewArgs, fs: &dyn Fs) -> Done {
    match &args.command {
        ReviewCommand::List { rule, revision } => list(args, rule.as_ref(), *revision, fs),
        ReviewCommand::Decide(decide_args) => decide(args, decide_args, fs),
    }
}

struct Loaded {
    rulebook: Rulebook,
    ledger: ApprovalLedger,
    sources: SourceModels,
}

fn load_rulebook(path: &Path, fs: &dyn Fs, problems: &mut DiagnosticState) -> Option<Rulebook> {
    let step = || -> Result<Rulebook, Vec<Diagnostic>> {
        let input = input_id(path).map_err(|d| vec![*d])?;
        let bytes = read_input(fs, path, MAX_RULEBOOK_BYTES).map_err(|d| vec![*d])?;
        let staged = rt_ir::parse_rulebook(&bytes, &input, MAX_RULEBOOK_BYTES, MAX_CONTAINER_DEPTH)
            .map_err(|r| r.into_diagnostics().as_slice().to_vec())?;
        // Parser warnings are not shown by the review group: `validate` reports
        // them.
        Ok(staged.into_parts().0)
    };
    collect(step(), problems)
}

fn load_ledger(
    path: &Path,
    create: bool,
    fs: &dyn Fs,
    problems: &mut DiagnosticState,
) -> Option<ApprovalLedger> {
    let step = || -> Result<ApprovalLedger, Vec<Diagnostic>> {
        let input = input_id(path).map_err(|d| vec![*d])?;
        let bytes = match fs.read(path, MAX_LEDGER_BYTES.saturating_add(1)) {
            Ok(bytes) => bytes,
            // `--create-ledger` starts a new ledger only when the file does not
            // exist.
            Err(e) if e.kind() == ErrorKind::NotFound && create => {
                return Ok(ApprovalLedger::empty())
            }
            Err(_) => read_input(fs, path, MAX_LEDGER_BYTES).map_err(|d| vec![*d])?,
        };
        parse_ledger(
            &input,
            &bytes,
            ParseLimits::new(MAX_LEDGER_BYTES, MAX_CONTAINER_DEPTH),
        )
    };
    collect(step(), problems)
}

fn load(args: &ReviewArgs, create_ledger: bool, fs: &dyn Fs) -> Result<Loaded, DiagnosticState> {
    let mut problems = DiagnosticState::empty();
    let rulebook = load_rulebook(&args.rulebook, fs, &mut problems);
    let ledger = load_ledger(&args.ledger, create_ledger, fs, &mut problems);
    let sources = load_sources(&args.source_models, fs, &mut problems);
    match (rulebook, ledger, sources) {
        (Some(rulebook), Some(ledger), Some(sources)) if problems.as_slice().is_empty() => {
            Ok(Loaded {
                rulebook,
                ledger,
                sources,
            })
        }
        _ => Err(problems),
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn decide(args: &ReviewArgs, d: &DecideArgs, fs: &dyn Fs) -> Done {
    // Step 1 of design §3.8.3: the exclusive lock, held until this function
    // returns.
    let lock_path = sibling(&args.ledger, ".lock");
    let _lock = match acquire_lock(fs, &lock_path) {
        Ok(lock) => lock,
        Err(e) => {
            return failed_one(Diagnostic::new(
                Code::ReviewLockUnavailable,
                format!(
                    "the lock '{}' could not be taken: {e}",
                    escape_controls(&lock_path.display().to_string())
                ),
                input_location(&lock_path),
            ))
        }
    };
    let loaded = match load(args, d.create_ledger, fs) {
        Ok(loaded) => loaded,
        Err(problems) => return failed(problems),
    };
    let decision = ReviewDecision {
        rule: d.rule.clone(),
        revision: d.revision,
        content_hash: d.content_hash.clone(),
        reviewer: d.reviewer.clone(),
        time: d.time.clone(),
        action: match &d.reject_reason {
            None => ReviewAction::Approve,
            Some(reason) => ReviewAction::Reject {
                reason: reason.clone(),
            },
        },
        batch_identities: AiIdentitySet::new(Vec::<String>::new()),
    };
    let plan = match plan_review(&loaded.rulebook, &loaded.ledger, &decision, &loaded.sources) {
        Ok(plan) => plan,
        Err(rejected) => return failed(rejected.into_diagnostics()),
    };
    // One file, one atomic rename of the complete new ledger (Req 26.19).
    if let Err(e) = commit_files(fs, &[(args.ledger.as_path(), plan.ledger_bytes())]) {
        return failed_one(Diagnostic::new(
            Code::ReviewWriteFailed,
            e.message().to_owned(),
            e.primary().clone(),
        ));
    }
    let verb = if d.reject_reason.is_some() {
        "rejected"
    } else {
        "approved"
    };
    Done {
        stdout: format!(
            "{verb} rule '{}' revision {} (ledger record {})\n",
            escape_controls(d.rule.as_str()),
            d.revision,
            plan.record_hash()
        )
        .into_bytes(),
        state: DiagnosticState::empty(),
        code: ExitCode::Success,
    }
}

fn list(
    args: &ReviewArgs,
    rule: Option<&RuleId>,
    revision: Option<RevisionNumber>,
    fs: &dyn Fs,
) -> Done {
    let loaded = match load(args, false, fs) {
        Ok(loaded) => loaded,
        Err(problems) => return failed(problems),
    };
    let rb = &loaded.rulebook;
    let statuses = derive_statuses(rb, &loaded.ledger);
    let keys: Vec<RuleKey> = match rule {
        // Without a selection: the revisions awaiting a decision.
        None => unresolved(rb, &statuses),
        // With a selection: the named revisions, whatever their status.
        Some(id) => {
            let mut found: Vec<RuleKey> = rb
                .revisions
                .iter()
                .map(|r| r.key.clone())
                .filter(|k| &k.id == id && revision.is_none_or(|n| n == k.revision))
                .collect();
            found.sort();
            if found.is_empty() {
                let which = revision.map_or(String::new(), |n| format!(" revision {n}"));
                return failed_one(Diagnostic::new(
                    Code::ReviewTargetNotFound,
                    format!("rule '{id}'{which} does not exist in the Rulebook"),
                    Location::Cli(rt_foundation::CliLocation::Argument {
                        position: 0,
                        name: Some("--rule".to_owned()),
                    }),
                ));
            }
            found
        }
    };
    let validation = rt_validate::validate(rb, &loaded.ledger, &loaded.sources, None);
    let mut out = String::new();
    if keys.is_empty() {
        out.push_str("no revision awaits a decision\n");
    }
    for key in &keys {
        if let Some(rev) = rb.revision(key) {
            let status = statuses
                .get(key)
                .copied()
                .unwrap_or(ApprovalStatus::Proposed);
            describe(&mut out, rb, rev, status, &validation, &loaded.sources);
        }
    }
    Done {
        stdout: out.into_bytes(),
        state: DiagnosticState::empty(),
        code: ExitCode::Success,
    }
}

fn describe(
    out: &mut String,
    rb: &Rulebook,
    rev: &RuleRevision,
    status: ApprovalStatus,
    validation: &rt_validate::ValidationResult,
    sources: &SourceModels,
) {
    let _ = writeln!(
        out,
        "rule '{}' revision {} status {}",
        escape_controls(rev.key.id.as_str()),
        rev.key.revision,
        status.name()
    );
    let origin = match &rev.origin {
        Origin::Manual { author: None } => "manual".to_owned(),
        Origin::Manual { author: Some(a) } => {
            format!("manual, author '{}'", escape_controls(a.as_str()))
        }
        Origin::AiProposal(o) => format!(
            "AI proposal '{}' (proposer '{}', provider '{}', model '{}')",
            escape_controls(&o.proposal_id),
            escape_controls(o.proposer.as_str()),
            escape_controls(&o.provider),
            escape_controls(&o.model)
        ),
    };
    let _ = writeln!(out, "  origin: {origin}");
    match rule_content_hash(&rb.ir_version, rev, &rb.documents) {
        Ok(hash) => {
            let _ = writeln!(out, "  content hash: {hash}");
        }
        Err(_) => {
            let _ = writeln!(
                out,
                "  content hash: none (a span names an undeclared document or one with two hashes)"
            );
        }
    }
    for d in validation.state().as_slice().iter().filter(|d| {
        d.rules()
            .iter()
            .any(|r| r.rule == rev.key.id && r.revision.is_none_or(|n| n == rev.key.revision))
    }) {
        for line in crate::render::diagnostic(d).lines() {
            let _ = writeln!(out, "  diagnostic: {line}");
        }
    }
    for line in span_lines(rb, rev, sources) {
        let _ = writeln!(out, "  {line}");
    }
}

/// One line group per span of the revision. The text is what the Explainer
/// resolved and verified against the Source_Model, or the code of the failure.
/// No text is invented.
fn span_lines(rb: &Rulebook, rev: &RuleRevision, sources: &SourceModels) -> Vec<String> {
    let trace_spans: Vec<Option<TraceSpan>> = rev
        .content
        .spans
        .iter()
        .map(|s| {
            let hash = rb.documents.iter().find(|d| d.id == s.document)?;
            Some(TraceSpan {
                document: s.document.clone(),
                sha256: DocumentHash::new(hash.sha256.as_str())?,
                start: s.start,
                end: s.end,
                line: s.line,
                column: s.column,
            })
        })
        .collect();
    let present: Vec<TraceSpan> = trace_spans.iter().flatten().cloned().collect();
    let verdicts = DecisionTrace::new(TraceParts {
        question: TraceQuestion {
            kind: QuestionKind::Value,
            target: "review".to_owned(),
            participant: None,
        },
        outcome: Outcome::indeterminate(Vec::new()),
        rules: vec![TraceRule {
            rule: rev.key.id.clone(),
            revision: rev.key.revision,
            spans: present,
        }],
        entries: Vec::new(),
        not_applied: Vec::new(),
        missing: Vec::new(),
        diagnostics: DiagnosticState::empty(),
    })
    .map(|trace| rt_explain::verify_trace(&trace, sources))
    .ok();
    let mut resolved = verdicts.iter().flat_map(|v| v.spans().iter());
    rev.content
        .spans
        .iter()
        .zip(trace_spans)
        .map(|(span, trace_span)| {
            let place = format!(
                "span {} bytes {}..{} (line {}, column {})",
                escape_controls(span.document.as_str()),
                span.start,
                span.end,
                span.line,
                span.column
            );
            let result = match trace_span.and_then(|_| resolved.next()) {
                Some(verdict) => match verdict.text() {
                    Some(text) => format!("text \"{}\"", escape_controls(text)),
                    None => match &verdict.resolution {
                        rt_explain::SpanResolution::Unresolved(code) => {
                            format!("not verified {}", code.id())
                        }
                        rt_explain::SpanResolution::Resolved(_) => "not verified".to_owned(),
                    },
                },
                None => "not verified: the document is not declared in the Rulebook".to_owned(),
            };
            format!("{place}: {result}")
        })
        .collect()
}
