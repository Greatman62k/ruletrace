//! AI_Configuration schema version 2 and the Req 35 checks (task 10.13).
//!
//! Every expected code, severity, exit code and Input_Location is written
//! by hand from Req 28.17 and Req 35. Line numbers refer to the fixed text
//! of [`V2`], so a line can be counted by eye.

use crate::profile::{META, OPENAI};
use crate::tests::{at, env_ok, no_env_read, one, run, with, MapEnv, BASE, CANARY};
use crate::{AiConfig, ProviderKind};
use rt_ai_adapter::Tri;
use rt_foundation::{Code, ExitCode, Severity};

// Line numbers: 1 schema_version, 2 enabled, 3 allow_document_text,
// 4 ai_identities, 5 transmission_log, 6 max_clauses_per_request,
// 8 [provider], 9 id, 10 protocol, 11 model, 12 endpoint,
// 13 credential_env_var, 14 reasoning_effort, 16 [limits], 17 to 20 limits.
pub(crate) const V2: &str = "schema_version = 2
enabled = true
allow_document_text = true
ai_identities = [\"meta\", \"muse-spark-1.3\"]
transmission_log = \"tx.jsonl\"
max_clauses_per_request = 20

[provider]
id = \"meta\"
protocol = \"openai-responses\"
model = \"muse-spark-1.3\"
endpoint = \"https://api.meta.ai\"
credential_env_var = \"MODEL_API_KEY\"
reasoning_effort = \"low\"

[limits]
total_deadline_secs = 120
attempt_timeout_secs = 90
max_retries = 0
max_response_bytes = 1048576
";

const V2_MOCK: &str = "schema_version = 2
enabled = true
script = \"recorded.json\"
allow_document_text = true
ai_identities = [\"mock-replay\"]
transmission_log = \"tx.jsonl\"

[provider]
id = \"mock-replay\"
protocol = \"mock-replay\"
model = \"model-1\"
";

pub(crate) fn v2(old: &str, new: &str) -> String {
    assert!(V2.contains(old), "{old}");
    V2.replace(old, new)
}

pub(crate) fn meta_env() -> MapEnv {
    MapEnv::new(&[("MODEL_API_KEY", CANARY)])
}

type Place = (Option<u32>, Option<u32>, Option<String>);

pub(crate) fn loc(line: u32, field: &str) -> Place {
    (Some(line), Some(1), Some(field.to_owned()))
}

fn missing(field: &str) -> Place {
    (None, None, Some(field.to_owned()))
}

fn essence(c: &AiConfig) -> String {
    let l = c.limits;
    format!(
        "{:?}",
        (
            (c.profile.id, c.provider, &c.model),
            (&c.endpoint, &c.credential_env_var, &c.script),
            (c.allow_document_text, &c.ai_identities),
            &c.transmission_log,
            (c.max_clauses_per_request, l.deadline_s, l.attempt_s),
            (l.retries, l.response_bytes),
            (&c.reasoning_effort, c.verify_model),
            (c.allow_provider_training, c.capabilities),
        )
    )
}

#[test]
fn a_version_2_meta_configuration_loads_with_the_values_as_written() {
    let env = meta_env();
    let ok = run(V2, &env).unwrap();
    assert_eq!(env.reads(), vec!["MODEL_API_KEY".to_owned()]);
    let c = ok.config;
    assert_eq!(c.profile.id, "meta");
    assert_eq!(c.provider, ProviderKind::OpenAiResponses);
    assert_eq!(c.model, "muse-spark-1.3");
    assert_eq!(c.endpoint.unwrap().host, "api.meta.ai");
    assert_eq!(c.credential_env_var.as_deref(), Some("MODEL_API_KEY"));
    assert_eq!(c.reasoning_effort.as_deref(), Some("low"));
    assert!(!c.verify_model);
    assert!(!c.allow_provider_training);
    assert_eq!(c.capabilities, META.capabilities);
    assert_eq!(c.ai_identities, ["meta", "muse-spark-1.3"]);
    let l = c.limits;
    assert_eq!(
        (l.deadline_s, l.attempt_s, l.retries, l.response_bytes),
        (120, 90, 0, 1_048_576)
    );
    assert_eq!(ok.credential.unwrap().expose(), CANARY);
}

#[test]
fn a_version_1_file_loads_with_the_same_values_as_its_version_2_equivalent() {
    let v2_openai = "schema_version = 2
enabled = true
allow_document_text = true
ai_identities = [\"openai-responses\", \"model-1\"]
transmission_log = \"tx.jsonl\"

[provider]
id = \"openai\"
protocol = \"openai-responses\"
model = \"model-1\"
endpoint = \"https://api.openai.com\"
credential_env_var = \"PROVIDER_CREDENTIAL\"
";
    let env = env_ok();
    let old = run(BASE, &env).unwrap().config;
    let new = run(v2_openai, &env_ok()).unwrap().config;
    assert_eq!(old.profile.id, "openai");
    assert_eq!(essence(&old), essence(&new));
}

#[test]
fn version_1_mock_replay_maps_to_the_mock_identifier_and_protocol() {
    let mock_v1 = "schema_version = 1
enabled = true
provider = \"mock-replay\"
model = \"model-1\"
script = \"recorded.json\"
allow_document_text = true
ai_identities = [\"mock-replay\"]
transmission_log = \"tx.jsonl\"
";
    let env = MapEnv::new(&[]);
    let old = run(mock_v1, &env).unwrap().config;
    let new = run(V2_MOCK, &env).unwrap().config;
    no_env_read(&env);
    assert_eq!(old.profile.id, "mock-replay");
    assert_eq!(old.provider, ProviderKind::MockReplay);
    assert_eq!(essence(&old), essence(&new));
}

#[test]
fn a_version_1_kind_that_is_not_built_in_is_still_an_unknown_provider() {
    let d = one(
        run(&with(("\"openai-responses\"", "\"meta\"")), &env_ok()),
        Code::AiProviderUnknown,
    );
    assert_eq!(at(&d), loc(3, "provider"));
}

#[test]
fn schema_versions_other_than_1_and_2_are_invalid() {
    for value in ["3", "0", "-1", "\"2\"", "2.0"] {
        let text = format!("schema_version = {value}\nenabled = false\n");
        let d = one(run(&text, &meta_env()), Code::AiConfigInvalid);
        assert_eq!(at(&d), loc(1, "schema_version"), "{value}");
    }
}

#[test]
fn the_shape_of_a_version_2_file_is_checked_field_by_field() {
    let cases: Vec<(String, Place)> = vec![
        (
            v2("id = \"meta\"\n", "id = \"meta\"\nbogus = 1\n"),
            loc(10, "provider.bogus"),
        ),
        (
            v2("max_retries = 0", "max_retries = 0\nbogus = 1"),
            loc(20, "limits.bogus"),
        ),
        (
            v2("[limits]\n", "[limits]\nmodel = \"x\"\n"),
            loc(17, "limits.model"),
        ),
        (v2("id = \"meta\"", "id = 5"), loc(9, "provider.id")),
    ];
    for (text, want) in cases {
        let d = one(run(&text, &meta_env()), Code::AiConfigInvalid);
        assert_eq!(at(&d), want, "{text}");
    }
}

#[test]
fn a_version_1_field_at_the_top_level_of_a_version_2_file_is_unknown() {
    let text = v2(
        "max_clauses_per_request = 20",
        "max_clauses_per_request = 20\nmodel = \"x\"",
    );
    let d = one(run(&text, &meta_env()), Code::AiConfigInvalid);
    assert_eq!(at(&d), loc(7, "model"));
}

#[test]
fn a_version_2_table_in_a_version_1_file_is_unknown() {
    let d = one(
        run(&format!("{BASE}[limits]\nmax_retries = 1\n"), &env_ok()),
        Code::AiConfigInvalid,
    );
    // The key of a table header starts after the opening bracket.
    assert_eq!(at(&d), (Some(10), Some(2), Some("limits".to_owned())));
}

#[test]
fn a_missing_provider_table_is_one_required_field_finding() {
    let text = V2.split("[provider]").next().unwrap().to_owned() + "[limits]\nmax_retries = 0\n";
    let d = one(run(&text, &meta_env()), Code::AiConfigInvalid);
    assert_eq!(at(&d), missing("provider"));
}

#[test]
fn a_provider_that_is_a_string_in_version_2_is_a_type_error_at_the_key() {
    let table = V2.split("[provider]").nth(1).unwrap();
    let rest = table.split("[limits]").nth(1).unwrap();
    let head = V2.split("[provider]").next().unwrap();
    let text = format!("{head}provider = \"meta\"\n\n[limits]{rest}");
    let d = one(run(&text, &meta_env()), Code::AiConfigInvalid);
    assert_eq!(at(&d), loc(8, "provider"));
}

#[test]
fn every_required_provider_field_is_reported_when_missing_while_enabled() {
    for (key, field) in [
        ("id = \"meta\"\n", "provider.id"),
        ("protocol = \"openai-responses\"\n", "provider.protocol"),
        ("model = \"muse-spark-1.3\"\n", "provider.model"),
        ("endpoint = \"https://api.meta.ai\"\n", "provider.endpoint"),
        (
            "credential_env_var = \"MODEL_API_KEY\"\n",
            "provider.credential_env_var",
        ),
    ] {
        let d = one(run(&v2(key, ""), &meta_env()), Code::AiConfigInvalid);
        assert_eq!(at(&d), missing(field), "{key}");
    }
}

#[test]
fn the_limits_of_version_2_are_range_checked_with_their_own_path() {
    for (old, new, line, field) in [
        (
            "max_retries = 0",
            "max_retries = 3",
            19,
            "limits.max_retries",
        ),
        (
            "total_deadline_secs = 120",
            "total_deadline_secs = 301",
            17,
            "limits.total_deadline_secs",
        ),
        (
            "max_response_bytes = 1048576",
            "max_response_bytes = 1023",
            20,
            "limits.max_response_bytes",
        ),
        (
            "max_clauses_per_request = 20",
            "max_clauses_per_request = 101",
            6,
            "max_clauses_per_request",
        ),
    ] {
        let d = one(run(&v2(old, new), &meta_env()), Code::AiConfigLimitInvalid);
        assert_eq!(at(&d), loc(line, field), "{new}");
    }
}

// Model identifier rules (Req 35.10).

#[test]
fn the_model_identifier_is_one_to_two_hundred_scalar_values_without_controls() {
    let long_ok = "m".repeat(200);
    let long_bad = "m".repeat(201);
    let multibyte_ok = "é".repeat(200);
    for ok in [long_ok.as_str(), multibyte_ok.as_str(), "x"] {
        let loaded = run(
            &v2("muse-spark-1.3\"\nendpoint", &format!("{ok}\"\nendpoint")),
            &meta_env(),
        );
        assert!(loaded.is_ok(), "{} scalar values", ok.chars().count());
    }
    for bad in [long_bad.as_str(), "", "a\\u0007b", "a\\nb", "tab\\there"] {
        let text = v2("muse-spark-1.3\"\nendpoint", &format!("{bad}\"\nendpoint"));
        let d = one(run(&text, &meta_env()), Code::AiConfigInvalid);
        assert_eq!(at(&d), loc(11, "provider.model"), "{bad}");
    }
}

// Pair table (Req 35.10, 28.12, 35.7).

#[test]
fn the_pair_table_accepts_exactly_three_pairs() {
    for (id, protocol, host) in [
        ("meta", "openai-responses", "api.meta.ai"),
        ("openai", "openai-responses", "api.openai.com"),
    ] {
        let text = v2("id = \"meta\"", &format!("id = \"{id}\""))
            .replace("https://api.meta.ai", &format!("https://{host}"))
            .replace("openai-responses", protocol)
            .replace("reasoning_effort = \"low\"\n", "");
        assert!(run(&text, &meta_env()).is_ok(), "{id}");
    }
    assert!(run(V2_MOCK, &MapEnv::new(&[])).is_ok());
}

#[test]
fn every_other_pair_is_an_unknown_provider_at_the_field_that_is_wrong() {
    // (text, line, field)
    let mock = |id: &str, protocol: &str| {
        V2_MOCK
            .replace("id = \"mock-replay\"", &format!("id = \"{id}\""))
            .replace(
                "protocol = \"mock-replay\"",
                &format!("protocol = \"{protocol}\""),
            )
    };
    let cases = [
        (
            v2(
                "protocol = \"openai-responses\"",
                "protocol = \"mock-replay\"",
            ),
            10,
            "provider.protocol",
        ),
        (
            v2("id = \"meta\"", "id = \"openai\"")
                .replace("api.meta.ai", "api.openai.com")
                .replace(
                    "protocol = \"openai-responses\"",
                    "protocol = \"mock-replay\"",
                ),
            10,
            "provider.protocol",
        ),
        (
            v2(
                "protocol = \"openai-responses\"",
                "protocol = \"anthropic-messages\"",
            ),
            10,
            "provider.protocol",
        ),
        (
            v2(
                "protocol = \"openai-responses\"",
                "protocol = \"OpenAI-Responses\"",
            ),
            10,
            "provider.protocol",
        ),
        (v2("id = \"meta\"", "id = \"Meta\""), 9, "provider.id"),
        (
            v2("id = \"meta\"", "id = \"meta-compatible\""),
            9,
            "provider.id",
        ),
        (v2("id = \"meta\"", "id = \"anthropic\""), 9, "provider.id"),
        (v2("id = \"meta\"", "id = \"mock\""), 9, "provider.id"),
        (
            mock("mock-replay", "openai-responses"),
            10,
            "provider.protocol",
        ),
    ];
    for (text, line, field) in cases {
        let d = one(run(&text, &meta_env()), Code::AiProviderUnknown);
        assert_eq!(d.code().exit_code(), ExitCode::InvalidInput);
        assert_eq!(at(&d), loc(line, field), "{text}");
    }
}

#[test]
fn an_empty_provider_identifier_is_invalid_and_is_not_a_host_exception() {
    let d = one(
        run(&v2("id = \"meta\"", "id = \"\""), &meta_env()),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d), loc(9, "provider.id"));
}

#[test]
fn an_unknown_identifier_gets_no_host_exception_and_is_still_unknown() {
    // Req 35.7: a custom compatible endpoint is not a provider of this
    // version. The id check, not the host check, rejects it.
    let text = v2("id = \"meta\"", "id = \"custom\"").replace("api.meta.ai", "llm.example.org");
    let d = one(run(&text, &meta_env()), Code::AiProviderUnknown);
    assert_eq!(at(&d), loc(9, "provider.id"));
}

#[test]
fn provider_protocol_model_and_credential_variable_are_validated_independently() {
    // One fault each, nothing else wrong: each is found at its own field.
    let d = one(
        run(&v2("MODEL_API_KEY", "model_api_key"), &meta_env()),
        Code::AiConfigInlineSecret,
    );
    assert_eq!(at(&d), loc(13, "provider.credential_env_var"));
    let d = one(
        run(
            &v2("muse-spark-1.3\"\nendpoint", "\"\nendpoint"),
            &meta_env(),
        ),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d), loc(11, "provider.model"));
    let d = one(
        run(&v2("id = \"meta\"", "id = \"zzz\""), &meta_env()),
        Code::AiProviderUnknown,
    );
    assert_eq!(at(&d), loc(9, "provider.id"));
    let d = one(
        run(&v2("openai-responses", "zzz"), &meta_env()),
        Code::AiProviderUnknown,
    );
    assert_eq!(at(&d), loc(10, "provider.protocol"));
}

// Host restriction table (Req 35.6, 28.6, 28.18).

fn host_text(id_endpoint: &str) -> String {
    v2("https://api.meta.ai", id_endpoint)
}

#[test]
fn the_official_host_is_accepted_in_any_letter_case_as_a_bare_origin() {
    for url in [
        "https://api.meta.ai",
        "https://API.META.AI",
        "https://Api.Meta.Ai/",
        "HTTPS://api.meta.ai",
    ] {
        let ok = run(&host_text(url), &meta_env()).unwrap();
        assert_eq!(ok.config.endpoint.unwrap().host, "api.meta.ai", "{url}");
    }
}

#[test]
fn loopback_endpoints_are_the_only_other_accepted_hosts() {
    for url in [
        "http://127.0.0.1:8080",
        "http://127.9.9.9",
        "http://localhost",
        "http://LOCALHOST:1",
        "https://[::1]:9",
        "https://127.0.0.1",
    ] {
        assert!(run(&host_text(url), &meta_env()).is_ok(), "{url}");
    }
}

#[test]
fn every_other_host_is_refused_with_rt_ai_0013_at_the_endpoint_field() {
    for url in [
        "https://evil.example",
        "https://api.meta.ai.evil.example",
        "https://evil-api.meta.ai",
        "https://sub.api.meta.ai",
        "https://meta.ai",
        "https://api.meta.ai.",
        "https://1.2.3.4",
        "https://[2001:db8::1]",
        "https://0.0.0.0",
        "https://[::]",
        "https://xn--pi-meta-0ya.example",
        "https://\u{0430}pi.meta.ai",
        "https://api.meta.ai%2eevil.example",
        "https://api.openai.com",
        "https://localhost.evil.example",
        "https://127.0.0.1.evil.example",
        "https://apimeta.ai",
        "https://api-meta.ai",
        "https://api.meta.ai.attacker.test",
    ] {
        let d = one(
            run(&host_text(url), &meta_env()),
            Code::AiConfigEndpointHostNotPermitted,
        );
        assert_eq!(at(&d), loc(12, "provider.endpoint"), "{url}");
        assert_eq!(d.severity(), Severity::Error);
        assert!(!format!("{d:?}").contains(url.trim_start_matches("https://")));
    }
}

#[test]
fn the_official_host_of_one_provider_is_refused_for_the_other() {
    let d = one(
        run(&host_text("https://api.openai.com"), &meta_env()),
        Code::AiConfigEndpointHostNotPermitted,
    );
    assert_eq!(at(&d), loc(12, "provider.endpoint"));
    let openai = v2("id = \"meta\"", "id = \"openai\"").replace("reasoning_effort = \"low\"\n", "");
    let d = one(
        run(&openai, &meta_env()),
        Code::AiConfigEndpointHostNotPermitted,
    );
    assert_eq!(at(&d), loc(12, "provider.endpoint"));
}

// These forms are rejected by an earlier check of Req 28.17, so the result
// is that earlier code, not RT-AI-0013.
#[test]
fn a_user_name_a_query_a_scheme_or_a_backslash_is_caught_by_an_earlier_check() {
    for (url, code) in [
        (
            "https://api.meta.ai@evil.example",
            Code::AiConfigInlineSecret,
        ),
        ("https://u:p@api.meta.ai", Code::AiConfigInlineSecret),
        ("https://api.meta.ai/?x=1", Code::AiConfigInlineSecret),
        ("http://api.meta.ai", Code::AiConfigEndpointInsecure),
        ("http://evil.example", Code::AiConfigEndpointInsecure),
        ("ftp://api.meta.ai", Code::AiConfigEndpointInsecure),
        (
            "https://evil.example\\@api.meta.ai",
            Code::AiConfigInlineSecret,
        ),
        ("api.meta.ai", Code::AiConfigInvalid),
    ] {
        let escaped = url.replace('\\', "\\\\");
        let d = one(run(&host_text(&escaped), &meta_env()), code);
        assert_eq!(at(&d), loc(12, "provider.endpoint"), "{url}");
    }
}

#[test]
fn a_version_1_openai_file_is_held_to_the_official_host_too() {
    let d = one(
        run(
            &with(("https://api.openai.com", "https://api.example.org")),
            &env_ok(),
        ),
        Code::AiConfigEndpointHostNotPermitted,
    );
    assert_eq!(at(&d), loc(5, "endpoint"));
    let ok = run(
        &with(("https://api.openai.com", "https://API.OpenAI.com")),
        &env_ok(),
    );
    assert!(ok.is_ok());
    let ok = run(
        &with(("https://api.openai.com", "http://127.0.0.1:4000")),
        &env_ok(),
    );
    assert!(ok.is_ok());
}

#[test]
fn the_host_check_makes_no_environment_read() {
    let env = meta_env();
    drop(run(&host_text("https://evil.example"), &env));
    no_env_read(&env);
}

// Model guard (Req 35.8).

#[test]
fn a_model_with_contributor_in_any_letter_case_is_refused_without_the_flag() {
    for model in [
        "muse-spark-1.3-contributor",
        "MUSE-SPARK-1.3-CONTRIBUTOR",
        "Contributor",
        "xContributorx",
        "muse-CoNtRiBuToR",
    ] {
        let text = v2(
            "muse-spark-1.3\"\nendpoint",
            &format!("{model}\"\nendpoint"),
        );
        let env = meta_env();
        let d = one(run(&text, &env), Code::AiModelTrainingModeRefused);
        assert_eq!(at(&d), loc(11, "provider.model"), "{model}");
        no_env_read(&env);
        let off = text.replace(
            "reasoning_effort = \"low\"",
            "reasoning_effort = \"low\"\nallow_provider_training = false",
        );
        let d = one(run(&off, &meta_env()), Code::AiModelTrainingModeRefused);
        assert_eq!(at(&d), loc(11, "provider.model"), "{model}");
    }
}

#[test]
fn the_explicit_flag_allows_the_contributor_model() {
    let text = v2(
        "muse-spark-1.3\"\nendpoint",
        "muse-spark-1.3-contributor\"\nendpoint",
    )
    .replace(
        "reasoning_effort = \"low\"",
        "reasoning_effort = \"low\"\nallow_provider_training = true",
    );
    let ok = run(&text, &meta_env()).unwrap();
    assert!(ok.config.allow_provider_training);
    assert_eq!(ok.config.model, "muse-spark-1.3-contributor");
}

#[test]
fn a_model_that_only_resembles_the_word_is_accepted() {
    for model in [
        "muse-spark-1.3",
        "contribute",
        "contrib-utor",
        "con tributor",
        "contributo",
    ] {
        let text = v2(
            "muse-spark-1.3\"\nendpoint",
            &format!("{model}\"\nendpoint"),
        );
        assert!(run(&text, &meta_env()).is_ok(), "{model}");
    }
}

#[test]
fn the_model_guard_applies_to_every_provider_and_to_version_1() {
    let d = one(
        run(
            &with(("model-1\"\nendpoint", "model-1-Contributor\"\nendpoint")),
            &env_ok(),
        ),
        Code::AiModelTrainingModeRefused,
    );
    assert_eq!(at(&d), loc(4, "model"));
    let mock = V2_MOCK.replace("model-1", "model-1-contributor");
    let d = one(
        run(&mock, &MapEnv::new(&[])),
        Code::AiModelTrainingModeRefused,
    );
    assert_eq!(at(&d), loc(11, "provider.model"));
}

#[test]
fn allow_provider_training_must_be_a_boolean() {
    let text = v2(
        "reasoning_effort = \"low\"",
        "allow_provider_training = \"true\"",
    );
    let d = one(run(&text, &meta_env()), Code::AiConfigInvalid);
    assert_eq!(at(&d), loc(14, "provider.allow_provider_training"));
}

// Reasoning effort (Req 35.11).

#[test]
fn the_documented_meta_efforts_are_accepted() {
    for effort in ["minimal", "low", "medium", "high", "xhigh", "max"] {
        let ok = run(&v2("\"low\"", &format!("\"{effort}\"")), &meta_env()).unwrap();
        assert_eq!(ok.config.reasoning_effort.as_deref(), Some(effort));
    }
}

// Review wave 2, issue 2: the reasoning page documents `max` for three
// models only. A model that the page does not list, a future model and the
// Contributor mode are refused at the effort field. Other efforts are not
// restricted by model.
#[test]
fn max_is_accepted_only_for_the_three_models_that_the_reasoning_page_lists() {
    let with_model = |model: &str| {
        v2(
            "model = \"muse-spark-1.3\"",
            &format!("model = \"{model}\""),
        )
        .replace("\"low\"", "\"max\"")
    };
    for model in ["muse-spark-1.3", "muse-ember-1.0", "muse-wonder-1.0"] {
        let ok = run(&with_model(model), &meta_env()).unwrap();
        assert_eq!(
            ok.config.reasoning_effort.as_deref(),
            Some("max"),
            "{model}"
        );
    }
    for model in [
        "muse-spark-1.2",
        "muse-spark-1.1",
        "muse-spark-1.30",
        "Muse-Spark-1.3",
        "muse-spark-1.3-contributor",
        "model-x",
    ] {
        let env = meta_env();
        let text = with_model(model).replace(
            "reasoning_effort = \"max\"",
            "reasoning_effort = \"max\"\nallow_provider_training = true",
        );
        let d = one(run(&text, &env), Code::AiConfigInvalid);
        assert_eq!(at(&d), loc(14, "provider.reasoning_effort"), "{model}");
        no_env_read(&env);
    }
}

#[test]
fn a_missing_model_with_max_is_reported_once_as_a_missing_field() {
    let text = v2("model = \"muse-spark-1.3\"\n", "").replace("\"low\"", "\"max\"");
    let found = run(&text, &meta_env()).unwrap_err();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code(), Code::AiConfigInvalid);
    assert!(
        found[0].message().contains("model"),
        "{}",
        found[0].message()
    );
}

#[test]
fn the_other_efforts_are_not_restricted_by_model() {
    for model in ["muse-spark-1.1", "muse-spark-1.2", "model-x"] {
        for effort in ["minimal", "low", "medium", "high", "xhigh"] {
            let text = v2(
                "model = \"muse-spark-1.3\"",
                &format!("model = \"{model}\""),
            )
            .replace("\"low\"", &format!("\"{effort}\""));
            assert!(run(&text, &meta_env()).is_ok(), "{model} {effort}");
        }
    }
}

#[test]
fn none_and_unknown_efforts_are_refused_for_meta() {
    for effort in ["none", "None", "LOW", "extreme", "low ", "", "default", "1"] {
        let env = meta_env();
        let d = one(
            run(&v2("\"low\"", &format!("\"{effort}\"")), &env),
            Code::AiConfigInvalid,
        );
        assert_eq!(at(&d), loc(14, "provider.reasoning_effort"), "{effort:?}");
        no_env_read(&env);
    }
}

#[test]
fn a_meta_configuration_without_an_effort_loads_and_records_none() {
    let ok = run(&v2("reasoning_effort = \"low\"\n", ""), &meta_env()).unwrap();
    assert_eq!(ok.config.reasoning_effort, None);
}

#[test]
fn an_effort_for_openai_fails_closed_as_an_unsupported_capability() {
    let text = v2("id = \"meta\"", "id = \"openai\"").replace("api.meta.ai", "api.openai.com");
    for effort in ["low", "none", "max"] {
        let env = meta_env();
        let text = text.replace("\"low\"", &format!("\"{effort}\""));
        let d = one(run(&text, &env), Code::AiCapabilityUnsupported);
        assert_eq!(at(&d), loc(14, "provider.reasoning_effort"), "{effort}");
        assert!(
            d.message().contains("reasoning_effort = unknown"),
            "{}",
            d.message()
        );
        no_env_read(&env);
    }
}

#[test]
fn an_effort_for_the_replay_provider_is_an_unsupported_capability_with_value_no() {
    let text = V2_MOCK.replace(
        "model = \"model-1\"",
        "model = \"model-1\"\nreasoning_effort = \"low\"",
    );
    let d = one(run(&text, &MapEnv::new(&[])), Code::AiCapabilityUnsupported);
    assert_eq!(at(&d), loc(12, "provider.reasoning_effort"));
    assert!(
        d.message().contains("reasoning_effort = no"),
        "{}",
        d.message()
    );
}

#[test]
fn openai_without_an_effort_meets_the_required_capabilities() {
    let text = v2("id = \"meta\"", "id = \"openai\"")
        .replace("api.meta.ai", "api.openai.com")
        .replace("reasoning_effort = \"low\"\n", "");
    let ok = run(&text, &meta_env()).unwrap();
    assert_eq!(ok.config.capabilities, OPENAI.capabilities);
}

// Capability declarations (Req 35.4, 35.3).

fn with_declarations(lines: &str) -> String {
    format!("{V2}\n[capabilities]\n{lines}")
}

#[test]
fn a_declaration_may_restate_the_profile_value() {
    let ok = run(
        &with_declarations("structured_output = \"yes\"\nreasoning_effort = \"yes\"\nusage_reporting = \"unknown\"\n"),
        &meta_env(),
    )
    .unwrap();
    assert_eq!(ok.config.capabilities, META.capabilities);
}

#[test]
fn a_declaration_may_lower_yes_to_no_and_the_lowered_value_is_used() {
    let text = with_declarations("reasoning_effort = \"no\"\n")
        .replace("reasoning_effort = \"low\"\n", "");
    let ok = run(&text, &meta_env()).unwrap();
    assert_eq!(ok.config.capabilities.reasoning_effort, Tri::No);
    assert_eq!(ok.config.capabilities.structured_output, Tri::Yes);
}

#[test]
fn a_lowered_capability_that_the_run_requires_is_rt_ai_0011_at_the_requiring_field() {
    let env = meta_env();
    let d = one(
        run(&with_declarations("reasoning_effort = \"no\"\n"), &env),
        Code::AiCapabilityUnsupported,
    );
    assert_eq!(at(&d), loc(14, "provider.reasoning_effort"));
    assert!(
        d.message().contains("reasoning_effort = no"),
        "{}",
        d.message()
    );
    no_env_read(&env);
    let d = one(
        run(
            &with_declarations("structured_output = \"no\"\n"),
            &meta_env(),
        ),
        Code::AiCapabilityUnsupported,
    );
    assert_eq!(at(&d), loc(9, "provider.id"));
    assert!(
        d.message().contains("structured_output = no"),
        "{}",
        d.message()
    );
}

#[test]
fn a_declaration_that_raises_a_profile_value_is_invalid_at_the_declaration() {
    // Meta usage_reporting is unknown, so yes raises it. OpenAI
    // reasoning_effort is unknown, so yes raises it too.
    let d = one(
        run(
            &with_declarations("usage_reporting = \"yes\"\n"),
            &meta_env(),
        ),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d), loc(23, "capabilities.usage_reporting"));
    let openai = v2("id = \"meta\"", "id = \"openai\"").replace("api.meta.ai", "api.openai.com");
    let text = format!("{openai}\n[capabilities]\nreasoning_effort = \"yes\"\n");
    let d = one(run(&text, &meta_env()), Code::AiConfigInvalid);
    assert_eq!(at(&d), loc(23, "capabilities.reasoning_effort"));
}

#[test]
fn a_declaration_that_is_neither_a_restatement_nor_yes_to_no_is_refused() {
    // Unknown to no, and yes to unknown, are not the two permitted moves.
    let d = one(
        run(
            &with_declarations("usage_reporting = \"no\"\n"),
            &meta_env(),
        ),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d), loc(23, "capabilities.usage_reporting"));
    let d = one(
        run(
            &with_declarations("structured_output = \"unknown\"\n"),
            &meta_env(),
        ),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d), loc(23, "capabilities.structured_output"));
}

#[test]
fn declarations_with_a_wrong_type_value_or_name_are_invalid() {
    for (lines, field) in [
        (
            "structured_output = true\n",
            "capabilities.structured_output",
        ),
        (
            "structured_output = \"maybe\"\n",
            "capabilities.structured_output",
        ),
        (
            "structured_output = \"Yes\"\n",
            "capabilities.structured_output",
        ),
        ("streaming = \"yes\"\n", "capabilities.streaming"),
    ] {
        let d = one(
            run(&with_declarations(lines), &meta_env()),
            Code::AiConfigInvalid,
        );
        assert_eq!(at(&d), loc(23, field), "{lines}");
    }
}

// Inline secrets inside the version 2 tables.

#[test]
fn a_credential_like_key_in_a_version_2_table_is_an_inline_secret_at_that_key() {
    for (text, line) in [
        (
            v2(
                "model = \"muse-spark-1.3\"\n",
                "model = \"muse-spark-1.3\"\napi_key = \"zz\"\n",
            ),
            12,
        ),
        (v2("max_retries = 0", "max_retries = 0\nmy_token = 1"), 20),
        (format!("{V2}\n[capabilities]\nsecret = 1\n"), 23),
    ] {
        let d = one(run(&text, &meta_env()), Code::AiConfigInlineSecret);
        assert_eq!(at(&d), (Some(line), Some(1), None), "{text}");
        let shown = format!("{d:?}");
        assert!(
            !shown.contains("api_key") && !shown.contains("my_token"),
            "{shown}"
        );
    }
}

#[test]
fn a_credential_like_value_in_a_version_2_field_is_never_echoed() {
    for (old, new, line, field) in [
        (
            "muse-spark-1.3\"\nendpoint",
            "sk-canary99\"\nendpoint",
            11,
            "provider.model",
        ),
        (
            "\"low\"",
            "\"Bearer canary99\"",
            14,
            "provider.reasoning_effort",
        ),
        (
            "MODEL_API_KEY",
            "AKIAcanary99",
            13,
            "provider.credential_env_var",
        ),
        (
            "https://api.meta.ai",
            "https://u:canary99@api.meta.ai",
            12,
            "provider.endpoint",
        ),
    ] {
        let d = one(run(&v2(old, new), &meta_env()), Code::AiConfigInlineSecret);
        assert_eq!(at(&d), loc(line, field), "{new}");
        assert!(!format!("{d:?}").contains("canary99"), "{d:?}");
    }
}

#[test]
fn the_credential_value_never_appears_in_a_loaded_configuration() {
    let ok = run(V2, &meta_env()).unwrap();
    let shown = format!("{ok:?}");
    assert!(!shown.contains(CANARY), "{shown}");
    assert!(shown.contains("[REDACTED]"));
}

#[test]
fn a_missing_credential_is_reported_at_the_nested_field_without_a_value() {
    let env = MapEnv::new(&[("OTHER", CANARY)]);
    let d = one(run(V2, &env), Code::AiCredentialMissing);
    assert_eq!(at(&d), loc(13, "provider.credential_env_var"));
    assert_eq!(env.reads(), vec!["MODEL_API_KEY".to_owned()]);
    assert!(!format!("{d:?}").contains(CANARY));
}

// Platform neutrality: paths are opaque strings.

#[test]
fn windows_and_posix_path_forms_load_unchanged_and_are_never_interpreted() {
    for path in [
        "C:\\\\Users\\\\x\\\\tx.jsonl",
        "D:/a/ruletrace/tx.jsonl",
        "\\\\\\\\server\\\\share\\\\tx.jsonl",
        "../relative/tx.jsonl",
        "/var/log/tx.jsonl",
    ] {
        let text = v2(
            "transmission_log = \"tx.jsonl\"",
            &format!("transmission_log = \"{path}\""),
        );
        let ok = run(&text, &meta_env()).unwrap();
        assert_eq!(
            ok.config.transmission_log,
            path.replace("\\\\", "\\"),
            "{path}"
        );
    }
    let literal = v2(
        "transmission_log = \"tx.jsonl\"",
        "transmission_log = 'C:\\Users\\x\\tx.jsonl'",
    );
    let ok = run(&literal, &meta_env()).unwrap();
    assert_eq!(ok.config.transmission_log, "C:\\Users\\x\\tx.jsonl");
}

#[test]
fn ai_identities_are_carried_through_unchanged_in_both_versions() {
    let ok = run(V2, &meta_env()).unwrap();
    assert_eq!(ok.config.ai_identities, ["meta", "muse-spark-1.3"]);
    let old = run(BASE, &env_ok()).unwrap();
    assert_eq!(old.config.ai_identities, ["openai-responses", "model-1"]);
    let d = one(
        run(&v2("[\"meta\", \"muse-spark-1.3\"]", "[1]"), &meta_env()),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d), loc(4, "ai_identities"));
}

#[test]
fn the_check_capabilities_exit_code_and_severity_are_the_catalogue_values() {
    let found = run(
        &with_declarations("reasoning_effort = \"no\"\n"),
        &meta_env(),
    )
    .unwrap_err();
    assert_eq!(found[0].code(), Code::AiCapabilityUnsupported);
    assert_eq!(found[0].severity(), Severity::Error);
    assert_eq!(found[0].code().exit_code(), ExitCode::InvalidInput);
}
