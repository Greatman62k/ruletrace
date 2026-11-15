//! The Proposal_Store writer for format version 2 and the reported
//! metadata (UD-37, Req 35.15 to 35.17, task 10.14). Expected texts are
//! written out by hand.

use crate::tests_memfs::MemFs;
use crate::tests_redact_fragments::{canaries, leaked, redactor};
use crate::tests_rig::{payload, Harness, ROOT};
use crate::tests_run_support::VecSink;
use crate::{store_batch, Redactor, Reported, StoreMeta, StoreOutcome};
use rt_ai_adapter::{
    AiAdapter, AttemptOutcome, AttemptReport, Identity, MonotonicClock, Protocol, ProviderId,
    ProviderMeta, ProviderRequest, ProviderResponse, Usage,
};
use rt_ai_mock::SimulatedClock;
use rt_foundation::{Code, InputId};
use rt_proposals::{parse_batch, print_batch, Extras, Format, StoredUsage};
use std::path::Path;
use std::time::Duration;

fn accepted(h: &Harness, request: usize) -> crate::AcceptedBatch {
    let planned = &h.plan.requests()[request];
    let id = InputId::new("response-1").unwrap();
    let request_id = rt_hash::Sha256Hex::parse(planned.id().as_str()).unwrap();
    crate::accept_batch(&crate::AcceptInput {
        response_id: &id,
        request_id: &request_id,
        completion: rt_ai_adapter::Completion::Complete,
        payload: payload(request).as_bytes(),
        response_limit: crate::tests_response::LIMIT,
        models: &h.models,
        clauses: planned.clauses(),
        declarations: &h.rulebook,
        redactor: &h.redactor,
        revise_rule: None,
    })
    .unwrap_or_else(|d| panic!("rejected: {d:?}"))
}

fn stored_text(h: &Harness, meta: &StoreMeta) -> String {
    let fs = MemFs::new();
    let outcome = store_batch(&fs, Path::new(ROOT), &accepted(h, 0), meta).unwrap();
    assert_eq!(outcome.outcome, StoreOutcome::Stored);
    String::from_utf8(fs.read(&h.batch_file(0)).unwrap()).unwrap()
}

fn meta_with(h: &Harness, reported: Reported) -> StoreMeta {
    StoreMeta {
        reported,
        ..h.meta.clone()
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
fn a_new_batch_is_version_2_with_the_protocol_and_no_other_optional_member() {
    let h = Harness::new("");
    let text = stored_text(&h, &h.meta);
    assert!(
        text.starts_with(r#"{"format":"ruletrace-proposals","version":2,"request_id":""#),
        "{text}"
    );
    assert!(
        text.contains(
            r#""provider":"openai-responses","model":"model-1","adapter_version":"mock-1","protocol":"openai-responses","ai_identities":"#
        ),
        "{text}"
    );
    for absent in ["reported_model", "response_id", "usage", "null"] {
        assert!(!text.contains(absent), "{absent}: {text}");
    }
    let parsed = parse_batch(&InputId::new("b").unwrap(), text.as_bytes()).unwrap();
    assert_eq!(
        parsed.format,
        Format::V2(Extras {
            protocol: Some("openai-responses".to_owned()),
            ..Extras::default()
        })
    );
    assert_eq!(print_batch(&parsed), text);
}

#[test]
fn reported_values_are_stored_in_their_own_members_apart_from_the_configured_model() {
    let h = Harness::new("");
    let reported = Reported {
        model: Some("provider-said-model".to_owned()),
        response_id: Some("resp_42".to_owned()),
        usage: StoredUsage::new(None, None, Some(9)),
    };
    let text = stored_text(&h, &meta_with(&h, reported));
    assert!(
        text.contains(
            r#""model":"model-1","adapter_version":"mock-1","protocol":"openai-responses","reported_model":"provider-said-model","response_id":"resp_42","usage":{"total_tokens":9},"ai_identities":"#
        ),
        "{text}"
    );
    let parsed = parse_batch(&InputId::new("b").unwrap(), text.as_bytes()).unwrap();
    assert_eq!(parsed.model, "model-1");
}

#[test]
fn a_response_without_usage_stores_no_usage_and_no_cost() {
    let h = Harness::new("");
    let text = stored_text(&h, &meta_with(&h, Reported::default()));
    assert!(!text.contains("usage"));
    assert!(!text.contains("cost"));
    assert!(!text.contains("tokens"));
}

#[test]
fn a_version_1_batch_at_the_final_name_makes_a_version_2_write_a_no_op() {
    let h = Harness::new("");
    let fs = MemFs::new();
    let old = print_batch(&rt_proposals::Batch {
        format: Format::V1,
        ..parse_batch(
            &InputId::new("b").unwrap(),
            stored_text(&h, &h.meta).as_bytes(),
        )
        .unwrap()
    });
    assert!(old.contains(r#""version":1,"#));
    fs.plant(&h.batch_file(0), old.as_bytes());
    fs.clear_ops();
    let report = store_batch(&fs, Path::new(ROOT), &accepted(&h, 0), &h.meta).unwrap();
    let StoreOutcome::AlreadyStored(warning) = report.outcome else {
        panic!("expected AlreadyStored");
    };
    assert_eq!(warning.code(), Code::AiBatchAlreadyStored);
    assert_eq!(warning.code().id(), "RT-AI-0301");
    assert_eq!(fs.read(&h.batch_file(0)).unwrap(), old.as_bytes());
    assert_eq!(fs.ops(), vec![format!("exists {}", h.batch_file(0))]);
    assert_eq!(fs.files().len(), 1);
}

fn text_meta(model: Option<&str>, id: Option<&str>) -> ProviderMeta {
    let mut meta = ProviderMeta::new(identity(), "configured");
    if let Some(m) = model {
        meta = meta.with_reported_model(m);
    }
    if let Some(i) = id {
        meta = meta.with_response_id(i);
    }
    meta
}

#[test]
fn reported_text_with_a_control_character_or_over_256_scalar_values_is_omitted() {
    let plain = Redactor::no_credential();
    let edge = "é".repeat(256);
    let over = "é".repeat(257);
    let kept = Reported::from_meta(&text_meta(Some(&edge), Some("ok")), &plain);
    assert_eq!(kept.model.as_deref(), Some(edge.as_str()));
    assert_eq!(kept.response_id.as_deref(), Some("ok"));
    for bad in [
        over.as_str(),
        "",
        "a\nb",
        "a\u{7}b",
        "a\tb",
        "a\u{7f}b",
        "a\u{85}b",
    ] {
        let found = Reported::from_meta(&text_meta(Some(bad), Some(bad)), &plain);
        assert_eq!(found.model, None, "{bad:?}");
        assert_eq!(found.response_id, None, "{bad:?}");
    }
}

#[test]
fn reported_text_with_a_format_or_bidi_control_character_is_omitted() {
    let plain = Redactor::no_credential();
    // Unicode category Cf examples and every explicit bidi control.
    let mut bad: Vec<char> = vec!['\u{ad}', '\u{61c}', '\u{200b}', '\u{200d}', '\u{2060}'];
    bad.extend(['\u{feff}', '\u{e0001}', '\u{e0041}', '\u{1d173}']);
    // The first and last scalar of every other Cf block.
    for n in [
        0x600, 0x605, 0x6dd, 0x70f, 0x890, 0x891, 0x8e2, 0x180e, 0x200c, 0x2064, 0x206a, 0x206f,
        0xfff9, 0xfffb, 0x110bd, 0x110cd, 0x13430, 0x1343f, 0x1bca0, 0x1bca3, 0x1d17a, 0xe0020,
        0xe007f,
    ] {
        bad.push(char::from_u32(n).unwrap());
    }
    bad.extend(['\u{200e}', '\u{200f}']);
    bad.extend('\u{202a}'..='\u{202e}');
    bad.extend('\u{2066}'..='\u{2069}');
    for c in bad {
        let text = format!("gpt{c}x");
        let found = Reported::from_meta(&text_meta(Some(&text), Some(&text)), &plain);
        assert_eq!(found.model, None, "U+{:04X}", u32::from(c));
        assert_eq!(found.response_id, None, "U+{:04X}", u32::from(c));
    }
}

#[test]
fn reported_text_with_visible_non_ascii_characters_is_kept() {
    let plain = Redactor::no_credential();
    // None of these is category Cc or Cf: letters, a combining mark, a
    // no-break space, a line separator, an emoji and a private use scalar.
    for text in [
        "modèle-é",
        "日本語",
        "a\u{34f}b",
        "a\u{a0}b",
        "a\u{2028}b",
        "\u{1f600}",
        "\u{e000}",
        "\u{2065}x",
    ] {
        let found = Reported::from_meta(&text_meta(Some(text), Some(text)), &plain);
        assert_eq!(found.model.as_deref(), Some(text), "{text:?}");
        assert_eq!(found.response_id.as_deref(), Some(text), "{text:?}");
    }
}

#[test]
fn a_bidi_control_never_reaches_the_stored_file() {
    let h = Harness::new("");
    let text = "id\u{202e}evil";
    let reported = Reported::from_meta(
        &text_meta(Some(text), Some("ok")),
        &Redactor::no_credential(),
    );
    let stored = stored_text(&h, &meta_with(&h, reported));
    assert!(!stored.contains("reported_model"), "{stored}");
    assert!(!stored.contains('\u{202e}'), "{stored}");
    assert!(stored.contains(r#""response_id":"ok""#), "{stored}");
}

#[test]
fn usage_is_kept_as_reported_and_never_derived_or_invented() {
    let plain = Redactor::no_credential();
    let with = |u: Usage| Reported::from_meta(&text_meta(None, None).with_usage(u), &plain).usage;
    assert_eq!(with(Usage::default()), None);
    assert_eq!(
        with(Usage {
            input_tokens: Some(3),
            output_tokens: Some(4),
            total_tokens: None,
        }),
        StoredUsage::new(Some(3), Some(4), None)
    );
    assert_eq!(
        with(Usage {
            total_tokens: Some(9),
            ..Usage::default()
        }),
        StoredUsage::new(None, None, Some(9))
    );
    // A count above 2^53 - 1 cannot be written as an RT-JSON integer.
    assert_eq!(
        with(Usage {
            input_tokens: Some(9_007_199_254_740_992),
            output_tokens: Some(1),
            total_tokens: None,
        }),
        StoredUsage::new(None, Some(1), None)
    );
    assert_eq!(
        Reported::from_meta(&text_meta(None, None), &plain),
        Reported::default()
    );
}

#[test]
fn a_credential_in_a_reported_value_is_redacted_whole_and_in_fragments() {
    for c in canaries() {
        let model = format!("model {c} end");
        let id: String = format!("id-{}", c.chars().skip(2).take(12).collect::<String>());
        let found = Reported::from_meta(&text_meta(Some(&model), Some(&id)), &redactor(c));
        let model = found.model.unwrap();
        let id = found.response_id.unwrap();
        assert_eq!(leaked(&model, c), None, "{c}: {model}");
        assert_eq!(leaked(&id, c), None, "{c}: {id}");
        assert!(model.contains(crate::REDACTED), "{c}: {model}");
        assert!(id.contains(crate::REDACTED), "{c}: {id}");
    }
}

struct Reporting {
    meta: ProviderMeta,
    clock: SimulatedClock,
}

impl AiAdapter for Reporting {
    fn attempt(&mut self, _: &ProviderRequest, _: Duration) -> AttemptReport {
        AttemptReport {
            outcome: AttemptOutcome::Response(ProviderResponse {
                completion: rt_ai_adapter::Completion::Complete,
                payload: payload(0).into_bytes(),
                meta: self.meta.clone(),
            }),
            elapsed: Duration::ZERO,
            detail: None,
        }
    }

    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }
}

fn run_store(c: &str, redact: bool, usage: Option<Usage>) -> String {
    let mut h = Harness::new("");
    h.plan = crate::tests_rig::plan_of(&h.config, &[0]);
    if redact {
        h.redactor = redactor(c);
    }
    let mut meta = text_meta(Some(&format!("reported {c}")), Some(&format!("resp-{c}")));
    if let Some(u) = usage {
        meta = meta.with_usage(u);
    }
    let mut adapter = Reporting {
        meta,
        clock: SimulatedClock::new(),
    };
    let fs = MemFs::new();
    let report = h.run(&mut adapter, &fs, &VecSink::default());
    assert!(report.summary.is_complete(), "{:?}", report.diagnostics);
    String::from_utf8(fs.read(&h.batch_file(0)).unwrap()).unwrap()
}

#[test]
fn a_run_stores_the_reported_metadata_of_the_response_redacted() {
    for c in canaries() {
        let text = run_store(
            c,
            true,
            Some(Usage {
                input_tokens: None,
                output_tokens: Some(5),
                total_tokens: None,
            }),
        );
        assert_eq!(leaked(&text, c), None, "{c}: {text}");
        assert!(
            text.contains(r#""reported_model":"reported [REDACTED]""#),
            "{c}: {text}"
        );
        assert!(
            text.contains(r#""usage":{"output_tokens":5},"#),
            "{c}: {text}"
        );
        assert!(text.contains(r#""model":"model-1""#), "{c}: {text}");
    }
}

#[test]
fn the_leak_check_detects_a_credential_when_nothing_is_redacted() {
    for c in canaries() {
        let text = run_store(c, false, None);
        assert!(leaked(&text, c).is_some(), "{c}: {text}");
    }
}

#[test]
fn a_version_1_configuration_maps_in_memory_to_the_provider_id_and_protocol() {
    use crate::ProviderKind;
    assert_eq!(ProviderKind::OpenAiResponses.provider_id(), "openai");
    assert_eq!(ProviderKind::OpenAiResponses.protocol(), "openai-responses");
    assert_eq!(ProviderKind::MockReplay.provider_id(), "mock-replay");
    assert_eq!(ProviderKind::MockReplay.protocol(), "mock-replay");
}
