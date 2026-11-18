//! The result classification of Req 31.12.
//!
//! The class exists for the Evaluate group and its tests, which compare it
//! with the exit code of `ruletrace evaluate` (0, 4, 2 and 70). The
//! Laboratory does not use it: it takes the class from the exit code of its
//! `ruletrace evaluate` subprocess (design §11.18.7).
//!
//! The derivation is `rt_explain::classify_evaluation`, the one function that
//! `ruletrace evaluate` also calls. `ResultClass` and `class_of_run` live in
//! `rt-explain` beside it.

pub(crate) use rt_explain::class_of_run;
pub use rt_explain::ResultClass;
