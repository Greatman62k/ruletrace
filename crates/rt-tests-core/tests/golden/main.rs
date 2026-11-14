//! Golden-file tests (design §7.3; Req 20.4, 21.11, 21.12, 28.11, 1.7).
#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::string_slice,
    missing_docs
)]

mod harness;
mod ingest;
mod slice;
