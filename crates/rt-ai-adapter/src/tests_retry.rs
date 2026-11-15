//! Retry decision tests. Expected values are hand-derived from Req 29 and
//! design §3.7.6; the property test compares against an independently
//! written oracle that works in integer nanoseconds.

use crate::retry::AttemptRecord as R;
use crate::retry::{RetryStep, RetryStop};
use crate::{next_step, ProviderFailure as F, RetryPolicy, RetryPolicyError};
use proptest::prelude::*;
use std::time::Duration;

fn s(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn go(wait: u64) -> RetryStep {
    RetryStep::Attempt {
        wait_before: s(wait),
    }
}

fn stop(x: RetryStop) -> RetryStep {
    RetryStep::Stop(x)
}

fn rl(secs: Option<u32>) -> R {
    R::Failure(F::RateLimited {
        retry_after_secs: secs,
    })
}

const T: R = R::Failure(F::Timeout);
const U: R = R::Failure(F::Unavailable);

#[test]
fn hand_derived_default_table() {
    let p = RetryPolicy::default();
    let eps = Duration::from_nanos(1);
    let cases: Vec<(&str, Vec<R>, Duration, RetryStep)> = vec![
        ("first attempt", vec![], s(0), go(0)),
        (
            "response",
            vec![R::Response],
            s(5),
            stop(RetryStop::Success),
        ),
        (
            "success after retry",
            vec![T, R::Response],
            s(25),
            stop(RetryStop::Success),
        ),
        // 60 - 20 = 40 >= 2 + 20.
        ("retry 1 fits", vec![T], s(20), go(2)),
        // Req 29 worked boundary: 60 - 42 = 18 < 8 + 20.
        (
            "worked 42 s",
            vec![T, T],
            s(42),
            stop(RetryStop::DeadlineExceeded { attempts: 2 }),
        ),
        // 60 - 38 = 22 = 2 + 20: equality permits.
        ("retry 1 equality", vec![T], s(38), go(2)),
        (
            "retry 1 one ns over",
            vec![T],
            s(38) + eps,
            stop(RetryStop::DeadlineExceeded { attempts: 1 }),
        ),
        // 60 - 32 = 28 = 8 + 20.
        ("retry 2 equality", vec![T, T], s(32), go(8)),
        (
            "retry 2 one ns over",
            vec![T, T],
            s(32) + eps,
            stop(RetryStop::DeadlineExceeded { attempts: 2 }),
        ),
        ("503 style, attempt 2", vec![U], s(0), go(2)),
        ("503 style, attempt 3", vec![U, U], s(2), go(8)),
        (
            "503 style exhausted",
            vec![U, U, U],
            s(10),
            stop(RetryStop::Exhausted { attempts: 3 }),
        ),
        (
            "exhausted wins over deadline",
            vec![U, U, U],
            s(500),
            stop(RetryStop::Exhausted { attempts: 3 }),
        ),
        (
            "past deadline",
            vec![T],
            s(61),
            stop(RetryStop::DeadlineExceeded { attempts: 1 }),
        ),
        ("429 without header", vec![rl(None)], s(0), go(2)),
        (
            "429 header 1 below schedule",
            vec![rl(Some(1))],
            s(0),
            go(2),
        ),
        ("429 header 0 unchanged", vec![rl(Some(0))], s(0), go(2)),
        (
            "429 header equal to schedule",
            vec![rl(Some(2))],
            s(0),
            go(2),
        ),
        (
            "429 header 3 above schedule",
            vec![rl(Some(3))],
            s(0),
            go(3),
        ),
        ("429 header 9 on retry 2", vec![U, rl(Some(9))], s(2), go(9)),
        (
            "429 header 5 on retry 2 uses 8",
            vec![U, rl(Some(5))],
            s(2),
            go(8),
        ),
        // 60 - 10 = 50 = 30 + 20.
        ("429 header fits exactly", vec![rl(Some(30))], s(10), go(30)),
        (
            "429 header one ns over",
            vec![rl(Some(30))],
            s(10) + eps,
            stop(RetryStop::DeadlineExceeded { attempts: 1 }),
        ),
        (
            "429 huge header",
            vec![rl(Some(u32::MAX))],
            s(0),
            stop(RetryStop::DeadlineExceeded { attempts: 1 }),
        ),
        (
            "500 in Other is transient",
            vec![R::Failure(F::Other {
                http_status: Some(503),
            })],
            s(0),
            go(2),
        ),
        (
            "auth",
            vec![R::Failure(F::AuthFailed)],
            s(0),
            stop(RetryStop::NonTransient(F::AuthFailed)),
        ),
        (
            "tls",
            vec![R::Failure(F::TlsFailed)],
            s(0),
            stop(RetryStop::NonTransient(F::TlsFailed)),
        ),
        (
            "400",
            vec![R::Failure(F::Other {
                http_status: Some(400),
            })],
            s(0),
            stop(RetryStop::NonTransient(F::Other {
                http_status: Some(400),
            })),
        ),
        (
            "other without status",
            vec![R::Failure(F::Other { http_status: None })],
            s(0),
            stop(RetryStop::NonTransient(F::Other { http_status: None })),
        ),
        (
            "non-transient after transient",
            vec![T, R::Failure(F::AuthFailed)],
            s(22),
            stop(RetryStop::NonTransient(F::AuthFailed)),
        ),
    ];
    for (name, history, elapsed, expected) in cases {
        assert_eq!(next_step(&p, &history, elapsed), expected, "{name}");
    }
}

#[test]
fn custom_policies() {
    let none = RetryPolicy::new(60, 20, 0).unwrap();
    assert_eq!(
        next_step(&none, &[T], s(0)),
        stop(RetryStop::Exhausted { attempts: 1 })
    );
    let one = RetryPolicy::new(60, 20, 1).unwrap();
    assert_eq!(next_step(&one, &[T], s(0)), go(2));
    assert_eq!(
        next_step(&one, &[T, T], s(0)),
        stop(RetryStop::Exhausted { attempts: 2 })
    );
    let small = RetryPolicy::new(10, 3, 2).unwrap();
    // 10 - 3 = 7 >= 2 + 3.
    assert_eq!(next_step(&small, &[T], s(3)), go(2));
    // 10 - 8 = 2 < 8 + 3.
    assert_eq!(
        next_step(&small, &[T, T], s(8)),
        stop(RetryStop::DeadlineExceeded { attempts: 2 })
    );
    // Exhaustion by timeouts alone: 3 + 2 + 3 + 8 + 3 = 19 fits in 20.
    let fits = RetryPolicy::new(20, 3, 2).unwrap();
    assert_eq!(next_step(&fits, &[T, T], s(8)), go(8));
    assert_eq!(
        next_step(&fits, &[T, T, T], s(19)),
        stop(RetryStop::Exhausted { attempts: 3 })
    );
}

#[test]
fn policy_ranges() {
    assert!(RetryPolicy::new(1, 1, 0).is_ok());
    assert!(RetryPolicy::new(300, 120, 2).is_ok());
    assert_eq!(RetryPolicy::new(0, 1, 0), Err(RetryPolicyError::Deadline));
    assert_eq!(
        RetryPolicy::new(301, 20, 2),
        Err(RetryPolicyError::Deadline)
    );
    assert_eq!(
        RetryPolicy::new(60, 0, 2),
        Err(RetryPolicyError::AttemptTimeout)
    );
    assert_eq!(
        RetryPolicy::new(200, 121, 2),
        Err(RetryPolicyError::AttemptTimeout)
    );
    assert_eq!(
        RetryPolicy::new(10, 11, 2),
        Err(RetryPolicyError::AttemptTimeout)
    );
    assert!(RetryPolicy::new(10, 10, 2).is_ok());
    assert_eq!(RetryPolicy::new(60, 20, 3), Err(RetryPolicyError::Retries));
    let d = RetryPolicy::default();
    assert_eq!(RetryPolicy::new(60, 20, 2), Ok(d));
}

fn failure_strategy() -> impl Strategy<Value = F> {
    prop_oneof![
        Just(F::Unavailable),
        Just(F::Timeout),
        proptest::option::of(0u32..120).prop_map(|r| F::RateLimited {
            retry_after_secs: r
        }),
        Just(F::AuthFailed),
        Just(F::TlsFailed),
        proptest::option::of(400u16..600).prop_map(|h| F::Other { http_status: h }),
    ]
}

fn transient(f: &F) -> bool {
    match f {
        F::Unavailable | F::Timeout | F::RateLimited { .. } => true,
        F::Other {
            http_status: Some(h),
        } => (500..600).contains(h),
        _ => false,
    }
}

// Independent oracle: integer nanoseconds, decisions written as a flat
// list of conditions per the requirement text.
fn oracle(
    deadline_s: u64,
    attempt_s: u64,
    retries: u64,
    history: &[R],
    elapsed_ns: u128,
) -> RetryStep {
    const NS: u128 = 1_000_000_000;
    let n = history.len() as u64;
    if n == 0 {
        return go(0);
    }
    match history[history.len() - 1] {
        R::Response => stop(RetryStop::Success),
        R::Failure(f) if !transient(&f) => stop(RetryStop::NonTransient(f)),
        R::Failure(f) => {
            if n > retries {
                return stop(RetryStop::Exhausted { attempts: n as u8 });
            }
            let scheduled: u128 = if n == 1 { 2 } else { 8 };
            let header: u128 = match f {
                F::RateLimited {
                    retry_after_secs: Some(r),
                } => u128::from(r),
                _ => 0,
            };
            let wait = scheduled.max(header) * NS;
            let remaining = (u128::from(deadline_s as u32) * NS).saturating_sub(elapsed_ns);
            if remaining >= wait + u128::from(attempt_s as u32) * NS {
                RetryStep::Attempt {
                    wait_before: Duration::from_nanos(wait as u64),
                }
            } else {
                stop(RetryStop::DeadlineExceeded { attempts: n as u8 })
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    // Validates: Requirements 29.1, 29.3, 29.5, 29.6, 29.13, 29.15.
    #[test]
    fn matches_oracle(
        deadline in 1u32..=300,
        attempt_raw in 1u32..=120,
        retries in 0u8..=2,
        prefix in proptest::collection::vec(failure_strategy().prop_filter("transient", transient), 0..3),
        last in prop_oneof![Just(None), failure_strategy().prop_map(Some)],
        elapsed_ms in 0u64..400_000,
        jitter_ns in prop_oneof![Just(0u64), Just(1u64), Just(999_999u64)],
    ) {
        let attempt = attempt_raw.min(deadline);
        let policy = RetryPolicy::new(deadline, attempt, retries).unwrap();
        let mut history: Vec<R> = prefix.into_iter().map(R::Failure).collect();
        history.push(last.map_or(R::Response, R::Failure));
        let elapsed = Duration::from_millis(elapsed_ms) + Duration::from_nanos(jitter_ns);
        let expected = oracle(
            u64::from(deadline), u64::from(attempt), u64::from(retries), &history,
            elapsed.as_nanos(),
        );
        prop_assert_eq!(next_step(&policy, &history, elapsed), expected);
    }

    // Boundary-focused: elapsed placed at deadline - (wait + attempt) +/- 1 ns.
    #[test]
    fn boundary_is_inclusive(
        attempt in 1u32..=100,
        extra in 0u32..=100,
        n in 1usize..=2,
    ) {
        let wait = if n == 1 { 2u32 } else { 8 };
        let deadline = (attempt + extra + wait + 1).min(300);
        let attempt = attempt.min(deadline);
        let policy = RetryPolicy::new(deadline, attempt, 2).unwrap();
        let history = vec![T; n];
        let edge = Duration::from_secs(u64::from(deadline - attempt - wait));
        prop_assert_eq!(next_step(&policy, &history, edge), go(u64::from(wait)));
        prop_assert_eq!(
            next_step(&policy, &history, edge + Duration::from_nanos(1)),
            stop(RetryStop::DeadlineExceeded { attempts: n as u8 })
        );
    }
}
