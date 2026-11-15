//! Provider-independent AI adapter interface (class aux, Req 18.12).
//!
//! The crate has no network client, no provider library and no dependency on
//! the compiler, engine, review or ledger crates. The retry decision in
//! [`retry`] is a pure function of attempt outcomes and elapsed time.
#![forbid(unsafe_code)]

mod adapter;
mod request;
pub mod retry;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_retry;

pub use adapter::{
    AiAdapter, AttemptOutcome, AttemptReport, Completion, MonotonicClock, ProviderFailure,
    ProviderMeta, ProviderResponse,
};
pub use request::{ProviderRequest, RequestId, RequestLimits};
pub use retry::{next_step, AttemptRecord, RetryPolicy, RetryPolicyError, RetryStep, RetryStop};
