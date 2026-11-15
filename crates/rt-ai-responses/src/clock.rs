//! The real monotonic clock. Only this crate reads a timer.

use rt_ai_adapter::MonotonicClock;
use std::time::{Duration, Instant};

/// Monotonic time from `std::time::Instant`, measured from construction.
#[derive(Clone, Copy, Debug)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    /// A clock whose origin is now.
    #[must_use]
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MonotonicClock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }

    fn wait(&self, d: Duration) {
        std::thread::sleep(d);
    }
}
