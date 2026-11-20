//! The Prize workflow module: input mapping, Question construction and
//! report presentation over the shared Rules_Engine (design §8, §11.3,
//! Req 31).
//!
//! The crate is a library. It has no binary target and performs no file,
//! network, clock, random, environment or subprocess access. It contains no
//! arithmetic, ordering, ranking or literal parameter: every eligibility
//! condition, weight, threshold, rounding mode and tie-breaking criterion is
//! in the Compiled_Rulebook, and every report value is copied from an
//! Outcome, a Decision_Trace or the Explainer's verification.
//!
//! The interface has three groups.
//!
//! - **Prepare** ([`prepare`], [`Prepared`]): maps the input files to
//!   Competition_Data and a Published_Result, builds the Questions and returns
//!   the canonical bytes of all three. `ruletrace-lab` uses it.
//! - **Present** ([`present`], [`ResultClass::of`]): builds the report from a
//!   Compiled_Rulebook and the Outcomes, Decision_Traces and verifications of
//!   an evaluation, and classifies the result. `ruletrace-lab` uses it with
//!   values that come from a `ruletrace evaluate` results file.
//! - **Evaluate** ([`evaluate`]): Prepare, one `rt_engine::evaluate` call for
//!   the complete Question list, Present. It exists for the tests of
//!   Req 31.4 and 31.5 and is not for `ruletrace-lab`, which must obtain
//!   every Outcome from a `ruletrace evaluate` subprocess (Req 36.5).
#![forbid(unsafe_code)]

mod classify;
mod evaluate;
mod mapping;
mod prepare;
mod questions;
mod report;

#[cfg(test)]
mod tests_classify;
#[cfg(test)]
mod tests_diagnostics;
#[cfg(test)]
mod tests_fixture;
#[cfg(test)]
mod tests_mapping;
#[cfg(test)]
mod tests_report;

pub use classify::ResultClass;
pub use evaluate::{evaluate, Evaluated, PrizeInputs};
pub use mapping::{FileInputs, NamedFile};
pub use prepare::{prepare, Prepared, Rejection};
pub use questions::PrizeRequest;
pub use report::{present, PresentError, PresentInputs};
