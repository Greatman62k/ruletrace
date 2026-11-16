//! Endpoint rules (Req 28.6, 28.18).

use crate::endpoint::{Endpoint, EndpointError as E};

#[test]
fn accepted_endpoints_and_their_urls() {
    let rows = [
        (
            "https://api.meta.ai",
            "https://api.meta.ai/v1/responses",
            "api.meta.ai",
            false,
        ),
        (
            "https://api.meta.ai/",
            "https://api.meta.ai/v1/responses",
            "api.meta.ai",
            false,
        ),
        (
            "HTTPS://API.Meta.AI",
            "https://API.Meta.AI/v1/responses",
            "api.meta.ai",
            false,
        ),
        (
            "http://127.0.0.1:8080",
            "http://127.0.0.1:8080/v1/responses",
            "127.0.0.1",
            true,
        ),
        (
            "http://127.1.2.3/base/",
            "http://127.1.2.3/base/v1/responses",
            "127.1.2.3",
            true,
        ),
        (
            "http://localhost:9",
            "http://localhost:9/v1/responses",
            "localhost",
            true,
        ),
        (
            "HTTP://LocalHost",
            "http://LocalHost/v1/responses",
            "localhost",
            true,
        ),
        ("http://[::1]:7", "http://[::1]:7/v1/responses", "::1", true),
        (
            "https://127.0.0.1:1",
            "https://127.0.0.1:1/v1/responses",
            "127.0.0.1",
            false,
        ),
    ];
    for (text, url, host, clear) in rows {
        let e = Endpoint::parse(text).unwrap_or_else(|e| panic!("{text}: {e:?}"));
        assert_eq!(e.url("/v1/responses"), url, "{text}");
        assert_eq!(e.host(), host, "{text}");
        assert_eq!(e.is_clear_text(), clear, "{text}");
    }
}

// Wave 3 issue 10: no trailing slash survives, so the request path never
// starts with `//`.
#[test]
fn every_trailing_slash_is_removed_from_the_base() {
    let rows = [
        ("https://h//", "https://h/v1/responses"),
        ("https://h///", "https://h/v1/responses"),
        ("https://h/", "https://h/v1/responses"),
        ("https://h/base//", "https://h/base/v1/responses"),
        ("https://h/a//b/", "https://h/a//b/v1/responses"),
    ];
    for (text, url) in rows {
        let e = Endpoint::parse(text).unwrap_or_else(|e| panic!("{text}: {e:?}"));
        assert_eq!(e.url("/v1/responses"), url, "{text}");
    }
}

#[test]
fn refused_endpoints() {
    let rows = [
        ("http://api.meta.ai", E::InsecureHost),
        ("http://example.com", E::InsecureHost),
        ("http://127.0.0.1.evil.com", E::InsecureHost),
        ("http://128.0.0.1", E::InsecureHost),
        ("http://127.0.0", E::InsecureHost),
        ("http://127.0.0.256", E::InsecureHost),
        ("http://10.0.0.1", E::InsecureHost),
        ("http://[::2]", E::InsecureHost),
        ("ftp://api.meta.ai", E::Scheme),
        ("api.meta.ai", E::Scheme),
        ("//api.meta.ai", E::Scheme),
        ("https://u@api.meta.ai", E::UserInfo),
        ("https://u:p@api.meta.ai", E::UserInfo),
        ("https://api.meta.ai?x=1", E::QueryOrFragment),
        ("https://api.meta.ai/#f", E::QueryOrFragment),
        ("https://", E::Authority),
        ("https://:443", E::Authority),
        ("https://api.meta.ai:", E::Authority),
        ("https://api.meta.ai:99999x", E::Authority),
        ("http://[::1]x", E::Authority),
        ("https://api.meta.ai/a b", E::Character),
        ("https://api.meta.ai\\x", E::Character),
        ("https://api.meta.ai\n", E::Character),
        ("https://api.meta.ai\u{200b}", E::Character),
        ("https://api.meta.ai/a\u{202e}b", E::Character),
        ("https://api\u{feff}.meta.ai", E::Character),
        ("https://api.meta.ai\u{ad}", E::Character),
        ("https://api.meta.ai/a\u{2060}", E::Character),
        ("https://api.meta.ai/\u{a0}", E::Character),
    ];
    for (text, want) in rows {
        assert_eq!(Endpoint::parse(text), Err(want), "{text:?}");
    }
}
