//! Proposal_Store format version 2 and the dual-version reader (UD-37,
//! Req 35.15 to 35.17, task 10.14). Expected texts are written out by hand,
//! so the printer is not the source of its own expectations.

use crate::*;
use rt_foundation::{Code, Diagnostic, InputId, Location};
use rt_hash::sha256;
use rt_ir::Identity;

const V1_BIG: &str = include_str!(
    "../../../tests/fixtures/proposal-store-v1/ab5e5c4a688908ce49628b8e0ab189dcd4ef095f97e745f13487e80a9de9f071.json"
);
const V1_SMALL: &str = include_str!(
    "../../../tests/fixtures/proposal-store-v1/be41f0e453a0518479e65a8da0bb7035989b86e5aa4727e7c851304c37c2d2b7.json"
);

fn input() -> InputId {
    InputId::new("batches/x.json").unwrap()
}

fn base(format: Format) -> Batch {
    let request_id = sha256(b"request");
    let payload = "{\"schema_version\":\"1\",\"proposals\":[]}";
    Batch {
        format,
        request_id,
        response_sha256: sha256(payload.as_bytes()),
        response: StoredResponse {
            completion: Completion::Complete,
            payload: payload.to_owned(),
        },
        provider: "meta".to_owned(),
        model: "configured-model".to_owned(),
        adapter_version: "0.1".to_owned(),
        ai_identities: vec![Identity::new("ai.bot").unwrap()],
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        proposals: Vec::new(),
    }
}

fn all() -> Extras {
    Extras {
        protocol: Some("openai-responses".to_owned()),
        reported_model: Some("reported-model".to_owned()),
        response_id: Some("resp_1".to_owned()),
        usage: StoredUsage::new(Some(10), Some(20), Some(30)),
    }
}

fn head(version: u32) -> String {
    let hash = sha256(b"request");
    let payload_hash = sha256(b"{\"schema_version\":\"1\",\"proposals\":[]}");
    format!(
        r#"{{"format":"ruletrace-proposals","version":{version},"request_id":"{hash}","response_sha256":"{payload_hash}","response":{{"completion":"complete","payload":"{{\"schema_version\":\"1\",\"proposals\":[]}}"}},"provider":"meta","model":"configured-model","adapter_version":"0.1","#
    )
}

const TAIL: &str =
    r#""ai_identities":["ai.bot"],"created_at":"2026-01-01T00:00:00Z","proposals":[]}"#;

fn v2_text(optional: &str) -> String {
    format!("{}{optional}{TAIL}\n", head(2))
}

fn codes(text: &str) -> Vec<Code> {
    parse_batch(&input(), text.as_bytes())
        .unwrap_err()
        .iter()
        .map(Diagnostic::code)
        .collect()
}

fn field_of(text: &str) -> String {
    let found = parse_batch(&input(), text.as_bytes()).unwrap_err();
    match found[0].primary() {
        Location::Input(l) => l.field.clone().unwrap_or_default(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn version_1_batches_of_the_earlier_goldens_are_read_and_printed_unchanged() {
    for text in [V1_BIG, V1_SMALL] {
        let batch = parse_batch(&input(), text.as_bytes()).unwrap();
        assert_eq!(batch.format, Format::V1);
        assert_eq!(batch.provider, "mock-replay");
        assert_eq!(print_batch(&batch), text);
        assert!(text.contains(r#""version":1,"#));
        assert!(!text.contains("protocol"));
    }
}

#[test]
fn a_version_2_batch_prints_every_optional_member_in_the_documented_order() {
    let expected = v2_text(
        r#""protocol":"openai-responses","reported_model":"reported-model","response_id":"resp_1","usage":{"input_tokens":10,"output_tokens":20,"total_tokens":30},"#,
    );
    let batch = base(Format::V2(all()));
    assert_eq!(print_batch(&batch), expected);
    assert_eq!(parse_batch(&input(), expected.as_bytes()).unwrap(), batch);
}

#[test]
fn a_version_2_batch_with_nothing_optional_prints_no_member_and_no_null() {
    let expected = v2_text("");
    let batch = base(Format::V2(Extras::default()));
    let printed = print_batch(&batch);
    assert_eq!(printed, expected);
    assert!(!printed.contains("null"));
    assert_eq!(parse_batch(&input(), expected.as_bytes()).unwrap(), batch);
}

#[test]
fn usage_with_only_a_total_stores_only_the_total() {
    let extras = Extras {
        usage: StoredUsage::new(None, None, Some(7)),
        ..Extras::default()
    };
    let printed = print_batch(&base(Format::V2(extras.clone())));
    assert_eq!(printed, v2_text(r#""usage":{"total_tokens":7},"#));
    let parsed = parse_batch(&input(), printed.as_bytes()).unwrap();
    let usage = match parsed.format {
        Format::V2(e) => e.usage.unwrap(),
        Format::V1 => panic!("version 2 expected"),
    };
    assert_eq!(
        (
            usage.input_tokens(),
            usage.output_tokens(),
            usage.total_tokens()
        ),
        (None, None, Some(7))
    );
}

#[test]
fn input_and_output_counts_are_never_summed_into_a_total() {
    let extras = Extras {
        usage: StoredUsage::new(Some(3), Some(4), None),
        ..Extras::default()
    };
    let printed = print_batch(&base(Format::V2(extras)));
    assert_eq!(
        printed,
        v2_text(r#""usage":{"input_tokens":3,"output_tokens":4},"#)
    );
    assert!(!printed.contains("total_tokens"));
}

#[test]
fn a_usage_without_any_count_does_not_exist() {
    assert_eq!(StoredUsage::new(None, None, None), None);
    let text = v2_text(r#""usage":{},"#);
    assert_eq!(codes(&text), vec![Code::AiStoreInvalid]);
    assert_eq!(field_of(&text), "usage");
}

#[test]
fn the_configured_model_stays_apart_from_the_reported_model() {
    let printed = print_batch(&base(Format::V2(all())));
    let parsed = parse_batch(&input(), printed.as_bytes()).unwrap();
    assert_eq!(parsed.model, "configured-model");
    let Format::V2(extras) = parsed.format else {
        panic!("version 2 expected");
    };
    assert_eq!(extras.reported_model.as_deref(), Some("reported-model"));
}

#[test]
fn an_unknown_version_is_refused_at_the_version_member() {
    for version in ["0", "3", "4", "10", "\"2\"", "1.5", "-1", "null", "true"] {
        let text = v2_text("").replacen(r#""version":2"#, &format!(r#""version":{version}"#), 1);
        assert_eq!(codes(&text), vec![Code::AiStoreInvalid], "{version}");
        assert_eq!(field_of(&text), "version", "{version}");
    }
}

#[test]
fn a_version_2_member_in_a_version_1_batch_is_unknown() {
    let v1 = print_batch(&base(Format::V1));
    for member in [
        r#""protocol":"x","#,
        r#""reported_model":"x","#,
        r#""response_id":"x","#,
        r#""usage":{"total_tokens":1},"#,
    ] {
        let text = v1.replacen(
            r#""ai_identities""#,
            &format!(r#"{member}"ai_identities""#),
            1,
        );
        assert_eq!(codes(&text), vec![Code::AiStoreInvalid], "{member}");
        let key = member.split('"').nth(1).unwrap();
        assert_eq!(field_of(&text), key);
    }
}

#[test]
fn unknown_members_of_a_version_2_batch_and_its_usage_are_refused() {
    for (text, field) in [
        (v2_text(r#""cost":1,"#), "cost"),
        (v2_text(r#""authorization":"Bearer x","#), "authorization"),
        (
            v2_text(r#""usage":{"total_tokens":1,"cost_usd":1},"#),
            "usage.cost_usd",
        ),
    ] {
        assert_eq!(codes(&text), vec![Code::AiStoreInvalid], "{field}");
        assert_eq!(field_of(&text), field);
    }
}

#[test]
fn a_wrong_type_or_value_of_an_optional_member_is_refused() {
    let long = "a".repeat(257);
    let cases = [
        (r#""protocol":1,"#.to_owned(), "protocol"),
        (r#""protocol":null,"#.to_owned(), "protocol"),
        (r#""protocol":"","#.to_owned(), "protocol"),
        (r#""reported_model":["x"],"#.to_owned(), "reported_model"),
        (format!(r#""reported_model":"{long}","#), "reported_model"),
        (
            r#""reported_model":"a\u0007b","#.to_owned(),
            "reported_model",
        ),
        (r#""response_id":{"id":"x"},"#.to_owned(), "response_id"),
        (r#""response_id":"a\nb","#.to_owned(), "response_id"),
        (r#""usage":null,"#.to_owned(), "usage"),
        (r#""usage":[1],"#.to_owned(), "usage"),
        (r#""usage":"1","#.to_owned(), "usage"),
        (
            r#""usage":{"total_tokens":"1"},"#.to_owned(),
            "usage.total_tokens",
        ),
        (
            r#""usage":{"total_tokens":-1},"#.to_owned(),
            "usage.total_tokens",
        ),
        (
            r#""usage":{"input_tokens":1.5},"#.to_owned(),
            "usage.input_tokens",
        ),
        (
            r#""usage":{"output_tokens":null},"#.to_owned(),
            "usage.output_tokens",
        ),
        (
            r#""usage":{"total_tokens":9007199254740992},"#.to_owned(),
            "usage.total_tokens",
        ),
    ];
    for (member, field) in cases {
        let text = v2_text(&member);
        assert_eq!(codes(&text), vec![Code::AiStoreInvalid], "{member}");
        assert_eq!(field_of(&text), field, "{member}");
    }
}

#[test]
fn a_reported_value_of_exactly_256_scalar_values_is_accepted() {
    let edge = "é".repeat(256);
    let text = v2_text(&format!(r#""reported_model":"{edge}","#));
    let parsed = parse_batch(&input(), text.as_bytes()).unwrap();
    let Format::V2(extras) = parsed.format else {
        panic!("version 2 expected");
    };
    assert_eq!(extras.reported_model.as_deref(), Some(edge.as_str()));
}

#[test]
fn optional_members_out_of_order_are_not_canonical() {
    let text = v2_text(r#""response_id":"r","protocol":"p","#);
    assert_eq!(codes(&text), vec![Code::AiStoreInvalid]);
    let late = v2_text("").replacen(r#""ai_identities""#, r#""protocol":"p","ai_identities""#, 1);
    assert_eq!(late, v2_text(r#""protocol":"p","#));
    let before = head(2).replacen(
        r#""adapter_version":"0.1","#,
        r#""protocol":"p","adapter_version":"0.1","#,
        1,
    );
    assert_eq!(
        codes(&format!("{before}{TAIL}\n")),
        vec![Code::AiStoreInvalid]
    );
}

#[test]
fn version_2_keeps_the_payload_hash_binding() {
    let tampered = v2_text("").replace("schema_version", "schema_versio");
    assert_eq!(codes(&tampered), vec![Code::AiStoreInvalid]);
    assert_eq!(field_of(&tampered), "response.payload");
}

#[test]
fn version_2_keeps_the_proposal_identifier_binding() {
    let good = print_batch(&crate::tests::batch());
    assert!(good.contains(r#""version":1,"#));
    let request = sha256(b"request");
    let mut b = crate::tests::batch();
    b.format = Format::V2(Extras::default());
    let text = print_batch(&b);
    assert!(text.contains(r#""version":2,"#));
    let id = proposal_id(&request, 0);
    let wrong = text.replacen(id.as_str(), sha256(b"other").as_str(), 1);
    assert_eq!(codes(&wrong), vec![Code::AiStoreInvalid]);
    assert_eq!(parse_batch(&input(), text.as_bytes()).unwrap(), b);
}

#[test]
fn a_version_2_flag_key_still_gets_its_dgn_code() {
    let text = v2_text("").replacen('{', r#"{"has_errors":false,"#, 1);
    assert_eq!(codes(&text), vec![Code::DiagnosticStateContradiction]);
}
