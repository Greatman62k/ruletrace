//! The live example configurations of the stand-in demonstration load
//! through the real `load_config` (task 10.7). The loader reads text only: it
//! reads no environment variable and opens no connection, so nothing here
//! reaches a network or needs a credential. The expectations are the values
//! the project documents for each provider.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rt_ai_tool::load_config;
use rt_foundation::InputId;
use std::path::Path;

fn example(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../demo/stand-in-ai/ai")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

fn load(name: &str, text: &str) -> rt_ai_tool::AiConfig {
    let input = InputId::new(name).unwrap();
    load_config(&input, text).unwrap_or_else(|d| panic!("{name}: {d:?}"))
}

#[test]
fn the_meta_example_loads_with_the_documented_values() {
    let text = example("meta.example.toml");
    let c = load("meta.example.toml", &text);
    assert_eq!(c.profile.id, "meta");
    assert_eq!(c.provider.name(), "openai-responses");
    assert_eq!(c.model, "muse-spark-1.3");
    assert_eq!(c.reasoning_effort.as_deref(), Some("low"));
    assert_eq!(c.credential_env_var.as_deref(), Some("MODEL_API_KEY"));
    let endpoint = c.endpoint.unwrap();
    assert_eq!(endpoint.url, "https://api.meta.ai");
    assert_eq!(endpoint.host, "api.meta.ai");
    assert_eq!(c.transmission_log, "transmission.log.jsonl");
}

#[test]
fn the_openai_example_loads_with_the_placeholder_model() {
    let text = example("openai.example.toml");
    let c = load("openai.example.toml", &text);
    assert_eq!(c.profile.id, "openai");
    assert_eq!(c.provider.name(), "openai-responses");
    assert_eq!(c.model, "REPLACE_WITH_YOUR_MODEL_ID");
    assert_eq!(c.reasoning_effort, None);
    assert_eq!(c.credential_env_var.as_deref(), Some("OPENAI_API_KEY"));
    let endpoint = c.endpoint.unwrap();
    assert_eq!(endpoint.url, "https://api.openai.com");
    assert_eq!(endpoint.host, "api.openai.com");
}

#[test]
fn the_openai_example_loads_with_a_model_of_the_users_choice() {
    let text = example("openai.example.toml").replace("REPLACE_WITH_YOUR_MODEL_ID", "my-model-1");
    assert_eq!(load("openai.example.toml", &text).model, "my-model-1");
}

#[test]
fn the_examples_name_the_credential_by_variable_only() {
    for name in ["meta.example.toml", "openai.example.toml"] {
        let text = example(name);
        let assigned: Vec<&str> = text
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .filter(|l| {
                let key = l
                    .split('=')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase();
                ["key", "secret", "token", "password", "credential"]
                    .iter()
                    .any(|w| key.contains(w))
            })
            .collect();
        assert_eq!(assigned.len(), 1, "{name}: {assigned:?}");
        assert!(assigned[0].starts_with("credential_env_var = \""), "{name}");
        let with_inline = format!("{text}\napi_key = \"sk-not-a-real-key\"\n");
        let refused = load_config(&InputId::new(name).unwrap(), &with_inline).unwrap_err();
        assert!(
            !refused.is_empty(),
            "{name}: an inline secret must be refused"
        );
    }
}

#[test]
fn an_example_with_a_foreign_host_or_a_disabled_flag_is_not_accepted() {
    let meta = example("meta.example.toml");
    let code = |text: &str| {
        let e = load_config(&InputId::new("x").unwrap(), text).unwrap_err();
        e.first().map(|d| d.code().to_string())
    };
    let foreign = meta.replace("https://api.meta.ai", "https://example.org");
    assert_eq!(code(&foreign).as_deref(), Some("RT-AI-0013"));
    let off = meta.replace("enabled = true", "enabled = false");
    assert_eq!(code(&off).as_deref(), Some("RT-AI-0001"));
}
