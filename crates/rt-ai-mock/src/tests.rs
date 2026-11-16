//! Step-script provider: format, simulated time and failure forms.

use crate::{MockProvider, MockScript, ScriptError, ScriptResult, SimulatedClock, Step};
use rt_ai_adapter::{
    next_step, AiAdapter, AttemptOutcome, AttemptRecord, Completion, MonotonicClock,
    ProviderFailure, ProviderRequest, RequestLimits, RetryPolicy, RetryStep,
};
use rt_foundation::{Code, InputId};
use rt_json::Value;
use std::time::Duration;

const EXHAUSTED_0: &str = "script exhausted: it has 0 steps";
const EXHAUSTED_1: &str = "script exhausted: it has 1 steps";

fn id() -> InputId {
    InputId::new("script.json").unwrap()
}

fn request() -> ProviderRequest {
    ProviderRequest {
        task: "propose_rules".into(),
        model: "m1".into(),
        limits: RequestLimits {
            deadline_s: 60,
            attempt_s: 20,
            retries: 2,
            response_bytes: 1_048_576,
        },
        reasoning_effort: None,
        context: vec![("clauses".into(), Value::array(vec![]))],
    }
}

fn script(steps: &str) -> MockScript {
    let text = format!(r#"{{"format":"ruletrace-mock-script","version":1,"steps":[{steps}]}}"#);
    MockScript::parse(&id(), text.as_bytes()).unwrap()
}

fn rejected(text: &str) -> ScriptError {
    MockScript::parse(&id(), text.as_bytes()).unwrap_err()
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

const T20: Duration = Duration::from_secs(20);

#[test]
fn design_example_parses_to_the_exact_steps() {
    let s = script(
        r#"{"elapsed_ms":20000,"result":{"failure":"timeout"}},
           {"elapsed_ms":150,"result":{"failure":"rate_limited","retry_after_secs":3}},
           {"elapsed_ms":900,"result":{"response":{"completion":"complete","payload":"{\"a\":1}"}}}"#,
    );
    let expected = vec![
        Step {
            elapsed: ms(20_000),
            result: ScriptResult::Failure(ProviderFailure::Timeout),
        },
        Step {
            elapsed: ms(150),
            result: ScriptResult::Failure(ProviderFailure::RateLimited {
                retry_after_secs: Some(3),
            }),
        },
        Step {
            elapsed: ms(900),
            result: ScriptResult::Response {
                completion: Completion::Complete,
                payload: b"{\"a\":1}".to_vec(),
            },
        },
    ];
    assert_eq!(s.steps(), expected.as_slice());
}

#[test]
fn attempts_consume_steps_and_advance_only_the_simulated_clock() {
    let s = script(
        r#"{"elapsed_ms":20000,"result":{"failure":"timeout"}},
           {"elapsed_ms":150,"result":{"failure":"rate_limited","retry_after_secs":3}},
           {"elapsed_ms":900,"result":{"response":{"completion":"complete","payload":"x"}}}"#,
    );
    let mut p = MockProvider::new(s);
    let r = request();
    assert_eq!(p.clock().now(), Duration::ZERO);
    assert_eq!(p.clock().now(), Duration::ZERO);

    let a = p.attempt(&r, T20);
    assert_eq!(a.outcome, AttemptOutcome::Failure(ProviderFailure::Timeout));
    assert_eq!(a.elapsed, T20);
    assert_eq!(p.clock().now(), T20);

    p.clock().wait(Duration::from_secs(2));
    assert_eq!(p.clock().now(), Duration::from_secs(22));

    let b = p.attempt(&r, T20);
    assert_eq!(
        b.outcome,
        AttemptOutcome::Failure(ProviderFailure::RateLimited {
            retry_after_secs: Some(3)
        })
    );
    assert_eq!(p.clock().now(), ms(22_150));

    let c = p.attempt(&r, T20);
    let AttemptOutcome::Response(resp) = c.outcome else {
        panic!("expected a response");
    };
    assert_eq!(resp.payload, b"x".to_vec());
    assert_eq!(resp.completion, Completion::Complete);
    assert_eq!(resp.meta.identity.provider.as_str(), "mock");
    assert_eq!(resp.meta.model, "m1");
    assert_eq!(p.clock().now(), ms(23_050));
    assert_eq!(p.steps_consumed(), 3);
    assert_eq!(c.detail, None);
}

#[test]
fn exhausted_script_is_an_internal_error_and_a_final_failure() {
    let mut p = MockProvider::new(script(
        r#"{"elapsed_ms":5,"result":{"failure":"unavailable"}}"#,
    ));
    let r = request();
    p.attempt(&r, T20);
    let before = p.clock().now();
    let report = p.attempt(&r, T20);
    assert_eq!(
        report.outcome,
        AttemptOutcome::Failure(ProviderFailure::Other { http_status: None })
    );
    assert_eq!(report.elapsed, Duration::ZERO);
    assert_eq!(p.clock().now(), before);
    assert_eq!(report.detail.as_deref(), Some(EXHAUSTED_1));
    assert!(!ProviderFailure::Other { http_status: None }.is_transient());
}

#[test]
fn empty_script_is_exhausted_at_once() {
    let mut p = MockProvider::new(MockScript::from_steps(vec![]));
    let report = p.attempt(&request(), T20);
    assert_eq!(report.detail.as_deref(), Some(EXHAUSTED_0));
}

// Red/green: detail must reach a caller holding only the trait object.
#[test]
fn exhaustion_detail_reaches_a_caller_holding_only_the_trait_object() {
    let mut p = MockProvider::new(MockScript::from_steps(vec![]));
    let adapter: &mut dyn AiAdapter = &mut p;
    let report = adapter.attempt(&request(), T20);
    assert_eq!(report.detail.as_deref(), Some(EXHAUSTED_0));
}

#[test]
fn response_later_than_the_timeout_is_never_delivered() {
    let s = script(
        r#"{"elapsed_ms":20001,"result":{"response":{"completion":"complete","payload":"late"}}}"#,
    );
    let mut p = MockProvider::new(s);
    let report = p.attempt(&request(), T20);
    assert_eq!(
        report.outcome,
        AttemptOutcome::Failure(ProviderFailure::Timeout)
    );
    assert_eq!(report.elapsed, T20);
    assert_eq!(p.clock().now(), T20);
}

#[test]
fn response_exactly_at_the_timeout_is_delivered() {
    let s = script(
        r#"{"elapsed_ms":20000,"result":{"response":{"completion":"complete","payload":"ok"}}}"#,
    );
    let mut p = MockProvider::new(s);
    let report = p.attempt(&request(), T20);
    assert!(matches!(report.outcome, AttemptOutcome::Response(_)));
    assert_eq!(report.elapsed, T20);
}

#[test]
fn identical_script_and_requests_give_identical_reports() {
    let text = r#"{"elapsed_ms":7,"result":{"failure":"unavailable"}},
        {"elapsed_ms":9,"result":{"response":{"completion":"refused","payload":"no"}}}"#;
    let run = || {
        let mut p = MockProvider::new(script(text));
        let r = request();
        vec![p.attempt(&r, T20), p.attempt(&r, T20)]
    };
    assert_eq!(run(), run());
}

#[test]
fn every_failure_form_maps_to_its_provider_failure() {
    let cases: [(&str, ProviderFailure); 12] = [
        (r#"{"failure":"timeout"}"#, ProviderFailure::Timeout),
        (r#"{"failure":"unavailable"}"#, ProviderFailure::Unavailable),
        (r#"{"failure":"auth_failed"}"#, ProviderFailure::AuthFailed),
        (r#"{"failure":"tls_failed"}"#, ProviderFailure::TlsFailed),
        (
            r#"{"failure":"other"}"#,
            ProviderFailure::Other { http_status: None },
        ),
        (
            r#"{"failure":"other","http_status":400}"#,
            ProviderFailure::Other {
                http_status: Some(400),
            },
        ),
        (
            r#"{"failure":"other","http_status":503,"retry_after_header":"5"}"#,
            ProviderFailure::Other {
                http_status: Some(503),
            },
        ),
        (
            r#"{"failure":"other","http_status":504}"#,
            ProviderFailure::GatewayTimeout,
        ),
        (
            r#"{"failure":"other","http_status":429,"retry_after_header":"9"}"#,
            ProviderFailure::RateLimited {
                retry_after_secs: Some(9),
            },
        ),
        (
            r#"{"failure":"rate_limited"}"#,
            ProviderFailure::RateLimited {
                retry_after_secs: None,
            },
        ),
        (
            r#"{"failure":"rate_limited","retry_after_header":"Wed, 21 Oct 2015 07:28:00 GMT"}"#,
            ProviderFailure::RateLimited {
                retry_after_secs: None,
            },
        ),
        (
            r#"{"failure":"rate_limited","retry_after_header":"0"}"#,
            ProviderFailure::RateLimited {
                retry_after_secs: Some(0),
            },
        ),
    ];
    for (result, expected) in cases {
        let s = script(&format!(r#"{{"elapsed_ms":1,"result":{result}}}"#));
        assert_eq!(
            s.steps()[0].result,
            ScriptResult::Failure(expected),
            "{result}"
        );
    }
}

#[test]
fn refused_and_incomplete_completions_are_scripted_responses() {
    let s = script(
        r#"{"elapsed_ms":1,"result":{"response":{"completion":"refused","payload":"r"}}},
           {"elapsed_ms":1,"result":{"response":{"completion":"incomplete","payload":"i"}}}"#,
    );
    let mut p = MockProvider::new(s);
    let r = request();
    for expected in [Completion::Refused, Completion::Incomplete] {
        let AttemptOutcome::Response(resp) = p.attempt(&r, T20).outcome else {
            panic!("expected a response");
        };
        assert_eq!(resp.completion, expected);
    }
}

#[test]
fn malformed_payload_text_is_returned_byte_for_byte() {
    let s = script(
        r#"{"elapsed_ms":1,"result":{"response":{"completion":"complete","payload":"{\"truncated\": [1, 2"}}}"#,
    );
    let mut p = MockProvider::new(s);
    let AttemptOutcome::Response(resp) = p.attempt(&request(), T20).outcome else {
        panic!("expected a response");
    };
    assert_eq!(resp.payload, b"{\"truncated\": [1, 2".to_vec());
}

#[test]
fn payload_file_must_be_resolved_by_the_harness() {
    let text = r#"{"elapsed_ms":1,"result":{"response":{"completion":"complete","payload_file":"p/one.json"}}}"#;
    let mut unresolved = MockProvider::new(script(text));
    let report = unresolved.attempt(&request(), T20);
    assert_eq!(
        report.detail.as_deref(),
        Some("step 0 names unresolved payload file p/one.json")
    );

    let mut s = script(text);
    assert_eq!(
        s.resolve_payload_files(|_| None),
        Err("p/one.json".to_owned())
    );
    s.resolve_payload_files(|p| (p == "p/one.json").then(|| "body".to_owned()))
        .unwrap();
    let mut p = MockProvider::new(s);
    let AttemptOutcome::Response(resp) = p.attempt(&request(), T20).outcome else {
        panic!("expected a response");
    };
    assert_eq!(resp.payload, b"body".to_vec());
}

// Default policy. Hand-derived timeline: attempt 1 ends at 0.1 s; the header
// "5" beats the 2 s schedule, so the wait is 5 s and attempt 2 starts at
// 5.1 s and ends at 5.2 s; the date header is absent, so the second wait is
// the 8 s schedule and attempt 3 starts at 13.2 s and ends at 13.3 s.
#[test]
fn scripted_rate_limits_drive_the_retry_loop_to_the_derived_time() {
    let s = script(
        r#"{"elapsed_ms":100,"result":{"failure":"other","http_status":429,"retry_after_header":"5"}},
           {"elapsed_ms":100,"result":{"failure":"rate_limited","retry_after_header":"Wed, 21 Oct 2015 07:28:00 GMT"}},
           {"elapsed_ms":100,"result":{"response":{"completion":"complete","payload":"ok"}}}"#,
    );
    let mut p = MockProvider::new(s);
    let policy = RetryPolicy::default();
    let r = request();
    let mut history = Vec::new();
    let mut waits = Vec::new();
    loop {
        let now = p.clock().now();
        match next_step(&policy, &history, now) {
            RetryStep::Attempt { wait_before } => {
                waits.push(wait_before);
                p.clock().wait(wait_before);
                let report = p.attempt(&r, policy.attempt_timeout());
                history.push(match report.outcome {
                    AttemptOutcome::Response(_) => AttemptRecord::Response,
                    AttemptOutcome::Failure(f) => AttemptRecord::Failure(f),
                });
            }
            RetryStep::Stop(_) => break,
        }
    }
    let secs = Duration::from_secs;
    assert_eq!(waits, vec![Duration::ZERO, secs(5), secs(8)]);
    assert_eq!(p.clock().now(), ms(13_300));
    assert_eq!(history.last(), Some(&AttemptRecord::Response));
}

#[test]
fn simulated_clock_moves_only_on_wait() {
    let c = SimulatedClock::new();
    assert_eq!(c.now(), Duration::ZERO);
    c.wait(Duration::from_secs(8));
    c.wait(ms(500));
    assert_eq!(c.now(), ms(8_500));
}

#[test]
fn invalid_scripts_are_rejected_with_the_offending_path() {
    let head = r#"{"format":"ruletrace-mock-script","version":1,"steps":"#;
    let bad: [(&str, &str); 13] = [
        (r#"{"format":"other","version":1,"steps":[]}"#, ".format"),
        (r#"{"format":"ruletrace-mock-script","version":2,"steps":[]}"#, ".version"),
        (r#"{"format":"ruletrace-mock-script","version":1}"#, ".steps"),
        (r#"{"format":"ruletrace-mock-script","version":1,"steps":[],"extra":1}"#, ".extra"),
        ("[{\"elapsed_ms\":1}]", "steps[0].result"),
        ("[{\"result\":{\"failure\":\"timeout\"}}]", "steps[0].elapsed_ms"),
        ("[{\"elapsed_ms\":1,\"result\":{}}]", "steps[0].result"),
        ("[{\"elapsed_ms\":1,\"result\":{\"failure\":\"timeout\",\"response\":{}}}]", "steps[0].result"),
        ("[{\"elapsed_ms\":1,\"result\":{\"failure\":\"gone\"}}]", "steps[0].result.failure"),
        ("[{\"elapsed_ms\":1,\"result\":{\"failure\":\"other\",\"http_status\":99}}]", "steps[0].result.http_status"),
        ("[{\"elapsed_ms\":1,\"result\":{\"failure\":\"other\",\"http_status\":600}}]", "steps[0].result.http_status"),
        ("[{\"elapsed_ms\":1,\"result\":{\"failure\":\"rate_limited\",\"retry_after_secs\":1000000000}}]", "steps[0].result.retry_after_secs"),
        ("[{\"elapsed_ms\":1,\"result\":{\"failure\":\"timeout\",\"retry_after_secs\":1}}]", "steps[0].result.retry_after_secs"),
    ];
    for (body, path) in bad {
        let text = if body.starts_with('{') {
            body.to_owned()
        } else {
            format!("{head}{body}}}")
        };
        let ScriptError::Invalid(i) = rejected(&text) else {
            panic!("expected Invalid for {text}");
        };
        assert_eq!(i.path, path, "{text}");
    }
}

#[test]
fn both_retry_after_forms_together_and_header_without_status_are_rejected() {
    let both = r#"{"format":"ruletrace-mock-script","version":1,"steps":[{"elapsed_ms":1,"result":{"failure":"rate_limited","retry_after_secs":1,"retry_after_header":"1"}}]}"#;
    let orphan = r#"{"format":"ruletrace-mock-script","version":1,"steps":[{"elapsed_ms":1,"result":{"failure":"other","retry_after_header":"1"}}]}"#;
    assert!(matches!(rejected(both), ScriptError::Invalid(_)));
    assert!(matches!(rejected(orphan), ScriptError::Invalid(_)));
}

#[test]
fn flag_keys_are_rejected_with_the_dgn_codes() {
    let one = r#"{"format":"ruletrace-mock-script","version":1,"steps":[],"has_errors":false}"#;
    let ScriptError::FlagKeys(d) = rejected(one) else {
        panic!("expected FlagKeys");
    };
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].code().id(), "RT-DGN-0001");
    assert_eq!(d[0].code(), Code::DiagnosticStateContradiction);

    let life =
        r#"{"format":"ruletrace-mock-script","version":1,"steps":[],"approval_status":"Approved"}"#;
    let ScriptError::FlagKeys(d) = rejected(life) else {
        panic!("expected FlagKeys");
    };
    assert_eq!(d[0].code().id(), "RT-DGN-0003");
}

#[test]
fn truncated_and_empty_input_are_parse_errors() {
    for text in ["", "{", r#"{"format":"ruletrace-mock-script","steps":["#] {
        assert!(matches!(rejected(text), ScriptError::Parse(_)), "{text:?}");
    }
}
