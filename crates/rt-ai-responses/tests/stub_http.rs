//! Loopback stub-server tests of the real transport (tasks 10.3, 10.15).
//! Expected values are written from the requirements and the Meta record of
//! UD-37, not computed by the crate.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use rt_ai_adapter::{
    next_step, AttemptRecord, Completion, ProviderFailure as F, RetryPolicy, RetryStep, Usage,
};
use rt_foundation::InputId;
use rt_json::{parse, ParseLimits};
use std::time::Duration;
use support::*;

const T: Duration = Duration::from_secs(5);

fn run(
    replies: Vec<Reply>,
    effort: Option<&str>,
    limit: u32,
) -> (rt_ai_adapter::AttemptReport, Vec<Captured>) {
    let stub = Stub::start(replies);
    let mut a = adapter(&stub.endpoint());
    let report = attempt(&mut a, &request(effort, limit), T);
    (report, stub.finish())
}

fn body_json(c: &Captured) -> rt_json::Value<rt_json::Position> {
    parse(
        &InputId::new("sent").unwrap(),
        c.body(),
        ParseLimits::new(1 << 20, 32),
    )
    .unwrap()
}

#[test]
fn success_with_reasoning_item_maps_text_meta_and_usage() {
    let env = message_envelope(
        "{\"a\":\"x\"}",
        r#","usage":{"input_tokens":11,"output_tokens":22,"total_tokens":33}"#,
    );
    let (report, got) = run(vec![json_ok(&env)], None, 1 << 20);
    let r = response_of(&report);
    assert_eq!(r.completion, Completion::Complete);
    assert_eq!(r.payload, b"{\"a\":\"x\"}");
    assert_eq!(r.meta.model, "muse-spark-1.3");
    assert_eq!(r.meta.reported_model.as_deref(), Some("muse-reported"));
    assert_eq!(r.meta.response_id.as_deref(), Some("resp_1"));
    assert_eq!(
        r.meta.usage,
        Some(Usage {
            input_tokens: Some(11),
            output_tokens: Some(22),
            total_tokens: Some(33)
        })
    );
    assert_eq!(r.meta.identity.provider.as_str(), "meta");
    assert_eq!(r.meta.identity.protocol.as_str(), "openai-responses");
    assert_eq!(report.detail, None);
    assert_eq!(got.len(), 1);
}

#[test]
fn usage_absent_stays_absent() {
    let (report, _) = run(vec![json_ok(&message_envelope("{}", ""))], None, 1 << 20);
    assert_eq!(response_of(&report).meta.usage, None);
}

#[test]
fn partial_usage_is_not_completed_by_derivation() {
    let env = message_envelope("{}", r#","usage":{"input_tokens":5,"output_tokens":6}"#);
    let (report, _) = run(vec![json_ok(&env)], None, 1 << 20);
    assert_eq!(
        response_of(&report).meta.usage,
        Some(Usage {
            input_tokens: Some(5),
            output_tokens: Some(6),
            total_tokens: None
        })
    );
}

#[test]
fn request_bytes_are_exactly_as_specified() {
    let (report, got) = run(
        vec![json_ok(&message_envelope("{}", ""))],
        Some("low"),
        1 << 20,
    );
    response_of(&report);
    let c = &got[0];
    assert_eq!(c.request_line(), "POST /v1/responses HTTP/1.1");
    let h = c.headers();
    let get = |k: &str| {
        h.iter()
            .filter(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(get("authorization"), vec![format!("Bearer {CANARY}")]);
    assert_eq!(get("content-type"), vec!["application/json"]);
    assert_eq!(get("accept"), vec!["application/json"]);
    assert_eq!(get("user-agent"), vec!["ruletrace-ai"]);
    assert_eq!(get("content-length"), vec![c.body().len().to_string()]);
    // The credential is in no header but Authorization, and not in the body.
    for (name, value) in &h {
        if name != "authorization" {
            assert!(!value.contains(CANARY), "{name}");
        }
    }
    assert!(!String::from_utf8_lossy(c.body()).contains(CANARY));
    assert!(!c.request_line().contains(CANARY));

    let b = body_json(c);
    let keys: Vec<&str> = b
        .as_object()
        .unwrap()
        .iter()
        .map(|m| m.key.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "model",
            "input",
            "text",
            "store",
            "max_output_tokens",
            "reasoning"
        ]
    );
    assert_eq!(
        b.get("model").unwrap().value.as_str(),
        Some("muse-spark-1.3")
    );
    assert_eq!(
        b.get("store").unwrap().value.kind(),
        &rt_json::Kind::Bool(false)
    );
    assert_eq!(
        b.get("max_output_tokens").unwrap().value.as_u64(),
        Some(8192)
    );
    let effort = &b.get("reasoning").unwrap().value;
    assert_eq!(effort.get("effort").unwrap().value.as_str(), Some("low"));
    let format = &b.get("text").unwrap().value.get("format").unwrap().value;
    let fkeys: Vec<&str> = format
        .as_object()
        .unwrap()
        .iter()
        .map(|m| m.key.as_str())
        .collect();
    assert_eq!(fkeys, ["type", "name", "schema", "strict"]);
    assert_eq!(
        format.get("type").unwrap().value.as_str(),
        Some("json_schema")
    );
    assert_eq!(
        format.get("name").unwrap().value.as_str(),
        Some("ruletrace_proposals")
    );
    assert_eq!(
        format.get("strict").unwrap().value.kind(),
        &rt_json::Kind::Bool(true)
    );
    assert!(format
        .get("schema")
        .unwrap()
        .value
        .get("properties")
        .is_some());
    assert!(b.get("tools").is_none());

    // The input is the constant preamble then the canonical request.
    let input = b.get("input").unwrap().value.as_str().unwrap();
    let canonical =
        String::from_utf8(request(Some("low"), 1 << 20).canonical_bytes().unwrap()).unwrap();
    assert_eq!(
        input,
        format!("{}{canonical}", rt_ai_responses::INPUT_PREAMBLE)
    );
    assert!(canonical.contains("\"reasoning_effort\":\"low\""));
}

#[test]
fn no_reasoning_member_without_a_configured_effort() {
    let (_, got) = run(vec![json_ok(&message_envelope("{}", ""))], None, 1 << 20);
    assert!(body_json(&got[0]).get("reasoning").is_none());
    assert!(!String::from_utf8_lossy(got[0].body()).contains("reasoning_effort"));
}

fn assert_failure(status: &str, headers: &[(&str, &str)], body: &str, want: F) {
    let (report, got) = run(vec![http(status, headers, body.as_bytes())], None, 1 << 20);
    assert_eq!(failure_of(&report), want, "{status}");
    assert_eq!(got.len(), 1, "{status}: no second connection");
    assert!(!format!("{report:?}").contains(CANARY));
}

#[test]
fn meta_error_shapes_map_by_status() {
    let err =
        |t: &str| format!(r#"{{"error":{{"message":"m","type":"{t}","param":null,"code":null}}}}"#);
    assert_failure(
        "401 Unauthorized",
        &[],
        &err("invalid_api_key"),
        F::AuthFailed,
    );
    // A changed type does not change the mapping (Req 35.14).
    assert_failure(
        "401 Unauthorized",
        &[],
        &err("something_new"),
        F::AuthFailed,
    );
    assert_failure(
        "403 Forbidden",
        &[],
        &err("permission_denied"),
        F::AuthFailed,
    );
    assert_failure(
        "429 Too Many Requests",
        &[("Retry-After", "7")],
        &err("rate_limit"),
        F::RateLimited {
            retry_after_secs: Some(7),
        },
    );
    assert_failure(
        "429 Too Many Requests",
        &[],
        &err("rate_limit"),
        F::RateLimited {
            retry_after_secs: None,
        },
    );
    assert_failure(
        "429 Too Many Requests",
        &[("Retry-After", "Wed, 21 Oct 2026 07:28:00 GMT")],
        "",
        F::RateLimited {
            retry_after_secs: None,
        },
    );
    assert_failure(
        "503 Service Unavailable",
        &[("Retry-After", "60")],
        &err("service_overloaded"),
        F::Unavailable,
    );
    assert_failure(
        "503 Service Unavailable",
        &[("Retry-After", "60")],
        &err("backend_unavailable"),
        F::Unavailable,
    );
    // OD-37-1, decided on 2026-11-16 (option B): a 504 is final.
    assert_failure(
        "504 Gateway Timeout",
        &[],
        &err("gateway_timeout"),
        F::GatewayTimeout,
    );
    assert_failure(
        "400 Bad Request",
        &[],
        &err("invalid_request_error"),
        F::Other {
            http_status: Some(400),
        },
    );
    assert_failure(
        "402 Payment Required",
        &[],
        &err("billing"),
        F::Other {
            http_status: Some(402),
        },
    );
    assert_failure(
        "404 Not Found",
        &[],
        "",
        F::Other {
            http_status: Some(404),
        },
    );
    assert_failure("500 Internal Server Error", &[], "", F::Unavailable);
}

#[test]
fn a_503_retry_after_does_not_lengthen_the_scheduled_wait() {
    let (report, _) = run(
        vec![http(
            "503 Service Unavailable",
            &[("Retry-After", "60")],
            b"{}",
        )],
        None,
        1 << 20,
    );
    let failure = failure_of(&report);
    let policy = RetryPolicy::new(60, 20, 2).unwrap();
    let history = [AttemptRecord::Failure(failure)];
    let step = next_step(&policy, &history, report.elapsed);
    assert_eq!(
        step,
        RetryStep::Attempt {
            wait_before: Duration::from_secs(2)
        }
    );
}

#[test]
fn a_429_retry_after_is_honoured_by_the_decision() {
    let (report, _) = run(
        vec![http(
            "429 Too Many Requests",
            &[("Retry-After", "5")],
            b"{}",
        )],
        None,
        1 << 20,
    );
    let policy = RetryPolicy::new(60, 20, 2).unwrap();
    let step = next_step(
        &policy,
        &[AttemptRecord::Failure(failure_of(&report))],
        report.elapsed,
    );
    assert_eq!(
        step,
        RetryStep::Attempt {
            wait_before: Duration::from_secs(5)
        }
    );
}

#[test]
fn redirect_is_not_followed_and_nothing_is_sent_to_the_target() {
    let target = Stub::start(vec![json_ok(&message_envelope("{}", ""))]);
    let location = format!("http://127.0.0.1:{}/v1/responses", target.port);
    for code in [
        "301 Moved Permanently",
        "302 Found",
        "307 Temporary Redirect",
        "308 Permanent Redirect",
    ] {
        let (report, got) = run(
            vec![http(code, &[("Location", &location)], b"")],
            None,
            1 << 20,
        );
        let status: u16 = code[..3].parse().unwrap();
        assert_eq!(
            failure_of(&report),
            F::Other {
                http_status: Some(status)
            },
            "{code}"
        );
        assert_eq!(got.len(), 1);
    }
    assert_eq!(target.connections(), 0);
    assert!(target.finish().is_empty());
}

#[test]
fn refusal_incomplete_and_bad_shapes() {
    let refusal = r#"{"id":"r","status":"completed","model":"m","output":[{"type":"message","content":[{"type":"refusal","refusal":"no"}]}]}"#;
    let (rep, _) = run(vec![json_ok(refusal)], None, 1 << 20);
    assert_eq!(response_of(&rep).completion, Completion::Refused);

    let incomplete = r#"{"id":"r","status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"model":"m","output":[]}"#;
    let (rep, _) = run(vec![json_ok(incomplete)], None, 1 << 20);
    assert_eq!(response_of(&rep).completion, Completion::Incomplete);

    let tool = r#"{"id":"r","status":"completed","output":[{"type":"function_call","name":"f","arguments":"{}"},{"type":"message","content":[{"type":"output_text","text":"{}"}]}]}"#;
    let two = r#"{"id":"r","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"{}"}]},{"type":"message","content":[{"type":"output_text","text":"{}"}]}]}"#;
    let none = r#"{"id":"r","status":"completed","output":[{"type":"reasoning","summary":[]}]}"#;
    for (name, body) in [
        ("tool", tool),
        ("two", two),
        ("none", none),
        ("garbage", "not json"),
    ] {
        let (rep, _) = run(vec![json_ok(body)], None, 1 << 20);
        let r = response_of(&rep);
        assert_eq!(r.completion, Completion::Complete, "{name}");
        assert!(r.payload.is_empty(), "{name}");
        assert!(rep.detail.is_some(), "{name}");
    }
}

#[test]
fn response_limit_boundary_is_exact() {
    let env = message_envelope("{}", "");
    let n = u32::try_from(env.len()).unwrap();
    // Exactly the limit: parsed normally.
    let (rep, _) = run(vec![json_ok(&env)], None, n);
    assert_eq!(response_of(&rep).payload, b"{}");
    // One byte over: the payload is the first limit+1 bytes, so the caller
    // sees a size above the limit.
    let (rep, _) = run(vec![json_ok(&env)], None, n - 1);
    let r = response_of(&rep);
    assert_eq!(r.payload.len(), usize::try_from(n).unwrap());
    assert_eq!(r.payload, env.as_bytes());
}

#[test]
fn oversize_body_is_read_only_up_to_limit_plus_one() {
    let big = vec![b'x'; 200_000];
    let (rep, _) = run(vec![json_ok(&String::from_utf8(big).unwrap())], None, 1024);
    assert_eq!(response_of(&rep).payload.len(), 1025);
}

#[test]
fn slow_body_past_the_attempt_timeout_is_a_timeout() {
    let head =
        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{\"x\"".to_vec();
    let stub = Stub::start(vec![Reply::Slow {
        head_and_part: head,
        pause: Duration::from_secs(3),
    }]);
    let mut a = adapter(&stub.endpoint());
    let started = std::time::Instant::now();
    let rep = attempt(&mut a, &request(None, 1 << 20), Duration::from_millis(400));
    assert_eq!(failure_of(&rep), F::Timeout);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(rep.elapsed >= Duration::from_millis(300) && rep.elapsed < Duration::from_secs(2));
    stub.finish();
}

#[test]
fn connection_refused_and_close_are_unavailable() {
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let mut a = adapter(&format!("http://127.0.0.1:{port}"));
    let rep = attempt(&mut a, &request(None, 1 << 20), T);
    assert_eq!(failure_of(&rep), F::Unavailable);

    let (rep, _) = run(vec![Reply::Close], None, 1 << 20);
    assert_eq!(failure_of(&rep), F::Unavailable);

    let truncated = b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\n{\"id\"".to_vec();
    let (rep, _) = run(vec![Reply::Raw(truncated)], None, 1 << 20);
    assert_eq!(failure_of(&rep), F::Unavailable);
}

#[test]
fn https_to_a_plain_text_server_is_a_tls_failure() {
    let stub = Stub::start(vec![Reply::Eager(b"HTTP/1.1 200 OK\r\n\r\n".to_vec())]);
    let mut a = adapter(&format!("https://127.0.0.1:{}", stub.port));
    let rep = attempt(&mut a, &request(None, 1 << 20), T);
    assert_eq!(failure_of(&rep), F::TlsFailed);
    let got = stub.finish();
    // Nothing of the request, and so no credential, was readable by it.
    for c in &got {
        assert!(!String::from_utf8_lossy(&c.raw).contains(CANARY));
    }
}

#[test]
fn debug_of_every_public_value_hides_the_canary() {
    let stub = Stub::start(vec![json_ok(&message_envelope("{}", ""))]);
    let mut a = adapter(&stub.endpoint());
    let rep = attempt(&mut a, &request(None, 1 << 20), T);
    let shown = format!("{a:?} {rep:?} {:?}", support::identity_of(&a));
    assert!(!shown.contains(CANARY), "{shown}");
    stub.finish();
}

// Req 29.2 as decided on 2026-11-16 (UD-37 OD-37-1, option B): the real
// transport and the real retry decision. A 504 ends the sequence after one
// attempt with no wait, even with two retries left; 500, 502 and 503 are
// retried on the 2 s schedule.
#[test]
fn a_504_ends_the_retry_sequence_after_one_attempt_and_other_5xx_do_not() {
    for (status, retried) in [
        ("500 Internal Server Error", true),
        ("502 Bad Gateway", true),
        ("503 Service Unavailable", true),
        ("504 Gateway Timeout", false),
    ] {
        let (report, got) = run(
            vec![http(status, &[], br#"{"error":{"type":"x"}}"#)],
            None,
            1 << 20,
        );
        assert_eq!(got.len(), 1, "{status}: one request on the wire");
        let failure = failure_of(&report);
        let policy = RetryPolicy::default();
        let step = next_step(&policy, &[AttemptRecord::Failure(failure)], report.elapsed);
        if retried {
            assert_eq!(
                step,
                RetryStep::Attempt {
                    wait_before: Duration::from_secs(2)
                },
                "{status}"
            );
        } else {
            assert_eq!(
                step,
                RetryStep::Stop(rt_ai_adapter::RetryStop::NonTransient(failure)),
                "{status}"
            );
        }
    }
}
