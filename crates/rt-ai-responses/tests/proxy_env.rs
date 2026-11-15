//! Proxy variables have no effect (Req 28.5, 28.19). This file is its own
//! test binary and holds one test, so changing the process environment
//! cannot race with another test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::time::Duration;
use support::*;

#[test]
fn proxy_environment_variables_are_ignored() {
    let proxy = Stub::start(vec![json_ok(&message_envelope("{\"via\":\"proxy\"}", ""))]);
    let proxy_url = format!("http://127.0.0.1:{}", proxy.port);
    for name in [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        std::env::set_var(name, &proxy_url);
    }
    for name in ["NO_PROXY", "no_proxy"] {
        std::env::remove_var(name);
    }
    let stub = Stub::start(vec![json_ok(&message_envelope("{\"via\":\"direct\"}", ""))]);
    let mut a = adapter(&stub.endpoint());
    let rep = attempt(&mut a, &request(None, 1 << 20), Duration::from_secs(5));
    assert_eq!(response_of(&rep).payload, b"{\"via\":\"direct\"}");
    assert_eq!(proxy.connections(), 0);
    assert_eq!(stub.finish().len(), 1);
    assert!(proxy.finish().is_empty());
}
