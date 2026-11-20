//! The Explainer: the only crate that resolves, hash-verifies and renders
//! source text for a Decision_Trace (design §3.5, Req 13.7 to 13.9, 13.13 to
//! 13.17, 13.19, 13.22).
//!
//! Inputs are a [`rt_trace::DecisionTrace`] and pre-loaded
//! [`rt_srcmodel::SourceModels`] only. The crate reads no file, environment,
//! clock or random source, and it does not depend on the engine. Verification
//! never alters the trace: it returns a separate [`Verification`] whose status
//! is derived from its spans and its diagnostics.
//!
//! Also here: the shared results-file serializer ([`results`]), the
//! Proof_Report builder and the application report model and writers
//! ([`report`]), and the exit class of an evaluation
//! ([`classify_evaluation`]), shared by `ruletrace evaluate` and the
//! Workflow_Modules.
#![forbid(unsafe_code)]

mod classify;
mod render;
pub mod report;
pub mod results;
mod verify;

pub use classify::{
    class_of_run, classify_evaluation, has_question_fault, EvaluationClass, ResultClass,
};
pub use render::render_trace;
pub use verify::{verify_trace, SpanResolution, SpanVerdict, Verification, VerificationStatus};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_classify;

#[cfg(test)]
mod tests_report;

#[cfg(test)]
mod tests_app_fixtures;

#[cfg(test)]
mod tests_app_report;

#[cfg(test)]
mod tests_app_property;

#[cfg(test)]
mod tests_app_gaps;

#[cfg(test)]
mod tests_assemble;

#[cfg(test)]
mod tests_result_class;

#[cfg(test)]
#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;
