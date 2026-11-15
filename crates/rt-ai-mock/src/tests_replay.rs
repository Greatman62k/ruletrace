//! Recorded replay: lookup by Request_Identifier, miss detail, validation.

use crate::{Origin, RecordedScript, ReplayProvider, ScriptError};
use rt_ai_adapter::{
    AiAdapter, AttemptOutcome, Completion, ProviderFailure, ProviderRequest, RequestLimits,
};
use rt_foundation::InputId;
use rt_json::Value;
use std::time::Duration;

// sha256sum of the canonical bytes of `request()`, computed outside this
// code base (the same value the adapter tests use).
const ID_A: &str = "b98f2750b87a8a8a1776233fa312f81bc685e75f07d5a91b4562915a47ac7a1f";
// An identifier that no test request has.
const ID_OTHER: &str = "0000000000000000000000000000000000000000000000000000000000000001";
// `printf '%s' '{"clauses":[]}' | sha256sum`.
const HASH_PAYLOAD: &str = "c29f837cede9d344db69480690ec544f033bffc904ecf8fa02658d036f70e2af";
// `printf '%s' 'second' | sha256sum`.
const HASH_SECOND: &str = "16367aacb67a4a017c8da8ab95682ccb390863780f7114dda0a0e0c55644c7c4";

fn id() -> InputId {
    InputId::new("recorded.json").unwrap()
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
        context: vec![
            ("clauses".into(), Value::array(vec![])),
            ("revise".into(), Value::null()),
        ],
    }
}

fn file(entries: &str) -> String {
    format!(
        r#"{{"format":"ruletrace-recorded-script","version":1,
        "provider":{{"kind":"openai-responses","model":"gpt-x","adapter_version":"0.1.0"}},
        "captured":"2025-01-02","entries":[{entries}]}}"#
    )
}

fn entry(request_id: &str, origin: &str, hash: &str, payload: &str) -> String {
    format!(
        r#"{{"request_id":"{request_id}","origin":"{origin}","completion":"complete","response_sha256":"{hash}","payload":"{payload}"}}"#
    )
}

fn good() -> String {
    let a = entry(ID_A, "recorded", HASH_PAYLOAD, r#"{\"clauses\":[]}"#);
    let b = entry(ID_OTHER, "authored", HASH_SECOND, "second");
    file(&format!("{a},{b}"))
}

fn load(text: &str) -> Result<RecordedScript, ScriptError> {
    RecordedScript::parse(&id(), text.as_bytes())
}

fn invalid_path(text: &str) -> String {
    match load(text) {
        Err(ScriptError::Invalid(i)) => i.path,
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn matching_request_yields_the_stored_payload_with_zero_elapsed() {
    let mut p = ReplayProvider::new(load(&good()).unwrap());
    let report = p.attempt(&request(), Duration::from_secs(20));
    assert_eq!(report.elapsed, Duration::ZERO);
    let AttemptOutcome::Response(resp) = report.outcome else {
        panic!("expected a response");
    };
    assert_eq!(resp.payload, br#"{"clauses":[]}"#.to_vec());
    assert_eq!(resp.completion, Completion::Complete);
    assert_eq!(resp.meta.identity.provider.as_str(), "mock-replay");
    assert_eq!(resp.meta.model, "gpt-x");
    assert_eq!(report.detail, None);
    assert_eq!(p.clock().now(), Duration::ZERO);
}

#[test]
fn unknown_request_fails_naming_the_request_identifier() {
    let only_other = file(&entry(ID_OTHER, "recorded", HASH_SECOND, "second"));
    let mut p = ReplayProvider::new(load(&only_other).unwrap());
    let report = p.attempt(&request(), Duration::from_secs(20));
    assert_eq!(
        report.outcome,
        AttemptOutcome::Failure(ProviderFailure::Other { http_status: None })
    );
    assert_eq!(report.elapsed, Duration::ZERO);
    let expected = format!("no entry for request {ID_A}");
    assert_eq!(report.detail.as_deref(), Some(expected.as_str()));
    assert!(!ProviderFailure::Other { http_status: None }.is_transient());
}

#[test]
fn any_change_to_the_request_misses() {
    let mut p = ReplayProvider::new(load(&good()).unwrap());
    let mut changed = request();
    changed.limits.retries = 1;
    let report = p.attempt(&changed, Duration::from_secs(20));
    assert!(matches!(report.outcome, AttemptOutcome::Failure(_)));
    let detail = report.detail.clone().unwrap();
    assert!(detail.starts_with("no entry for request "));
    assert!(!detail.contains(ID_A));
    // A hit afterwards clears the detail.
    let hit = p.attempt(&request(), Duration::from_secs(20));
    assert!(matches!(hit.outcome, AttemptOutcome::Response(_)));
    assert_eq!(hit.detail, None);
    assert_eq!(p.attempts(), 2);
}

#[test]
fn request_without_canonical_bytes_is_a_final_failure() {
    let mut p = ReplayProvider::new(load(&good()).unwrap());
    let mut broken = request();
    broken.context.push(("model".into(), Value::string("dup")));
    let report = p.attempt(&broken, Duration::from_secs(20));
    assert_eq!(
        report.outcome,
        AttemptOutcome::Failure(ProviderFailure::Other { http_status: None })
    );
    assert!(report.detail.unwrap().contains("no canonical bytes"));
}

// Red/green: the consumer holds only `&mut dyn AiAdapter`, so the
// Request_Identifier must arrive through the report (RT-AI-0105).
#[test]
fn miss_detail_reaches_a_caller_holding_only_the_trait_object() {
    let only_other = file(&entry(ID_OTHER, "recorded", HASH_SECOND, "second"));
    let mut p = ReplayProvider::new(load(&only_other).unwrap());
    let adapter: &mut dyn AiAdapter = &mut p;
    let report = adapter.attempt(&request(), Duration::from_secs(20));
    let expected = format!("no entry for request {ID_A}");
    assert_eq!(report.detail.as_deref(), Some(expected.as_str()));
}

#[test]
fn parsed_script_exposes_metadata_origin_and_sorted_entries() {
    let s = load(&good()).unwrap();
    assert_eq!(s.provider_kind, "openai-responses");
    assert_eq!(s.model, "gpt-x");
    assert_eq!(s.adapter_version, "0.1.0");
    assert_eq!(s.captured, "2025-01-02");
    let got: Vec<(&str, Origin)> = s
        .entries()
        .map(|e| (e.request_id.as_str(), e.origin))
        .collect();
    assert_eq!(
        got,
        vec![(ID_OTHER, Origin::Authored), (ID_A, Origin::Recorded)]
    );
}

#[test]
fn hash_that_does_not_match_the_payload_is_rejected() {
    let bad = file(&entry(ID_A, "recorded", HASH_SECOND, r#"{\"clauses\":[]}"#));
    assert_eq!(invalid_path(&bad), "entries[0].response_sha256");
    let one_byte = file(&entry(
        ID_A,
        "recorded",
        HASH_PAYLOAD,
        r#"{\"clauses\":[ ]}"#,
    ));
    assert_eq!(invalid_path(&one_byte), "entries[0].response_sha256");
}

#[test]
fn malformed_ids_origins_completions_and_duplicates_are_rejected() {
    let upper = ID_A.to_uppercase();
    let short = &ID_A[..63];
    let cases = [
        (
            file(&entry(&upper, "recorded", HASH_SECOND, "second")),
            "entries[0].request_id",
        ),
        (
            file(&entry(short, "recorded", HASH_SECOND, "second")),
            "entries[0].request_id",
        ),
        (
            file(&entry(ID_A, "ai", HASH_SECOND, "second")),
            "entries[0].origin",
        ),
        (
            file(
                &entry(ID_A, "recorded", HASH_SECOND, "second")
                    .replace("\"complete\"", "\"refused\""),
            ),
            "entries[0].completion",
        ),
        (
            file(&format!(
                "{},{}",
                entry(ID_A, "recorded", HASH_SECOND, "second"),
                entry(ID_A, "authored", HASH_SECOND, "second")
            )),
            "entries[1].request_id",
        ),
    ];
    for (text, path) in cases {
        assert_eq!(invalid_path(&text), path);
    }
}

#[test]
fn schema_violations_are_rejected() {
    let good_entry = entry(ID_A, "recorded", HASH_SECOND, "second");
    let extra = good_entry.replace("\"payload\"", "\"note\":\"x\",\"payload\"");
    assert_eq!(invalid_path(&file(&extra)), "entries[0].note");
    let no_provider =
        r#"{"format":"ruletrace-recorded-script","version":1,"captured":"c","entries":[]}"#;
    assert_eq!(invalid_path(no_provider), ".provider");
    let wrong_format = good().replace("ruletrace-recorded-script", "ruletrace-mock-script");
    assert_eq!(invalid_path(&wrong_format), ".format");
    let wrong_version = good().replace("\"version\":1", "\"version\":2");
    assert_eq!(invalid_path(&wrong_version), ".version");
}

#[test]
fn flag_keys_and_truncation_are_rejected() {
    let flagged = good().replace("\"captured\"", "\"no_errors\":true,\"captured\"");
    let Err(ScriptError::FlagKeys(d)) = load(&flagged) else {
        panic!("expected FlagKeys");
    };
    assert_eq!(d[0].code().id(), "RT-DGN-0001");
    let both = good().replace(
        "\"captured\"",
        "\"has_errors\":false,\"no_errors\":true,\"captured\"",
    );
    let Err(ScriptError::FlagKeys(d)) = load(&both) else {
        panic!("expected FlagKeys");
    };
    assert_eq!(d[0].code().id(), "RT-DGN-0002");
    let cut = &good()[..60];
    assert!(matches!(load(cut), Err(ScriptError::Parse(_))));
}
