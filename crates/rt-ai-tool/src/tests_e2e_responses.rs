//! End to end through `run_with` with the real `rt-ai-responses` adapter
//! against a loopback stub server (task 10.5, Req 35.12 to 35.20).
//!
//! Every expected code, exit code and count is written by hand from the
//! catalogue and the requirements. The retry clock is simulated, so no test
//! sleeps. The stub is the one of the integration crate's own tests.

#[path = "../../rt-ai-responses/tests/support/mod.rs"]
pub(crate) mod stub;

use crate::tests::MapEnv;
use crate::tests_cli_rig::Fixture;
use crate::tests_config_v2::V2;
use crate::tests_gate_rig::{run_providers, COMPLETE_ONE};
use crate::tests_redact_fragments::leaked;
use crate::tests_rig::payload;
use crate::{responses_adapter, AiConfig, Credential, ProviderKind, Providers, StdFs};
use rt_ai_adapter::{AdapterInfo, AiAdapter, ModelCatalog, MonotonicClock};
use rt_foundation::{Diagnostic, ExitCode};
use rt_hash::sha256;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;
use stub::{http, json_ok, message_envelope, Reply, Stub};

// No run of eight characters of it may occur in a field name or a message.
pub(crate) const CANARY: &str = "zq81xk4mp2-Qw7Lb3Rt9";

const USAGE: &str = r#","usage":{"input_tokens":11,"output_tokens":22,"total_tokens":33}"#;

// A clock that advances only through `wait` and remembers every wait.
#[derive(Clone, Default)]
pub(crate) struct Waits {
    pub(crate) now: Rc<Cell<Duration>>,
    pub(crate) waits: Rc<RefCell<Vec<Duration>>>,
}

struct Recording(Waits);

impl MonotonicClock for Recording {
    fn now(&self) -> Duration {
        self.0.now.get()
    }

    fn wait(&self, d: Duration) {
        self.0.now.set(self.0.now.get() + d);
        self.0.waits.borrow_mut().push(d);
    }
}

// The built-in hosted provider with the clock replaced.
pub(crate) struct Stubbed(pub(crate) Waits);

impl Providers for Stubbed {
    fn is_linked(&self, kind: ProviderKind) -> bool {
        kind == ProviderKind::OpenAiResponses
    }

    fn adapter<'a>(
        &self,
        config: &AiConfig,
        credential: Option<&Credential>,
    ) -> Result<(Box<dyn AiAdapter + 'a>, String), Diagnostic> {
        let credential = credential.expect("a hosted run reads a credential");
        let adapter =
            responses_adapter(config, credential)?.with_clock(Box::new(Recording(self.0.clone())));
        let version = adapter.identity().adapter_version;
        Ok((Box::new(adapter), version))
    }

    fn catalog<'a>(
        &self,
        config: &AiConfig,
        credential: Option<&Credential>,
    ) -> Result<Box<dyn ModelCatalog + 'a>, Diagnostic> {
        let credential = credential.expect("a hosted run reads a credential");
        let adapter =
            responses_adapter(config, credential)?.with_clock(Box::new(Recording(self.0.clone())));
        Ok(Box::new(adapter))
    }
}

pub(crate) struct Rig {
    pub(crate) fixture: Fixture,
    pub(crate) waits: Waits,
}

pub(crate) struct Limits {
    pub(crate) retries: u8,
    pub(crate) response_bytes: u32,
}

pub(crate) const DEFAULT: Limits = Limits {
    retries: 2,
    response_bytes: 1_048_576,
};

pub(crate) fn config_text(fixture: &Fixture, endpoint: &str, limits: &Limits) -> String {
    let log = crate::tests_cli_rig::toml_path(&fixture.path("work/tx.jsonl"));
    V2.replace(
        "transmission_log = \"tx.jsonl\"",
        &format!("transmission_log = \"{log}\""),
    )
    .replace("https://api.meta.ai", endpoint)
    .replace(
        "max_retries = 0",
        &format!("max_retries = {}", limits.retries),
    )
    .replace(
        "max_response_bytes = 1048576",
        &format!("max_response_bytes = {}", limits.response_bytes),
    )
}

impl Rig {
    pub(crate) fn new(name: &str, endpoint: &str, limits: &Limits) -> Self {
        let fixture = Fixture::new(name);
        fixture.write_config(&config_text(&fixture, endpoint, limits));
        std::fs::create_dir_all(fixture.path("work")).unwrap();
        Self {
            fixture,
            waits: Waits::default(),
        }
    }

    pub(crate) fn run(&self) -> crate::tests_cli_rig::Ran {
        let env = MapEnv::new(&[("MODEL_API_KEY", CANARY)]);
        let ran = run_providers(
            &self.fixture.consented(),
            &env,
            &Stubbed(self.waits.clone()),
            &StdFs,
        );
        self.assert_no_canary(&ran);
        ran
    }

    pub(crate) fn batch_path(&self) -> std::path::PathBuf {
        let id = self.fixture.request_id();
        self.fixture.store_file(&format!("batches/{id}.json"))
    }

    pub(crate) fn waits(&self) -> Vec<Duration> {
        self.waits.waits.borrow().clone()
    }

    pub(crate) fn log_lines(&self) -> Vec<String> {
        std::fs::read_to_string(self.fixture.path("work/tx.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    // The canary appears in no output, file, log or summary, whole or as a
    // run of eight characters.
    pub(crate) fn assert_no_canary(&self, ran: &crate::tests_cli_rig::Ran) {
        for text in [&ran.stdout, &ran.stderr] {
            assert_eq!(leaked(text, CANARY), None, "{text}");
        }
        for name in self.fixture.files() {
            let bytes = std::fs::read(self.fixture.path(&name)).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            assert_eq!(leaked(&text, CANARY), None, "{name}");
        }
    }
}

pub(crate) fn success(extra: &str) -> Reply {
    json_ok(&message_envelope(&payload(0), extra))
}

pub(crate) fn errors(ran: &crate::tests_cli_rig::Ran) -> Vec<String> {
    ran.shapes()
        .into_iter()
        .filter(|(severity, _)| severity == "error")
        .map(|(_, id)| id)
        .collect()
}

pub(crate) fn warnings(ran: &crate::tests_cli_rig::Ran) -> Vec<String> {
    ran.shapes()
        .into_iter()
        .filter(|(severity, _)| severity == "warning")
        .map(|(_, id)| id)
        .collect()
}

pub(crate) fn store_is_empty(fixture: &Fixture) -> bool {
    fixture
        .files()
        .iter()
        .all(|f| !f.starts_with("store/batches/"))
}

pub(crate) fn exit(code: ExitCode) -> u8 {
    code.value()
}

#[test]
fn success_with_usage_stores_a_version_2_batch_with_the_reported_metadata() {
    let stub = Stub::start(vec![success(USAGE)]);
    let rig = Rig::new("e2e-usage", &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert_eq!(exit(ran.code), 0, "{}", ran.stderr);
    assert_eq!(ran.stdout, COMPLETE_ONE);
    assert!(errors(&ran).is_empty(), "{}", ran.stderr);
    assert!(
        ran.stderr.contains(
            ": provider meta, endpoint host 127.0.0.1, documents ['rules.md'], clauses 1,"
        ),
        "{}",
        ran.stderr
    );
    assert_eq!(stub.connections(), 1);
    let batch = std::fs::read_to_string(rig.batch_path()).unwrap();
    for member in [
        r#""version":2"#,
        r#""provider":"meta""#,
        r#""protocol":"openai-responses""#,
        r#""model":"muse-spark-1.3""#,
        r#""reported_model":"muse-reported""#,
        r#""response_id":"resp_1""#,
        r#""usage":{"input_tokens":11,"output_tokens":22,"total_tokens":33}"#,
        r#""adapter_version":"responses-1/prompt-3""#,
    ] {
        assert!(batch.contains(member), "{member} missing in {batch}");
    }
    assert!(!batch.contains("null"), "{batch}");
    let captured = stub.finish();
    assert_eq!(captured.len(), 1);
}

#[test]
fn success_without_usage_stores_no_usage_member_and_no_null() {
    let stub = Stub::start(vec![success("")]);
    let rig = Rig::new("e2e-no-usage", &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert_eq!(exit(ran.code), 0, "{}", ran.stderr);
    let batch = std::fs::read_to_string(rig.batch_path()).unwrap();
    assert!(
        batch.contains(r#""reported_model":"muse-reported""#),
        "{batch}"
    );
    assert!(!batch.contains("usage"), "{batch}");
    assert!(!batch.contains("null"), "{batch}");
    drop(stub.finish());
}

#[test]
fn the_configured_model_is_authoritative_over_the_reported_one() {
    let stub = Stub::start(vec![success("")]);
    let rig = Rig::new("e2e-model", &stub.endpoint(), &DEFAULT);
    assert_eq!(exit(rig.run().code), 0);
    let batch = std::fs::read_to_string(rig.batch_path()).unwrap();
    assert!(batch.contains(r#""model":"muse-spark-1.3""#), "{batch}");
    assert!(
        batch.contains(r#""reported_model":"muse-reported""#),
        "{batch}"
    );
    assert!(!batch.contains(r#""model":"muse-reported""#), "{batch}");
    drop(stub.finish());
}

// The stub received exactly the request that the plan hashes to the
// Request_Identifier of the dry run, wrapped in the pinned body layout.
#[test]
fn the_stub_receives_the_exact_request_bytes() {
    let stub = Stub::start(vec![success("")]);
    let rig = Rig::new("e2e-bytes", &stub.endpoint(), &DEFAULT);
    let id = rig.fixture.request_id();
    assert_eq!(exit(rig.run().code), 0);
    let captured = stub.finish();
    let sent = &captured[0];
    assert_eq!(sent.request_line(), "POST /v1/responses HTTP/1.1");
    let headers = sent.headers();
    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    let bearer = format!("Bearer {CANARY}");
    assert_eq!(header("authorization"), Some(bearer.as_str()));
    let body = std::str::from_utf8(sent.body()).unwrap();
    assert!(!body.contains(CANARY));
    let preamble = rt_ai_responses::INPUT_PREAMBLE
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    let head = format!(r#"{{"model":"muse-spark-1.3","input":"{preamble}"#);
    let tail_at = body
        .find(r#"","text":{"format":{"type":"json_schema","name":"ruletrace_proposals","schema":"#)
        .unwrap();
    assert!(body.starts_with(&head), "{body}");
    let escaped_request = &body[head.len()..tail_at];
    let canonical = unescape(escaped_request);
    assert_eq!(sha256(canonical.as_bytes()).as_str(), id);
    assert!(canonical.contains("Alpha beta."), "{canonical}");
    let schema = crate::proposal_schema_bytes(crate::Ud32Option::CONFIRMED).unwrap();
    let compact = rt_json::write_compact(
        &rt_json::parse(
            &rt_foundation::InputId::new("schema").unwrap(),
            &schema,
            rt_json::ParseLimits::new(1 << 20, 64),
        )
        .unwrap(),
    );
    let want_tail = format!(
        r#"","text":{{"format":{{"type":"json_schema","name":"ruletrace_proposals","schema":{compact},"strict":true}}}},"store":false,"max_output_tokens":8192,"reasoning":{{"effort":"low"}}}}"#
    );
    assert_eq!(&body[tail_at..], want_tail);
}

fn unescape(text: &str) -> String {
    let quoted = format!("\"{text}\"");
    let id = rt_foundation::InputId::new("string").unwrap();
    let value = rt_json::parse(
        &id,
        quoted.as_bytes(),
        rt_json::ParseLimits::new(1 << 24, 4),
    )
    .unwrap();
    value.as_str().unwrap().to_owned()
}

#[test]
fn a_401_is_rt_ai_0104_exit_5_after_one_attempt_and_stores_nothing() {
    let stub = Stub::start(vec![http(
        "401 Unauthorized",
        &[("Content-Type", "application/json")],
        br#"{"error":{"message":"bad key","type":"invalid_api_key","code":null}}"#,
    )]);
    let rig = Rig::new("e2e-401", &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert_eq!(exit(ran.code), 5, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0104"], "{}", ran.stderr);
    assert!(warnings(&ran).is_empty(), "{}", ran.stderr);
    assert_eq!(stub.connections(), 1);
    assert!(rig.waits().is_empty());
    assert!(store_is_empty(&rig.fixture));
    assert_eq!(rig.log_lines().len(), 1);
    assert!(
        ran.stdout.starts_with("run summary: incomplete\n"),
        "{}",
        ran.stdout
    );
    drop(stub.finish());
}

#[test]
fn a_429_with_retry_after_waits_the_longer_time_on_the_simulated_clock_then_succeeds() {
    let stub = Stub::start(vec![
        http(
            "429 Too Many Requests",
            &[("Retry-After", "30")],
            br#"{"error":{"type":"rate_limit"}}"#,
        ),
        success(USAGE),
    ]);
    let rig = Rig::new("e2e-429", &stub.endpoint(), &DEFAULT);
    let started = std::time::Instant::now();
    let ran = rig.run();
    assert_eq!(exit(ran.code), 0, "{}", ran.stderr);
    assert_eq!(
        warnings(&ran).first().map(String::as_str),
        Some("RT-AI-0103")
    );
    assert_eq!(stub.connections(), 2);
    assert_eq!(rig.waits(), [Duration::from_secs(30)]);
    assert_eq!(rig.waits.now.get(), Duration::from_secs(30));
    // The wait is 30 s on the simulated clock. A real sleep would take at
    // least that long, so this bound has a wide margin over a loaded run.
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "the test slept"
    );
    assert_eq!(rig.log_lines().len(), 2);
    assert!(rig.batch_path().exists());
    drop(stub.finish());
}

#[test]
fn a_refusal_is_rt_ai_0205_exit_6_and_stores_nothing() {
    let refusal = r#"{"id":"resp_r","object":"response","status":"completed","model":"m","output":[{"type":"message","id":"msg","status":"completed","role":"assistant","content":[{"type":"refusal","refusal":"no"}]}]}"#;
    let stub = Stub::start(vec![json_ok(refusal)]);
    let rig = Rig::new("e2e-refusal", &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert_eq!(exit(ran.code), 6, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0205"], "{}", ran.stderr);
    assert!(store_is_empty(&rig.fixture));
    assert_eq!(stub.connections(), 1);
    drop(stub.finish());
}

#[test]
fn an_incomplete_response_is_rt_ai_0205_exit_6_and_stores_nothing() {
    let incomplete = r#"{"id":"resp_i","object":"response","status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"model":"m","output":[]}"#;
    let stub = Stub::start(vec![json_ok(incomplete)]);
    let rig = Rig::new("e2e-incomplete", &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert_eq!(exit(ran.code), 6, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0205"], "{}", ran.stderr);
    assert!(store_is_empty(&rig.fixture));
    drop(stub.finish());
}

#[test]
fn malformed_json_is_rt_ai_0201_exit_6() {
    let stub = Stub::start(vec![json_ok("{not json")]);
    let rig = Rig::new("e2e-malformed", &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert_eq!(exit(ran.code), 6, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0201"], "{}", ran.stderr);
    assert!(store_is_empty(&rig.fixture));
    drop(stub.finish());
}

#[test]
fn an_envelope_with_a_tool_call_is_rt_ai_0201_exit_6() {
    let call = r#"{"id":"r","status":"completed","output":[{"type":"function_call","name":"x","arguments":"{}"}]}"#;
    let stub = Stub::start(vec![json_ok(call)]);
    let rig = Rig::new("e2e-tool", &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert_eq!(errors(&ran), ["RT-AI-0201"], "{}", ran.stderr);
    assert_eq!(exit(ran.code), 6);
    drop(stub.finish());
}

#[test]
fn a_body_of_limit_plus_one_bytes_is_rt_ai_0203_exit_6() {
    let limit = 1024_u32;
    let body = vec![b' '; usize::try_from(limit).unwrap() + 1];
    let stub = Stub::start(vec![http("200 OK", &[], &body)]);
    let rig = Rig::new(
        "e2e-oversize",
        &stub.endpoint(),
        &Limits {
            retries: 2,
            response_bytes: limit,
        },
    );
    let ran = rig.run();
    assert_eq!(exit(ran.code), 6, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0203"], "{}", ran.stderr);
    assert!(store_is_empty(&rig.fixture));
    drop(stub.finish());
}

#[test]
fn a_much_larger_body_is_also_rt_ai_0203() {
    let body = vec![b' '; 50_000];
    let stub = Stub::start(vec![http("200 OK", &[], &body)]);
    let rig = Rig::new(
        "e2e-oversize-big",
        &stub.endpoint(),
        &Limits {
            retries: 2,
            response_bytes: 1024,
        },
    );
    let ran = rig.run();
    assert_eq!(errors(&ran), ["RT-AI-0203"], "{}", ran.stderr);
    drop(stub.finish());
}

// Req 29.2 as decided on 2026-11-16 (UD-37 OD-37-1, option B): a 504 is a
// final failure for every adapter. With two retries configured the run
// still makes one attempt, waits never, ends with RT-AI-0105 and exit 5,
// stores nothing, and writes one Transmission_Log entry.
const PROVIDER_TEXT: &str = "provider-sentinel-gateway-text";

fn gateway() -> Reply {
    http(
        "504 Gateway Timeout",
        &[],
        format!(r#"{{"error":{{"type":"gateway_timeout","message":"{PROVIDER_TEXT}"}}}}"#)
            .as_bytes(),
    )
}

fn assert_final_504(name: &str, limits: &Limits) {
    let stub = Stub::start(vec![gateway(), gateway(), gateway()]);
    let rig = Rig::new(name, &stub.endpoint(), limits);
    let ran = rig.run();
    assert_eq!(exit(ran.code), 5, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0105"], "{}", ran.stderr);
    assert!(warnings(&ran).is_empty(), "{}", ran.stderr);
    assert_eq!(stub.connections(), 1, "attempts");
    assert!(rig.waits().is_empty());
    assert_eq!(rig.waits.now.get(), Duration::ZERO, "simulated clock");
    assert!(store_is_empty(&rig.fixture));
    let log = rig.log_lines();
    assert_eq!(log.len(), 1, "{log:?}");
    assert!(log[0].contains(&rig.fixture.request_id()), "{log:?}");
    assert!(ran.stderr.contains("HTTP status 504"), "{}", ran.stderr);
    assert!(!ran.stderr.contains(PROVIDER_TEXT), "{}", ran.stderr);
    drop(stub.finish());
}

#[test]
fn a_504_is_final_after_one_attempt_with_two_retries_configured() {
    assert_final_504("e2e-504", &DEFAULT);
}

#[test]
fn a_504_is_final_after_one_attempt_with_max_retries_zero() {
    assert_final_504(
        "e2e-504-zero",
        &Limits {
            retries: 0,
            response_bytes: 1_048_576,
        },
    );
}

#[test]
fn a_500_a_502_and_a_503_are_still_retried_on_the_schedule() {
    for (status, name) in [
        ("500 Internal Server Error", "e2e-5xx-500"),
        ("502 Bad Gateway", "e2e-5xx-502"),
        ("503 Service Unavailable", "e2e-5xx-503"),
    ] {
        let reply = || http(status, &[], b"{}");
        let stub = Stub::start(vec![reply(), reply(), reply()]);
        let rig = Rig::new(name, &stub.endpoint(), &DEFAULT);
        let ran = rig.run();
        assert_eq!(exit(ran.code), 5, "{status}: {}", ran.stderr);
        assert_eq!(errors(&ran), ["RT-AI-0106"], "{status}: {}", ran.stderr);
        assert_eq!(
            warnings(&ran),
            ["RT-AI-0101", "RT-AI-0101", "RT-AI-0101"],
            "{status}"
        );
        assert_eq!(stub.connections(), 3, "{status}");
        assert_eq!(
            rig.waits(),
            [Duration::from_secs(2), Duration::from_secs(8)],
            "{status}"
        );
        assert!(store_is_empty(&rig.fixture));
        let captured = stub.finish();
        assert!(captured.iter().all(|c| c.body() == captured[0].body()));
    }
}

#[test]
fn a_400_is_final_rt_ai_0105_and_never_retried() {
    let stub = Stub::start(vec![http("400 Bad Request", &[], b"{}")]);
    let rig = Rig::new("e2e-400", &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert_eq!(exit(ran.code), 5, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0105"], "{}", ran.stderr);
    assert_eq!(stub.connections(), 1);
    drop(stub.finish());
}

// No fallback: after failures the same configured model is sent again,
// and no other model name appears in any request.
#[test]
fn every_attempt_names_the_configured_model_and_no_other_model_is_tried() {
    let stub = Stub::start(vec![
        http("503 Service Unavailable", &[("Retry-After", "60")], b""),
        success(""),
    ]);
    let rig = Rig::new("e2e-no-fallback", &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert_eq!(exit(ran.code), 0, "{}", ran.stderr);
    // The Retry-After of a 503 is treated as absent (Req 29.3).
    assert_eq!(rig.waits(), [Duration::from_secs(2)]);
    let captured = stub.finish();
    assert_eq!(captured.len(), 2);
    for sent in &captured {
        let body = std::str::from_utf8(sent.body()).unwrap();
        assert!(body.starts_with(r#"{"model":"muse-spark-1.3","#), "{body}");
    }
}

#[test]
fn a_redirect_is_not_followed_and_nothing_is_sent_to_its_target() {
    let target = Stub::start(vec![success("")]);
    let location = format!("{}/stolen", target.endpoint());
    let stub = Stub::start(vec![http(
        "302 Found",
        &[("Location", location.as_str())],
        b"",
    )]);
    let rig = Rig::new("e2e-redirect", &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert_eq!(exit(ran.code), 5, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0105"], "{}", ran.stderr);
    assert_eq!(stub.connections(), 1);
    assert_eq!(target.connections(), 0);
    assert!(target.finish().is_empty());
    drop(stub.finish());
}

#[test]
fn the_transmission_log_names_no_document_text_and_no_credential() {
    let stub = Stub::start(vec![success("")]);
    let rig = Rig::new("e2e-log", &stub.endpoint(), &DEFAULT);
    assert_eq!(exit(rig.run().code), 0);
    let lines = rig.log_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with(r#"{"provider":"meta","protocol":"openai-responses","model":"#),
        "{}",
        lines[0]
    );
    assert!(
        lines[0].contains(r#""model":"muse-spark-1.3""#),
        "{}",
        lines[0]
    );
    assert!(
        lines[0].contains(r#""endpoint_host":"127.0.0.1""#),
        "{}",
        lines[0]
    );
    assert!(!lines[0].contains("Alpha beta"), "{}", lines[0]);
    drop(stub.finish());
}

#[test]
fn a_dry_run_sends_nothing_and_reads_no_credential() {
    let stub = Stub::start(vec![success("")]);
    let rig = Rig::new("e2e-dry", &stub.endpoint(), &DEFAULT);
    let env = MapEnv::new(&[("MODEL_API_KEY", CANARY)]);
    let ran = run_providers(
        &rig.fixture.args(&["--dry-run"]),
        &env,
        &Stubbed(rig.waits.clone()),
        &StdFs,
    );
    assert_eq!(exit(ran.code), 0, "{}", ran.stderr);
    assert!(env.reads().is_empty());
    assert_eq!(stub.connections(), 0);
    assert!(rig.log_lines().is_empty());
    drop(stub.finish());
}

#[test]
fn missing_consent_stops_before_any_connection() {
    let stub = Stub::start(vec![success("")]);
    let rig = Rig::new("e2e-consent", &stub.endpoint(), &DEFAULT);
    let env = MapEnv::new(&[("MODEL_API_KEY", CANARY)]);
    let ran = run_providers(
        &rig.fixture.args(&[]),
        &env,
        &Stubbed(rig.waits.clone()),
        &StdFs,
    );
    assert_eq!(exit(ran.code), 2, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0008"], "{}", ran.stderr);
    assert_eq!(stub.connections(), 0);
    drop(stub.finish());
}

// A credential that the clause text contains is refused by the request
// scan before the log entry and before any send (Req 28.14).
#[test]
fn a_credential_inside_the_clause_text_is_rt_ai_0308_and_nothing_is_sent() {
    let stub = Stub::start(vec![success("")]);
    let rig = Rig::new("e2e-scan", &stub.endpoint(), &DEFAULT);
    let env = MapEnv::new(&[("MODEL_API_KEY", "Alpha")]);
    let ran = run_providers(
        &rig.fixture.consented(),
        &env,
        &Stubbed(rig.waits.clone()),
        &StdFs,
    );
    assert_eq!(exit(ran.code), 2, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0308"], "{}", ran.stderr);
    assert_eq!(stub.connections(), 0);
    assert!(rig.log_lines().is_empty());
    assert!(store_is_empty(&rig.fixture));
    drop(stub.finish());
}

#[test]
fn a_second_run_is_idempotent_and_sends_nothing() {
    let stub = Stub::start(vec![success("")]);
    let rig = Rig::new("e2e-idem", &stub.endpoint(), &DEFAULT);
    assert_eq!(exit(rig.run().code), 0);
    let again = rig.run();
    assert_eq!(exit(again.code), 0, "{}", again.stderr);
    assert!(
        warnings(&again).contains(&"RT-AI-0301".to_owned()),
        "{}",
        again.stderr
    );
    assert_eq!(stub.connections(), 1);
    drop(stub.finish());
}

// Req 35.11: the effort is part of the canonical request only when it is
// configured, so a different configuration gives another identifier.
#[test]
fn a_configured_effort_changes_the_request_identifier_and_none_keeps_it() {
    let stub = Stub::start(Vec::new());
    let rig = Rig::new("e2e-effort-id", &stub.endpoint(), &DEFAULT);
    let with_low = rig.fixture.request_id();
    let text = config_text(&rig.fixture, &stub.endpoint(), &DEFAULT);
    rig.fixture
        .write_config(&text.replace("reasoning_effort = \"low\"\n", ""));
    let without = rig.fixture.request_id();
    rig.fixture
        .write_config(&text.replace("reasoning_effort = \"low\"", "reasoning_effort = \"high\""));
    let with_high = rig.fixture.request_id();
    assert_ne!(with_low, without);
    assert_ne!(with_low, with_high);
    assert_ne!(without, with_high);
    assert_eq!(stub.connections(), 0);
    drop(stub.finish());
}

fn refused_before_any_connection(name: &str, edit: impl Fn(&str) -> String, code: &str) {
    let stub = Stub::start(vec![success("")]);
    let rig = Rig::new(name, &stub.endpoint(), &DEFAULT);
    let text = edit(&config_text(&rig.fixture, &stub.endpoint(), &DEFAULT));
    rig.fixture.write_config(&text);
    let env = MapEnv::new(&[("MODEL_API_KEY", CANARY)]);
    let ran = run_providers(
        &rig.fixture.consented(),
        &env,
        &Stubbed(rig.waits.clone()),
        &StdFs,
    );
    assert_eq!(exit(ran.code), 2, "{}", ran.stderr);
    assert_eq!(errors(&ran), [code], "{}", ran.stderr);
    assert_eq!(stub.connections(), 0);
    assert!(rig.log_lines().is_empty());
    assert!(store_is_empty(&rig.fixture));
    rig.assert_no_canary(&ran);
    drop(stub.finish());
}

#[test]
fn the_openai_profile_with_an_effort_is_rt_ai_0011_before_any_connection() {
    refused_before_any_connection(
        "e2e-capability",
        |text| {
            text.replace("id = \"meta\"", "id = \"openai\"")
                .replace("\"meta\", \"muse", "\"openai\", \"muse")
        },
        "RT-AI-0011",
    );
}

#[test]
fn a_contributor_model_is_rt_ai_0012_before_any_connection() {
    refused_before_any_connection(
        "e2e-contributor",
        |text| {
            text.replace(
                "muse-spark-1.3\"\nendpoint",
                "muse-spark-1.3-contributor\"\nendpoint",
            )
        },
        "RT-AI-0012",
    );
}

#[test]
fn a_foreign_host_is_rt_ai_0013_before_any_connection() {
    let foreign = |text: &str| {
        let at = text.find("endpoint = \"").unwrap();
        let end = text[at..].find('\n').unwrap() + at;
        format!(
            "{}endpoint = \"https://evil.example\"{}",
            &text[..at],
            &text[end..]
        )
    };
    refused_before_any_connection("e2e-host", foreign, "RT-AI-0013");
}

#[test]
fn a_missing_credential_is_rt_ai_0007_before_any_connection() {
    let stub = Stub::start(vec![success("")]);
    let rig = Rig::new("e2e-no-credential", &stub.endpoint(), &DEFAULT);
    let env = MapEnv::new(&[]);
    let ran = run_providers(
        &rig.fixture.consented(),
        &env,
        &Stubbed(rig.waits.clone()),
        &StdFs,
    );
    assert_eq!(exit(ran.code), 2, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0007"], "{}", ran.stderr);
    assert_eq!(stub.connections(), 0);
    drop(stub.finish());
}

// Review wave 3, issue 3: a value that an HTTP header cannot carry is refused
// at the credential stage, before the consent log and any connection, and the
// message never echoes it.
#[test]
fn a_header_unsafe_credential_is_rt_ai_0007_before_any_connection() {
    let cases = [
        ("trailing newline", format!("{CANARY}\n")),
        (
            "carriage return and line feed",
            format!("{CANARY}\r\nX-Evil: 1"),
        ),
        ("inner space", format!("{CANARY} tail")),
        ("tab", format!("{CANARY}\tx")),
        ("delete character", format!("{CANARY}\u{7f}")),
        ("non-ascii", format!("{CANARY}\u{e9}")),
        ("zero width space", format!("{CANARY}\u{200b}")),
    ];
    for (name, value) in cases {
        let stub = Stub::start(vec![success("")]);
        let rig = Rig::new("e2e-bad-credential", &stub.endpoint(), &DEFAULT);
        let env = MapEnv::new(&[("MODEL_API_KEY", value.as_str())]);
        let before = rig.fixture.files();
        let ran = run_providers(
            &rig.fixture.consented(),
            &env,
            &Stubbed(rig.waits.clone()),
            &StdFs,
        );
        assert_eq!(exit(ran.code), 2, "{name}: {}", ran.stderr);
        assert_eq!(errors(&ran), ["RT-AI-0007"], "{name}: {}", ran.stderr);
        assert!(
            ran.stderr.contains("cannot be sent in an HTTP header"),
            "{name}: {}",
            ran.stderr
        );
        rig.assert_no_canary(&ran);
        assert_eq!(rig.fixture.files(), before, "{name}: no file written");
        assert!(
            !rig.fixture.path("work/tx.jsonl").exists(),
            "{name}: no consent log entry"
        );
        assert_eq!(stub.connections(), 0, "{name}");
        drop(stub.finish());
    }
}

// A visible ASCII value is still accepted (the canary itself).
#[test]
fn a_visible_ascii_credential_is_not_refused_by_the_header_check() {
    let stub = Stub::start(vec![success("")]);
    let rig = Rig::new("e2e-ok-credential", &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert!(
        !errors(&ran).contains(&"RT-AI-0007".to_string()),
        "{}",
        ran.stderr
    );
    drop(stub.finish());
}

// Review wave 2, issue 5: a configuration that the real run refuses at the
// capability check fails the dry run the same way, with no connection, no
// credential read and no file written.
#[test]
fn a_dry_run_fails_the_capability_check_like_the_real_run() {
    let stub = Stub::start(Vec::new());
    let rig = Rig::new("e2e-dry-capability", &stub.endpoint(), &DEFAULT);
    let text = config_text(&rig.fixture, &stub.endpoint(), &DEFAULT)
        .replace("id = \"meta\"", "id = \"openai\"")
        .replace("\"meta\", \"muse", "\"openai\", \"muse");
    rig.fixture.write_config(&text);
    let before = rig.fixture.files();
    let env = MapEnv::new(&[("MODEL_API_KEY", CANARY)]);
    let dry = run_providers(
        &rig.fixture.args(&["--dry-run"]),
        &env,
        &Stubbed(rig.waits.clone()),
        &StdFs,
    );
    let real = run_providers(
        &rig.fixture.consented(),
        &env,
        &Stubbed(rig.waits.clone()),
        &StdFs,
    );
    assert_eq!(exit(dry.code), 2, "{}", dry.stderr);
    assert_eq!(errors(&dry), ["RT-AI-0011"], "{}", dry.stderr);
    assert_eq!(dry.stderr, real.stderr);
    assert_eq!(dry.stdout, "");
    assert!(env.reads().is_empty());
    assert_eq!(stub.connections(), 0);
    assert_eq!(rig.fixture.files(), before);
    drop(stub.finish());
}
