//! RT-JSON: the strict JSON subset of design §4.1.1, with a positioned parser,
//! canonical compact and pretty writers, and the shared flag-key check for
//! decoders.
//!
//! The crate does no file access. Callers pass bytes and an
//! [`InputId`](rt_foundation::InputId).
#![forbid(unsafe_code)]

mod flags;
mod parse;
mod value;
mod write;

pub use flags::{check_flag_keys, FLAG_KEYS, LIFECYCLE_KEYS};
pub use parse::{parse, ParseLimits};
pub use value::{DuplicateKey, Kind, Member, Position, Value, MAX_SAFE_INTEGER};
pub use write::{write_compact, write_pretty};
