//! The adapter trait and the types that cross it. No provider-specific type.

use crate::{Capabilities, Identity, ProviderRequest, Usage};
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

/// What an adapter declares about itself, apart from how it attempts a
/// request. Pure and network-free.
pub trait AdapterInfo {
    /// Who this adapter is.
    fn identity(&self) -> Identity;
    /// What it supports. An adapter that establishes nothing returns
    /// [`Capabilities::UNKNOWN`].
    fn capabilities(&self) -> Capabilities;
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

/// Metadata of a response. Built only through [`ProviderMeta::new`] and the
/// `with_` methods, so that a later field does not break an implementation.
///
/// `model` is the configured model that the request carried. The reported
/// model, response identifier and usage are untrusted audit data: no
/// decision, identifier or approval reads them (Req 35.15).
///
/// ```compile_fail,E0639
/// use rt_ai_adapter::{Identity, Protocol, ProviderId, ProviderMeta};
/// let identity = Identity::new(ProviderId::new("mock").unwrap(), Protocol::MockReplay, "1");
/// let _ = ProviderMeta {
///     identity,
///     model: "m".to_owned(),
///     reported_model: None,
///     response_id: None,
///     usage: None,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProviderMeta {
    /// Who answered.
    pub identity: Identity,
    /// The configured model identifier, as sent.
    pub model: String,
    /// The model name that the provider reported, if it did.
    pub reported_model: Option<String>,
    /// The response identifier that the provider reported, if it did.
    pub response_id: Option<String>,
    /// The token counts that the provider reported, if it did.
    pub usage: Option<Usage>,
}

impl ProviderMeta {
    /// Metadata with nothing reported.
    #[must_use]
    pub fn new(identity: Identity, model: &str) -> Self {
        Self {
            identity,
            model: model.to_owned(),
            reported_model: None,
            response_id: None,
            usage: None,
        }
    }

    /// Sets the model name that the provider reported.
    #[must_use]
    pub fn with_reported_model(mut self, reported: &str) -> Self {
        self.reported_model = Some(reported.to_owned());
        self
    }

    /// Sets the response identifier that the provider reported.
    #[must_use]
    pub fn with_response_id(mut self, id: &str) -> Self {
        self.response_id = Some(id.to_owned());
        self
    }

    /// Sets the token counts that the provider reported.
    #[must_use]
    pub fn with_usage(mut self, usage: Usage) -> Self {
        self.usage = Some(usage);
        self
    }
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
