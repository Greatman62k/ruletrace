//! Stand-in Laboratory and raw recorder for the LT-6 spike (design §11.18.5, option A).
//!
//! Standard library only. The secret generator reads `/dev/urandom`, so the
//! browser protocol is supported on Unix (Linux and macOS) only; on other
//! platforms `util::random_hex` returns an `Unsupported` error. The server and
//! recorder themselves take the secret as a parameter and are platform neutral.

#![forbid(unsafe_code)]

pub mod http;
pub mod lab;
pub mod recorder;
pub mod util;
