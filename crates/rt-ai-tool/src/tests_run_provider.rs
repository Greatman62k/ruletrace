//! Provider-call tests through the mock provider and its simulated clock
//! (design 3.7.6; Req 29.2 to 29.6, 29.8, 29.12, 29.15, 29.18). The
//! simulated time after each run is worked out by hand from the policy:
//! waits of 2 s and 8 s, attempt timeout 20 s, deadline 60 s.

use crate::tests_memfs::MemFs;
use crate::tests_rig::{payload, plan_of, Harness};
use crate::tests_run::{fail, ok, ok_with, provider};
use crate::tests_run_support::{shapes, VecSink};
use crate::RunReport;
use rt_ai_adapter::{MonotonicClock, ProviderFailure};
use rt_ai_mock::MockProvider;
use rt_foundation::{Code, ExitCode, InputId, Location, Severity};
use rt_proposals::parse_batch;

fn single() -> Harness {
    let mut h = Harness::new("");
    h.plan = plan_of(&h.config, &[0]);
    h
}

fn run_one(steps: Vec<rt_ai_mock::Step>) -> (RunReport, MemFs, MockProvider, Harness) {
    let h = single();
    let fs = MemFs::new();
    let mut p = provider(steps);
    let report = h.run(&mut p, &fs, &VecSink::default());
    (report, fs, p, h)
}

fn clock_ms(p: &MockProvider) -> u128 {
    p.simulated_clock().now().as_millis()
}

fn codes(report: &RunReport) -> Vec<&'static str> {
    shapes(&report.diagnostics).iter().map(|s| s.0).collect()
}

fn unavailable() -> ProviderFailure {
    ProviderFailure::Unavailable
}

#[test]
fn rt_ai_0101_unavailable_warns_and_retries_after_two_seconds() {
    let (r, fs, p, h) = run_one(vec![fail(unavailable(), 0), ok(0)]);
    assert_eq!(shapes(&r.diagnostics), vec![("RT-AI-0101", "warning")]);
    assert_eq!(
        r.diagnostics[0].message(),
        "attempt 1 failed: AI_PROVIDER_UNAVAILABLE"
    );
    match r.diagnostics[0].primary() {
        Location::Input(at) => assert_eq!(at.input.as_str(), format!("request-{}", h.id(0))),
        other => panic!("{other:?}"),
    }
    assert_eq!(r.exit, ExitCode::Success);
    assert_eq!(p.steps_consumed(), 2);
    assert_eq!(clock_ms(&p), 2_000 + 100);
    assert!(fs.read(&h.batch_file(0)).is_some());
}

#[test]
fn rt_ai_0101_covers_http_5xx_reported_as_other() {
    let (r, _, _, _) = run_one(vec![
        fail(
            ProviderFailure::Other {
                http_status: Some(503),
            },
            0,
        ),
        ok(0),
    ]);
    assert_eq!(shapes(&r.diagnostics), vec![("RT-AI-0101", "warning")]);
}

#[test]
fn rt_ai_0102_a_timeout_warns_and_costs_the_attempt_timeout() {
    let (r, _, p, _) = run_one(vec![fail(ProviderFailure::Timeout, 20_000), ok(0)]);
    assert_eq!(shapes(&r.diagnostics), vec![("RT-AI-0102", "warning")]);
    assert_eq!(clock_ms(&p), 20_000 + 2_000 + 100);
}

#[test]
fn a_step_slower_than_the_attempt_timeout_is_a_timeout_and_never_stored() {
    let late = payload(0).replace("clause 0", "late response");
    let (r, fs, p, h) = run_one(vec![
        crate::tests_run::ok_with_ms(late.clone(), 25_000),
        ok(0),
    ]);
    assert_eq!(shapes(&r.diagnostics), vec![("RT-AI-0102", "warning")]);
    assert_eq!(p.steps_consumed(), 2);
    let stored = fs.read(&h.batch_file(0)).unwrap();
    let batch = parse_batch(&InputId::new("b").unwrap(), &stored).unwrap();
    assert_eq!(batch.response.payload, payload(0));
    assert!(!String::from_utf8_lossy(&stored).contains("late response"));
}

#[test]
fn late_responses_on_every_attempt_store_nothing_and_leak_into_no_output() {
    let late = payload(0).replace("clause 0", "late response");
    let (r, fs, _, _) = run_one(vec![
        crate::tests_run::ok_with_ms(late.clone(), 25_000),
        crate::tests_run::ok_with_ms(late, 25_000),
    ]);
    assert_eq!(codes(&r), vec!["RT-AI-0102", "RT-AI-0102", "RT-AI-0107"]);
    assert!(fs.files().is_empty());
    for d in &r.diagnostics {
        assert!(!crate::render_diagnostic(d).contains("late response"));
    }
}

#[test]
fn rt_ai_0103_rate_limit_waits_the_larger_of_retry_after_and_the_schedule() {
    let limited = |secs| ProviderFailure::RateLimited {
        retry_after_secs: secs,
    };
    let (r, _, p, _) = run_one(vec![fail(limited(Some(5)), 0), ok(0)]);
    assert_eq!(shapes(&r.diagnostics), vec![("RT-AI-0103", "warning")]);
    assert_eq!(clock_ms(&p), 5_000 + 100);
    let (_, _, p, _) = run_one(vec![fail(limited(Some(0)), 0), ok(0)]);
    assert_eq!(clock_ms(&p), 2_000 + 100);
    let (_, _, p, _) = run_one(vec![fail(limited(None), 0), ok(0)]);
    assert_eq!(clock_ms(&p), 2_000 + 100);
}

#[test]
fn a_retry_after_that_fills_the_deadline_exactly_is_permitted() {
    let limited = ProviderFailure::RateLimited {
        retry_after_secs: Some(40),
    };
    let (r, fs, p, h) = run_one(vec![fail(limited, 0), ok(0)]);
    assert_eq!(r.exit, ExitCode::Success);
    assert_eq!(clock_ms(&p), 40_000 + 100);
    assert!(fs.read(&h.batch_file(0)).is_some());
}

#[test]
fn a_retry_after_that_does_not_fit_gives_rt_ai_0107_without_waiting() {
    let limited = ProviderFailure::RateLimited {
        retry_after_secs: Some(41),
    };
    let (r, fs, p, _) = run_one(vec![fail(limited, 0), ok(0)]);
    assert_eq!(
        shapes(&r.diagnostics),
        vec![("RT-AI-0103", "warning"), ("RT-AI-0107", "error")]
    );
    assert_eq!(r.exit.value(), 5);
    assert_eq!(p.steps_consumed(), 1);
    assert_eq!(clock_ms(&p), 0);
    assert!(fs.files().is_empty());
}

#[test]
fn rt_ai_0104_authentication_failure_is_final_after_an_earlier_warning() {
    let (r, fs, p, _) = run_one(vec![
        fail(unavailable(), 0),
        fail(ProviderFailure::AuthFailed, 0),
        ok(0),
    ]);
    assert_eq!(
        shapes(&r.diagnostics),
        vec![("RT-AI-0101", "warning"), ("RT-AI-0104", "error")]
    );
    assert_eq!(r.exit.value(), 5);
    assert_eq!(p.steps_consumed(), 2);
    assert!(fs.files().is_empty());
}

#[test]
fn rt_ai_0105_other_provider_errors_are_final_and_never_retried() {
    for status in [400_u16, 302, 404] {
        let (r, _, p, _) = run_one(vec![
            fail(
                ProviderFailure::Other {
                    http_status: Some(status),
                },
                0,
            ),
            ok(0),
        ]);
        assert_eq!(
            shapes(&r.diagnostics),
            vec![("RT-AI-0105", "error")],
            "{status}"
        );
        assert_eq!(r.exit.value(), 5, "{status}");
        assert_eq!(p.steps_consumed(), 1, "{status}");
    }
}

#[test]
fn rt_ai_0105_carries_the_adapter_detail() {
    let (r, _, _, _) = run_one(vec![]);
    assert_eq!(shapes(&r.diagnostics), vec![("RT-AI-0105", "error")]);
    assert_eq!(
        r.diagnostics[0].message(),
        "AI_PROVIDER_ERROR (script exhausted: it has 0 steps)"
    );
}

#[test]
fn rt_ai_0108_a_tls_failure_is_final() {
    let (r, _, p, _) = run_one(vec![fail(ProviderFailure::TlsFailed, 0), ok(0)]);
    assert_eq!(shapes(&r.diagnostics), vec![("RT-AI-0108", "error")]);
    assert_eq!(r.exit.value(), 5);
    assert_eq!(p.steps_consumed(), 1);
}

#[test]
fn rt_ai_0106_three_transient_failures_exhaust_the_retries() {
    let (r, fs, p, _) = run_one(vec![
        fail(unavailable(), 0),
        fail(unavailable(), 0),
        fail(unavailable(), 0),
        ok(0),
    ]);
    assert_eq!(
        codes(&r),
        vec!["RT-AI-0101", "RT-AI-0101", "RT-AI-0101", "RT-AI-0106"]
    );
    let last = r.diagnostics.last().unwrap();
    assert_eq!(last.severity(), Severity::Error);
    assert_eq!(
        last.message(),
        "3 attempts failed; the last was AI_PROVIDER_UNAVAILABLE"
    );
    assert_eq!(r.exit.value(), 5);
    assert_eq!(p.steps_consumed(), 3);
    assert_eq!(clock_ms(&p), 2_000 + 8_000);
    assert!(fs.files().is_empty());
}

#[test]
fn rt_ai_0107_two_timeouts_leave_too_little_time_for_a_third_attempt() {
    let (r, fs, p, _) = run_one(vec![
        fail(ProviderFailure::Timeout, 20_000),
        fail(ProviderFailure::Timeout, 20_000),
        ok(0),
    ]);
    assert_eq!(codes(&r), vec!["RT-AI-0102", "RT-AI-0102", "RT-AI-0107"]);
    let last = r.diagnostics.last().unwrap();
    assert_eq!(
        last.message(),
        "the deadline cannot fit another attempt after 2; the last failure was AI_PROVIDER_TIMEOUT"
    );
    assert_eq!(r.exit.value(), 5);
    assert_eq!(p.steps_consumed(), 2);
    assert_eq!(clock_ms(&p), 42_000);
    assert!(codes(&r).iter().all(|c| *c != "RT-AI-0106"));
    assert!(fs.files().is_empty());
}

#[test]
fn success_after_zero_one_and_two_retries_stores_the_same_bytes() {
    let scripts = [
        vec![ok(0)],
        vec![fail(unavailable(), 0), ok(0)],
        vec![
            fail(unavailable(), 0),
            fail(ProviderFailure::Timeout, 20_000),
            ok(0),
        ],
    ];
    let mut stored = Vec::new();
    for (n, steps) in scripts.into_iter().enumerate() {
        let (r, fs, p, h) = run_one(steps);
        assert_eq!(r.exit, ExitCode::Success);
        assert_eq!(r.diagnostics.len(), n);
        assert_eq!(p.steps_consumed(), n + 1);
        assert_eq!(fs.files().len(), 1);
        stored.push(fs.read(&h.batch_file(0)).unwrap());
    }
    assert_eq!(stored[0], stored[1]);
    assert_eq!(stored[1], stored[2]);
}

#[test]
fn a_complete_but_bad_response_is_rejected_once_without_a_retry() {
    for (text, code, exit) in [
        ("not json".to_owned(), "RT-AI-0201", 6),
        (
            payload(0).replace("\"quote\":\"Alpha beta.\"", "\"quote\":\"Alpha gamma.\""),
            "RT-PRV-0104",
            3,
        ),
    ] {
        let (r, fs, p, _) = run_one(vec![ok_with(text), ok(0)]);
        assert_eq!(codes(&r), vec![code]);
        assert_eq!(r.exit.value(), exit);
        assert_eq!(p.steps_consumed(), 1);
        assert!(fs.files().is_empty());
    }
}

#[test]
fn the_failing_code_decides_the_exit_code() {
    let (r, _, _, _) = run_one(vec![fail(ProviderFailure::AuthFailed, 0)]);
    assert_eq!(r.diagnostics[0].code(), Code::AiProviderAuthFailed);
    assert_eq!(r.diagnostics[0].code().exit_code(), r.exit);
}
