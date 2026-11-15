//! The pure retry and deadline decision (Req 29.1 to 29.6, 29.13, 29.15).
//!
//! `next_step` reads no clock and never sleeps. The caller measures
//! `elapsed` with a monotonic timer, waits, then attempts.

use crate::ProviderFailure;
use std::time::Duration;

const SCHEDULE: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(8)];

/// Deadline, attempt timeout and retry count, validated to the ranges of
/// Req 29.1. The waits before retries are fixed at 2 s and 8 s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    deadline: Duration,
    attempt_timeout: Duration,
    retries: u8,
}

/// A policy value outside the Req 29.1 ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryPolicyError {
    /// Deadline not in 1..=300 seconds.
    Deadline,
    /// Attempt timeout not in 1..=120 seconds or above the deadline.
    AttemptTimeout,
    /// Retries above 2.
    Retries,
}

impl RetryPolicy {
    /// Builds a policy from whole seconds and a retry count.
    ///
    /// # Errors
    /// Returns the first value outside its range.
    pub fn new(deadline_s: u32, attempt_s: u32, retries: u8) -> Result<Self, RetryPolicyError> {
        if !(1..=300).contains(&deadline_s) {
            return Err(RetryPolicyError::Deadline);
        }
        if !(1..=120).contains(&attempt_s) || attempt_s > deadline_s {
            return Err(RetryPolicyError::AttemptTimeout);
        }
        if usize::from(retries) > SCHEDULE.len() {
            return Err(RetryPolicyError::Retries);
        }
        Ok(Self {
            deadline: Duration::from_secs(u64::from(deadline_s)),
            attempt_timeout: Duration::from_secs(u64::from(attempt_s)),
            retries,
        })
    }

    /// Total deadline.
    #[must_use]
    pub fn deadline(&self) -> Duration {
        self.deadline
    }

    /// Per-attempt timeout.
    #[must_use]
    pub fn attempt_timeout(&self) -> Duration {
        self.attempt_timeout
    }

    /// Maximum number of retries.
    #[must_use]
    pub fn retries(&self) -> u8 {
        self.retries
    }
}

impl Default for RetryPolicy {
    /// 60 s deadline, 20 s attempt timeout, 2 retries.
    fn default() -> Self {
        Self {
            deadline: Duration::from_secs(60),
            attempt_timeout: Duration::from_secs(20),
            retries: 2,
        }
    }
}

/// The outcome of one finished attempt, as the decision sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptRecord {
    /// A complete response was received. It is never retried (Req 29.12).
    Response,
    /// The attempt failed.
    Failure(ProviderFailure),
}

/// What the caller does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryStep {
    /// Wait `wait_before`, then start the next attempt.
    Attempt {
        /// Waiting time before the attempt; zero for the first one.
        wait_before: Duration,
    },
    /// Stop.
    Stop(RetryStop),
}

/// Why the sequence ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryStop {
    /// The last attempt received a complete response.
    Success,
    /// Every permitted attempt ended in a transient failure.
    Exhausted {
        /// Number of attempts made.
        attempts: u8,
    },
    /// A retry was permitted by count but does not fit the deadline.
    DeadlineExceeded {
        /// Number of attempts made.
        attempts: u8,
    },
    /// A non-transient failure ended the last attempt.
    NonTransient(ProviderFailure),
}

/// Decides the next step from the finished attempts and the monotonic time
/// elapsed since the first attempt started.
///
/// Only the last record decides; earlier ones are transient failures by
/// construction of the caller's loop. An empty history starts attempt one.
#[must_use]
pub fn next_step(policy: &RetryPolicy, history: &[AttemptRecord], elapsed: Duration) -> RetryStep {
    let Some(last) = history.last() else {
        return RetryStep::Attempt {
            wait_before: Duration::ZERO,
        };
    };
    let failure = match last {
        AttemptRecord::Response => return RetryStep::Stop(RetryStop::Success),
        AttemptRecord::Failure(f) => f,
    };
    if !failure.is_transient() {
        return RetryStep::Stop(RetryStop::NonTransient(*failure));
    }
    let attempts = u8::try_from(history.len()).unwrap_or(u8::MAX);
    let retries_used = history.len() - 1;
    if retries_used >= usize::from(policy.retries) {
        return RetryStep::Stop(RetryStop::Exhausted { attempts });
    }
    let Some(scheduled) = SCHEDULE.get(retries_used) else {
        return RetryStep::Stop(RetryStop::Exhausted { attempts });
    };
    let mut wait = *scheduled;
    if let ProviderFailure::RateLimited {
        retry_after_secs: Some(secs),
    } = failure
    {
        wait = wait.max(Duration::from_secs(u64::from(*secs)));
    }
    let remaining = policy.deadline.saturating_sub(elapsed);
    match wait.checked_add(policy.attempt_timeout) {
        Some(needed) if remaining >= needed => RetryStep::Attempt { wait_before: wait },
        _ => RetryStep::Stop(RetryStop::DeadlineExceeded { attempts }),
    }
}
