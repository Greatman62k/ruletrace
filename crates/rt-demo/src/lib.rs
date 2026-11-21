//! Demonstration harness (class tool, used by tests only): manifest checks, the
//! runbook parser, the subprocess runner and golden comparison.
//!
//! The harness is document-agnostic. It names no document and reads no
//! environment variable; the caller passes `RT_BIN_DIR`, `RT_DEMO_BLESS` and
//! `CI` as values.
#![forbid(unsafe_code)]

pub mod coverage;
pub mod error;
pub mod golden;
pub mod guard;
mod json;
pub mod leak;
pub mod manifest;
pub mod runbook;
pub mod runner;
pub mod summary;

pub use error::{DemoError, Refusal};
pub use golden::{check_golden, first_difference, Mode};
pub use guard::{authorize_live, classify, Class, LiveAuthorization};
pub use manifest::{check_manifest, Manifest, ManifestDocument};
pub use runbook::{parse_runbook, Runbook, Step, StepOutput};
pub use runner::{
    binary_path, copy_demo, run_live_step, run_runbook, run_runbook_recorded, run_step, Captured,
    LiveContext, StepRecord,
};
pub use summary::{describe_evaluate, describe_ledger, diagnostic_codes};

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;
