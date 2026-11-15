//! HTTP status to [`ProviderFailure`] (Req 35.14, Req 29.2, Req 29.3).
//!
//! The decision reads the status and nothing else about an error response:
//! not its body, and not the provider's error `type` or `code`. Only a 429
//! looks at a header.

use rt_ai_adapter::ProviderFailure;

/// The failure for a non-2xx status. `retry_after` holds the values of every
/// `Retry-After` header of the response.
///
/// - 401 and 403: authentication failure.
/// - 429: rate limited, with the header read by [`retry_after_secs`].
/// - every 5xx, including 504: unavailable. A `Retry-After` of a 503 is
///   not read (Req 29.3). Whether 504 should stay transient is the open
///   decision OD-37-1.
/// - 3xx, 400, 402 and every other status: another provider error.
pub(crate) fn failure_for(status: u16, retry_after: &[&str]) -> ProviderFailure {
    match status {
        401 | 403 => ProviderFailure::AuthFailed,
        429 => ProviderFailure::RateLimited {
            retry_after_secs: retry_after_secs(retry_after),
        },
        500..=599 => ProviderFailure::Unavailable,
        other => ProviderFailure::Other {
            http_status: Some(other),
        },
    }
}

/// Whole seconds of a `Retry-After` header of 1 to 9 ASCII digits. A date,
/// any other syntax, or more than one header is absent.
pub(crate) fn retry_after_secs(values: &[&str]) -> Option<u32> {
    let [value] = values else {
        return None;
    };
    let digits = (1..=9).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_digit());
    if digits {
        value.parse().ok()
    } else {
        None
    }
}
