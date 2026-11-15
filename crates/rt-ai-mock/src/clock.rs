//! The simulated monotonic clock.

use rt_ai_adapter::MonotonicClock;
use std::cell::Cell;
use std::time::Duration;

/// A clock that starts at zero and advances only through [`wait`] or an
/// attempt of the mock that reports elapsed time.
///
/// [`wait`]: MonotonicClock::wait
#[derive(Debug, Default)]
pub struct SimulatedClock {
    now: Cell<Duration>,
}

impl SimulatedClock {
    /// A clock at time zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // Saturates so an extreme script cannot wrap time backwards.
    pub(crate) fn advance(&self, d: Duration) {
        self.now.set(self.now.get().saturating_add(d));
    }
}

impl MonotonicClock for SimulatedClock {
    fn now(&self) -> Duration {
        self.now.get()
    }

    fn wait(&self, d: Duration) {
        self.advance(d);
    }
}
