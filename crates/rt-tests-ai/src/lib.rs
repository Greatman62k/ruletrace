//! Cross-crate tests that need both the AI crates and the review path.
//!
//! The crate has no library code. It exists so that the tests which touch
//! `ruletrace-ai` and the ledger live outside every crate that the
//! dependency rules keep apart.
#![forbid(unsafe_code)]
