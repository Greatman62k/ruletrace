//! The adapter trait and the types that cross it. No provider-specific type.

use crate::ProviderRequest;
use std::time::Duration;

/// A source of monotonic time. Only an integration crate reads a real timer;
/// the mock provider supplies a simulated one.
pub trait MonotonicClock {
    /// Time since an arbitrary fixed origin; never decreases.
    fn now(&self) -> Duration;
    /// Waits for `d`. A simulated clock advances by `d` without sleeping.
    fn wait(&self, d: Duration);
}

/// One provider. Implementations never retry and never sleep inside
/// [`AiAdapter::attempt`]; the caller drives [`crate::next_step`].
pub trait AiAdapter {
    /// One attempt with the given per-attempt timeout. Returns untrusted
    /// bytes or a typed failure.
    fn attempt(&mut self, request: &ProviderRequest, timeout: Duration) -> AttemptReport;
    /// The clock the caller uses to measure waits and the deadline.
    fn clock(&self) -> &dyn MonotonicClock;
}

/// The result of one attempt and the monotonic time it took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptReport {
    /// What the attempt produced.
    pub outcome: AttemptOutcome,
    /// Monotonic time spent in the attempt.
    pub elapsed: Duration,
    /// Explanation of a failure that the typed failure cannot carry, for
    /// example the Request_Identifier of a replay miss. `None` otherwise.
    pub detail: Option<String>,
}

/// Either a complete 2xx response or a typed failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// A complete response received within the limits.
    Response(ProviderResponse),
    /// The attempt failed.
    Failure(ProviderFailure),
}

/// Untrusted provider output with provider metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderResponse {
    /// Whether the provider finished normally.
    pub completion: Completion,
    /// Raw response bytes. Never trusted.
    pub payload: Vec<u8>,
    /// Provider and model identification for auditing.
    pub meta: ProviderMeta,
}

/// Provider and model names reported for a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMeta {
    /// Provider kind, for example `mock`.
    pub provider: String,
    /// Model identifier.
    pub model: String,
}

/// How the provider ended a complete response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completion {
    /// Finished normally.
    Complete,
    /// The provider refused.
    Refused,
    /// The output was cut short.
    Incomplete,
}

/// A failed attempt. Transient failures are timeout, unavailability, HTTP
/// 429 and HTTP 5xx (Req 29.2); everything else is final.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderFailure {
    /// Connection refused or reset, or another outage.
    Unavailable,
    /// The per-attempt timeout ended the attempt.
    Timeout,
    /// HTTP 429. `retry_after_secs` is present only for a decimal-integer
    /// header of 1 to 9 digits; the adapter ignores any other form.
    RateLimited {
        /// Whole seconds from `Retry-After`.
        retry_after_secs: Option<u32>,
    },
    /// Authentication failed.
    AuthFailed,
    /// TLS negotiation or verification failed.
    TlsFailed,
    /// Any other failure, with the HTTP status when there was one.
    Other {
        /// HTTP status, if a response was received.
        http_status: Option<u16>,
    },
}

impl ProviderFailure {
    /// Whether Req 29.2 permits a retry after this failure. A 5xx status in
    /// `Other` counts as transient so an adapter cannot mask it.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Unavailable | Self::Timeout | Self::RateLimited { .. } => true,
            Self::Other { http_status } => matches!(http_status, Some(500..=599)),
            Self::AuthFailed | Self::TlsFailed => false,
        }
    }
}
