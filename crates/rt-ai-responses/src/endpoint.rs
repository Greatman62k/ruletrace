//! The endpoint that receives the credential (Req 28.6, 28.18).
//!
//! The crate re-checks what the configuration loader checks, so that no
//! caller can hand it an endpoint that would send the credential over a
//! clear-text connection to a remote host.

use std::net::{Ipv4Addr, Ipv6Addr};

/// Why a text is not an acceptable endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointError {
    /// Not `https://` and not `http://` to a loopback host.
    Scheme,
    /// `http://` to a host that is not loopback.
    InsecureHost,
    /// The authority is empty, malformed or has an invalid port.
    Authority,
    /// A user name or password.
    UserInfo,
    /// A query or a fragment.
    QueryOrFragment,
    /// A control character, a space or a backslash.
    Character,
}

/// A validated endpoint: `https`, or `http` to loopback for tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    base: String,
    host: String,
    clear_text: bool,
}

impl Endpoint {
    /// Validates `text`. The scheme is compared case-insensitively and
    /// written in lower case.
    ///
    /// # Errors
    /// The first rule that `text` breaks.
    pub fn parse(text: &str) -> Result<Self, EndpointError> {
        if has_unusable_character(text) {
            return Err(EndpointError::Character);
        }
        let (clear_text, rest) = split_scheme(text).ok_or(EndpointError::Scheme)?;
        if rest.contains('?') || rest.contains('#') {
            return Err(EndpointError::QueryOrFragment);
        }
        let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
        if authority.contains('@') {
            return Err(EndpointError::UserInfo);
        }
        let host = host_of(authority).ok_or(EndpointError::Authority)?;
        if clear_text && !is_loopback_host(host) {
            return Err(EndpointError::InsecureHost);
        }
        let scheme = if clear_text { "http" } else { "https" };
        let path = path.trim_end_matches('/');
        Ok(Self {
            base: format!("{scheme}://{authority}{path}"),
            host: host.to_ascii_lowercase(),
            clear_text,
        })
    }

    /// The host, in lower case and without port or brackets.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Whether the connection is clear text, which only loopback allows.
    #[must_use]
    pub fn is_clear_text(&self) -> bool {
        self.clear_text
    }

    /// The full URL of `path`, which starts with `/`.
    #[must_use]
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

// Returns whether the scheme is clear text, and the text after `://`.
fn split_scheme(text: &str) -> Option<(bool, &str)> {
    let (scheme, rest) = text.split_once("://")?;
    if scheme.eq_ignore_ascii_case("https") {
        Some((false, rest))
    } else if scheme.eq_ignore_ascii_case("http") {
        Some((true, rest))
    } else {
        None
    }
}

// The host of an authority, without brackets or port, or `None` when the
// authority is empty or the port is not a number of at most 5 digits.
fn host_of(authority: &str) -> Option<&str> {
    let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
        let (host, after) = inner.split_once(']')?;
        match after {
            "" => (host, None),
            _ => (host, Some(after.strip_prefix(':')?)),
        }
    } else {
        match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    if host.is_empty() {
        return None;
    }
    if let Some(port) = port {
        let digits = !port.is_empty() && port.len() <= 5;
        if !digits || !port.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    }
    Some(host)
}

/// True when `text` has a character that no endpoint may contain: a
/// control character, white space, a backslash, or a Unicode format
/// character (general category Cf) such as U+200B or U+202E. The tool and
/// the adapter share this check, so they agree on every endpoint text.
#[must_use]
pub fn has_unusable_character(text: &str) -> bool {
    text.chars()
        .any(|c| c.is_control() || c.is_whitespace() || c == '\\' || is_format(c))
}

// The characters of general category Cf in Unicode 15, by range.
fn is_format(c: char) -> bool {
    matches!(u32::from(c),
        0xAD | 0x600..=0x605 | 0x61C | 0x6DD | 0x70F | 0x890..=0x891 | 0x8E2
        | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064
        | 0x2066..=0x206F | 0xFEFF | 0xFFF9..=0xFFFB | 0x110BD | 0x110CD
        | 0x13430..=0x1343F | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A
        | 0xE0001 | 0xE0020..=0xE007F)
}

/// The one loopback definition of the tool and the adapter (Req 28.6):
/// `localhost` in any ASCII case, an address of 127.0.0.0/8 and the IPv6
/// address `::1`, written in any form that `std` parses to those addresses.
/// `host` has no brackets. A mapped address such as `::ffff:127.0.0.1`,
/// `127.1` and a name with a trailing dot are not loopback.
#[must_use]
pub fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<Ipv4Addr>().is_ok_and(|a| a.is_loopback())
        || host.parse::<Ipv6Addr>().is_ok_and(|a| a.is_loopback())
}
