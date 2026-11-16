//! Tests of `Redactor::redact_fragments` and of the Transmission_Log
//! writer that uses it (Req 28.8, 28.9). The oracle for "no run of eight
//! characters" is a plain sliding window over the credential and a
//! substring search, independent of the redactor's own indexes.

use crate::{Credential, LogContext, LogSink, Redactor, TransmissionLog, REDACTED};
use rt_ai_adapter::{ProviderRequest, RequestLimits};
use rt_foundation::{Code, DocumentId, InputId, InputLocation, Location};
use rt_hash::sha256;
use rt_json::Value;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

pub(crate) const CANARY: &str = "sk-canary-Zq7x93LmN";
pub(crate) const LONG: &str =
    "sk-canary-Zq7x93LmN-Rt5vK2pWd8Hc4Jf1Bs6Ug0Ya3Xe9Ni7Oo-Pl2Mm5Kk8Jj1Hh4Gg7Ff0Dd";
pub(crate) const DOTTED: &str = "sk.canary.Zq7x.93LmN.Rt5v";
pub(crate) const BRACKETED: &str = "[sk-canary-Zq7x93LmN]";

pub(crate) fn canaries() -> [&'static str; 4] {
    [CANARY, LONG, DOTTED, BRACKETED]
}

/// The first run of eight consecutive characters of `credential` that
/// occurs in `text`, if any.
pub(crate) fn leaked(text: &str, credential: &str) -> Option<String> {
    let chars: Vec<char> = credential.chars().collect();
    chars
        .windows(8)
        .map(|w| w.iter().collect::<String>())
        .find(|w| text.contains(w.as_str()))
}

pub(crate) fn redactor(credential: &str) -> Redactor {
    Redactor::new(&Credential::new(credential.to_owned()).unwrap())
}

#[test]
fn the_long_canary_is_longer_than_sixty_characters() {
    assert!(LONG.chars().count() > 60);
}

#[test]
fn a_whole_credential_is_replaced_as_by_redact() {
    let r = redactor(CANARY);
    let text = format!("key {CANARY} end");
    assert_eq!(r.redact_fragments(&text), format!("key {REDACTED} end"));
}

#[test]
fn a_cut_copy_is_replaced_by_one_marker() {
    let r = redactor(CANARY);
    assert_eq!(
        r.redact_fragments("see sk-canary-Zq7 end"),
        format!("see {REDACTED} end")
    );
    assert_eq!(
        r.redact_fragments("see canary-Zq7x93 end"),
        format!("see {REDACTED} end")
    );
}

#[test]
fn a_piece_of_seven_characters_is_left_alone() {
    let r = redactor(CANARY);
    assert_eq!(r.redact_fragments("a sk-cana b"), "a sk-cana b");
}

#[test]
fn text_without_a_window_is_unchanged() {
    let r = redactor(CANARY);
    for text in ["", "short", "nothing to see here", "sk-cana"] {
        assert_eq!(r.redact_fragments(text), text, "{text:?}");
    }
    assert_eq!(r.redact_fragments("sk-canary"), REDACTED);
}

#[test]
fn a_short_credential_is_matched_whole_only() {
    let r = redactor("abc12");
    assert_eq!(
        r.redact_fragments("xabc12y abc1"),
        format!("x{REDACTED}y abc1")
    );
}

#[test]
fn no_credential_changes_nothing() {
    let text = format!("{CANARY} and sk-canary-Zq7");
    assert_eq!(Redactor::no_credential().redact_fragments(&text), text);
}

#[test]
fn the_json_escaped_form_is_cut_too() {
    let r = redactor("k\"ey-0123456789");
    let text = r#"copy \"ey-0123 here"#;
    assert_eq!(r.redact_fragments(text), format!("copy {REDACTED} here"));
}

#[test]
fn multibyte_credentials_are_windowed_by_character() {
    let r = redactor("κλμνξοπρστυφ");
    assert_eq!(
        r.redact_fragments("a κλμνξοπρ b"),
        format!("a {REDACTED} b")
    );
    assert_eq!(r.redact_fragments("a κλμνξοπ b"), "a κλμνξοπ b");
}

#[test]
fn redacting_twice_equals_redacting_once() {
    for c in canaries() {
        let r = redactor(c);
        let text = format!("a {c} b {} c {}", &c[..12], &c[c.len() - 10..]);
        let once = r.redact_fragments(&text);
        assert_eq!(r.redact_fragments(&once), once, "{c}");
        assert_eq!(leaked(&once, c), None, "{c}: {once}");
    }
}

#[test]
fn the_oracle_sees_a_leak_in_unredacted_text() {
    for c in canaries() {
        let cut: String = c.chars().take(12).collect();
        assert!(leaked(&cut, c).is_some(), "{c}");
        assert!(leaked("nothing", c).is_none());
    }
}

#[derive(Clone, Default)]
struct Bytes(Rc<RefCell<Vec<u8>>>);

impl LogSink for Bytes {
    fn append(&mut self, line: &[u8]) -> io::Result<()> {
        self.0.borrow_mut().extend_from_slice(line);
        Ok(())
    }
}

fn request() -> ProviderRequest {
    ProviderRequest {
        task: "propose".to_owned(),
        model: "model-1".to_owned(),
        limits: RequestLimits {
            deadline_s: 60,
            attempt_s: 20,
            retries: 2,
            response_bytes: 1_048_576,
        },
        reasoning_effort: None,
        context: vec![("clauses".to_owned(), Value::array(vec![]))],
    }
}

fn log_written_for(credential: &str, redact: bool) -> String {
    let cut =
        |from: usize, to: usize| -> String { credential.chars().skip(from).take(to).collect() };
    let context = LogContext {
        provider: format!("provider-{}", cut(0, 12)),
        protocol: format!("protocol-{}", cut(1, 12)),
        model: format!("model-{credential}"),
        endpoint_host: format!("{}.example.org", cut(2, 14)),
        documents: vec![
            (
                DocumentId::new(format!("doc-{credential}.md")).unwrap(),
                sha256(b"a"),
            ),
            (
                DocumentId::new(format!("{}-tail", cut(4, 20))).unwrap(),
                sha256(b"b"),
            ),
        ],
        clauses: 2,
    };
    let at = Location::Input(InputLocation {
        input: InputId::new("ai.toml").unwrap(),
        line: None,
        column: None,
        field: Some("transmission_log".to_owned()),
    });
    let sink = Bytes::default();
    let r = if redact {
        redactor(credential)
    } else {
        Redactor::no_credential()
    };
    let mut log = TransmissionLog::new(sink.clone(), r, at);
    let time = format!("t-{}", cut(1, 10));
    log.record(&context, &request(), 1, &time).unwrap();
    let bytes = sink.0.borrow().clone();
    String::from_utf8(bytes).unwrap()
}

#[test]
fn the_transmission_log_holds_no_run_of_eight_credential_characters() {
    for c in canaries() {
        let written = log_written_for(c, true);
        assert_eq!(leaked(&written, c), None, "{c}: {written}");
        assert!(written.ends_with('\n') && written.matches('\n').count() == 1);
        assert!(written.contains("\"attempt\":1"), "{written}");
        assert!(written.contains("\"clauses\":2"), "{written}");
    }
}

#[test]
fn the_log_test_detects_a_leak_when_nothing_is_redacted() {
    for c in canaries() {
        let written = log_written_for(c, false);
        assert!(leaked(&written, c).is_some(), "{c}: {written}");
    }
}

#[test]
fn a_log_failure_message_keeps_its_code() {
    struct Failing;
    impl LogSink for Failing {
        fn append(&mut self, _: &[u8]) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        }
    }
    let at = Location::None(Code::AiTransmissionLogFailed);
    let mut log = TransmissionLog::new(Failing, redactor(CANARY), at);
    let context = LogContext {
        provider: "p".to_owned(),
        protocol: "q".to_owned(),
        model: "m".to_owned(),
        endpoint_host: String::new(),
        documents: vec![],
        clauses: 0,
    };
    let err = log.record(&context, &request(), 1, "t").unwrap_err();
    assert_eq!(err.code(), Code::AiTransmissionLogFailed);
}
