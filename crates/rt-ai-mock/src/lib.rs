//! Deterministic mock provider (class aux, Req 23.25, 33.8).
//!
//! [`MockProvider`] plays a step script and [`ReplayProvider`] replays a
//! recorded script keyed by Request_Identifier. Neither uses a network, a
//! clock, a random source or the environment. Time is a [`SimulatedClock`]
//! that moves only when a step runs or the caller waits.
#![forbid(unsafe_code)]

mod clock;
mod mock;
mod replay;
mod retry_after;
mod script;
mod support;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_info;
#[cfg(test)]
mod tests_replay;
#[cfg(test)]
mod tests_retry_after;

pub use clock::SimulatedClock;
pub use mock::MockProvider;
pub use replay::{Origin, RecordedEntry, RecordedScript, ReplayProvider};
pub use retry_after::{failure_from_http, parse_retry_after};
pub use script::{MockScript, ScriptResult, Step};
pub use support::{ScriptError, ScriptInvalid};

#[cfg(test)]
#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;
