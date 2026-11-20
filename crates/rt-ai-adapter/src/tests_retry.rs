//! Retry decision tests. Expected values are hand-derived from Req 29 and
//! design §3.7.6; the property test compares against an independently
//! written oracle that works in integer nanoseconds.

use crate::proptest_floor::property_tests;
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
            "504 in Other is final, with retries left",
            vec![R::Failure(F::Other {
                http_status: Some(504),
            })],
            s(0),
            stop(RetryStop::NonTransient(F::Other {
                http_status: Some(504),
            })),
        ),
        (
            "gateway timeout is final, with retries left",
            vec![R::Failure(F::GatewayTimeout)],
            s(0),
            stop(RetryStop::NonTransient(F::GatewayTimeout)),
        ),
        (
            "502 in Other is transient",
            vec![R::Failure(F::Other {
                http_status: Some(502),
            })],
            s(0),
            go(2),
        ),
        (
            "500 in Other is transient",
            vec![R::Failure(F::Other {
                http_status: Some(500),
            })],
            s(0),
            go(2),
        ),
        (
            "504 after a transient failure stops without a wait",
            vec![
                T,
                R::Failure(F::Other {
                    http_status: Some(504),
                }),
            ],
            s(22),
            stop(RetryStop::NonTransient(F::Other {
                http_status: Some(504),
            })),
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

fn transient(f: &F) -> bool {
    match f {
        F::Unavailable | F::Timeout | F::RateLimited { .. } => true,
        // Req 29.2 as decided on 2026-11-16 (UD-37 OD-37-1, option B):
        // HTTP 504 is final; every other 5xx stays transient.
        F::Other {
            http_status: Some(h),
        } => (500..600).contains(h) && *h != 504,
        _ => false,
    }
}

// Req 29.4: seconds waited before retry 1 and retry 2. Literals from the
// requirement text, not read from the implementation.
const REQ_29_4_WAIT_SECS: [u128; 2] = [2, 8];

// Req 29.1 defaults: total deadline, attempt timeout and retry count.
const REQ_29_1_DEFAULTS: (u64, u64, u8) = (60, 20, 2);

const NS: u128 = 1_000_000_000;

// Independent oracle: integer nanoseconds, decisions written as a flat
// list of conditions per the requirement text.
fn oracle(
    deadline_s: u64,
    attempt_s: u64,
    retries: u64,
    history: &[R],
    elapsed_ns: u128,
) -> RetryStep {
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
            let scheduled = REQ_29_4_WAIT_SECS[(n - 1) as usize];
            let header: u128 = match f {
                F::RateLimited {
                    retry_after_secs: Some(r),
                } => u128::from(r),
                _ => 0,
            };
            let wait = scheduled.max(header) * NS;
            let remaining = (u128::from(deadline_s) * NS).saturating_sub(elapsed_ns);
            if remaining >= wait + u128::from(attempt_s) * NS {
                RetryStep::Attempt {
                    wait_before: Duration::from_nanos(wait as u64),
                }
            } else {
                stop(RetryStop::DeadlineExceeded { attempts: n as u8 })
            }
        }
    }
}

fn any_failure() -> impl Strategy<Value = F> {
    prop_oneof![
        Just(F::Unavailable),
        Just(F::Timeout),
        prop_oneof![
            Just(None),
            (0u32..120).prop_map(Some),
            (290u32..=310).prop_map(Some),
            Just(Some(999_999_999)),
            Just(Some(u32::MAX)),
        ]
        .prop_map(|r| F::RateLimited {
            retry_after_secs: r
        }),
        Just(F::AuthFailed),
        Just(F::TlsFailed),
        proptest::option::of(400u16..600).prop_map(|h| F::Other { http_status: h }),
        // Listed on purpose: 504 must occur often, not by chance.
        Just(F::Other {
            http_status: Some(504)
        }),
        Just(F::GatewayTimeout),
    ]
}

fn transient_failure() -> impl Strategy<Value = F> {
    prop_oneof![
        Just(F::Unavailable),
        Just(F::Timeout),
        prop_oneof![Just(None), (0u32..120).prop_map(Some)].prop_map(|r| F::RateLimited {
            retry_after_secs: r
        }),
        (500u16..600)
            .prop_filter("504 is final", |h| *h != 504)
            .prop_map(|h| F::Other {
                http_status: Some(h)
            }),
    ]
}

fn non_transient_failure() -> impl Strategy<Value = F> {
    prop_oneof![
        Just(F::AuthFailed),
        Just(F::TlsFailed),
        Just(F::Other { http_status: None }),
        (400u16..500).prop_map(|h| F::Other {
            http_status: Some(h)
        }),
        Just(F::Other {
            http_status: Some(504)
        }),
        Just(F::GatewayTimeout),
    ]
}

// (deadline s, attempt timeout s, retries) within the Req 29.1 ranges.
fn policy_parts() -> impl Strategy<Value = (u32, u32, u8)> {
    (1u32..=300, 1u32..=120, 0u8..=2).prop_map(|(d, a, r)| (d, a.min(d), r))
}

fn elapsed_strategy() -> impl Strategy<Value = Duration> {
    (
        0u64..400_000,
        prop_oneof![Just(0u64), Just(1u64), Just(999_999u64)],
    )
        .prop_map(|(ms, ns)| Duration::from_millis(ms) + Duration::from_nanos(ns))
}

// Simulated timer: runs the decision loop against a scripted provider.
// An attempt longer than the timeout is cut to a Timeout failure.
struct Run {
    history: Vec<R>,
    waits: Vec<Duration>,
    starts: Vec<Duration>,
    end: Duration,
    stop: Option<RetryStop>,
}

fn drive(policy: &RetryPolicy, script: &[(Option<F>, u64)]) -> Run {
    let timeout = policy.attempt_timeout();
    let mut run = Run {
        history: Vec::new(),
        waits: Vec::new(),
        starts: Vec::new(),
        end: Duration::ZERO,
        stop: None,
    };
    for _ in 0..8 {
        match next_step(policy, &run.history, run.end) {
            RetryStep::Stop(x) => {
                run.stop = Some(x);
                break;
            }
            RetryStep::Attempt { wait_before } => {
                run.waits.push(wait_before);
                run.end += wait_before;
                run.starts.push(run.end);
                let Some((outcome, ms)) = script.get(run.history.len()) else {
                    break;
                };
                let took = Duration::from_millis(*ms);
                // Same boundary as the mock: a response arriving exactly at
                // the timeout is delivered; only a later one is cut.
                if took > timeout {
                    run.end += timeout;
                    run.history.push(R::Failure(F::Timeout));
                } else {
                    run.end += took;
                    run.history.push(outcome.map_or(R::Response, R::Failure));
                }
            }
        }
    }
    run
}

property_tests! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    // Validates: Requirements 29.1, 29.3, 29.5, 29.6, 29.13, 29.15.
    #[test]
    fn matches_oracle(
        deadline in 1u32..=300,
        attempt_raw in 1u32..=120,
        retries in 0u8..=2,
        prefix in proptest::collection::vec(transient_failure(), 0..3),
        last in prop_oneof![Just(None), any_failure().prop_map(Some)],
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

#[test]
fn default_policy_matches_req_29_1_literals() {
    let (d, a, r) = REQ_29_1_DEFAULTS;
    let p = RetryPolicy::default();
    assert_eq!(p.deadline(), s(d));
    assert_eq!(p.attempt_timeout(), s(a));
    assert_eq!(p.retries(), r);
}

property_tests! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    // Validates: Requirements 29.12 (no retry after a complete response).
    #[test]
    fn response_is_never_retried(
        (d, a, r) in policy_parts(),
        prefix in proptest::collection::vec(any_failure(), 0..4),
        elapsed in elapsed_strategy(),
    ) {
        let policy = RetryPolicy::new(d, a, r).unwrap();
        let mut history: Vec<R> = prefix.into_iter().map(R::Failure).collect();
        history.push(R::Response);
        prop_assert_eq!(
            next_step(&policy, &history, elapsed),
            stop(RetryStop::Success)
        );
    }

    // Validates: Requirements 29.2, 29.15. A non-transient failure stops
    // at once, after any earlier transient ones, at any elapsed time.
    #[test]
    fn non_transient_stops_immediately(
        (d, a, r) in policy_parts(),
        prefix in proptest::collection::vec(transient_failure(), 0..3),
        last in non_transient_failure(),
        elapsed in elapsed_strategy(),
    ) {
        let policy = RetryPolicy::new(d, a, r).unwrap();
        let mut history: Vec<R> = prefix.into_iter().map(R::Failure).collect();
        history.push(R::Failure(last));
        prop_assert_eq!(
            next_step(&policy, &history, elapsed),
            stop(RetryStop::NonTransient(last))
        );
    }

    // Validates: Requirements 29.5, 29.6. Exhaustion depends on the count
    // only; the deadline stop arises only while retries remain.
    #[test]
    fn exhaustion_and_deadline_are_exclusive(
        (d, a, r) in policy_parts(),
        n in 1usize..=3,
        failures in proptest::collection::vec(transient_failure(), 3),
        elapsed in elapsed_strategy(),
    ) {
        let policy = RetryPolicy::new(d, a, r).unwrap();
        let history: Vec<R> = failures[..n].iter().copied().map(R::Failure).collect();
        let step = next_step(&policy, &history, elapsed);
        let retries_left = n <= usize::from(r);
        match step {
            RetryStep::Stop(RetryStop::Exhausted { attempts }) => {
                prop_assert!(!retries_left);
                prop_assert_eq!(usize::from(attempts), n);
            }
            RetryStep::Stop(RetryStop::DeadlineExceeded { attempts }) => {
                prop_assert!(retries_left);
                prop_assert_eq!(usize::from(attempts), n);
                prop_assert!((1..=3).contains(&attempts));
            }
            RetryStep::Attempt { .. } => prop_assert!(retries_left),
            other => prop_assert!(false, "unexpected {:?}", other),
        }
        if !retries_left {
            prop_assert_eq!(
                step,
                stop(RetryStop::Exhausted { attempts: n as u8 })
            );
        }
    }

    // Validates: Requirements 29.3, 29.4. Wait is the larger of the
    // schedule and an integer Retry-After; 0 leaves the schedule alone.
    #[test]
    fn retry_after_raises_wait(
        n in 1usize..=2,
        header in prop_oneof![
            Just(None),
            (0u32..=12).prop_map(Some),
            (290u32..=310).prop_map(Some),
            Just(Some(999_999_999)),
            Just(Some(u32::MAX)),
        ],
    ) {
        // Deadline 300 s, attempt timeout 1 s, elapsed 0: wait + 1 <= 300.
        let policy = RetryPolicy::new(300, 1, 2).unwrap();
        let mut history = vec![U; n - 1];
        history.push(rl(header));
        let scheduled = u64::try_from(REQ_29_4_WAIT_SECS[n - 1]).unwrap();
        let wait = scheduled.max(u64::from(header.unwrap_or(0)));
        let expected = if wait < 300 {
            go(wait)
        } else {
            stop(RetryStop::DeadlineExceeded { attempts: n as u8 })
        };
        prop_assert_eq!(next_step(&policy, &history, s(0)), expected);
    }

    // Validates: Requirements 29.5, 29.13. More elapsed time never turns a
    // refusal into a permission, and a permission keeps the same wait.
    #[test]
    fn decision_is_monotone_in_elapsed(
        (d, a, r) in policy_parts(),
        failures in proptest::collection::vec(transient_failure(), 1..=3),
        e1 in elapsed_strategy(),
        e2 in elapsed_strategy(),
    ) {
        let policy = RetryPolicy::new(d, a, r).unwrap();
        let history: Vec<R> = failures.into_iter().map(R::Failure).collect();
        let (lo, hi) = if e1 <= e2 { (e1, e2) } else { (e2, e1) };
        if let RetryStep::Attempt { wait_before } = next_step(&policy, &history, hi) {
            prop_assert_eq!(
                next_step(&policy, &history, lo),
                RetryStep::Attempt { wait_before }
            );
        }
    }

    // Validates: Requirements 29.1, 29.4, 29.5, 29.6, 29.13, 29.15, 23.17.
    // Whole runs over a simulated timer: waits follow the schedule, no
    // attempt can end after the deadline, one terminal stop, one failed
    // attempt per Warning.
    #[test]
    fn simulated_runs_obey_policy(
        (d, a, r) in policy_parts(),
        script in proptest::collection::vec(
            (
                prop_oneof![
                    1 => Just(None),
                    5 => any_failure().prop_map(Some),
                ],
                0u64..130_000,
            ),
            3,
        ),
    ) {
        let policy = RetryPolicy::new(d, a, r).unwrap();
        let run = drive(&policy, &script);
        let n = run.history.len();
        prop_assert!(n >= 1 && n <= usize::from(r) + 1);
        prop_assert!(run.end <= s(u64::from(d)));
        for start in &run.starts {
            prop_assert!(*start + s(u64::from(a)) <= s(u64::from(d)));
        }
        prop_assert_eq!(run.waits[0], Duration::ZERO);
        for i in 1..run.waits.len() {
            let scheduled = s(u64::try_from(REQ_29_4_WAIT_SECS[i - 1]).unwrap());
            let header = match run.history[i - 1] {
                R::Failure(F::RateLimited { retry_after_secs: Some(h) }) => s(u64::from(h)),
                _ => Duration::ZERO,
            };
            prop_assert_eq!(run.waits[i], scheduled.max(header));
        }
        let last = run.history[n - 1];
        let failed = run.history.iter().filter(|x| matches!(x, R::Failure(_))).count();
        match run.stop {
            Some(RetryStop::Success) => {
                prop_assert_eq!(last, R::Response);
                prop_assert_eq!(failed, n - 1);
            }
            Some(RetryStop::NonTransient(f)) => {
                prop_assert_eq!(last, R::Failure(f));
                prop_assert!(!transient(&f));
            }
            Some(RetryStop::Exhausted { attempts }) => {
                prop_assert_eq!(usize::from(attempts), n);
                prop_assert_eq!(n, usize::from(r) + 1);
                prop_assert_eq!(failed, n);
            }
            Some(RetryStop::DeadlineExceeded { attempts }) => {
                prop_assert_eq!(usize::from(attempts), n);
                prop_assert!(n <= usize::from(r));
                prop_assert_eq!(failed, n);
            }
            None => prop_assert!(false, "run did not stop"),
        }
    }
}

#[test]
fn drive_delivers_a_response_exactly_at_the_timeout_and_cuts_a_later_one() {
    let policy = RetryPolicy::default();
    let at = drive(&policy, &[(None, 20_000)]);
    assert_eq!(at.history, vec![R::Response]);
    assert_eq!(at.end, s(20));

    let late = drive(&policy, &[(None, 20_001), (None, 1)]);
    assert_eq!(late.history[0], R::Failure(F::Timeout));
    assert_eq!(late.starts[0], Duration::ZERO);
    assert_eq!(late.starts[1], s(22));
}
