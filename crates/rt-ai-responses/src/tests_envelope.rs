//! Envelope mapping (Req 35.13). Bodies are written by hand in the shape of
//! the OpenAI Responses documentation and of the Meta record of UD-37.

use crate::envelope::{map, Counts, Verdict};

const REASONING: &str = r#"{"type":"reasoning","id":"rs_1","summary":[]}"#;

fn message(parts: &str) -> String {
    format!(
        r#"{{"type":"message","id":"msg_1","status":"completed","role":"assistant","content":[{parts}]}}"#
    )
}

fn text(t: &str) -> String {
    format!(r#"{{"type":"output_text","text":"{t}","annotations":[]}}"#)
}

fn envelope(status: &str, output: &[String], extra: &str) -> Vec<u8> {
    format!(
        r#"{{"id":"resp_9","object":"response","status":"{status}","model":"muse-x","output":[{}]{extra}}}"#,
        output.join(",")
    )
    .into_bytes()
}

#[test]
fn completed_message_gives_the_text() {
    let body = envelope("completed", &[message(&text("{\\\"a\\\":1}"))], "");
    let m = map(&body);
    assert_eq!(m.verdict, Verdict::Text("{\"a\":1}".to_owned()));
    assert_eq!(m.reported_model.as_deref(), Some("muse-x"));
    assert_eq!(m.response_id.as_deref(), Some("resp_9"));
    assert_eq!(m.usage, None);
}

#[test]
fn any_number_of_reasoning_items_before_or_after_is_ignored() {
    for n in [0usize, 1, 2, 5] {
        let mut out: Vec<String> = vec![REASONING.to_owned(); n];
        out.push(message(&text("ok")));
        out.extend(vec![REASONING.to_owned(); n]);
        let m = map(&envelope("completed", &out, ""));
        assert_eq!(m.verdict, Verdict::Text("ok".to_owned()), "n = {n}");
    }
}

#[test]
fn usage_is_exactly_what_was_reported() {
    type Triple = (Option<u64>, Option<u64>, Option<u64>);
    let cases: [(&str, Option<Triple>); 8] = [
        (
            r#","usage":{"input_tokens":10,"output_tokens":20,"total_tokens":30}"#,
            Some((Some(10), Some(20), Some(30))),
        ),
        (
            r#","usage":{"input_tokens":10}"#,
            Some((Some(10), None, None)),
        ),
        (
            r#","usage":{"output_tokens":0}"#,
            Some((None, Some(0), None)),
        ),
        (
            r#","usage":{"total_tokens":7}"#,
            Some((None, None, Some(7))),
        ),
        (
            r#","usage":{"input_tokens":10,"output_tokens":20}"#,
            Some((Some(10), Some(20), None)),
        ),
        (r#","usage":{}"#, None),
        (
            r#","usage":{"input_tokens":-1,"output_tokens":1.5,"total_tokens":"3"}"#,
            None,
        ),
        (r#","usage":[1,2]"#, None),
    ];
    for (extra, want) in cases {
        let m = map(&envelope("completed", &[message(&text("t"))], extra));
        let want = want.map(|(input, output, total)| Counts {
            input,
            output,
            total,
        });
        assert_eq!(m.usage, want, "{extra}");
    }
}

#[test]
fn refusal_incomplete_and_other_statuses() {
    let refusal = message(r#"{"type":"refusal","refusal":"no"}"#);
    assert_eq!(
        map(&envelope("completed", &[refusal], "")).verdict,
        Verdict::Refused
    );
    for status in ["incomplete", "failed", "cancelled", "in_progress", "queued"] {
        let m = map(&envelope(status, &[message(&text("t"))], ""));
        assert_eq!(m.verdict, Verdict::Incomplete, "{status}");
    }
}

#[test]
fn malformed_shapes_are_malformed() {
    let m1 = message(&text("a"));
    let two_texts = message(&format!("{},{}", text("a"), text("b")));
    let mixed = message(&format!(
        "{},{}",
        text("a"),
        r#"{"type":"refusal","refusal":"x"}"#
    ));
    let two_refusals =
        message(r#"{"type":"refusal","refusal":"x"},{"type":"refusal","refusal":"y"}"#);
    let tool = r#"{"type":"function_call","id":"fc_1","name":"f","arguments":"{}"}"#.to_owned();
    let web = r#"{"type":"web_search_call","id":"ws_1"}"#.to_owned();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "two messages",
            envelope("completed", &[m1.clone(), m1.clone()], ""),
        ),
        (
            "no message",
            envelope("completed", &[REASONING.to_owned()], ""),
        ),
        ("empty output", envelope("completed", &[], "")),
        (
            "tool call before",
            envelope("completed", &[tool.clone(), m1.clone()], ""),
        ),
        (
            "tool call after",
            envelope("completed", &[m1.clone(), tool], ""),
        ),
        ("web search", envelope("completed", &[web, m1.clone()], "")),
        ("two output_text", envelope("completed", &[two_texts], "")),
        ("text and refusal", envelope("completed", &[mixed], "")),
        ("two refusals", envelope("completed", &[two_refusals], "")),
        (
            "unknown content",
            envelope("completed", &[message(r#"{"type":"image","url":"x"}"#)], ""),
        ),
        ("empty content", envelope("completed", &[message("")], "")),
        (
            "text not a string",
            envelope(
                "completed",
                &[message(r#"{"type":"output_text","text":5}"#)],
                "",
            ),
        ),
        (
            "item without type",
            envelope("completed", &[r#"{"id":"x"}"#.to_owned()], ""),
        ),
        ("not json", b"<html>".to_vec()),
        ("empty body", Vec::new()),
        ("array", b"[]".to_vec()),
        ("no status", br#"{"output":[]}"#.to_vec()),
        ("status not string", br#"{"status":1,"output":[]}"#.to_vec()),
        ("no output", br#"{"status":"completed"}"#.to_vec()),
        (
            "output not array",
            br#"{"status":"completed","output":{}}"#.to_vec(),
        ),
        (
            "duplicate key",
            br#"{"status":"completed","status":"completed","output":[]}"#.to_vec(),
        ),
        ("truncated", br#"{"status":"comp"#.to_vec()),
    ];
    for (name, body) in cases {
        let m = map(&body);
        assert!(matches!(m.verdict, Verdict::Malformed(_)), "{name}: {m:?}");
    }
}

#[test]
fn a_tool_call_is_malformed_even_when_status_is_incomplete_is_not_read() {
    // A non-completed status wins before the output is inspected.
    let tool = r#"{"type":"function_call","id":"fc_1","name":"f","arguments":"{}"}"#.to_owned();
    assert_eq!(
        map(&envelope("incomplete", &[tool], "")).verdict,
        Verdict::Incomplete
    );
}

#[test]
fn reported_metadata_must_be_strings() {
    let body = br#"{"id":5,"model":["x"],"status":"completed","output":[]}"#;
    let m = map(body);
    assert_eq!((m.reported_model, m.response_id), (None, None));
}
