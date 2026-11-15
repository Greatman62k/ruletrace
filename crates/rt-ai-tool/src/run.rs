//! The call loop (design 3.7.6, 3.7.7; Req 23.35 to 23.37, 29.5, 29.6,
//! 29.8, 29.10, 29.12, 29.15, 29.17).
//!
//! Requests run one after another in plan order. Each request is checked
//! against the store, sent through the retry decision [`next_step`],
//! accepted and stored. The first failure ends the run: no later request
//! is sent, earlier batches stay, and the report says the run is
//! incomplete.

use crate::accept::{accept_batch, AcceptInput};
use crate::config::AiConfig;
use crate::fs::StoreFs;
use crate::plan::{ClauseId, PlannedRequest, RunPlan};
use crate::redact::Redactor;
use crate::store::{
    already_stored_warning, batch_exists, store_batch, FailureOutcome, StoreMeta, StoreOutcome,
};
use crate::summary::RunSummary;
use crate::txlog::{LogSink, TransmissionLog};
use rt_ai_adapter::{
    next_step, AiAdapter, AttemptOutcome, AttemptRecord, ProviderFailure, ProviderResponse,
    RetryStep, RetryStop,
};
use rt_foundation::{
    CliLocation, Code, Diagnostic, ExitCode, InputId, InputLocation, Location, RuleId, Severity,
};
use rt_hash::Sha256Hex;
use rt_ir::Rulebook;
use rt_srcmodel::SourceModels;
use std::path::Path;
use std::time::Duration;

/// Everything the loop reads. All of it is explicit input.
pub struct RunInput<'a> {
    /// The validated configuration.
    pub config: &'a AiConfig,
    /// The requests of the run.
    pub plan: &'a RunPlan,
    /// Source_Models for provenance verification.
    pub models: &'a SourceModels,
    /// The Rulebook whose declarations the proposals are checked in.
    pub declarations: &'a Rulebook,
    /// Matches the credential; [`Redactor::no_credential`] for none.
    pub redactor: &'a Redactor,
    /// The Proposal_Store directory.
    pub store_root: &'a Path,
    /// What every stored batch records.
    pub meta: &'a StoreMeta,
    /// Caller-supplied time written to each Transmission_Log entry.
    pub log_time: &'a str,
    /// The Rule identifier of a revision run, `None` for any other run.
    pub revise_rule: Option<&'a RuleId>,
}

/// The result of a run.
#[derive(Clone, Debug)]
pub struct RunReport {
    /// The clause lists, to be written to standard output.
    pub summary: RunSummary,
    /// Every Diagnostic of the run in the order it arose.
    pub diagnostics: Vec<Diagnostic>,
    /// The exit code: that of the failing request, or success.
    pub exit: ExitCode,
}

struct Done {
    diagnostics: Vec<Diagnostic>,
    stored: bool,
}

struct Failed {
    diagnostics: Vec<Diagnostic>,
    outcome: FailureOutcome,
}

fn input_at(id: &str) -> Location {
    match InputId::new(id) {
        Ok(input) => Location::Input(InputLocation {
            input,
            line: None,
            column: None,
            field: None,
        }),
        Err(_) => Location::None(Code::InternalError),
    }
}

fn attempt_noun(attempts: usize) -> &'static str {
    if attempts == 1 {
        "attempt"
    } else {
        "attempts"
    }
}

struct Loop<'a, 'b> {
    input: &'a RunInput<'b>,
}

impl Loop<'_, '_> {
    // Provider text is untrusted, so runs of credential characters are
    // removed before it reaches a message.
    fn safe(&self, text: &str) -> String {
        self.input.redactor.redact_fragments(text)
    }

    fn diagnostic(&self, code: Code, message: String, request: &PlannedRequest) -> Diagnostic {
        let at = input_at(&format!("request-{}", request.id().as_str()));
        Diagnostic::new(code, self.safe(&message), at)
    }

    // Req 23.17: a provider failure is located at the `provider` field of
    // the configuration, not at the request.
    fn provider_failure(&self, code: Code, message: String) -> Diagnostic {
        let at = self.input.config.provider_at.clone();
        Diagnostic::new(code, self.safe(&message), at)
    }

    fn attempt_warning(
        &self,
        failure: ProviderFailure,
        attempt: usize,
        detail: Option<&str>,
    ) -> Diagnostic {
        let code = transient_code(failure);
        let mut message = format!("attempt {attempt} failed: {}", code.mnemonic());
        if let Some(detail) = detail {
            message.push_str(&format!(" ({detail})"));
        }
        self.provider_failure(code, message)
    }

    // One request's attempts. Per-attempt Warnings are pushed to `notes` as
    // they arise, so they are present for every ending.
    fn call(
        &self,
        request: &PlannedRequest,
        adapter: &mut dyn AiAdapter,
        log: &mut TransmissionLog<impl LogSink>,
        notes: &mut Vec<Diagnostic>,
    ) -> Result<ProviderResponse, Diagnostic> {
        let policy = &self.input.config.policy;
        let started = adapter.clock().now();
        let mut history: Vec<AttemptRecord> = Vec::new();
        let mut response: Option<ProviderResponse> = None;
        let mut last_warning: Option<Code> = None;
        let mut last_detail: Option<String> = None;
        loop {
            let elapsed = adapter.clock().now().saturating_sub(started);
            match next_step(policy, &history, elapsed) {
                RetryStep::Attempt { wait_before } => {
                    if wait_before > Duration::ZERO {
                        adapter.clock().wait(wait_before);
                    }
                    let attempt = history.len() + 1;
                    let number = u32::try_from(attempt).unwrap_or(u32::MAX);
                    let report = log.attempt_logged(
                        adapter,
                        request.log_context(),
                        request.request(),
                        policy.attempt_timeout(),
                        number,
                        self.input.log_time,
                    )?;
                    match report.outcome {
                        AttemptOutcome::Response(r) => {
                            response = Some(r);
                            history.push(AttemptRecord::Response);
                        }
                        AttemptOutcome::Failure(f) => {
                            if f.is_transient() {
                                let w = self.attempt_warning(f, attempt, report.detail.as_deref());
                                last_warning = Some(w.code());
                                notes.push(w);
                            }
                            last_detail = report.detail;
                            history.push(AttemptRecord::Failure(f));
                        }
                    }
                }
                RetryStep::Stop(RetryStop::Success) => {
                    return response.take().ok_or_else(|| {
                        self.diagnostic(
                            Code::InternalError,
                            "the retry decision reported a response that was not kept".to_owned(),
                            request,
                        )
                    });
                }
                RetryStep::Stop(RetryStop::Exhausted { attempts }) => {
                    return Err(self.provider_failure(
                        Code::AiProviderRetryExhausted,
                        format!(
                            "{attempts} {} failed; the last was {}",
                            attempt_noun(usize::from(attempts)),
                            last_name(last_warning)
                        ),
                    ));
                }
                RetryStep::Stop(RetryStop::DeadlineExceeded { attempts }) => {
                    return Err(self.provider_failure(
                        Code::AiProviderDeadlineExceeded,
                        format!(
                            "the deadline cannot fit another attempt after {attempts}; \
                             the last failure was {}",
                            last_name(last_warning)
                        ),
                    ));
                }
                RetryStep::Stop(RetryStop::NonTransient(f)) => {
                    let code = final_code(f);
                    let attempts = history.len();
                    let mut message = format!(
                        "{attempts} {} failed; the last was {}",
                        attempt_noun(attempts),
                        code.mnemonic()
                    );
                    if let Some(detail) = &last_detail {
                        message.push_str(&format!(" ({detail})"));
                    }
                    return Err(self.provider_failure(code, message));
                }
            }
        }
    }

    fn process(
        &self,
        request: &PlannedRequest,
        adapter: &mut dyn AiAdapter,
        log: &mut TransmissionLog<impl LogSink>,
        fs: &dyn StoreFs,
    ) -> Result<Done, Failed> {
        let root = self.input.store_root;
        let mut notes: Vec<Diagnostic> = Vec::new();
        let id = request.id().as_str();
        let (Some(request_id), Ok(response_id)) =
            (Sha256Hex::parse(id), InputId::new(format!("response-{id}")))
        else {
            return Err(Failed {
                diagnostics: vec![self.diagnostic(
                    Code::InternalError,
                    "the request or response identifier could not be built".to_owned(),
                    request,
                )],
                outcome: FailureOutcome::Refused,
            });
        };
        match batch_exists(fs, root, &request_id) {
            Err(d) => {
                return Err(Failed {
                    diagnostics: vec![d],
                    outcome: FailureOutcome::Refused,
                })
            }
            Ok(true) => {
                return Ok(Done {
                    diagnostics: vec![already_stored_warning(root, &request_id)],
                    stored: false,
                });
            }
            Ok(false) => {}
        }
        let response = match self.call(request, adapter, log, &mut notes) {
            Ok(r) => r,
            Err(d) => {
                notes.push(d);
                return Err(Failed {
                    diagnostics: notes,
                    outcome: FailureOutcome::Refused,
                });
            }
        };
        let accepted = accept_batch(&AcceptInput {
            response_id: &response_id,
            request_id: &request_id,
            completion: response.completion,
            payload: &response.payload,
            response_limit: u64::from(self.input.config.limits.response_bytes),
            models: self.input.models,
            clauses: request.clauses(),
            declarations: self.input.declarations,
            redactor: self.input.redactor,
            revise_rule: self.input.revise_rule,
        });
        let accepted = match accepted {
            Ok(a) => a,
            Err(found) => {
                notes.extend(found);
                return Err(Failed {
                    diagnostics: notes,
                    outcome: FailureOutcome::Refused,
                });
            }
        };
        match store_batch(fs, root, &accepted, self.input.meta) {
            Ok(report) => {
                notes.extend(report.warnings);
                match report.outcome {
                    StoreOutcome::Stored => {
                        notes.extend(accepted.blocking_warning());
                        Ok(Done {
                            diagnostics: notes,
                            stored: true,
                        })
                    }
                    StoreOutcome::AlreadyStored(warning) => {
                        notes.push(warning);
                        Ok(Done {
                            diagnostics: notes,
                            stored: false,
                        })
                    }
                }
            }
            Err(failure) => {
                notes.extend(failure.warnings);
                // A kept batch is in the store, so a reviewer will meet its
                // blocking findings and the Warning says so.
                if failure.outcome == FailureOutcome::KeptUnconfirmed {
                    notes.extend(accepted.blocking_warning());
                }
                notes.push(failure.error);
                Err(Failed {
                    diagnostics: notes,
                    outcome: failure.outcome,
                })
            }
        }
    }
}

fn transient_code(failure: ProviderFailure) -> Code {
    match failure {
        ProviderFailure::Timeout => Code::AiProviderTimeout,
        ProviderFailure::RateLimited { .. } => Code::AiProviderRateLimited,
        _ => Code::AiProviderUnavailable,
    }
}

fn final_code(failure: ProviderFailure) -> Code {
    match failure {
        ProviderFailure::AuthFailed => Code::AiProviderAuthFailed,
        ProviderFailure::TlsFailed => Code::AiProviderTlsFailed,
        _ => Code::AiProviderError,
    }
}

fn last_name(code: Option<Code>) -> &'static str {
    code.map_or("none", Code::mnemonic)
}

/// Runs every request of the plan in order and stops at the first failure.
///
/// A request whose batch is already stored is skipped with `RT-AI-0301` and
/// no provider call. After a failure no later request is sent, the batches
/// of earlier requests stay, and `RT-AI-0306` is added when this run
/// stored one of them. The report never calls an incomplete run complete.
///
/// When the directory sync of a request fails after the link, the whole
/// batch file stays in the store. The run ends with `RT-AI-0303` and the
/// Run_Summary lists those clauses as stored with durability not
/// confirmed, not as unprocessed. `RT-AI-0306` still depends only on
/// batches of earlier requests.
///
/// A Transmission_Log entry is appended before each attempt. When the
/// append fails, `RT-AI-0010` ends the run and that attempt is not made.
pub fn run_requests<S: LogSink>(
    input: &RunInput<'_>,
    adapter: &mut dyn AiAdapter,
    log: &mut TransmissionLog<S>,
    fs: &dyn StoreFs,
) -> RunReport {
    let runner = Loop { input };
    let requested: Vec<ClauseId> = input
        .plan
        .requests()
        .iter()
        .flat_map(|r| r.clauses().iter().cloned())
        .collect();
    let mut processed: Vec<ClauseId> = Vec::new();
    let mut stored: Vec<ClauseId> = Vec::new();
    let mut unconfirmed: Vec<ClauseId> = Vec::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut exit = ExitCode::Success;
    for request in input.plan.requests() {
        match runner.process(request, adapter, log, fs) {
            Ok(done) => {
                diagnostics.extend(done.diagnostics);
                processed.extend(request.clauses().iter().cloned());
                if done.stored {
                    stored.extend(request.clauses().iter().cloned());
                }
            }
            Err(failed) => {
                if failed.outcome == FailureOutcome::KeptUnconfirmed {
                    unconfirmed.extend(request.clauses().iter().cloned());
                }
                exit = failed
                    .diagnostics
                    .iter()
                    .find(|d| d.severity() != Severity::Warning)
                    .map_or(ExitCode::Internal, |d| d.code().exit_code());
                diagnostics.extend(failed.diagnostics);
                break;
            }
        }
    }
    if exit != ExitCode::Success && !stored.is_empty() {
        diagnostics.push(Diagnostic::new(
            Code::AiRunIncomplete,
            "an earlier batch was kept and a later request failed; \
             the run is incomplete"
                .to_owned(),
            Location::Cli(CliLocation::Command("propose".to_owned())),
        ));
    }
    RunReport {
        summary: RunSummary::with_unconfirmed(&requested, &processed, &stored, &unconfirmed),
        diagnostics,
        exit,
    }
}
