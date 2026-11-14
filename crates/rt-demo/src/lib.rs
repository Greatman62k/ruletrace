//! Demonstration harness (class tool, used by tests only): manifest checks, the runbook parser,
//! the subprocess runner and golden comparison.
//!
//! The harness is document-agnostic. It names no document and reads no environment variable;
//! the caller passes `RT_BIN_DIR`, `RT_DEMO_BLESS` and `CI` as values.
#![forbid(unsafe_code)]

pub mod coverage;
pub mod error;
pub mod golden;
mod json;
pub mod manifest;
pub mod runbook;
pub mod runner;

pub use error::DemoError;
pub use golden::{check_golden, first_difference, Mode};
pub use manifest::{check_manifest, Manifest, ManifestDocument};
pub use runbook::{parse_runbook, Runbook, Step, StepOutput};
pub use runner::{binary_path, copy_demo, run_runbook, run_step, Captured};

#[cfg(test)]
mod tests;
