//! HTTP status mapping (Req 35.14, Req 29.3). Each row is written by hand
//! from the requirement text.

use crate::status::{failure_for, retry_after_secs};
use rt_ai_adapter::ProviderFailure as F;

#[test]
fn status_table() {
    let other = |s| F::Other {
        http_status: Some(s),
    };
    let rows: [(u16, F); 17] = [
        (401, F::AuthFailed),
        (403, F::AuthFailed),
        (400, other(400)),
        (402, other(402)),
        (404, other(404)),
        (408, other(408)),
        (422, other(422)),
        (301, other(301)),
        (302, other(302)),
        (307, other(307)),
        (500, F::Unavailable),
        (502, F::Unavailable),
        (503, F::Unavailable),
        (504, F::Unavailable),
        (599, F::Unavailable),
        (600, other(600)),
        (199, other(199)),
    ];
    for (status, want) in rows {
        assert_eq!(failure_for(status, &[]), want, "status {status}");
    }
}

#[test]
fn a_503_ignores_retry_after_and_a_429_reads_it() {
    assert_eq!(failure_for(503, &["60"]), F::Unavailable);
    assert_eq!(
        failure_for(429, &["60"]),
        F::RateLimited {
            retry_after_secs: Some(60)
        }
    );
    assert_eq!(
        failure_for(429, &[]),
        F::RateLimited {
            retry_after_secs: None
        }
    );
}

#[test]
fn retry_after_syntax_table() {
    let rows: [(&[&str], Option<u32>); 13] = [
        (&["1"], Some(1)),
        (&["0"], Some(0)),
        (&["60"], Some(60)),
        (&["999999999"], Some(999_999_999)),
        (&["1000000000"], None),
        (&[""], None),
        (&["-1"], None),
        (&["+5"], None),
        (&["1.5"], None),
        (&[" 5"], None),
        (&["Wed, 21 Oct 2026 07:28:00 GMT"], None),
        (&["5", "6"], None),
        (&[], None),
    ];
    for (values, want) in rows {
        assert_eq!(retry_after_secs(values), want, "{values:?}");
    }
}
