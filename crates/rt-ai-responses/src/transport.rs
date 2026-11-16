//! The HTTPS transport over `ureq` with `rustls` (Req 28.6, 28.19).
//!
//! Each attempt builds its own agent, so no connection, cookie or setting
//! survives between attempts. The agent never follows a redirect, never
//! reads a proxy from the environment, and keeps the default TLS settings
//! with the embedded root store. Nothing here can turn off certificate or
//! host name verification, and no source line may try (a test scans for it).

use crate::status;
use rt_ai_adapter::ProviderFailure;
use std::io::Read;
use std::time::Duration;
use ureq::config::Config;
use ureq::Agent;

/// The `User-Agent` of every request, a constant with no version or host.
const USER_AGENT: &str = "ruletrace-ai";

/// One POST to send.
pub(crate) struct Call<'a> {
    pub url: &'a str,
    pub clear_text: bool,
    pub authorization: &'a str,
    pub body: &'a [u8],
    pub timeout: Duration,
    pub read_limit: u64,
}

/// A 2xx answer: the status and at most `read_limit` bytes of the body.
#[derive(Debug)]
pub(crate) struct Reply {
    pub body: Vec<u8>,
}

// Unit tests of this crate must never open a connection to a real host.
// Integration tests link the library without `cfg(test)`, so they rely on
// the source scan in `rt-ai-tool/tests/network_guard.rs`.
#[cfg(test)]
pub(crate) fn assert_loopback_in_tests(url: &str) {
    let loopback = crate::endpoint::Endpoint::parse(url)
        .is_ok_and(|endpoint| crate::endpoint::is_loopback_host(endpoint.host()));
    assert!(
        loopback,
        "a test sent to a non-loopback endpoint; tests may use loopback stubs only"
    );
}

fn agent(call: &Call<'_>) -> Agent {
    #[cfg(test)]
    assert_loopback_in_tests(call.url);
    let config = Config::builder()
        .timeout_global(Some(call.timeout))
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .proxy(None)
        .https_only(!call.clear_text)
        .user_agent(USER_AGENT)
        .build();
    Agent::new_with_config(config)
}

/// Sends the request and returns the 2xx body, or the failure.
///
/// Reads at most `read_limit` bytes of the body. An error response is
/// classified from its status alone and its body is not read.
pub(crate) fn post(call: &Call<'_>) -> Result<Reply, ProviderFailure> {
    let response = agent(call)
        .post(call.url)
        .header("Authorization", call.authorization)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .send(call.body)
        .map_err(|e| classify(&e))?;
    read(response, call.read_limit)
}

/// Sends a GET with the credential and no body, under the same rules as
/// [`post`]. `call.body` is not sent.
pub(crate) fn get(call: &Call<'_>) -> Result<Reply, ProviderFailure> {
    let response = agent(call)
        .get(call.url)
        .header("Authorization", call.authorization)
        .header("Accept", "application/json")
        .call()
        .map_err(|e| classify(&e))?;
    read(response, call.read_limit)
}

fn read(
    response: ureq::http::Response<ureq::Body>,
    read_limit: u64,
) -> Result<Reply, ProviderFailure> {
    let code = response.status().as_u16();
    if !(200..300).contains(&code) {
        let values: Vec<String> = response
            .headers()
            .get_all("retry-after")
            .iter()
            .map(|v| v.to_str().unwrap_or("").to_owned())
            .collect();
        let refs: Vec<&str> = values.iter().map(String::as_str).collect();
        return Err(status::failure_for(code, &refs));
    }
    let mut reader = response
        .into_body()
        .into_with_config()
        .limit(u64::MAX)
        .reader()
        .take(read_limit);
    let mut body = Vec::new();
    reader.read_to_end(&mut body).map_err(|e| classify_io(&e))?;
    Ok(Reply { body })
}

fn classify(error: &ureq::Error) -> ProviderFailure {
    match error {
        ureq::Error::Timeout(_) => ProviderFailure::Timeout,
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => ProviderFailure::TlsFailed,
        ureq::Error::Io(e) => classify_io(e),
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => ProviderFailure::Unavailable,
        _ => ProviderFailure::Other { http_status: None },
    }
}

fn classify_io(error: &std::io::Error) -> ProviderFailure {
    if matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ) {
        return ProviderFailure::Timeout;
    }
    if let Some(inner) = error.get_ref() {
        if let Some(e) = inner.downcast_ref::<ureq::Error>() {
            return classify(e);
        }
        if inner.downcast_ref::<rustls::Error>().is_some() {
            return ProviderFailure::TlsFailed;
        }
    }
    ProviderFailure::Unavailable
}

#[cfg(test)]
mod tests {
    use super::{assert_loopback_in_tests, get, Call};
    use std::time::Duration;

    #[test]
    fn loopback_urls_pass_the_guard() {
        for url in [
            "http://127.0.0.1:9/v1",
            "http://[::1]:9",
            "https://localhost/x",
        ] {
            assert_loopback_in_tests(url);
        }
    }

    #[test]
    #[should_panic(expected = "non-loopback endpoint")]
    fn a_real_host_panics_before_any_connection() {
        let call = Call {
            url: "https://api.openai.com/v1/models",
            clear_text: false,
            authorization: "Bearer x",
            body: b"",
            timeout: Duration::from_millis(1),
            read_limit: 1,
        };
        let _ = get(&call);
    }

    #[test]
    #[should_panic(expected = "non-loopback endpoint")]
    fn an_ip_literal_that_is_not_loopback_panics() {
        assert_loopback_in_tests("https://[2001:db8::1]/v1");
    }
}
