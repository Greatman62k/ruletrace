//! Req 29.3: Retry-After parsing and HTTP classification, with expected
//! values derived by hand from the requirement text.

use crate::{failure_from_http, parse_retry_after};
use proptest::prelude::*;
use rt_ai_adapter::{next_step, AttemptRecord, ProviderFailure, RetryPolicy, RetryStep, RetryStop};
use std::time::Duration;

#[test]
fn accepts_one_to_nine_ascii_digits() {
    let table: [(&str, Option<u32>); 8] = [
        ("3", Some(3)),
        ("0", Some(0)),
        ("007", Some(7)),
        ("120", Some(120)),
        ("999999999", Some(999_999_999)),
        ("000000009", Some(9)),
        ("100000000", Some(100_000_000)),
        ("000000000", Some(0)),
    ];
    for (text, expected) in table {
        assert_eq!(parse_retry_after(text), expected, "{text:?}");
    }
}

#[test]
fn treats_every_other_syntax_as_absent() {
    let absent = [
        "",
        "1000000000",
        "0000000000",
        "12345678901234567890",
        " 3",
        "3 ",
        "3\n",
        "+3",
        "-3",
        "3.0",
        "1e3",
        "0x10",
        "3,4",
        "\u{663}",
        "Wed, 21 Oct 2015 07:28:00 GMT",
        "Wednesday, 21-Oct-15 07:28:00 GMT",
    ];
    for text in absent {
        assert_eq!(parse_retry_after(text), None, "{text:?}");
    }
}

#[test]
fn header_is_read_only_for_status_429() {
    let rl = |secs| ProviderFailure::RateLimited {
        retry_after_secs: secs,
    };
    assert_eq!(failure_from_http(429, Some("3")), rl(Some(3)));
    assert_eq!(failure_from_http(429, Some("0")), rl(Some(0)));
    assert_eq!(failure_from_http(429, None), rl(None));
    assert_eq!(
        failure_from_http(429, Some("Wed, 21 Oct 2015 07:28:00 GMT")),
        rl(None)
    );
    assert_eq!(failure_from_http(429, Some("1000000000")), rl(None));
    for status in [301_u16, 400, 401, 404, 500, 503, 599] {
        assert_eq!(
            failure_from_http(status, Some("30")),
            ProviderFailure::Other {
                http_status: Some(status)
            },
            "status {status}"
        );
    }
}

#[test]
fn a_504_is_final_and_the_neighbouring_5xx_are_transient() {
    let gateway = failure_from_http(504, Some("30"));
    assert_eq!(gateway, ProviderFailure::GatewayTimeout);
    assert!(!gateway.is_transient());
    for status in [500_u16, 501, 502, 503, 505, 599] {
        assert!(failure_from_http(status, None).is_transient(), "{status}");
    }
}

#[test]
fn a_scripted_other_504_cannot_be_retried() {
    let scripted = ProviderFailure::Other {
        http_status: Some(504),
    };
    assert!(!scripted.is_transient());
    assert_eq!(
        after(scripted, 0),
        RetryStep::Stop(rt_ai_adapter::RetryStop::NonTransient(scripted))
    );
}

fn after(failure: ProviderFailure, elapsed_s: u64) -> RetryStep {
    next_step(
        &RetryPolicy::default(),
        &[AttemptRecord::Failure(failure)],
        Duration::from_secs(elapsed_s),
    )
}

fn wait(secs: u64) -> RetryStep {
    RetryStep::Attempt {
        wait_before: Duration::from_secs(secs),
    }
}

// Default policy: deadline 60 s, attempt timeout 20 s, first wait 2 s.
#[test]
fn parsed_header_drives_the_wait_through_the_retry_decision() {
    let from = |status, header| failure_from_http(status, header);
    // 0 leaves the 2 s schedule unchanged; 1 is below it; 5 exceeds it.
    assert_eq!(after(from(429, Some("0")), 0), wait(2));
    assert_eq!(after(from(429, Some("1")), 0), wait(2));
    assert_eq!(after(from(429, Some("5")), 0), wait(5));
    // A date, a bad syntax and a header on a 503 are absent: 2 s.
    assert_eq!(
        after(from(429, Some("Wed, 21 Oct 2015 07:28:00 GMT")), 0),
        wait(2)
    );
    assert_eq!(after(from(429, Some("5s")), 0), wait(2));
    assert_eq!(after(from(503, Some("30")), 0), wait(2));
}

#[test]
fn parsed_header_value_is_bounded_by_the_deadline() {
    let fits = failure_from_http(429, Some("40"));
    // Remaining 60 s equals wait 40 s plus timeout 20 s: permitted.
    assert_eq!(after(fits, 0), wait(40));
    // One second later, 59 s remain and 60 s are needed.
    assert_eq!(
        after(fits, 1),
        RetryStep::Stop(RetryStop::DeadlineExceeded { attempts: 1 })
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    // Built from a number, so the expected value is not computed by parsing.
    #[test]
    fn padded_numbers_of_up_to_nine_digits_round_trip(n in 0_u32..1_000_000_000, pad in 0_usize..9) {
        let digits = n.to_string();
        let zeros = pad % (10 - digits.len());
        let text = format!("{}{}", "0".repeat(zeros), digits);
        prop_assert!(text.len() <= 9);
        prop_assert_eq!(parse_retry_after(&text), Some(n));
    }

    #[test]
    fn ten_or_more_digits_are_absent(head in 1_u8..=9, tail in "[0-9]{9,30}") {
        let text = format!("{head}{tail}");
        prop_assert_eq!(parse_retry_after(&text), None);
    }

    #[test]
    fn one_foreign_character_makes_the_header_absent(
        n in 0_u32..100_000_000,
        at in 0_usize..9,
        junk in prop::sample::select(vec![' ', '+', '-', '.', 'e', 'x', ',', '\n', '\u{663}', 'G']),
    ) {
        let mut chars: Vec<char> = n.to_string().chars().collect();
        let index = at % (chars.len() + 1);
        chars.insert(index, junk);
        let text: String = chars.into_iter().collect();
        prop_assert_eq!(parse_retry_after(&text), None);
    }
}
