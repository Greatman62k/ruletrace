//! Fixture-based isolation and trace-verification tests (Req 16.2 to 16.7, 20.4
//! to 20.6, 9.7, 13.15 to 13.18). They run in one process on the committed
//! Slice 0 fixtures, with every Source_Model loaded before the Explainer runs.
#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

mod isolation;
mod load;
mod trace_verification;
