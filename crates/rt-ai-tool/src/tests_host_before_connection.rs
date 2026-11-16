//! A host other than the official one is refused before any adapter, and so
//! any transport, exists (task 10.15, Req 35.6, task 10.13).
//!
//! The loopback stub cannot count a connection to a foreign host, because
//! the foreign host is not the stub. These tests count the other side: the
//! number of times the run asks for an adapter or a catalogue. No adapter
//! means no transport, so no connection can be made. Expected codes and
//! exit codes are written by hand.

use crate::tests::MapEnv;
use crate::tests_cli_rig::Fixture;
use crate::tests_e2e_responses::{config_text, errors, exit, Rig, CANARY, DEFAULT};
use crate::tests_gate_rig::run_providers;
use crate::{AiConfig, Credential, ProviderKind, Providers, StdFs};
use rt_ai_adapter::{AiAdapter, ModelCatalog};
use rt_foundation::{Code, Diagnostic};
use std::cell::Cell;
use std::ffi::OsString;

// Counts every request for an adapter or a catalogue and builds none.
#[derive(Default)]
struct Counting {
    adapters: Cell<usize>,
    catalogs: Cell<usize>,
}

impl Counting {
    fn refusal(config: &AiConfig) -> Diagnostic {
        Diagnostic::new(
            Code::InternalError,
            "the counting provider builds nothing".to_owned(),
            config.provider_at.clone(),
        )
    }
}

impl Providers for Counting {
    fn is_linked(&self, kind: ProviderKind) -> bool {
        kind == ProviderKind::OpenAiResponses
    }

    fn adapter<'a>(
        &self,
        config: &AiConfig,
        _: Option<&Credential>,
    ) -> Result<(Box<dyn AiAdapter + 'a>, String), Diagnostic> {
        self.adapters.set(self.adapters.get() + 1);
        Err(Self::refusal(config))
    }

    fn catalog<'a>(
        &self,
        config: &AiConfig,
        _: Option<&Credential>,
    ) -> Result<Box<dyn ModelCatalog + 'a>, Diagnostic> {
        self.catalogs.set(self.catalogs.get() + 1);
        Err(Self::refusal(config))
    }
}

fn with_endpoint(fixture: &Fixture, url: &str) {
    let text = config_text(fixture, "http://127.0.0.1:9", &DEFAULT);
    let edited = text.replace(
        "endpoint = \"http://127.0.0.1:9\"",
        &format!("endpoint = \"{url}\""),
    );
    assert_ne!(edited, text);
    fixture.write_config(&edited);
}

fn models_args(fixture: &Fixture) -> Vec<OsString> {
    vec![
        "models".into(),
        "--config".into(),
        fixture.path("ai.toml").into(),
    ]
}

// Endpoints that are not `https://api.meta.ai` for the `meta` profile, each
// with the hand-written code of its refusal: a user name is an inline secret
// (RT-AI-0003), a clear-text scheme on a non-loopback host is RT-AI-0004, and
// any other host or path is RT-AI-0013.
const FOREIGN: [(&str, &str); 6] = [
    ("https://evil.example", "RT-AI-0013"),
    ("https://api.openai.com", "RT-AI-0013"),
    ("https://api.meta.ai.evil.example", "RT-AI-0013"),
    ("https://evil.example/api.meta.ai", "RT-AI-0013"),
    ("http://api.meta.ai", "RT-AI-0004"),
    ("https://api.meta.ai@evil.example", "RT-AI-0003"),
];

#[test]
fn a_foreign_host_asks_for_no_adapter_in_propose() {
    for (url, code) in FOREIGN {
        let rig = Rig::new("host-propose", "http://127.0.0.1:9", &DEFAULT);
        with_endpoint(&rig.fixture, url);
        let env = MapEnv::new(&[("MODEL_API_KEY", CANARY)]);
        let counting = Counting::default();
        let ran = run_providers(&rig.fixture.consented(), &env, &counting, &StdFs);
        assert_eq!(exit(ran.code), 2, "{url}: {}", ran.stderr);
        assert_eq!(errors(&ran), [code], "{url}: {}", ran.stderr);
        assert_eq!(counting.adapters.get(), 0, "{url}");
        assert_eq!(counting.catalogs.get(), 0, "{url}");
        assert!(rig.log_lines().is_empty(), "{url}: no transmission entry");
        rig.assert_no_canary(&ran);
    }
}

#[test]
fn a_foreign_host_asks_for_no_catalogue_in_models() {
    for (url, code) in FOREIGN {
        let rig = Rig::new("host-models", "http://127.0.0.1:9", &DEFAULT);
        with_endpoint(&rig.fixture, url);
        let env = MapEnv::new(&[("MODEL_API_KEY", CANARY)]);
        let counting = Counting::default();
        let ran = run_providers(&models_args(&rig.fixture), &env, &counting, &StdFs);
        assert_eq!(exit(ran.code), 2, "{url}: {}", ran.stderr);
        assert_eq!(errors(&ran), [code], "{url}: {}", ran.stderr);
        assert_eq!(counting.adapters.get(), 0, "{url}");
        assert_eq!(counting.catalogs.get(), 0, "{url}");
        assert!(rig.log_lines().is_empty(), "{url}: no transmission entry");
        rig.assert_no_canary(&ran);
    }
}

// Control: the same rig with the official origin does reach the provider, so
// the counters above are able to see a request for one.
#[test]
fn the_official_origin_does_ask_for_an_adapter() {
    let rig = Rig::new("host-control", "http://127.0.0.1:9", &DEFAULT);
    with_endpoint(&rig.fixture, "https://api.meta.ai");
    let env = MapEnv::new(&[("MODEL_API_KEY", CANARY)]);
    let counting = Counting::default();
    let ran = run_providers(&rig.fixture.consented(), &env, &counting, &StdFs);
    assert_eq!(counting.adapters.get(), 1, "{}", ran.stderr);
    assert_eq!(counting.catalogs.get(), 0);
}
