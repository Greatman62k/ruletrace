//! `ruletrace review list`, `approve` and `reject` (design §3.8.3, §3.9.1; Req
//! 5.8 to 5.12, 17.11, 23.12 to 23.14, 26.16 to 26.19).
//!
//! The review group reads no environment variable, no AI configuration and no
//! clock. The Reviewer and the time come only from arguments, and the planning
//! is the pure [`rt_review::plan_review`]. This module owns the I/O around it:
//!
//! * `approve` and `reject` take the exclusive lock on `<ledger>.lock`
//!   (`RT-APR-0212` when held, `RT-APR-0215` when the file system has no
//!   advisory locks, both exit 70), recover an interrupted commit, read the
//!   Rulebook, the ledger, the Proposal_Store and the Source_Models, and plan.
//!   A refusal is exactly one Diagnostic and no file changes. The lock file is
//!   a control file that holds no data and is not part of the review state.
//! * A review of a revision that exists in the Rulebook changes the ledger
//!   only, so one atomic rename of the complete new file is enough
//!   ([`commit_files`]).
//! * The adoption of a Pending_Proposal changes the ledger and the Rulebook,
//!   which the journaled two-file commit ([`commit_pair`]) replaces together.
//! * `list` takes no lock and writes nothing. Source text comes from the
//!   Explainer ([`rt_explain::verify_trace`]) over a trace that holds the spans
//!   of the revision.

use crate::args::{DecideArgs, ReviewArgs, ReviewCommand};
use crate::commit::{
    commit_pair, locking_unsupported, read_pair, recover, recovery_notice, retained_warning, Finish,
};
use crate::io::{commit_files, input_location, sibling, Fs};
use crate::validate::{collect, exit_code_of, input_id, load_sources, read_input};
use rt_foundation::{
    escape_controls, Code, Diagnostic, DiagnosticState, ExitCode, InputId, Location,
    RevisionNumber, RuleId, MAX_CONTAINER_DEPTH, MAX_LEDGER_BYTES, MAX_PROPOSAL_BATCH_BYTES,
    MAX_RULEBOOK_BYTES,
};
use rt_ir::{rule_content_hash, ApprovalStatus, Origin, RuleKey, RuleRevision, Rulebook};
use rt_json::ParseLimits;
use rt_ledger::{derive_statuses, parse_ledger, unresolved, AiIdentitySet, ApprovalLedger};
use rt_proposals::{parse_batch, pending_proposals, Batch, PendingProposal};
use rt_review::{
    plan_review_with_store, prospective_adoption, Prospective, ReviewAction, ReviewDecision,
};
use rt_srcmodel::SourceModels;
use rt_trace::{
    DecisionTrace, DocumentHash, Outcome, QuestionKind, TraceParts, TraceQuestion, TraceRule,
    TraceSpan,
};
use std::fmt::Write as _;
use std::io::ErrorKind;
use std::path::Path;

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
        ReviewCommand::List {
            rule,
            rule_at,
            revision,
        } => list(args, rule.as_ref(), rule_at.as_deref(), *revision, fs),
        ReviewCommand::Decide(decide_args) => decide(args, decide_args, fs),
    }
}

struct Loaded {
    rulebook: Rulebook,
    ledger: ApprovalLedger,
    sources: SourceModels,
    batches: Vec<Batch>,
}

fn store_invalid(path: &Path, why: String) -> Diagnostic {
    Diagnostic::new(Code::ReviewProposalStoreInvalid, why, input_location(path))
}

/// A batch file above the limit (Req 19.3, 19.8). The size is the size of the
/// file, taken before any of its content is read.
fn oversize_batch(path: &Path, input: &InputId, size: u64) -> Diagnostic {
    Diagnostic::new(
        Code::LimitFileTooLarge,
        format!(
            "input file '{input}' is {size} bytes, larger than the effective limit of \
             {MAX_PROPOSAL_BATCH_BYTES} bytes"
        ),
        input_location(path),
    )
}

/// Reads every `batches/*.json` of a Proposal_Store (design §3.7.5). The store
/// is only read here (D-8).
fn load_batches(dir: &Path, fs: &dyn Fs, problems: &mut DiagnosticState) -> Vec<Batch> {
    let batches_dir = dir.join("batches");
    let shown = escape_controls(&batches_dir.display().to_string());
    let names = match fs.list_dir(&batches_dir) {
        Ok(names) => names,
        Err(e) => {
            problems.push(store_invalid(
                &batches_dir,
                format!("the Proposal_Store directory '{shown}' cannot be read: {e}"),
            ));
            return Vec::new();
        }
    };
    let mut batches = Vec::new();
    for path in names
        .iter()
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
    {
        let step = || -> Result<Batch, Vec<Diagnostic>> {
            let input = input_id(path).map_err(|d| vec![*d])?;
            let shown = || escape_controls(&path.display().to_string());
            let size = fs.len(path).map_err(|e| {
                vec![store_invalid(
                    path,
                    format!("the batch '{}' cannot be read: {e}", shown()),
                )]
            })?;
            if size > MAX_PROPOSAL_BATCH_BYTES {
                return Err(vec![oversize_batch(path, &input, size)]);
            }
            let bytes = fs
                .read(path, MAX_PROPOSAL_BATCH_BYTES.saturating_add(1))
                .map_err(|e| {
                    vec![store_invalid(
                        path,
                        format!("the batch '{}' cannot be read: {e}", shown()),
                    )]
                })?;
            parse_batch(&input, &bytes)
        };
        match step() {
            Ok(batch) => batches.push(batch),
            Err(ds) => {
                // A batch that does not parse makes the whole store invalid, so
                // that no proposal is adopted from a store that is not trusted.
                for d in ds {
                    if d.code() == Code::LimitFileTooLarge {
                        problems.push(d);
                    } else {
                        problems.push(store_invalid(path, d.message().to_owned()));
                    }
                }
            }
        }
    }
    batches
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
    let batches = args
        .proposals
        .as_deref()
        .map(|dir| load_batches(dir, fs, &mut problems))
        .unwrap_or_default();
    match (rulebook, ledger, sources) {
        (Some(rulebook), Some(ledger), Some(sources)) if problems.as_slice().is_empty() => {
            Ok(Loaded {
                rulebook,
                ledger,
                sources,
                batches,
            })
        }
        _ => Err(problems),
    }
}

fn decide(args: &ReviewArgs, d: &DecideArgs, fs: &dyn Fs) -> Done {
    // What recovery did stays visible when the decision itself is refused.
    let mut notices = DiagnosticState::empty();
    let mut done = decide_locked(args, d, fs, &mut notices);
    for notice in notices.as_slice() {
        done.state.push(notice.clone());
    }
    done.state = done.state.sorted();
    done
}

fn decide_locked(
    args: &ReviewArgs,
    d: &DecideArgs,
    fs: &dyn Fs,
    notices: &mut DiagnosticState,
) -> Done {
    // Step 1 of design §3.8.3: the exclusive lock, held until this function
    // returns.
    let lock_path = sibling(&args.ledger, ".lock");
    let _lock = match fs.try_lock(&lock_path) {
        Ok(lock) => lock,
        Err(e) if e.kind() == std::io::ErrorKind::Unsupported => {
            return failed_one(locking_unsupported(
                &lock_path,
                "to change the Rulebook and the ledger",
            ))
        }
        Err(e) => {
            let why = if e.kind() == std::io::ErrorKind::WouldBlock {
                "another review command holds it".to_owned()
            } else {
                e.to_string()
            };
            return failed_one(Diagnostic::new(
                Code::ReviewLockUnavailable,
                format!(
                    "the lock '{}' could not be taken: {why}",
                    escape_controls(&lock_path.display().to_string())
                ),
                input_location(&lock_path),
            ));
        }
    };
    // Step 2: an interrupted commit is finished or undone before anything is
    // read, so every later step sees two consistent files.
    match recover(fs, &args.ledger, &args.rulebook) {
        Ok(how) => {
            if let Some(notice) = recovery_notice(&args.ledger, &how) {
                notices.push(notice);
            }
        }
        Err(d) => return failed_one(*d),
    }
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
            Some((site, reason)) => ReviewAction::Reject {
                reason: reason.clone(),
                site: site.clone(),
            },
        },
        // The adopted revision's Origin already names the AI identities of
        // its batch, so no further set is passed. The slice-ai attack test
        // refuses each of them as a Reviewer.
        batch_identities: AiIdentitySet::new(Vec::<String>::new()),
        sites: (*d.sites).clone(),
    };
    let plan = match plan_review_with_store(
        &loaded.rulebook,
        &loaded.ledger,
        &decision,
        &loaded.sources,
        &loaded.batches,
    ) {
        Ok(plan) => plan,
        Err(rejected) => return failed(rejected.into_diagnostics()),
    };
    let committed = match plan.rulebook_bytes() {
        // An adoption changes both files (Req 26.19).
        Some(rulebook_bytes) => commit_pair(
            fs,
            (args.ledger.as_path(), plan.ledger_bytes()),
            (args.rulebook.as_path(), rulebook_bytes),
        ),
        // One file, one atomic rename of the complete new ledger.
        None => commit_files(fs, &[(args.ledger.as_path(), plan.ledger_bytes())])
            .map(|()| Finish::Clean)
            .map_err(|e| {
                Box::new(Diagnostic::new(
                    Code::ReviewWriteFailed,
                    e.message().to_owned(),
                    e.primary().clone(),
                ))
            }),
    };
    match committed {
        Ok(Finish::Clean) => {}
        Ok(Finish::JournalRetained) => notices.push(retained_warning(&args.ledger)),
        Err(d) => return failed_one(*d),
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
    rule_at: Option<&Location>,
    revision: Option<RevisionNumber>,
    fs: &dyn Fs,
) -> Done {
    // The Rulebook and the ledger are read once as one pair, or not at all.
    // Only `approve` and `reject` recover a journal.
    let pair = match read_pair(fs, &args.ledger, &args.rulebook) {
        Ok(pair) => pair,
        Err(blocked) => return failed_one(*blocked),
    };
    let loaded = match load(args, false, &pair) {
        Ok(loaded) => loaded,
        Err(problems) => return failed(problems),
    };
    let rb = &loaded.rulebook;
    let statuses = derive_statuses(rb, &loaded.ledger);
    // Without `--rule` no argument names the rule, so a refused proposal is
    // placed at the store it came from.
    let adoption_at = rule_at.cloned().unwrap_or_else(|| match &args.proposals {
        Some(dir) => input_location(dir),
        None => Location::None(Code::ReviewProposalStoreInvalid),
    });
    // Pending_Proposals are derived from the store and the Rulebook only
    // (Req 26.28). A selection keeps the proposals of that rule, and a
    // revision selection keeps those whose prospective number matches.
    let pending: Vec<(PendingProposal<'_>, Result<Prospective, Diagnostic>)> =
        pending_proposals(&loaded.batches, rb)
            .into_iter()
            .filter(|p| {
                p.proposal
                    .rule
                    .as_ref()
                    .is_some_and(|(d, _)| rule.is_none_or(|id| &d.id == id))
            })
            .map(|p| (p, prospective_adoption(rb, &p, &adoption_at)))
            .filter(|(_, r)| match (revision, r) {
                (Some(n), Ok(prospective)) => prospective.revision == n,
                (Some(_), Err(_)) => false,
                (None, _) => true,
            })
            .collect();
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
            if found.is_empty() && pending.is_empty() {
                let which = revision.map_or(String::new(), |n| format!(" revision {n}"));
                return failed_one(Diagnostic::new(
                    Code::ReviewTargetNotFound,
                    format!("rule '{id}'{which} does not exist in the Rulebook"),
                    adoption_at,
                ));
            }
            found
        }
    };
    let validation = rt_validate::validate(rb, &loaded.ledger, &loaded.sources, None);
    let mut out = String::new();
    if keys.is_empty() && pending.is_empty() {
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
    for (p, prospective) in &pending {
        describe_pending(&mut out, rb, p, prospective, &loaded.sources);
    }
    Done {
        stdout: out.into_bytes(),
        state: DiagnosticState::empty(),
        code: ExitCode::Success,
    }
}

fn origin_text(origin: &Origin) -> String {
    match origin {
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
    }
}

/// One Pending_Proposal: the revision and hash its adoption would give, the
/// stored findings, and the source text the Explainer verified for the draft.
fn describe_pending(
    out: &mut String,
    rb: &Rulebook,
    pending: &PendingProposal<'_>,
    prospective: &Result<Prospective, Diagnostic>,
    sources: &SourceModels,
) {
    let proposal = pending.proposal;
    let rule = proposal
        .rule
        .as_ref()
        .map_or_else(String::new, |(d, _)| escape_controls(d.id.as_str()));
    match prospective {
        Ok(p) => {
            let _ = writeln!(
                out,
                "pending proposal '{}' for rule '{rule}' prospective revision {}",
                proposal.proposal_id, p.revision
            );
            let _ = writeln!(out, "  origin: {}", origin_text(&p.adopted.origin));
            let _ = writeln!(out, "  prospective content hash: {}", p.content_hash);
        }
        Err(d) => {
            let _ = writeln!(
                out,
                "pending proposal '{}' for rule '{rule}' cannot be adopted",
                proposal.proposal_id
            );
            for line in crate::render::diagnostic(d).lines() {
                let _ = writeln!(out, "  diagnostic: {line}");
            }
        }
    }
    for d in &proposal.outcome {
        let _ = writeln!(
            out,
            "  diagnostic: {} {}: {}",
            crate::render::severity_name(d.code.severity()),
            d.code.id(),
            escape_controls(&d.message)
        );
    }
    if let Ok(p) = prospective {
        for line in span_lines(rb, &p.adopted, sources) {
            let _ = writeln!(out, "  {line}");
        }
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
    let origin = origin_text(&rev.origin);
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
