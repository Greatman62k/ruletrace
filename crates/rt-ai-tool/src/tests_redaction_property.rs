//! Property 37, credential redaction (Req 28.9, 23.22, 23.23).
//!
//! The credential is an arbitrary string with control characters, quotes,
//! backslashes and Unicode. Texts are built from filler drawn from a
//! private-use alphabet that a credential never contains, so the expected
//! output follows from the construction and not from the redactor.

use crate::tests::{run, MapEnv, BASE};
use crate::{Credential, LogContext, LogSink, Out, Redactor, TransmissionLog, REDACTED};
use proptest::prelude::*;
use rt_ai_adapter::{ProviderRequest, RequestLimits};
use rt_foundation::{
    escape_controls, Code, Diagnostic, DocumentId, InputId, InputLocation, Location,
};
use rt_hash::sha256;
use rt_json::{parse, write_compact, ParseLimits, Value};
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

#[derive(Clone, Copy, Debug)]
enum Form {
    Raw,
    Bearer,
    Json,
    Escaped,
    BearerJson,
    BearerEscaped,
}

fn is_filler(c: char) -> bool {
    ('\u{e000}'..='\u{e0ff}').contains(&c)
}

fn credential_char() -> impl Strategy<Value = char> {
    prop_oneof![
        proptest::sample::select(vec![
            '"', '\\', '\n', '\r', '\t', '\0', '\u{1b}', '\u{7f}', '\u{85}', '\u{2028}', '/', '\'',
            '[', ']', ' ', 'B',
        ]),
        (0x20u8..0x7f).prop_map(char::from),
        (0u8..0x20).prop_map(char::from),
        any::<char>().prop_filter("filler alphabet", |c| !is_filler(*c)),
    ]
}

fn credential(min: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(credential_char(), min..=20)
        .prop_map(|chars| chars.into_iter().collect::<String>())
        .prop_filter("must not contain the marker", |v| !v.contains("REDACTED"))
}

fn filler() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        (0xe000u32..0xe100).prop_map(|n| char::from_u32(n).unwrap()),
        1..4,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

fn form() -> impl Strategy<Value = Form> {
    proptest::sample::select(vec![
        Form::Raw,
        Form::Bearer,
        Form::Json,
        Form::Escaped,
        Form::BearerJson,
        Form::BearerEscaped,
    ])
}

fn json_form(value: &str) -> String {
    let quoted = write_compact(&Value::<()>::string(value));
    quoted[1..quoted.len() - 1].to_owned()
}

fn rendering(value: &str, form: Form) -> String {
    match form {
        Form::Raw => value.to_owned(),
        Form::Bearer => format!("Bearer {value}"),
        Form::Json => json_form(value),
        Form::Escaped => escape_controls(value),
        Form::BearerJson => format!("Bearer {}", json_form(value)),
        Form::BearerEscaped => format!("Bearer {}", escape_controls(value)),
    }
}

/// A generated text and the output that its construction requires.
#[derive(Debug)]
struct Case {
    value: String,
    text: String,
    expected: String,
    plain: bool,
}

fn echo_case(min: usize) -> impl Strategy<Value = Case> {
    (
        credential(min),
        proptest::collection::vec((filler(), form()), 1..6),
        filler(),
    )
        .prop_map(|(value, parts, tail)| {
            let mut text = String::new();
            let mut expected = String::new();
            let mut plain = true;
            for (lead, form) in &parts {
                text.push_str(lead);
                text.push_str(&rendering(&value, *form));
                expected.push_str(lead);
                expected.push_str(REDACTED);
                plain &= matches!(form, Form::Raw | Form::Bearer);
            }
            text.push_str(&tail);
            expected.push_str(&tail);
            Case {
                value,
                text,
                expected,
                plain,
            }
        })
}

// The credential is `r s r`, so a chain `r (s r)^k` holds k occurrences
// that overlap in `r`. Every character of the chain belongs to one.
fn overlap_case() -> impl Strategy<Value = Case> {
    (
        proptest::collection::vec(credential_char(), 1..6),
        proptest::collection::vec(credential_char(), 0..8),
        2usize..5,
        filler(),
        filler(),
    )
        .prop_filter("long enough", |(r, s, ..)| 2 * r.len() + s.len() >= 6)
        .prop_map(|(r, s, k, lead, tail)| {
            let r: String = r.into_iter().collect();
            let s: String = s.into_iter().collect();
            let value = format!("{r}{s}{r}");
            let chain = format!("{r}{}", format!("{s}{r}").repeat(k));
            Case {
                text: format!("{lead}{chain}{tail}"),
                expected: format!("{lead}{REDACTED}{tail}"),
                value,
                plain: true,
            }
        })
        .prop_filter("must not contain the marker", |c| {
            !c.value.contains("REDACTED")
        })
}

// Every occurrence of the value lies inside the `Bearer <value>` span, so
// the spans contain one another and the whole chain is one marker. The
// value is a slice of "Bearer " so that it also occurs inside the prefix.
fn contained_case() -> impl Strategy<Value = Case> {
    const PREFIX: &str = "Bearer ";
    (
        0usize..PREFIX.len(),
        1usize..=PREFIX.len(),
        filler(),
        filler(),
    )
        .prop_filter("slice fits", |(from, len, ..)| from + len <= PREFIX.len())
        .prop_map(|(from, len, lead, tail)| {
            let value = PREFIX[from..from + len].to_owned();
            Case {
                text: format!("{lead}Bearer {value}{tail}"),
                expected: format!("{lead}{REDACTED}{tail}"),
                value,
                plain: true,
            }
        })
}

fn field_location(field: &str) -> Location {
    Location::Input(InputLocation {
        input: InputId::new("ai.toml").unwrap(),
        line: Some(9),
        column: Some(1),
        field: Some(field.to_owned()),
    })
}

fn ai_codes() -> Vec<Code> {
    Code::ALL
        .iter()
        .copied()
        .filter(|c| c.id().starts_with("RT-AI-"))
        .collect()
}

fn check_out_streams(case: &Case, redactor: &Redactor) -> Result<(), TestCaseError> {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let mut out = Out::new(&mut stdout, &mut stderr, redactor.clone());
    out.stdout(&case.text).unwrap();
    out.stderr(&case.text).unwrap();
    for code in ai_codes() {
        let d = Diagnostic::new(code, case.text.clone(), field_location(&case.text));
        out.diagnostic(&d).unwrap();
    }
    drop(out);
    prop_assert_eq!(String::from_utf8(stdout).unwrap(), case.expected.clone());
    let mut want = case.expected.clone();
    for code in ai_codes() {
        let d = Diagnostic::new(code, case.expected.clone(), field_location(&case.expected));
        want.push_str(&crate::render_diagnostic(&d));
    }
    prop_assert_eq!(String::from_utf8(stderr).unwrap(), want);
    Ok(())
}

struct CaptureSink(Rc<RefCell<Vec<u8>>>);

impl LogSink for CaptureSink {
    fn append(&mut self, line: &[u8]) -> io::Result<()> {
        self.0.borrow_mut().extend_from_slice(line);
        Ok(())
    }
}

// The model field stands for any text that a log entry could echo.
fn record_with_model(redactor: &Redactor, model: &str) -> String {
    let request = ProviderRequest {
        task: "propose".to_owned(),
        model: model.to_owned(),
        limits: RequestLimits {
            deadline_s: 60,
            attempt_s: 20,
            retries: 2,
            response_bytes: 1_048_576,
        },
        context: Vec::new(),
    };
    let context = LogContext {
        provider: "openai-responses".to_owned(),
        model: model.to_owned(),
        endpoint_host: "api.example.org".to_owned(),
        documents: vec![(DocumentId::new("a.md").unwrap(), sha256(b"a"))],
        clauses: 1,
    };
    let bytes = Rc::new(RefCell::new(Vec::new()));
    let mut log = TransmissionLog::new(
        CaptureSink(bytes.clone()),
        redactor.clone(),
        field_location("transmission_log"),
    );
    log.record(&context, &request, 1, "2025-01-02T03:04:05Z")
        .unwrap();
    let line = String::from_utf8(bytes.borrow().clone()).unwrap();
    line
}

fn check_log_line(case: &Case, redactor: &Redactor) -> Result<(), TestCaseError> {
    let line = record_with_model(redactor, &case.text);
    for needle in [
        case.value.clone(),
        json_form(&case.value),
        escape_controls(&case.value),
    ] {
        prop_assert!(!line.contains(&needle), "{line:?}");
    }
    if case.plain {
        let id = InputId::new("log").unwrap();
        let parsed = parse(&id, line.as_bytes(), ParseLimits::new(1 << 20, 8)).unwrap();
        let model = parsed
            .as_object()
            .unwrap()
            .iter()
            .find(|m| m.key == "model")
            .and_then(|m| m.value.as_str().map(str::to_owned));
        prop_assert_eq!(model.as_deref(), Some(case.expected.as_str()));
    }
    Ok(())
}

fn check_debug(value: &str) -> Result<(), TestCaseError> {
    let credential = Credential::new(value.to_owned()).unwrap();
    let redactor = Redactor::new(&credential);
    prop_assert_eq!(format!("{credential:?}"), "Credential([REDACTED])");
    prop_assert_eq!(format!("{credential:#?}"), "Credential([REDACTED])");
    prop_assert_eq!(format!("{redactor:?}"), "Redactor([REDACTED])");
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", value)]);
    let authorized = run(BASE, &env).unwrap();
    let debug_forms = format!("{value:?}");
    for shown in [format!("{authorized:?}"), format!("{authorized:#?}")] {
        for needle in [
            value,
            &json_form(value),
            &escape_controls(value),
            &debug_forms[1..debug_forms.len() - 1],
        ] {
            prop_assert!(!shown.contains(needle), "{shown}");
        }
        prop_assert!(shown.contains("[REDACTED]"));
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    // Every rendering of the value is replaced, whatever the value.
    #[test]
    fn echoes_are_replaced_exactly(case in echo_case(1)) {
        let redactor = Redactor::new(&Credential::new(case.value.clone()).unwrap());
        prop_assert_eq!(redactor.redact(&case.text), case.expected.clone());
    }

    // Overlapping occurrences leave no fragment of the credential.
    #[test]
    fn overlapping_occurrences_are_replaced_whole(case in overlap_case()) {
        let redactor = Redactor::new(&Credential::new(case.value.clone()).unwrap());
        prop_assert_eq!(redactor.redact(&case.text), case.expected.clone());
    }

    // A span inside a longer span adds nothing to the marker and leaks
    // nothing.
    #[test]
    fn contained_occurrences_are_replaced_whole(case in contained_case()) {
        let redactor = Redactor::new(&Credential::new(case.value.clone()).unwrap());
        prop_assert_eq!(redactor.redact(&case.text), case.expected.clone());
    }

    // Standard output, standard error, Diagnostics of every AI code and
    // the Transmission_Log carry no form of the value.
    #[test]
    fn no_output_surface_carries_the_value(case in echo_case(6)) {
        let redactor = Redactor::new(&Credential::new(case.value.clone()).unwrap());
        check_out_streams(&case, &redactor)?;
        check_log_line(&case, &redactor)?;
    }

    #[test]
    fn overlapping_occurrences_are_replaced_on_every_surface(case in overlap_case()) {
        prop_assume!(case.value.chars().count() >= 6);
        let redactor = Redactor::new(&Credential::new(case.value.clone()).unwrap());
        check_out_streams(&case, &redactor)?;
        check_log_line(&case, &redactor)?;
    }

    // Debug output never shows the value.
    #[test]
    fn debug_output_never_shows_the_value(value in credential(6)) {
        check_debug(&value)?;
    }
}

// Spans 0..10 ("Bearer ear"), 1..4 and 7..10 all lie inside the first.
// Keeping the end of the last merged span instead of the largest leaks
// the bytes 4..7.
#[test]
fn contained_spans_give_one_marker_and_leak_nothing() {
    let redactor = Redactor::new(&Credential::new("ear".to_owned()).unwrap());
    assert_eq!(redactor.redact("Bearer ear"), "[REDACTED]");
    assert_eq!(
        redactor.redact("x Bearer ear y ear"),
        "x [REDACTED] y [REDACTED]"
    );
}

// Overlap-dense text: every position starts an occurrence. The bound is
// far above the linear cost and far below the quadratic one.
#[test]
fn redaction_time_is_linear_on_overlap_dense_text() {
    let redactor = Redactor::new(&Credential::new("a".repeat(5_000)).unwrap());
    let text = "a".repeat(1_000_000);
    let started = std::time::Instant::now();
    let redacted = redactor.redact(&text);
    let elapsed = started.elapsed();
    assert_eq!(redacted, "[REDACTED]");
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "redaction took {elapsed:?}"
    );
}
