//! Export of batches to a Recorded_Provider_Script (task 10.6, Req 33.8,
//! 28.15). The expected script text is written out by hand.

use crate::*;
use rt_hash::{sha256, Sha256Hex};
use rt_ir::Identity;

const PAYLOAD_A: &str = "{\"schema_version\":\"2\",\"proposals\":[]}";
const PAYLOAD_B: &str = "{\"x\":1}";

fn batch(seed: &str, payload: &str, format: Format) -> Batch {
    Batch {
        format,
        request_id: sha256(seed.as_bytes()),
        response_sha256: sha256(payload.as_bytes()),
        response: StoredResponse {
            completion: Completion::Complete,
            payload: payload.to_owned(),
        },
        provider: "meta".to_owned(),
        model: "muse-spark-1.3".to_owned(),
        adapter_version: "responses-1/prompt-2".to_owned(),
        ai_identities: vec![Identity::new("meta").unwrap()],
        created_at: "2026-11-16T05:13:25Z".to_owned(),
        proposals: Vec::new(),
    }
}

fn v2() -> Format {
    Format::V2(Extras {
        protocol: Some("openai-responses".to_owned()),
        reported_model: Some("muse-spark-1.3".to_owned()),
        response_id: Some("resp_abc".to_owned()),
        usage: StoredUsage::new(Some(1), Some(2), Some(3)),
    })
}

fn header() -> ScriptHeader {
    ScriptHeader {
        kind: "openai-responses".to_owned(),
        provider_id: Some("meta".to_owned()),
        model: "muse-spark-1.3".to_owned(),
        captured: "captured by hand in a test".to_owned(),
    }
}

fn named(file: &str, batch: Batch) -> NamedBatch {
    NamedBatch {
        file: file.to_owned(),
        batch,
    }
}

fn hex(seed: &str) -> String {
    sha256(seed.as_bytes()).as_str().to_owned()
}

#[test]
fn the_script_is_the_documented_text_with_entries_in_request_order() {
    let a = batch("a", PAYLOAD_A, v2());
    let b = batch("b", PAYLOAD_B, v2());
    // sha256("a") = ca97..., sha256("b") = 3e23...: "b" sorts first.
    assert!(hex("b") < hex("a"));
    let out = export_script(&header(), &[named("a.json", a), named("b.json", b)]).unwrap();
    let want = format!(
        "{{\n  \"format\": \"ruletrace-recorded-script\",\n  \"version\": 1,\n  \"provider\": {{\n    \"kind\": \"openai-responses\",\n    \"id\": \"meta\",\n    \"model\": \"muse-spark-1.3\",\n    \"adapter_version\": \"responses-1/prompt-2\"\n  }},\n  \"captured\": \"captured by hand in a test\",\n  \"entries\": [\n    {{\n      \"request_id\": \"{}\",\n      \"origin\": \"recorded\",\n      \"completion\": \"complete\",\n      \"response_sha256\": \"{}\",\n      \"payload\": \"{{\\\"x\\\":1}}\"\n    }},\n    {{\n      \"request_id\": \"{}\",\n      \"origin\": \"recorded\",\n      \"completion\": \"complete\",\n      \"response_sha256\": \"{}\",\n      \"payload\": \"{{\\\"schema_version\\\":\\\"2\\\",\\\"proposals\\\":[]}}\"\n    }}\n  ]\n}}\n",
        hex("b"),
        sha256(PAYLOAD_B.as_bytes()),
        hex("a"),
        sha256(PAYLOAD_A.as_bytes()),
    );
    assert_eq!(String::from_utf8(out).unwrap(), want);
}

#[test]
fn equal_inputs_in_any_order_give_equal_bytes() {
    let a = || named("a.json", batch("a", PAYLOAD_A, v2()));
    let b = || named("b.json", batch("b", PAYLOAD_B, v2()));
    let one = export_script(&header(), &[a(), b()]).unwrap();
    let two = export_script(&header(), &[b(), a()]).unwrap();
    assert_eq!(one, two);
}

#[test]
fn reported_metadata_is_not_written_because_the_format_has_no_member_for_it() {
    let out = export_script(&header(), &[named("a.json", batch("a", PAYLOAD_A, v2()))]).unwrap();
    let text = String::from_utf8(out).unwrap();
    for absent in [
        "resp_abc",
        "usage",
        "reported_model",
        "created_at",
        "ai_identities",
    ] {
        assert!(!text.contains(absent), "{absent}");
    }
}

#[test]
fn a_version_1_batch_and_a_version_2_batch_without_extras_fabricate_nothing() {
    let mut v1 = batch("a", PAYLOAD_A, Format::V1);
    v1.provider = "openai-responses".to_owned();
    let bare = batch("b", PAYLOAD_B, Format::V2(Extras::default()));
    let mut no_id = header();
    no_id.provider_id = None;
    let out = export_script(&no_id, &[named("a.json", v1), named("b.json", bare)]).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("\"kind\": \"openai-responses\",\n    \"model\""));
    assert!(!text.contains("\"id\""));
    for absent in ["resp_", "usage", "reported"] {
        assert!(!text.contains(absent), "{absent}");
    }
}

#[test]
fn a_hash_that_differs_from_the_recorded_hash_fails_naming_file_and_request() {
    let mut b = batch("a", PAYLOAD_A, v2());
    b.response_sha256 = sha256(b"something else");
    let err = export_script(&header(), &[named("batches/x.json", b)]).unwrap_err();
    assert_eq!(
        err,
        ExportError::HashMismatch {
            file: "batches/x.json".to_owned(),
            request_id: hex("a"),
            recorded: sha256(b"something else").as_str().to_owned(),
            computed: sha256(PAYLOAD_A.as_bytes()).as_str().to_owned(),
        }
    );
    let text = err.to_string();
    assert!(
        text.contains("batches/x.json") && text.contains(&hex("a")),
        "{text}"
    );
}

#[test]
fn a_credential_looking_string_in_a_payload_refuses_the_script_without_echoing_it() {
    let canary = "sk-canary0123456789abcdef";
    for payload in [
        format!("{{\"rationale\":\"{canary}\"}}"),
        "{\"rationale\":\"Bearer abc\"}".to_owned(),
        "{\"r\":\"ghp_x\"}".to_owned(),
        "{\"r\":\"AKIAX\"}".to_owned(),
        "{\"r\":\"sk_live\"}".to_owned(),
    ] {
        let err =
            export_script(&header(), &[named("c.json", batch("c", &payload, v2()))]).unwrap_err();
        assert!(
            matches!(&err, ExportError::CredentialLooking { file, request_id, .. }
                if file == "c.json" && *request_id == hex("c")),
            "{err:?}"
        );
        assert!(!err.to_string().contains(canary));
    }
    // Inside a word the prefix is not a credential.
    let fine = "{\"r\":\"risk-taking task_x\"}";
    assert!(export_script(&header(), &[named("c.json", batch("c", fine, v2()))]).is_ok());
}

#[test]
fn a_credential_looking_capture_text_refuses_the_script() {
    let mut h = header();
    h.captured = "key sk-abcdef".to_owned();
    let err = export_script(&h, &[named("a.json", batch("a", PAYLOAD_A, v2()))]).unwrap_err();
    assert!(matches!(err, ExportError::CredentialLooking { ref file, .. } if file == "header"));
}

#[test]
fn two_prompt_profiles_are_never_merged_into_one_script() {
    let two = batch("a", PAYLOAD_A, v2());
    let mut three = batch("b", PAYLOAD_B, v2());
    three.adapter_version = "responses-1/prompt-3".to_owned();
    let err = export_script(
        &header(),
        &[named("a.json", two.clone()), named("b.json", three.clone())],
    )
    .unwrap_err();
    assert!(
        matches!(
            &err,
            ExportError::Mismatch {
                member: "adapter_version",
                ..
            }
        ),
        "{err:?}"
    );
    // Each exports alone, keeping its own version.
    let only_two =
        String::from_utf8(export_script(&header(), &[named("a.json", two)]).unwrap()).unwrap();
    let only_three =
        String::from_utf8(export_script(&header(), &[named("b.json", three)]).unwrap()).unwrap();
    assert!(only_two.contains("\"adapter_version\": \"responses-1/prompt-2\""));
    assert!(only_three.contains("\"adapter_version\": \"responses-1/prompt-3\""));
    assert_ne!(only_two, only_three);
}

type Change = fn(&mut ScriptHeader);

#[test]
fn a_header_that_disagrees_with_a_batch_is_refused_and_nothing_is_relabelled() {
    let cases: [(Change, &str); 3] = [
        (|h| h.model = "muse-ember-1.0".to_owned(), "model"),
        (|h| h.kind = "openai-chat".to_owned(), "protocol"),
        (|h| h.provider_id = Some("openai".to_owned()), "provider"),
    ];
    for (change, member) in cases {
        let mut h = header();
        change(&mut h);
        let err = export_script(&h, &[named("a.json", batch("a", PAYLOAD_A, v2()))]).unwrap_err();
        assert!(
            matches!(&err, ExportError::Mismatch { member: m, .. } if *m == member),
            "{member}: {err:?}"
        );
    }
}

#[test]
fn a_batch_of_the_offline_replay_is_never_exported_as_recorded() {
    let mut v1 = batch("a", PAYLOAD_A, Format::V1);
    v1.provider = "mock-replay".to_owned();
    let replayed = batch(
        "b",
        PAYLOAD_B,
        Format::V2(Extras {
            protocol: Some("mock-replay".to_owned()),
            ..Extras::default()
        }),
    );
    let mut no_id = header();
    no_id.provider_id = None;
    for b in [v1, replayed] {
        let err = export_script(&no_id, &[named("r.json", b)]).unwrap_err();
        assert!(
            matches!(err, ExportError::NotProviderOutput { .. }),
            "{err:?}"
        );
    }
    let mut replay_header = header();
    replay_header.kind = "mock-replay".to_owned();
    let err = export_script(
        &replay_header,
        &[named("a.json", batch("a", PAYLOAD_A, v2()))],
    )
    .unwrap_err();
    assert!(
        matches!(err, ExportError::NotProviderOutput { .. }),
        "{err:?}"
    );
}

#[test]
fn an_empty_store_and_a_repeated_request_are_refused() {
    assert_eq!(export_script(&header(), &[]), Err(ExportError::Empty));
    let a = named("a.json", batch("a", PAYLOAD_A, v2()));
    let again = named("a2.json", batch("a", PAYLOAD_A, v2()));
    assert!(matches!(
        export_script(&header(), &[a, again]),
        Err(ExportError::Duplicate { .. })
    ));
}

#[test]
fn the_hash_of_every_entry_is_recomputed_from_the_payload() {
    let out = export_script(&header(), &[named("a.json", batch("a", PAYLOAD_A, v2()))]).unwrap();
    let text = String::from_utf8(out).unwrap();
    let want: Sha256Hex = sha256(PAYLOAD_A.as_bytes());
    assert!(text.contains(&format!("\"response_sha256\": \"{want}\"")));
}
