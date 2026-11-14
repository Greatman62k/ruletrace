//! Outcomes, Decision_Traces, Kleene three-valued logic and replay (design
//! §3.6.3).
//!
//! This crate holds the data the engine produces and the Explainer consumes. It
//! depends on neither: the engine builds a [`DecisionTrace`], the Explainer
//! verifies its spans against pre-loaded Source_Models, and [`replay`]
//! re-derives every step from the trace alone.
//!
//! A trace holds Locations and document hashes only, never source text (Req
//! 10.11). Numbers are [`rt_numeric::ExactNumber`]; no floating-point type
//! appears in any public item.
#![forbid(unsafe_code)]

mod outcome;
mod replay;
mod serial;
mod trace;
mod tri;

pub use outcome::{Outcome, Place, RankPlace, Reason, TraceValue};
pub use replay::{replay, ReplayError};
pub use trace::{
    CompareCriterion, CompareEntry, CriterionResult, DecisionEntry, DecisionOp, DecisionTrace,
    DecisionValue, DocumentHash, Entry, MissingInput, NotApplied, NotAppliedReason, QuestionKind,
    RankDirection, ReadEntry, StepEntry, StepFault, StepOp, StepResult, TraceError, TraceParts,
    TraceQuestion, TraceRule, TraceSpan,
};
pub use tri::Tri;

#[cfg(test)]
mod tests;
