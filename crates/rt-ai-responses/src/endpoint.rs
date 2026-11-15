//! The endpoint that receives the credential (Req 28.6, 28.18).
//!
//! The crate re-checks what the configuration loader checks, so that no
//! caller can hand it an endpoint that would send the credential over a
//! clear-text connection to a remote host.

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
        if text
            .chars()
            .any(|c| c.is_control() || c == ' ' || c == '\\')
        {
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
        if clear_text && !is_loopback(host) {
            return Err(EndpointError::InsecureHost);
        }
        let scheme = if clear_text { "http" } else { "https" };
        let path = path.strip_suffix('/').unwrap_or(path);
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

fn is_loopback(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") || host == "::1" {
        return true;
    }
    let parts: Vec<&str> = host.split('.').collect();
    let [first, rest @ ..] = parts.as_slice() else {
        return false;
    };
    let octet = |s: &str| {
        !s.is_empty() && s.len() <= 3 && s.bytes().all(|b| b.is_ascii_digit()) && {
            s.parse::<u16>().is_ok_and(|n| n <= 255)
        }
    };
    parts.len() == 4 && *first == "127" && rest.iter().all(|p| octet(p))
}
