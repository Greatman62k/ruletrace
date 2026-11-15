//! The endpoint of the provider identifiers `meta` and `openai` is exactly
//! `https://<official host>` (Req 35.6, decision of the orchestrator on
//! wave 2 of UD-37). Every expectation is written by hand.
//!
//! A user name and a query are caught by the earlier inline-secret check
//! of Req 28.17 and give RT-AI-0003, as pinned in `tests_config_v2.rs`.

use crate::tests::{at, one, run};
use crate::tests_config_v2::{loc, meta_env, v2};
use rt_foundation::{Code, Severity};

const META_URL: &str = "https://api.meta.ai";
const OPENAI_URL: &str = "https://api.openai.com";

fn meta_text(url: &str) -> String {
    v2(META_URL, url)
}

fn openai_text(url: &str) -> String {
    v2("id = \"meta\"", "id = \"openai\"")
        .replace("reasoning_effort = \"low\"\n", "")
        .replace(
            &format!("endpoint = \"{META_URL}\""),
            &format!("endpoint = \"{url}\""),
        )
}

#[test]
fn the_bare_official_origin_is_accepted_in_any_letter_case_with_an_optional_slash() {
    for url in [
        "https://api.meta.ai",
        "https://api.meta.ai/",
        "https://API.META.AI",
        "https://Api.Meta.Ai/",
        "HTTPS://api.meta.ai",
    ] {
        let ok = run(&meta_text(url), &meta_env()).unwrap();
        assert_eq!(ok.config.endpoint.unwrap().host, "api.meta.ai", "{url}");
    }
    for url in [
        "https://api.openai.com",
        "https://api.openai.com/",
        "https://API.OpenAI.com/",
    ] {
        let ok = run(&openai_text(url), &meta_env()).unwrap();
        assert_eq!(ok.config.endpoint.unwrap().host, "api.openai.com", "{url}");
    }
}

// Forms of the official host that are not the bare origin.
const NOT_BARE: [&str; 12] = [
    "{h}:443",
    "{h}:8443",
    "{h}:80",
    "{h}:443/",
    "{h}/v1",
    "{h}/v1/",
    "{h}/evil",
    "{h}//",
    "{h}/.",
    "{h}#frag",
    "{h}/#frag",
    "{h}#",
];

#[test]
fn a_port_a_path_or_a_fragment_is_refused_with_rt_ai_0013_for_meta() {
    for template in NOT_BARE {
        let url = template.replace("{h}", META_URL);
        let d = one(
            run(&meta_text(&url), &meta_env()),
            Code::AiConfigEndpointHostNotPermitted,
        );
        assert_eq!(at(&d), loc(12, "provider.endpoint"), "{url}");
        assert_eq!(d.severity(), Severity::Error);
        assert_eq!(d.code().id(), "RT-AI-0013");
    }
}

#[test]
fn a_port_a_path_or_a_fragment_is_refused_with_rt_ai_0013_for_openai() {
    for template in NOT_BARE {
        let url = template.replace("{h}", OPENAI_URL);
        let d = one(
            run(&openai_text(&url), &meta_env()),
            Code::AiConfigEndpointHostNotPermitted,
        );
        assert_eq!(at(&d), loc(12, "provider.endpoint"), "{url}");
        assert_eq!(d.severity(), Severity::Error);
    }
}

#[test]
fn a_trailing_dot_is_refused_with_or_without_a_slash() {
    for url in [
        "https://api.meta.ai.",
        "https://api.meta.ai./",
        "https://API.META.AI.",
    ] {
        let d = one(
            run(&meta_text(url), &meta_env()),
            Code::AiConfigEndpointHostNotPermitted,
        );
        assert_eq!(at(&d), loc(12, "provider.endpoint"), "{url}");
    }
}

#[test]
fn a_query_is_caught_by_the_earlier_inline_secret_check() {
    for url in [
        "https://api.meta.ai?x=1",
        "https://api.meta.ai/?x=1",
        "https://api.meta.ai?",
    ] {
        let d = one(
            run(&meta_text(url), &meta_env()),
            Code::AiConfigInlineSecret,
        );
        assert_eq!(at(&d), loc(12, "provider.endpoint"), "{url}");
    }
}

#[test]
fn a_loopback_endpoint_may_carry_a_port_and_a_path() {
    for url in [
        "http://127.0.0.1:8080",
        "http://127.0.0.1:8080/v1",
        "https://[::1]:9/",
        "http://localhost:1",
    ] {
        assert!(run(&meta_text(url), &meta_env()).is_ok(), "{url}");
        assert!(run(&openai_text(url), &meta_env()).is_ok(), "{url}");
    }
}
