//! The URL that the transport builds from an endpoint text (review wave 2,
//! issue 3). The request line is compared as exact text, so a doubled `/`
//! or a changed path cannot pass.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use rt_ai_adapter::ModelCatalog;
use std::time::Duration;
use support::*;

const T: Duration = Duration::from_secs(5);

fn endpoints(port: u16) -> Vec<String> {
    vec![
        format!("http://127.0.0.1:{port}"),
        format!("http://127.0.0.1:{port}/"),
        format!("HTTP://127.0.0.1:{port}"),
        format!("HTTP://127.0.0.1:{port}/"),
        format!("hTtP://127.0.0.1:{port}/"),
    ]
}

#[test]
fn every_spelling_of_the_endpoint_sends_the_same_request_line_for_a_response() {
    for shape in 0..5 {
        let stub = Stub::start(vec![json_ok(&message_envelope("{\"a\":\"x\"}", ""))]);
        let endpoint = endpoints(stub.port).remove(shape);
        let mut a = adapter(&endpoint);
        let report = attempt(&mut a, &request(None, 1 << 20), T);
        response_of(&report);
        let got = stub.finish();
        assert_eq!(
            got[0].request_line(),
            "POST /v1/responses HTTP/1.1",
            "{endpoint}"
        );
    }
}

#[test]
fn every_spelling_of_the_endpoint_sends_the_same_request_line_for_the_catalogue() {
    for shape in 0..5 {
        let stub = Stub::start(vec![json_ok(r#"{"data":[{"id":"m"}]}"#)]);
        let endpoint = endpoints(stub.port).remove(shape);
        let mut a = adapter(&endpoint);
        assert_eq!(a.list_models(T, 4096).unwrap(), ["m"], "{endpoint}");
        let got = stub.finish();
        assert_eq!(
            got[0].request_line(),
            "GET /v1/models HTTP/1.1",
            "{endpoint}"
        );
    }
}

#[test]
fn a_path_prefix_is_kept_once_and_a_trailing_slash_is_dropped() {
    let stub = Stub::start(vec![json_ok(r#"{"data":[]}"#)]);
    let endpoint = format!("http://127.0.0.1:{}/proxy/", stub.port);
    let mut a = adapter(&endpoint);
    assert!(a.list_models(T, 4096).unwrap().is_empty());
    let got = stub.finish();
    assert_eq!(got[0].request_line(), "GET /proxy/v1/models HTTP/1.1");
}
