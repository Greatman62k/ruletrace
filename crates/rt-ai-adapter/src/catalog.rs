//! The model catalogue contract (UD-37, Req 35.18, 35.19).
//!
//! A catalogue lists the model identifiers that a provider reports. It is
//! read for display and for the `verify_model` check only. Nothing in this
//! crate selects a model from it.

use crate::{MonotonicClock, ProviderFailure};
use std::time::Duration;

/// Why one catalogue attempt produced no list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogFailure {
    /// The request failed. The retry decision reads the inner failure.
    Provider(ProviderFailure),
    /// A 2xx answer that is not a catalogue. The reason is a fixed text.
    Malformed(&'static str),
    /// The answer is longer than the response limit.
    TooLarge,
}

/// A provider that can list its models.
pub trait ModelCatalog {
    /// One attempt to list the models, with the per-attempt `timeout` and
    /// the response limit in bytes. Never retries and never sleeps. The
    /// identifiers are returned as reported: not sorted, not deduplicated.
    ///
    /// # Errors
    /// The failure of the attempt.
    fn list_models(
        &mut self,
        timeout: Duration,
        response_limit: u32,
    ) -> Result<Vec<String>, CatalogFailure>;

    /// The clock the caller uses to measure waits and the deadline.
    fn clock(&self) -> &dyn MonotonicClock;
}
