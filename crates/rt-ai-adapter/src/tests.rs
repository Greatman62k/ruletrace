//! Request identity, failure classification and a driver loop over the
//! trait with a simulated clock.

use crate::retry::AttemptRecord;
use crate::{
    next_step, AiAdapter, AttemptOutcome, AttemptReport, Completion, Identity, MonotonicClock,
    Protocol, ProviderFailure, ProviderId, ProviderMeta, ProviderRequest, ProviderResponse,
    RequestLimits, RetryPolicy, RetryStep, RetryStop,
};
use rt_json::Value;
use std::cell::Cell;
use std::time::Duration;

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
        context: vec![
            ("clauses".into(), Value::array(vec![])),
            ("revise".into(), Value::null()),
        ],
    }
}

#[test]
fn canonical_bytes_and_id_match_independent_values() {
    let expected = r#"{"v":1,"task":"propose_rules","model":"m1","limits":{"deadline_s":60,"attempt_s":20,"retries":2,"response_bytes":1048576},"clauses":[],"revise":null}"#;
    let r = request();
    assert_eq!(r.canonical_bytes().unwrap(), expected.as_bytes());
    // sha256sum of the bytes above, computed outside this code base.
    assert_eq!(
        r.request_id().unwrap().as_str(),
        "b98f2750b87a8a8a1776233fa312f81bc685e75f07d5a91b4562915a47ac7a1f"
    );
}

#[test]
fn any_difference_changes_the_id() {
    let base = request().request_id().unwrap();
    let mut a = request();
    a.model = "m2".into();
    let mut b = request();
    b.task = "other".into();
    let mut c = request();
    c.limits.retries = 1;
    let mut d = request();
    d.context[0].1 = Value::array(vec![Value::string("x")]);
    for changed in [a, b, c, d] {
        assert_ne!(changed.request_id().unwrap(), base);
    }
    assert_eq!(request().request_id().unwrap(), base);
}

#[test]
fn a_request_without_an_effort_keeps_its_pinned_identifier() {
    // The same bytes and sha256sum as before the effort field existed.
    let r = request();
    assert!(r.reasoning_effort.is_none());
    assert!(!String::from_utf8(r.canonical_bytes().unwrap())
        .unwrap()
        .contains("reasoning_effort"));
    assert_eq!(
        r.request_id().unwrap().as_str(),
        "b98f2750b87a8a8a1776233fa312f81bc685e75f07d5a91b4562915a47ac7a1f"
    );
}

#[test]
fn an_effort_is_in_the_bytes_after_the_limits_and_changes_the_identifier() {
    let mut r = request();
    r.reasoning_effort = Some("low".to_owned());
    let expected = r#"{"v":1,"task":"propose_rules","model":"m1","limits":{"deadline_s":60,"attempt_s":20,"retries":2,"response_bytes":1048576},"reasoning_effort":"low","clauses":[],"revise":null}"#;
    assert_eq!(r.canonical_bytes().unwrap(), expected.as_bytes());
    // sha256sum of the bytes above, computed outside this code base.
    assert_eq!(
        r.request_id().unwrap().as_str(),
        "76dbc119c31c34d4e501b2142bd44ae3737888f3c976e2bd4f800e383bbf7911"
    );
    let mut high = request();
    high.reasoning_effort = Some("high".to_owned());
    assert_ne!(high.request_id(), r.request_id());
    assert_ne!(r.request_id(), request().request_id());
}

#[test]
fn a_context_key_that_repeats_the_effort_gives_no_bytes() {
    let mut r = request();
    r.reasoning_effort = Some("low".to_owned());
    r.context
        .push(("reasoning_effort".into(), Value::string("high")));
    assert!(r.canonical_bytes().is_none());
}

#[test]
fn colliding_context_key_gives_no_bytes() {
    let mut r = request();
    r.context.push(("model".into(), Value::string("x")));
    assert!(r.canonical_bytes().is_none());
    assert!(r.request_id().is_none());
}

#[test]
fn transient_classification() {
    use ProviderFailure as F;
    let transient = [
        F::Unavailable,
        F::Timeout,
        F::RateLimited {
            retry_after_secs: None,
        },
        F::Other {
            http_status: Some(500),
        },
        F::Other {
            http_status: Some(502),
        },
        F::Other {
            http_status: Some(503),
        },
        F::Other {
            http_status: Some(505),
        },
        F::Other {
            http_status: Some(599),
        },
    ];
    let fatal = [
        F::AuthFailed,
        F::TlsFailed,
        F::Other { http_status: None },
        F::Other {
            http_status: Some(499),
        },
        F::Other {
            http_status: Some(600),
        },
        F::Other {
            http_status: Some(302),
        },
        // UD-37 OD-37-1, option B: a gateway timeout is never retried.
        F::GatewayTimeout,
        F::Other {
            http_status: Some(504),
        },
    ];
    assert!(transient.iter().all(F::is_transient));
    assert!(fatal.iter().all(|f| !f.is_transient()));
}

struct Sim(Cell<Duration>);

impl MonotonicClock for Sim {
    fn now(&self) -> Duration {
        self.0.get()
    }
    fn wait(&self, d: Duration) {
        self.0.set(self.0.get() + d);
    }
}

struct Scripted {
    clock: Sim,
    script: Vec<AttemptReport>,
    calls: usize,
}

impl AiAdapter for Scripted {
    fn attempt(&mut self, _r: &ProviderRequest, _t: Duration) -> AttemptReport {
        let report = self.script[self.calls].clone();
        self.calls += 1;
        self.clock.0.set(self.clock.0.get() + report.elapsed);
        report
    }
    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }
}

fn timeout_report() -> AttemptReport {
    AttemptReport {
        outcome: AttemptOutcome::Failure(ProviderFailure::Timeout),
        elapsed: Duration::from_secs(20),
        detail: None,
    }
}

fn drive(adapter: &mut dyn AiAdapter) -> (RetryStop, usize) {
    let policy = RetryPolicy::default();
    let req = request();
    let start = adapter.clock().now();
    let mut history = Vec::new();
    loop {
        let elapsed = adapter.clock().now() - start;
        match next_step(&policy, &history, elapsed) {
            RetryStep::Stop(s) => return (s, history.len()),
            RetryStep::Attempt { wait_before } => {
                adapter.clock().wait(wait_before);
                let report = adapter.attempt(&req, policy.attempt_timeout());
                history.push(match report.outcome {
                    AttemptOutcome::Response(_) => AttemptRecord::Response,
                    AttemptOutcome::Failure(f) => AttemptRecord::Failure(f),
                });
            }
        }
    }
}

#[test]
fn driver_reproduces_worked_example_through_the_trait() {
    let mut a = Scripted {
        clock: Sim(Cell::new(Duration::ZERO)),
        script: vec![timeout_report(), timeout_report()],
        calls: 0,
    };
    let (stop, attempts) = drive(&mut a);
    assert_eq!(stop, RetryStop::DeadlineExceeded { attempts: 2 });
    assert_eq!(attempts, 2);
    assert_eq!(a.calls, 2);
    assert_eq!(a.clock.now(), Duration::from_secs(42));
}

#[test]
fn driver_succeeds_after_one_retry_with_waits_on_the_simulated_clock() {
    let ok = AttemptReport {
        outcome: AttemptOutcome::Response(ProviderResponse {
            completion: Completion::Complete,
            payload: b"{}".to_vec(),
            meta: ProviderMeta::new(
                Identity::new(ProviderId::new("mock").unwrap(), Protocol::MockReplay, "1"),
                "m1",
            ),
        }),
        elapsed: Duration::from_secs(1),
        detail: None,
    };
    let mut a = Scripted {
        clock: Sim(Cell::new(Duration::ZERO)),
        script: vec![timeout_report(), ok],
        calls: 0,
    };
    assert_eq!(drive(&mut a), (RetryStop::Success, 2));
    assert_eq!(a.clock.now(), Duration::from_secs(20 + 2 + 1));
}
