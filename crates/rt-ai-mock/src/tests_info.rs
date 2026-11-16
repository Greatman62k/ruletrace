//! Identity and capabilities of the mock providers (UD-37, Req 35.1,
//! 35.2). Expected values are written out, not read from the providers.

use crate::{MockProvider, MockScript, RecordedScript, ReplayProvider};
use rt_ai_adapter::{
    negotiate, AdapterInfo, AiAdapter, AttemptOutcome, Capabilities, Protocol, ProviderMeta,
    ProviderRequest, RequestLimits, Required, Tri, Unmet,
};
use rt_foundation::InputId;
use rt_json::Value;
use std::time::Duration;

fn id() -> InputId {
    InputId::new("script.json").unwrap()
}

fn request() -> ProviderRequest {
    ProviderRequest {
        task: "propose_rules".into(),
        model: "m1".into(),
        limits: RequestLimits {
            deadline_s: 60,
            attempt_s: 20,
            retries: 2,
            response_bytes: 1_048_576,
        },
        reasoning_effort: None,
        context: vec![("clauses".into(), Value::array(vec![]))],
    }
}

fn step_provider() -> MockProvider {
    let text = r#"{"format":"ruletrace-mock-script","version":1,"steps":[{"elapsed_ms":1,"result":{"response":{"completion":"complete","payload":"x"}}}]}"#;
    MockProvider::new(MockScript::parse(&id(), text.as_bytes()).unwrap())
}

fn replay_provider() -> ReplayProvider {
    let text = r#"{"format":"ruletrace-recorded-script","version":1,
        "provider":{"kind":"openai-responses","model":"gpt-x","adapter_version":"0.1.0"},
        "captured":"2025-01-02","entries":[]}"#;
    ReplayProvider::new(RecordedScript::parse(&id(), text.as_bytes()).unwrap())
}

// The replay provider declares a reasoning effort as supported, because the
// effort is part of the Request_Identifier that a recording is keyed by.
const REPLAY: Capabilities = Capabilities {
    structured_output: Tri::Yes,
    reasoning_effort: Tri::Yes,
    usage_reporting: Tri::No,
};

const SCHEMA_ONLY: Capabilities = Capabilities {
    structured_output: Tri::Yes,
    reasoning_effort: Tri::No,
    usage_reporting: Tri::No,
};

#[test]
fn the_step_provider_declares_its_identity_and_capabilities() {
    let p = step_provider();
    let identity = p.identity();
    assert_eq!(identity.provider.as_str(), "mock");
    assert_eq!(identity.protocol, Protocol::MockReplay);
    assert_eq!(identity.adapter_version, "mock-step-1");
    assert_eq!(p.capabilities(), SCHEMA_ONLY);
}

#[test]
fn the_replay_provider_declares_the_adapter_version_of_its_script() {
    let p = replay_provider();
    let identity = p.identity();
    assert_eq!(identity.provider.as_str(), "mock-replay");
    assert_eq!(identity.protocol, Protocol::MockReplay);
    assert_eq!(identity.adapter_version, "0.1.0");
    assert_eq!(p.capabilities(), REPLAY);
}

#[test]
fn a_response_carries_the_identity_and_nothing_reported() {
    let mut p = step_provider();
    let report = p.attempt(&request(), Duration::from_secs(20));
    let AttemptOutcome::Response(resp) = report.outcome else {
        panic!("expected a response");
    };
    let want = ProviderMeta::new(p.identity(), "m1");
    assert_eq!(resp.meta, want);
    assert_eq!(resp.meta.reported_model, None);
    assert_eq!(resp.meta.response_id, None);
    assert_eq!(resp.meta.usage, None);
}

#[test]
fn negotiation_against_a_mock_follows_its_declared_capabilities() {
    let need_schema = Required {
        structured_output: true,
        reasoning_effort: false,
    };
    let need_both = Required {
        structured_output: true,
        reasoning_effort: true,
    };
    let step = step_provider().capabilities();
    let replay = replay_provider().capabilities();
    for caps in [&step, &replay] {
        assert_eq!(negotiate(&need_schema, caps), Ok(()));
    }
    assert_eq!(
        negotiate(&need_both, &step),
        Err(vec![Unmet {
            capability: "reasoning_effort",
            value: Tri::No
        }])
    );
    assert_eq!(negotiate(&need_both, &replay), Ok(()));
}
