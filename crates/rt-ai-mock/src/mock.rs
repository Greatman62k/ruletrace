//! The step-script provider.

use crate::clock::SimulatedClock;
use crate::script::{MockScript, ScriptResult};
use rt_ai_adapter::{
    AiAdapter, AttemptOutcome, AttemptReport, MonotonicClock, ProviderFailure, ProviderMeta,
    ProviderRequest, ProviderResponse,
};
use std::time::Duration;

/// Plays a [`MockScript`], one step per attempt.
///
/// Contract: each attempt consumes the next step and advances the
/// [`SimulatedClock`] by the step's time. A step that takes longer than the
/// attempt timeout is reported as `Timeout` after exactly the timeout, so a
/// late response is never delivered. Identical script and requests give
/// identical reports.
#[derive(Debug)]
pub struct MockProvider {
    script: MockScript,
    next: usize,
    clock: SimulatedClock,
}

impl MockProvider {
    /// A provider at its first step with the clock at zero.
    #[must_use]
    pub fn new(script: MockScript) -> Self {
        Self {
            script,
            next: 0,
            clock: SimulatedClock::new(),
        }
    }

    /// The simulated clock, for assertions on elapsed time.
    #[must_use]
    pub fn simulated_clock(&self) -> &SimulatedClock {
        &self.clock
    }

    /// Number of attempts that consumed a step.
    #[must_use]
    pub fn steps_consumed(&self) -> usize {
        self.next
    }

    fn fault(detail: String) -> AttemptReport {
        AttemptReport {
            outcome: AttemptOutcome::Failure(ProviderFailure::Other { http_status: None }),
            elapsed: Duration::ZERO,
            detail: Some(detail),
        }
    }
}

impl AiAdapter for MockProvider {
    fn attempt(&mut self, request: &ProviderRequest, timeout: Duration) -> AttemptReport {
        let index = self.next;
        let Some(step) = self.script.steps.get(index) else {
            let steps = self.script.steps.len();
            return Self::fault(format!("script exhausted: it has {steps} steps"));
        };
        let step = step.clone();
        self.next += 1;
        let outcome = match step.result {
            ScriptResult::PayloadFile { path, .. } => {
                return Self::fault(format!("step {index} names unresolved payload file {path}"));
            }
            ScriptResult::Failure(failure) => AttemptOutcome::Failure(failure),
            ScriptResult::Response {
                completion,
                payload,
            } => AttemptOutcome::Response(ProviderResponse {
                completion,
                payload,
                meta: ProviderMeta {
                    provider: "mock".to_owned(),
                    model: request.model.clone(),
                },
            }),
        };
        let report = if step.elapsed > timeout {
            AttemptReport {
                outcome: AttemptOutcome::Failure(ProviderFailure::Timeout),
                elapsed: timeout,
                detail: None,
            }
        } else {
            AttemptReport {
                outcome,
                elapsed: step.elapsed,
                detail: None,
            }
        };
        self.clock.advance(report.elapsed);
        report
    }

    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }
}
