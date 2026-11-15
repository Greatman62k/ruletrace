//! No source line of the crate offers a way to weaken TLS verification
//! (Req 28.6). A scan is a weak guard, so it only catches the obvious names.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

const SOURCES: [(&str, &str); 3] = [
    ("transport.rs", include_str!("../src/transport.rs")),
    ("adapter.rs", include_str!("../src/adapter.rs")),
    ("endpoint.rs", include_str!("../src/endpoint.rs")),
];

#[test]
fn no_verification_or_redirect_or_proxy_switch_is_used() {
    let code = |s: &str| {
        s.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    for (name, text) in SOURCES {
        let text = code(text);
        for forbidden in [
            "disable_verification",
            "dangerous",
            "danger(",
            "NoVerifier",
            "ServerCertVerifier",
            "PlatformVerifier",
            "root_certs",
            "Config::default",
            "new_with_defaults",
            "Agent::new(",
            "from_env",
            "std::env",
        ] {
            assert!(!text.contains(forbidden), "{name} contains {forbidden}");
        }
    }
}

#[test]
fn transport_pins_the_safe_settings() {
    let t = include_str!("../src/transport.rs");
    for needed in [
        "max_redirects(0)",
        "max_redirects_will_error(false)",
        ".proxy(None)",
        "http_status_as_error(false)",
        ".https_only(!call.clear_text)",
    ] {
        assert!(t.contains(needed), "{needed}");
    }
}
