//! Turning HTTP-like failures into a [`ProviderFailure`] (Req 29.3).

use rt_ai_adapter::ProviderFailure;

/// Parses a `Retry-After` header value.
///
/// Only a non-empty run of 1 to 9 ASCII digits counts. The HTTP layer has
/// already removed surrounding whitespace, so any space, sign, fraction,
/// date form or longer number is treated as absent. A value of 0 is
/// returned as `Some(0)`; the retry decision then keeps the schedule.
#[must_use]
pub fn parse_retry_after(value: &str) -> Option<u32> {
    if value.is_empty() || value.len() > 9 {
        return None;
    }
    let mut seconds: u32 = 0;
    for byte in value.bytes() {
        if !byte.is_ascii_digit() {
            return None;
        }
        seconds = seconds * 10 + u32::from(byte - b'0');
    }
    Some(seconds)
}

/// Classifies an HTTP status with an optional `Retry-After` header value.
///
/// The header is read only for status 429; on any other status it is
/// ignored. Statuses other than 429 become [`ProviderFailure::Other`]
/// carrying the status, so a 5xx stays transient and a 4xx stays final.
#[must_use]
pub fn failure_from_http(status: u16, retry_after: Option<&str>) -> ProviderFailure {
    if status == 429 {
        return ProviderFailure::RateLimited {
            retry_after_secs: retry_after.and_then(parse_retry_after),
        };
    }
    ProviderFailure::Other {
        http_status: Some(status),
    }
}
