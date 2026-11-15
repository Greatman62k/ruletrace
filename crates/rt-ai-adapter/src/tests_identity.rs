//! Provider identity, protocol, usage and the metadata constructor
//! (UD-37, Req 35.1, 35.15).

use crate::{
    Identity, Protocol, ProviderId, ProviderIdError, ProviderMeta, Usage, MAX_PROVIDER_ID_LEN,
};

#[test]
fn provider_ids_that_the_profiles_use_are_valid() {
    for ok in [
        "meta",
        "openai",
        "mock-replay",
        "mock",
        "a",
        "a1-b2",
        "x-1-y",
    ] {
        assert_eq!(ProviderId::new(ok).unwrap().as_str(), ok);
    }
    let longest = format!("a{}", "b".repeat(MAX_PROVIDER_ID_LEN - 1));
    assert_eq!(ProviderId::new(&longest).unwrap().as_str(), longest);
}

#[test]
fn each_invalid_provider_id_gives_its_own_error() {
    let too_long = format!("a{}", "b".repeat(MAX_PROVIDER_ID_LEN));
    let cases: [(&str, ProviderIdError); 13] = [
        ("", ProviderIdError::Empty),
        (&too_long, ProviderIdError::TooLong),
        ("Meta", ProviderIdError::BadStart),
        ("1meta", ProviderIdError::BadStart),
        ("-meta", ProviderIdError::BadStart),
        ("éa", ProviderIdError::BadStart),
        ("me ta", ProviderIdError::BadCharacter),
        ("me_ta", ProviderIdError::BadCharacter),
        ("meta.ai", ProviderIdError::BadCharacter),
        ("metA", ProviderIdError::BadCharacter),
        ("mé", ProviderIdError::BadCharacter),
        ("meta/", ProviderIdError::BadCharacter),
        ("meta-", ProviderIdError::TrailingHyphen),
    ];
    for (text, expected) in cases {
        assert_eq!(ProviderId::new(text), Err(expected), "{text:?}");
    }
}

#[test]
fn protocol_names_round_trip_and_nothing_else_parses() {
    for (protocol, name) in [
        (Protocol::OpenAiResponses, "openai-responses"),
        (Protocol::MockReplay, "mock-replay"),
    ] {
        assert_eq!(protocol.as_str(), name);
        assert_eq!(Protocol::parse(name), Some(protocol));
    }
    for other in [
        "",
        "OpenAI-Responses",
        "openai",
        "responses",
        "openai-responses ",
    ] {
        assert_eq!(Protocol::parse(other), None, "{other:?}");
    }
}

fn identity() -> Identity {
    Identity::new(
        ProviderId::new("meta").unwrap(),
        Protocol::OpenAiResponses,
        "r1",
    )
}

#[test]
fn metadata_starts_with_nothing_reported() {
    let meta = ProviderMeta::new(identity(), "muse-spark-1.3");
    assert_eq!(meta.identity, identity());
    assert_eq!(meta.model, "muse-spark-1.3");
    assert_eq!(meta.reported_model, None);
    assert_eq!(meta.response_id, None);
    assert_eq!(meta.usage, None);
}

#[test]
fn reported_values_are_kept_apart_from_the_configured_model() {
    let usage = Usage {
        input_tokens: Some(12),
        output_tokens: None,
        total_tokens: None,
    };
    let meta = ProviderMeta::new(identity(), "configured")
        .with_reported_model("reported")
        .with_response_id("resp_1")
        .with_usage(usage);
    assert_eq!(meta.model, "configured");
    assert_eq!(meta.reported_model.as_deref(), Some("reported"));
    assert_eq!(meta.response_id.as_deref(), Some("resp_1"));
    assert_eq!(meta.identity, identity());
}

#[test]
fn usage_is_never_derived_from_other_counts() {
    // Input and output given, total absent: the total stays absent.
    let usage = Usage {
        input_tokens: Some(3),
        output_tokens: Some(4),
        total_tokens: None,
    };
    let kept = ProviderMeta::new(identity(), "m").with_usage(usage).usage;
    assert_eq!(kept, Some(usage));
    assert_eq!(kept.and_then(|u| u.total_tokens), None);
    // Only a total given: the parts stay absent.
    let total_only = Usage {
        total_tokens: Some(9),
        ..Usage::default()
    };
    let kept = ProviderMeta::new(identity(), "m")
        .with_usage(total_only)
        .usage;
    assert_eq!(
        (
            kept.and_then(|u| u.input_tokens),
            kept.and_then(|u| u.output_tokens)
        ),
        (None, None)
    );
    assert_eq!(
        Usage::default(),
        Usage {
            input_tokens: None,
            output_tokens: None,
            total_tokens: None
        }
    );
}
