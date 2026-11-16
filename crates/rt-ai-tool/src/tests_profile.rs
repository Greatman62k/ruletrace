//! Provider_Profile tests (task 10.12, Req 35.5, 35.11). Every expected
//! value is written by hand from `tools/decisions/UD-37.md` and the vendor
//! pages that `docs/provider-profiles.md` lists.

use crate::profile::{
    profile_for, Limit, SchemaMode, META, MOCK_REPLAY, MODELS_PATH, OPENAI, PROFILES,
    RESPONSES_PATH,
};
use rt_ai_adapter::{Protocol, Tri};

const PROFILE_SOURCE: &str = include_str!("profile.rs");
const PROFILE_DOCS: &str = include_str!("../../../docs/provider-profiles.md");

#[test]
fn the_meta_profile_states_the_documented_constants() {
    let p = &META;
    assert_eq!(p.id, "meta");
    assert_eq!(p.protocol, Protocol::OpenAiResponses);
    assert_eq!(p.official_host, Some("api.meta.ai"));
    assert_eq!(p.schema_mode, SchemaMode::JsonSchemaStrict);
    assert_eq!(p.capabilities.structured_output, Tri::Yes);
    assert_eq!(p.capabilities.reasoning_effort, Tri::Yes);
    assert_eq!(p.recursive_schema, Tri::No);
    assert_eq!(p.max_schema_depth, Some(10));
    assert_eq!(p.max_schema_properties, Some(5000));
    assert_eq!(p.store_control, Tri::Yes);
    assert_eq!(p.model_discovery, Tri::Yes);
    assert_eq!(p.context_window, Limit::Unknown);
    assert_eq!(p.output_limit, Limit::Unknown);
    assert_eq!(
        p.reasoning_efforts,
        ["minimal", "low", "medium", "high", "xhigh", "max"]
    );
}

#[test]
fn only_the_meta_profile_limits_max_to_the_three_documented_models() {
    assert_eq!(
        META.max_effort_models,
        Some(["muse-spark-1.3", "muse-ember-1.0", "muse-wonder-1.0"].as_slice())
    );
    assert_eq!(OPENAI.max_effort_models, None);
    assert_eq!(MOCK_REPLAY.max_effort_models, None);
}

#[test]
fn the_meta_profile_does_not_accept_none() {
    assert!(!META.reasoning_efforts.contains(&"none"));
    assert!(!META.reasoning_efforts.is_empty());
}

#[test]
fn the_openai_profile_leaves_effort_and_usage_unknown() {
    let p = &OPENAI;
    assert_eq!(p.id, "openai");
    assert_eq!(p.official_host, Some("api.openai.com"));
    assert_eq!(p.protocol, Protocol::OpenAiResponses);
    assert_eq!(p.capabilities.structured_output, Tri::Yes);
    assert_eq!(p.capabilities.reasoning_effort, Tri::Unknown);
    assert_eq!(p.capabilities.usage_reporting, Tri::Unknown);
    assert!(p.reasoning_efforts.is_empty());
    assert_eq!(p.store_control, Tri::Unknown);
    assert_eq!(p.model_discovery, Tri::Unknown);
    assert_eq!(p.max_schema_depth, Some(10));
    assert_eq!(p.max_schema_properties, Some(5000));
}

#[test]
fn the_mock_replay_profile_matches_the_replay_provider_and_has_no_host() {
    let p = &MOCK_REPLAY;
    assert_eq!(p.official_host, None);
    assert_eq!(p.protocol, Protocol::MockReplay);
    assert_eq!(p.capabilities.structured_output, Tri::Yes);
    // Changed on 2026-11-16 from No: an effort is part of the
    // Request_Identifier, so a recording made under it must replay.
    assert_eq!(p.capabilities.reasoning_effort, Tri::Yes);
    assert_eq!(p.capabilities.usage_reporting, Tri::No);
}

#[test]
fn a_profile_is_found_by_its_exact_identifier_only() {
    assert_eq!(profile_for("meta").map(|p| p.id), Some("meta"));
    assert_eq!(profile_for("openai").map(|p| p.id), Some("openai"));
    assert_eq!(
        profile_for("mock-replay").map(|p| p.id),
        Some("mock-replay")
    );
    for other in [
        "Meta",
        "OPENAI",
        "meta ",
        "",
        "anthropic",
        "mock",
        "meta-compatible",
    ] {
        assert!(profile_for(other).is_none(), "{other:?}");
    }
    assert_eq!(PROFILES.len(), 3);
}

#[test]
fn the_endpoint_paths_and_the_output_budget_are_the_documented_values() {
    assert_eq!(RESPONSES_PATH, "/v1/responses");
    assert_eq!(MODELS_PATH, "/v1/models");
    for p in PROFILES {
        assert!(p.max_output_tokens >= 16, "{}", p.id);
    }
}

#[test]
fn every_real_profile_names_a_url_and_a_date_for_each_fact_in_the_documentation() {
    for p in [&META, &OPENAI] {
        assert!(!p.documentation.is_empty(), "{}", p.id);
        for fact in p.documentation {
            assert!(fact.url.starts_with("https://"), "{}", fact.url);
            assert!(fact.read_on.starts_with("2026-11"), "{}", fact.read_on);
            assert!(
                PROFILE_DOCS.contains(fact.url),
                "docs/provider-profiles.md lacks {}",
                fact.url
            );
        }
        assert!(PROFILE_DOCS.contains(p.id), "{}", p.id);
        let host = p.official_host.unwrap();
        assert!(PROFILE_DOCS.contains(host), "{host}");
    }
}

#[test]
fn the_profile_module_has_no_network_environment_file_or_credential_code() {
    for line in PROFILE_SOURCE.lines() {
        if line.starts_with("use ") {
            assert_eq!(line, "use rt_ai_adapter::{Capabilities, Protocol, Tri};");
        }
    }
    for banned in [
        "std::",
        "env::",
        "TcpStream",
        "Credential",
        "EnvReader",
        "read_credential",
        "unsafe",
        "static mut",
        "thread_local",
    ] {
        assert!(
            !PROFILE_SOURCE.contains(banned),
            "profile.rs contains {banned}"
        );
    }
}
