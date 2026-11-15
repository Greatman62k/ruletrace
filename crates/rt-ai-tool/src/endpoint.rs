//! Endpoint URL checks (Req 28.6, 28.18). A small strict parser: only the
//! scheme and the host matter here, and anything unusual is rejected.

use std::net::{Ipv4Addr, Ipv6Addr};

/// The parts of an endpoint that the checks and the log need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedEndpoint {
    pub(crate) scheme: String,
    pub(crate) host: String,
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

fn split_host_port(authority: &str) -> Option<&str> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, tail) = rest.split_once(']')?;
        let port_ok = tail.is_empty() || tail.strip_prefix(':').is_some_and(valid_port);
        return port_ok.then_some(host);
    }
    match authority.split_once(':') {
        None => Some(authority),
        Some((host, port)) => valid_port(port).then_some(host),
    }
}

fn valid_port(port: &str) -> bool {
    port.parse::<u16>().is_ok_and(|p| p > 0) && port.bytes().all(|b| b.is_ascii_digit())
}

/// Parses an endpoint. `None` when it is not a well-formed absolute URL
/// with a non-empty host. Backslashes, whitespace and control characters
/// are refused so that two parsers cannot disagree on the host.
pub(crate) fn parse(raw: &str) -> Option<ParsedEndpoint> {
    if raw.contains('\\') || raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let (scheme, _) = raw.split_once("://")?;
    if !valid_scheme(scheme) {
        return None;
    }
    let host = split_host_port(authority(raw)?)?;
    if host.is_empty() {
        return None;
    }
    Some(ParsedEndpoint {
        scheme: scheme.to_ascii_lowercase(),
        host: host.to_ascii_lowercase(),
    })
}

/// True for `localhost`, 127.0.0.0/8 and `::1`.
pub(crate) fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host.parse::<Ipv4Addr>().is_ok_and(|a| a.is_loopback())
        || host.parse::<Ipv6Addr>().is_ok_and(|a| a.is_loopback())
}

/// https anywhere, or http to a loopback host.
pub(crate) fn transport_permitted(endpoint: &ParsedEndpoint) -> bool {
    endpoint.scheme == "https" || (endpoint.scheme == "http" && is_loopback(&endpoint.host))
}
