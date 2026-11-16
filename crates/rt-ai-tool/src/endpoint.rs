//! Endpoint URL checks (Req 28.6, 28.18). A small strict parser: only the
//! scheme and the host matter here, and anything unusual is rejected.

/// The parts of an endpoint that the checks and the log need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedEndpoint {
    pub(crate) scheme: String,
    pub(crate) host: String,
    /// True when the URL is the bare origin: no port, no path other than
    /// a single `/`, no query and no fragment.
    pub(crate) bare_origin: bool,
    /// True when the URL has a fragment. The adapter refuses one, so
    /// validation does too.
    pub(crate) has_fragment: bool,
}

fn authority(raw: &str) -> Option<&str> {
    let (_, rest) = raw.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    rest.get(..end)
}

/// True when the URL carries a user name, a password or a query.
pub(crate) fn has_userinfo_or_query(raw: &str) -> bool {
    authority(raw).is_some_and(|a| a.contains('@')) || raw.contains('?')
}

fn valid_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// The host and whether the authority names a port.
fn split_host_port(authority: &str) -> Option<(&str, bool)> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, tail) = rest.split_once(']')?;
        let port_ok = tail.is_empty() || tail.strip_prefix(':').is_some_and(valid_port);
        return port_ok.then_some((host, !tail.is_empty()));
    }
    match authority.split_once(':') {
        None => Some((authority, false)),
        Some((host, port)) => valid_port(port).then_some((host, true)),
    }
}

// At most five digits, like the adapter, so `001271` is refused by both.
fn valid_port(port: &str) -> bool {
    port.len() <= 5
        && port.parse::<u16>().is_ok_and(|p| p > 0)
        && port.bytes().all(|b| b.is_ascii_digit())
}

/// Parses an endpoint. `None` when it is not a well-formed absolute URL
/// with a non-empty host. Backslashes, whitespace and control characters
/// are refused so that two parsers cannot disagree on the host.
pub(crate) fn parse(raw: &str) -> Option<ParsedEndpoint> {
    if rt_ai_responses::has_unusable_character(raw) {
        return None;
    }
    let (scheme, _) = raw.split_once("://")?;
    if !valid_scheme(scheme) {
        return None;
    }
    let authority = authority(raw)?;
    let (host, has_port) = split_host_port(authority)?;
    if host.is_empty() {
        return None;
    }
    // The authority is a prefix of the text after `://`, so what follows
    // it is the path, the query and the fragment.
    let (_, rest) = raw.split_once("://")?;
    let tail = rest.get(authority.len()..)?;
    Some(ParsedEndpoint {
        scheme: scheme.to_ascii_lowercase(),
        host: host.to_ascii_lowercase(),
        bare_origin: !has_port && matches!(tail, "" | "/"),
        has_fragment: tail.contains('#'),
    })
}

/// True for the loopback hosts of Req 28.6. The definition is the
/// adapter's, so a host that validation accepts also builds an adapter.
pub(crate) fn is_loopback(host: &str) -> bool {
    rt_ai_responses::is_loopback_host(host)
}

/// https anywhere, or http to a loopback host.
pub(crate) fn transport_permitted(endpoint: &ParsedEndpoint) -> bool {
    endpoint.scheme == "https" || (endpoint.scheme == "http" && is_loopback(&endpoint.host))
}

/// The endpoint host for a provider with an official host: that host after
/// ASCII lower-casing and as a bare origin, or a loopback address for the
/// stub server of the tests, which may carry a port and a path (Req 35.6).
/// Everything else is refused, including a suffix, a prefix, a trailing
/// dot, an IP literal, a port, a path, a query and a fragment of the
/// official host, and a fragment on a loopback endpoint (Req 35.6: a
/// loopback host may carry a port and a path only). The default port 443 is refused too, because this
/// parser does not normalize it.
pub(crate) fn host_permitted(endpoint: &ParsedEndpoint, official: &str) -> bool {
    !endpoint.has_fragment
        && ((endpoint.host == official && endpoint.bare_origin) || is_loopback(&endpoint.host))
}

/// Why endpoint validation refuses a URL, in the order of the checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// Not a well-formed absolute URL (RT-AI-0002).
    Invalid,
    /// `http` to a host that is not loopback, or another scheme (RT-AI-0004).
    Insecure,
    /// Not the official host of the provider or loopback (RT-AI-0013).
    HostNotPermitted,
}

/// The endpoint checks that follow the userinfo and query check, in one
/// place so that the configuration loader and the tests run the same
/// code. `official` is the official host of a hosted provider.
pub(crate) fn check(raw: &str, official: Option<&str>) -> Result<ParsedEndpoint, Refusal> {
    let parsed = parse(raw).ok_or(Refusal::Invalid)?;
    if !transport_permitted(&parsed) {
        return Err(Refusal::Insecure);
    }
    match official {
        Some(host) if !host_permitted(&parsed, host) => Err(Refusal::HostNotPermitted),
        // The adapter keeps no fragment, so no endpoint may carry one.
        _ if parsed.has_fragment => Err(Refusal::Invalid),
        _ => Ok(parsed),
    }
}
