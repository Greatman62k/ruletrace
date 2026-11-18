//! The Tournament standings workflow module (`bracket-check`): input mapping,
//! Question construction and report presentation over the shared
//! Rules_Engine (design §8.7, §11.3, Req 32, 31).
//!
//! The crate is a library. It has no binary target and performs no file,
//! network, clock, random, environment or subprocess access. It applies no
//! ranking, no tie-breaking, no seeding and no threshold of its own: every
//! condition, weight, threshold, rounding mode and tie-breaking criterion is
//! in the Compiled_Rulebook, and every report value is copied from an
//! Outcome, a Decision_Trace or the Explainer's verification. The report has
//! no pairing, round, bracket position, seed, slot, advancement or
//! elimination field (Req 32.5).
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
//!
//! # Qualification and the deferred Rank_Reference
//!
//! A Qualification_Predicate is an eligibility Target (Req 32.2). A
//! qualification that reads a Rank_Position needs the `rank_reference`
//! operation of task 9.3, which is deferred. Until then the Validator reports
//! such a clause as Unsupported (RT-VAL-0003, "rank reference"), no
//! Compiled_Rulebook exists for it, and this module is never given one. The
//! request has no rank cutoff field. Asking for a ranking Target as a
//! qualification gives `APP_TARGET_KIND_MISMATCH` and no engine call. The
//! Tie_Group statement of Req 32.3 is not implemented.
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
mod tests_fixture;
#[cfg(test)]
mod tests_mapping;
#[cfg(test)]
mod tests_published;
#[cfg(test)]
mod tests_report;

pub use classify::ResultClass;
pub use evaluate::{evaluate, BracketInputs, Evaluated};
pub use mapping::{FileInputs, NamedFile};
pub use prepare::{prepare, Prepared, Rejection};
pub use questions::BracketRequest;
pub use report::{present, PresentError, PresentInputs};
