//! Transmission_Log, redaction, output and permission tests (Req 28.8,
//! 28.9, 28.10, 28.16, 28.20; Req 23.19 to 23.23).

use crate::tests::{MapEnv, CANARY};
use crate::{
    load, permission_warning, render_diagnostic, Consent, Credential, FileLog, LogContext, LogSink,
    Out, ProcessEnv, Redactor, TransmissionLog,
};
use proptest::prelude::*;
use rt_ai_adapter::{
    AiAdapter, AttemptOutcome, AttemptReport, MonotonicClock, ProviderFailure, ProviderRequest,
    RequestLimits,
};
use rt_ai_mock::SimulatedClock;
use rt_foundation::{
    Code, Diagnostic, DocumentId, ExitCode, InputId, InputLocation, Location, Severity,
};
use rt_hash::sha256;
use rt_json::Value;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::time::Duration;

const CLAUSE_TEXT: &str = "THE CLAUSE TEXT THAT MUST NOT BE LOGGED";

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
        context: vec![(
            "clauses".to_owned(),
            Value::array(vec![Value::string(CLAUSE_TEXT)]),
        )],
    }
}

fn context() -> LogContext {
    LogContext {
        provider: "openai".to_owned(),
        protocol: "openai-responses".to_owned(),
        model: "model-1".to_owned(),
        endpoint_host: "api.openai.com".to_owned(),
        documents: vec![(DocumentId::new("a.md").unwrap(), sha256(b"document a"))],
        clauses: 3,
    }
}

fn field_at() -> Location {
    Location::Input(InputLocation {
        input: InputId::new("ai.toml").unwrap(),
        line: Some(9),
        column: Some(1),
        field: Some("transmission_log".to_owned()),
    })
}

type Events = Rc<RefCell<Vec<String>>>;

struct VecSink {
    lines: Rc<RefCell<Vec<String>>>,
    events: Events,
    fail: bool,
}

impl LogSink for VecSink {
    fn append(&mut self, line: &[u8]) -> io::Result<()> {
        if self.fail {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "disk"));
        }
        self.events.borrow_mut().push("log".to_owned());
        self.lines
            .borrow_mut()
            .push(String::from_utf8(line.to_vec()).unwrap());
        Ok(())
    }
}

struct Adapter {
    events: Events,
    clock: SimulatedClock,
    attempts: usize,
}

impl AiAdapter for Adapter {
    fn attempt(&mut self, _: &ProviderRequest, _: Duration) -> AttemptReport {
        self.attempts += 1;
        self.events.borrow_mut().push("attempt".to_owned());
        AttemptReport {
            outcome: AttemptOutcome::Failure(ProviderFailure::Timeout),
            elapsed: Duration::from_secs(20),
            detail: None,
        }
    }
    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }
}

struct Rig {
    log: TransmissionLog<VecSink>,
    adapter: Adapter,
    lines: Rc<RefCell<Vec<String>>>,
    events: Events,
}

fn rig(fail: bool, redactor: Redactor) -> Rig {
    let events: Events = Rc::default();
    let lines: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = VecSink {
        lines: lines.clone(),
        events: events.clone(),
        fail,
    };
    Rig {
        log: TransmissionLog::new(sink, redactor, field_at()),
        adapter: Adapter {
            events: events.clone(),
            clock: SimulatedClock::new(),
            attempts: 0,
        },
        lines,
        events,
    }
}

fn canary() -> Credential {
    Credential::new(CANARY.to_owned()).unwrap()
}

#[test]
fn entry_has_exactly_the_specified_fields_and_no_text() {
    let mut r = rig(false, Redactor::no_credential());
    let req = request();
    r.log
        .record(&context(), &req, 1, "2025-01-02T03:04:05Z")
        .unwrap();
    let canonical = req.canonical_bytes().unwrap();
    let expected = format!(
        "{{\"provider\":\"openai\",\"protocol\":\"openai-responses\",\"model\":\"model-1\",\"endpoint_host\":\"api.openai.com\",\"documents\":[{{\"document\":\"a.md\",\"sha256\":\"{}\"}}],\"clauses\":3,\"request_bytes\":{},\"request_id\":\"{}\",\"attempt\":1,\"time\":\"2025-01-02T03:04:05Z\"}}\n",
        sha256(b"document a"),
        canonical.len(),
        sha256(&canonical),
    );
    assert_eq!(r.lines.borrow().as_slice(), std::slice::from_ref(&expected));
    assert!(!expected.contains(CLAUSE_TEXT));
}

#[test]
fn entry_is_written_before_the_attempt_and_each_retry_has_its_own_entry() {
    let mut r = rig(false, Redactor::no_credential());
    let req = request();
    for attempt in 1..=3 {
        let report = r
            .log
            .attempt_logged(
                &mut r.adapter,
                &context(),
                &req,
                Duration::from_secs(20),
                attempt,
                "2025-01-02T03:04:05Z",
            )
            .unwrap();
        assert_eq!(
            report.outcome,
            AttemptOutcome::Failure(ProviderFailure::Timeout)
        );
    }
    assert_eq!(
        r.events.borrow().as_slice(),
        ["log", "attempt", "log", "attempt", "log", "attempt"]
    );
    let lines = r.lines.borrow();
    for (i, line) in lines.iter().enumerate() {
        assert!(line.contains(&format!("\"attempt\":{}", i + 1)), "{line}");
    }
}

#[test]
fn failed_append_is_rt_ai_0010_exit_70_and_nothing_is_sent() {
    let mut r = rig(true, Redactor::no_credential());
    let d = r
        .log
        .attempt_logged(
            &mut r.adapter,
            &context(),
            &request(),
            Duration::from_secs(20),
            1,
            "t",
        )
        .unwrap_err();
    assert_eq!(d.code(), Code::AiTransmissionLogFailed);
    assert_eq!(d.severity(), Severity::Error);
    assert_eq!(d.code().exit_code(), ExitCode::Internal);
    assert_eq!(d.primary(), &field_at());
    assert_eq!(r.adapter.attempts, 0);
    assert!(r.events.borrow().is_empty());
}

#[test]
fn request_without_canonical_bytes_is_refused_before_sending() {
    let mut bad = request();
    bad.context.push(("task".to_owned(), Value::string("dup")));
    let mut r = rig(false, Redactor::no_credential());
    let d = r
        .log
        .attempt_logged(
            &mut r.adapter,
            &context(),
            &bad,
            Duration::from_secs(20),
            1,
            "t",
        )
        .unwrap_err();
    assert_eq!(d.code(), Code::AiTransmissionLogFailed);
    assert_eq!(r.adapter.attempts, 0);
}

#[test]
fn log_line_is_redacted_even_when_a_field_carries_the_credential() {
    let mut r = rig(false, Redactor::new(&canary()));
    let mut ctx = context();
    ctx.model = format!("m-{CANARY}");
    r.log.record(&ctx, &request(), 1, "t").unwrap();
    let line = r.lines.borrow()[0].clone();
    assert!(!line.contains(CANARY), "{line}");
    assert!(line.contains("m-[REDACTED]"));
}

#[test]
fn file_log_appends_lines_and_keeps_earlier_ones() {
    let scratch = crate::tests_scratch::ScratchDir::new("log-append");
    let path = scratch.path().join("append.jsonl");
    let mut log = TransmissionLog::without_credential(FileLog::new(path.clone()), field_at());
    log.record(&context(), &request(), 1, "t1").unwrap();
    log.record(&context(), &request(), 2, "t2").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("\"attempt\":1") && lines[1].contains("\"attempt\":2"));
    assert!(text.ends_with('\n'));
}

#[test]
fn file_log_in_a_missing_directory_fails_with_rt_ai_0010() {
    let scratch = crate::tests_scratch::ScratchDir::new("log-missing");
    let path = scratch
        .path()
        .join("absent")
        .join("nested")
        .join("tx.jsonl");
    let mut log = TransmissionLog::without_credential(FileLog::new(path.clone()), field_at());
    let d = log.record(&context(), &request(), 1, "t").unwrap_err();
    assert_eq!(d.code(), Code::AiTransmissionLogFailed);
    assert!(!path.exists());
}

#[test]
fn redactor_replaces_value_bearer_form_and_json_escaped_form() {
    let tricky = Credential::new("ab\"c\\d".to_owned()).unwrap();
    let r = Redactor::new(&tricky);
    assert_eq!(r.redact("x ab\"c\\d y"), "x [REDACTED] y");
    assert_eq!(
        r.redact("Authorization: Bearer ab\"c\\d."),
        "Authorization: [REDACTED]."
    );
    assert_eq!(
        r.redact("{\"k\":\"ab\\\"c\\\\d\"}"),
        "{\"k\":\"[REDACTED]\"}"
    );
    let r = Redactor::new(&canary());
    assert_eq!(
        r.redact(&format!("{CANARY}{CANARY}")),
        "[REDACTED][REDACTED]"
    );
    assert_eq!(r.redact("no secret here"), "no secret here");
}

#[test]
fn credential_cannot_be_empty() {
    assert!(Credential::new(String::new()).is_none());
}

struct Shared(Rc<RefCell<Vec<u8>>>);

impl io::Write for Shared {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

type Buffer = Rc<RefCell<Vec<u8>>>;

fn captured() -> (Out<Shared, Shared>, Buffer, Buffer) {
    let o: Rc<RefCell<Vec<u8>>> = Rc::default();
    let e: Rc<RefCell<Vec<u8>>> = Rc::default();
    (
        Out::without_credential(Shared(o.clone()), Shared(e.clone())),
        o,
        e,
    )
}

#[test]
fn out_writes_redacted_text_to_the_right_stream() {
    let (mut out, o, e) = captured();
    out.stdout(&format!("before {CANARY}\n")).unwrap();
    out.set_redactor(Redactor::new(&canary()));
    out.stdout(&format!("summary {CANARY}\n")).unwrap();
    out.stderr(&format!("Bearer {CANARY} failed\n")).unwrap();
    let d = Diagnostic::new(
        Code::AiProviderError,
        format!("provider echoed {CANARY}"),
        field_at(),
    );
    out.diagnostic(&d).unwrap();
    let o = String::from_utf8(o.borrow().clone()).unwrap();
    let e = String::from_utf8(e.borrow().clone()).unwrap();
    assert_eq!(o, format!("before {CANARY}\nsummary [REDACTED]\n"));
    assert_eq!(
        e,
        "[REDACTED] failed\nerror RT-AI-0105: provider echoed [REDACTED]\n  at input 'ai.toml' line 9 column 1 field transmission_log\n"
    );
}

#[test]
fn rejected_values_never_reach_the_rendered_diagnostics() {
    let secret_values = [
        ("model = \"model-1\"", "model = \"sk-CANARYSECRET1\""),
        (
            "https://api.openai.com",
            "https://u:CANARYSECRET1@api.openai.com",
        ),
        (
            "https://api.openai.com",
            "https://api.openai.com/?k=CANARYSECRET1",
        ),
        ("PROVIDER_CREDENTIAL", "canarysecret1"),
    ];
    let base = "schema_version = 1\nenabled = true\nprovider = \"openai-responses\"\nmodel = \"model-1\"\nendpoint = \"https://api.openai.com\"\ncredential_env_var = \"PROVIDER_CREDENTIAL\"\nallow_document_text = true\nai_identities = [\"x\"]\ntransmission_log = \"tx\"\n";
    let docs: std::collections::BTreeSet<DocumentId> = [DocumentId::new("a.md").unwrap()].into();
    for (from, to) in secret_values {
        let text = base.replace(from, to);
        let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CANARY)]);
        let consent = Consent::named(docs.clone(), Location::None(Code::InternalError));
        let found = load(
            &InputId::new("ai.toml").unwrap(),
            &text,
            &consent,
            &docs,
            &env,
        )
        .unwrap_err();
        assert!(env.reads().is_empty());
        for d in found {
            assert_eq!(d.code(), Code::AiConfigInlineSecret);
            let shown = render_diagnostic(&d);
            assert!(!shown.to_lowercase().contains("canarysecret1"), "{shown}");
        }
    }
}

#[test]
fn credential_read_from_the_environment_is_redacted_in_provider_errors() {
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CANARY)]);
    let docs: std::collections::BTreeSet<DocumentId> = [DocumentId::new("a.md").unwrap()].into();
    let consent = Consent::named(docs.clone(), Location::None(Code::InternalError));
    let text = "schema_version = 1\nenabled = true\nprovider = \"openai-responses\"\nmodel = \"model-1\"\nendpoint = \"https://api.openai.com\"\ncredential_env_var = \"PROVIDER_CREDENTIAL\"\nallow_document_text = true\nai_identities = [\"x\"]\ntransmission_log = \"tx\"\n";
    let ok = load(
        &InputId::new("ai.toml").unwrap(),
        text,
        &consent,
        &docs,
        &env,
    )
    .unwrap();
    let (mut out, o, e) = captured();
    out.set_redactor(Redactor::new(ok.credential.as_ref().unwrap()));
    let echoed = Diagnostic::new(
        Code::AiProviderError,
        format!("server said: bad key {CANARY}"),
        field_at(),
    );
    out.diagnostic(&echoed).unwrap();
    out.stdout(&format!("{{\"echo\":\"{CANARY}\"}}\n")).unwrap();
    for stream in [o, e] {
        let s = String::from_utf8(stream.borrow().clone()).unwrap();
        assert!(!s.contains(CANARY), "{s}");
    }
}

#[test]
fn permission_warning_depends_only_on_group_and_other_read_bits() {
    let input = InputId::new("ai.toml").unwrap();
    for mode in [0o100_600, 0o600, 0o700, 0o200, 0o020, 0o002, 0o120] {
        assert!(permission_warning(&input, Some(mode)).is_none(), "{mode:o}");
    }
    for mode in [0o100_640, 0o604, 0o644, 0o040, 0o004, 0o777] {
        let d = permission_warning(&input, Some(mode)).unwrap();
        assert_eq!(
            (d.code(), d.severity()),
            (Code::AiConfigPermissions, Severity::Warning)
        );
        assert_eq!(d.code().exit_code(), ExitCode::Success);
        assert_eq!(
            d.primary(),
            &Location::Input(InputLocation {
                input: input.clone(),
                line: None,
                column: None,
                field: None
            })
        );
    }
    assert!(permission_warning(&input, None).is_none());
}

#[cfg(unix)]
#[test]
fn file_mode_reports_the_real_permission_bits() {
    use crate::file_mode;
    use std::os::unix::fs::PermissionsExt;
    let scratch = crate::tests_scratch::ScratchDir::new("log-mode");
    let path = scratch.path().join("mode.toml");
    std::fs::write(&path, "x").unwrap();
    let input = InputId::new("ai.toml").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(permission_warning(&input, file_mode(&path)).is_some());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(permission_warning(&input, file_mode(&path)).is_none());
    std::fs::remove_file(&path).unwrap();
    assert_eq!(file_mode(&path), None);
}

// Req 28.10 applies only where the platform reports permissions.
#[cfg(not(unix))]
#[test]
fn file_mode_reports_nothing_where_the_platform_has_no_permission_bits() {
    let scratch = crate::tests_scratch::ScratchDir::new("log-mode");
    let path = scratch.path().join("mode.toml");
    std::fs::write(&path, "x").unwrap();
    assert_eq!(crate::file_mode(&path), None);
}

#[test]
fn process_env_reports_an_unset_variable_as_none() {
    use crate::EnvReader;
    assert_eq!(
        ProcessEnv.get("RT_AI_TOOL_TEST_VARIABLE_THAT_IS_NOT_SET"),
        None
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    // The credential never survives redaction, in any surrounding text or
    // form, and text without the credential is unchanged.
    #[test]
    fn redaction_removes_every_form_of_the_credential(
        value in "[ -~]{8,40}",
        before in "[a-z ]{0,20}",
        after in "[a-z ]{0,20}",
        form in 0usize..4,
    ) {
        let credential = Credential::new(value.clone()).unwrap();
        let r = Redactor::new(&credential);
        let shown = match form {
            0 => value.clone(),
            1 => format!("Bearer {value}"),
            2 => format!("{value}{value}"),
            _ => value.replace('\\', "\\\\").replace('"', "\\\""),
        };
        let text = format!("{before}{shown}{after}");
        let out = r.redact(&text);
        prop_assert!(!out.contains(&value), "{out}");
        prop_assert!(out.contains("[REDACTED]"));
        let clean = format!("{before}{after}");
        prop_assume!(!clean.contains(&value));
        prop_assert_eq!(r.redact(&clean), clean);
    }
}

#[test]
fn credential_with_a_control_character_is_redacted_in_every_rendered_form() {
    let value = "ctl-ab\u{1b}cd\u{7}-tail9";
    let credential = Credential::new(value.to_owned()).unwrap();
    let (mut out, o, e) = captured();
    out.set_redactor(Redactor::new(&credential));
    let d = Diagnostic::new(
        Code::AiProviderError,
        format!("server said: bad key {value} and Bearer {value}"),
        field_at(),
    );
    out.diagnostic(&d).unwrap();
    out.stdout(&format!("raw {value}\n")).unwrap();
    out.stdout("escaped ctl-ab\\u{1b}cd\\u{7}-tail9\n").unwrap();
    let all = [o, e]
        .iter()
        .map(|b| String::from_utf8(b.borrow().clone()).unwrap())
        .collect::<String>();
    assert!(!all.contains("tail9"), "{all:?}");
    assert!(!all.contains("ctl-ab"), "{all:?}");
    assert!(all.contains("[REDACTED]"));
}

#[test]
fn redactor_replaces_overlapping_occurrences_with_one_marker() {
    let quotes = Credential::new("\"".repeat(6)).unwrap();
    let r = Redactor::new(&quotes);
    assert_eq!(r.redact(&"\"".repeat(11)), "[REDACTED]");
    assert_eq!(r.redact(&"\"".repeat(5)), "\"\"\"\"\"");
}

#[test]
fn redactor_never_rewrites_an_earlier_marker() {
    let bracket = Credential::new("[".to_owned()).unwrap();
    let r = Redactor::new(&bracket);
    assert_eq!(r.redact("Bearer ["), "[REDACTED]");
    assert_eq!(r.redact("a[b"), "a[REDACTED]b");
}
