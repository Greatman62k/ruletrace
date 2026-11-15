//! Configuration, consent and credential tests (Req 28.1 to 28.7, 28.16,
//! 28.17, 28.18). Expected codes, positions and exit codes are written by
//! hand from the requirements.

use crate::{load, Authorized, Consent, EnvReader, ProviderKind};
use proptest::prelude::*;
use rt_foundation::{
    CliLocation, Code, Diagnostic, DocumentId, ExitCode, InputId, Location, Severity,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct MapEnv {
    values: BTreeMap<String, String>,
    reads: RefCell<Vec<String>>,
}

impl MapEnv {
    pub(crate) fn new(pairs: &[(&str, &str)]) -> Self {
        Self {
            values: pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            reads: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn reads(&self) -> Vec<String> {
        self.reads.borrow().clone()
    }
}

impl EnvReader for MapEnv {
    fn get(&self, name: &str) -> Option<String> {
        self.reads.borrow_mut().push(name.to_owned());
        self.values.get(name).cloned()
    }
}

pub(crate) const CANARY: &str = "canary-7f3a9c1e5d42";

pub(crate) const BASE: &str = "schema_version = 1
enabled = true
provider = \"openai-responses\"
model = \"model-1\"
endpoint = \"https://api.example.org\"
credential_env_var = \"PROVIDER_CREDENTIAL\"
allow_document_text = true
ai_identities = [\"openai-responses\", \"model-1\"]
transmission_log = \"tx.jsonl\"
";

const MOCK: &str = "schema_version = 1
enabled = true
provider = \"mock-replay\"
model = \"model-1\"
script = \"recorded.json\"
allow_document_text = true
ai_identities = [\"openai-responses\"]
transmission_log = \"tx.jsonl\"
";

fn doc(name: &str) -> DocumentId {
    DocumentId::new(name).unwrap()
}

fn docs(names: &[&str]) -> BTreeSet<DocumentId> {
    names.iter().map(|n| doc(n)).collect()
}

fn cli_arg() -> Location {
    Location::Cli(CliLocation::Argument {
        position: 7,
        name: Some("--consent".to_owned()),
    })
}

fn consent(names: &[&str]) -> Consent {
    Consent::named(docs(names), cli_arg())
}

fn input() -> InputId {
    InputId::new("ai.toml").unwrap()
}

pub(crate) fn run(text: &str, env: &MapEnv) -> Result<Authorized, Vec<Diagnostic>> {
    load(&input(), text, &consent(&["a.md"]), &docs(&["a.md"]), env)
}

pub(crate) fn env_ok() -> MapEnv {
    MapEnv::new(&[("PROVIDER_CREDENTIAL", CANARY)])
}

/// The single Diagnostic of a failed load, after checking code, severity and
/// exit code.
pub(crate) fn one(result: Result<Authorized, Vec<Diagnostic>>, code: Code) -> Diagnostic {
    let found = result.unwrap_err();
    assert_eq!(found.len(), 1, "{found:?}");
    let d = found.into_iter().next().unwrap();
    assert_eq!(d.code(), code);
    assert_eq!(d.severity(), Severity::Error);
    assert_eq!(d.code().exit_code(), ExitCode::InvalidInput);
    d
}

pub(crate) fn at(d: &Diagnostic) -> (Option<u32>, Option<u32>, Option<String>) {
    match d.primary() {
        Location::Input(i) => {
            assert_eq!(i.input, input());
            (i.line, i.column, i.field.clone())
        }
        other => panic!("not an input location: {other:?}"),
    }
}

pub(crate) fn with(replace: (&str, &str)) -> String {
    assert!(BASE.contains(replace.0), "{}", replace.0);
    BASE.replace(replace.0, replace.1)
}

pub(crate) fn no_env_read(env: &MapEnv) {
    assert!(env.reads().is_empty(), "read {:?}", env.reads());
}

#[test]
fn valid_real_configuration_reads_only_the_named_variable() {
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CANARY), ("OTHER", "x")]);
    let ok = run(BASE, &env).unwrap();
    assert_eq!(env.reads(), vec!["PROVIDER_CREDENTIAL".to_owned()]);
    let c = ok.config;
    assert_eq!(c.provider, ProviderKind::OpenAiResponses);
    assert_eq!(c.endpoint.unwrap().host, "api.example.org");
    assert_eq!(c.max_clauses_per_request, 20);
    let l = c.limits;
    assert_eq!(
        (l.deadline_s, l.attempt_s, l.retries, l.response_bytes),
        (60, 20, 2, 1_048_576)
    );
    assert_eq!(ok.credential.unwrap().expose(), CANARY);
}

#[test]
fn mock_replay_needs_no_endpoint_and_reads_no_environment() {
    let env = MapEnv::new(&[]);
    let ok = run(MOCK, &env).unwrap();
    no_env_read(&env);
    assert_eq!(ok.config.provider, ProviderKind::MockReplay);
    assert!(ok.credential.is_none());
    assert_eq!(ok.config.script.as_deref(), Some("recorded.json"));
}

#[test]
fn mock_replay_refuses_endpoint_and_credential_fields() {
    for extra in [
        "endpoint = \"https://api.example.org\"\n",
        "credential_env_var = \"PROVIDER_CREDENTIAL\"\n",
    ] {
        let env = env_ok();
        let d = one(run(&format!("{MOCK}{extra}"), &env), Code::AiConfigInvalid);
        assert_eq!(at(&d).0, Some(9));
        no_env_read(&env);
    }
}

#[test]
fn real_provider_refuses_script_and_requires_endpoint_and_credential_name() {
    let env = env_ok();
    let d = one(
        run(&format!("{BASE}script = \"x.json\"\n"), &env),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d), (Some(10), Some(1), Some("script".to_owned())));
    for name in ["endpoint", "credential_env_var"] {
        let text: String = BASE
            .lines()
            .filter(|l| !l.starts_with(name))
            .map(|l| format!("{l}\n"))
            .collect();
        let d = one(run(&text, &env_ok()), Code::AiConfigInvalid);
        assert_eq!(at(&d), (None, None, Some(name.to_owned())));
    }
}

#[test]
fn syntax_error_reports_line_and_column() {
    let env = env_ok();
    let d = one(
        run("schema_version = 1\nenabled = = true\n", &env),
        Code::AiConfigInvalid,
    );
    let (line, column, field) = at(&d);
    assert_eq!((line, column, field), (Some(2), Some(11), None));
    no_env_read(&env);
}

#[test]
fn syntax_error_comes_before_a_secret_in_the_same_file() {
    let d = one(
        run("api_key = \"x\"\nenabled = = true\n", &env_ok()),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d).0, Some(2));
}

#[test]
fn inline_secret_field_names_are_rejected_case_insensitively() {
    for name in [
        "api_key",
        "Authorization",
        "my_TOKEN",
        "client_secret",
        "PassWord",
        "monkey",
    ] {
        let env = env_ok();
        let text = format!("{BASE}{name} = \"v\"\n");
        let d = one(run(&text, &env), Code::AiConfigInlineSecret);
        assert_eq!(at(&d), (Some(10), Some(1), None), "{name}");
        assert!(!d.message().contains(name), "{name} echoed");
        no_env_read(&env);
    }
}

#[test]
fn inline_secret_values_are_rejected_in_any_field_and_never_echoed() {
    for value in [
        "sk-abc123",
        "sk_abc123",
        "Bearer abc123",
        "ghp_abc123",
        "AKIAABC123",
    ] {
        let env = env_ok();
        let text = with(("model = \"model-1\"", &format!("model = \"{value}\"")));
        let d = one(run(&text, &env), Code::AiConfigInlineSecret);
        assert_eq!(
            at(&d),
            (Some(4), Some(1), Some("model".to_owned())),
            "{value}"
        );
        assert!(!d.message().contains(&value[3..]), "{value} echoed");
        no_env_read(&env);
    }
}

#[test]
fn inline_secret_values_inside_arrays_are_rejected() {
    let text = with((
        "[\"openai-responses\", \"model-1\"]",
        "[\"ok\", \"sk-zzz\"]",
    ));
    let d = one(run(&text, &env_ok()), Code::AiConfigInlineSecret);
    assert_eq!(at(&d), (Some(8), Some(1), Some("ai_identities".to_owned())));
}

#[test]
fn nested_names_and_values_are_scanned_for_secrets() {
    let cases = [
        ("[extra]\napi_key = 1\n", None),
        ("extra = { password = \"p\" }\n", None),
        ("[[extra]]\nauthorization = \"p\"\n", None),
        ("extra = { inner = \"ghp_zzz\" }\n", Some("extra")),
        ("[extra]\ninner = \"AKIAZZZ\"\n", Some("extra")),
        ("[[extra]]\ninner = \"sk-zzz\"\n", Some("extra")),
    ];
    for (tail, field) in cases {
        let env = env_ok();
        let d = one(
            run(&format!("{BASE}{tail}"), &env),
            Code::AiConfigInlineSecret,
        );
        assert_eq!(at(&d).0, Some(10), "{tail}");
        assert_eq!(at(&d).2.as_deref(), field, "{tail}");
        no_env_read(&env);
    }
}

#[test]
fn values_that_only_resemble_credential_prefixes_are_accepted() {
    for value in [
        "xsk-1",
        "skip",
        "akia-lower",
        "ghp-dash",
        "bearer-lower",
        "Beare",
    ] {
        let env = env_ok();
        assert!(run(&with(("model-1", value)), &env).is_ok(), "{value}");
    }
}

#[test]
fn invalid_credential_env_var_names_are_inline_secret_errors() {
    for name in [
        "lower_case",
        "1STARTS_WITH_DIGIT",
        "HAS-DASH",
        "HAS SPACE",
        "",
    ] {
        let env = env_ok();
        let d = one(
            run(&with(("PROVIDER_CREDENTIAL", name)), &env),
            Code::AiConfigInlineSecret,
        );
        assert_eq!(
            at(&d),
            (Some(6), Some(1), Some("credential_env_var".to_owned()))
        );
        no_env_read(&env);
    }
    for name in ["_UNDERSCORE_FIRST", "A1_B2", "Z"] {
        assert!(run(
            &with(("PROVIDER_CREDENTIAL", name)),
            &MapEnv::new(&[(name, "v")])
        )
        .is_ok());
    }
}

#[test]
fn endpoint_with_user_name_password_or_query_is_an_inline_secret() {
    for url in [
        "https://user:pass@api.example.org",
        "https://user@api.example.org/v1",
        "https://api.example.org/v1?token=abc",
        "http://u:p@evil.example.com",
    ] {
        let env = env_ok();
        let d = one(
            run(&with(("https://api.example.org", url)), &env),
            Code::AiConfigInlineSecret,
        );
        assert_eq!(
            at(&d),
            (Some(5), Some(1), Some("endpoint".to_owned())),
            "{url}"
        );
        assert!(!d.message().contains(url), "{url} echoed");
        assert!(!d.message().contains(":pass") && !d.message().contains("token=abc"));
        no_env_read(&env);
    }
    let ok = with(("https://api.example.org", "https://api.example.org/a@b"));
    assert!(run(&ok, &env_ok()).is_ok());
}

#[test]
fn unknown_wrong_type_and_missing_fields_are_invalid_config() {
    let d = one(
        run(&format!("{BASE}extra = 1\n"), &env_ok()),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d), (Some(10), Some(1), Some("extra".to_owned())));
    let d = one(
        run(&with(("enabled = true", "enabled = \"yes\"")), &env_ok()),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d), (Some(2), Some(1), Some("enabled".to_owned())));
    let d = one(
        run(
            &with(("[\"openai-responses\", \"model-1\"]", "[1]")),
            &env_ok(),
        ),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d), (Some(8), Some(1), Some("ai_identities".to_owned())));
    let d = one(
        run(
            &with(("schema_version = 1", "schema_version = 2")),
            &env_ok(),
        ),
        Code::AiConfigInvalid,
    );
    assert_eq!(
        at(&d),
        (Some(1), Some(1), Some("schema_version".to_owned()))
    );
    let d = one(
        run(&with(("model = \"model-1\"", "model = \"\"")), &env_ok()),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d).2.as_deref(), Some("model"));
}

#[test]
fn every_required_field_is_reported_when_missing_while_enabled() {
    for name in [
        "schema_version",
        "provider",
        "model",
        "allow_document_text",
        "ai_identities",
        "transmission_log",
    ] {
        let text: String = BASE
            .lines()
            .filter(|l| !l.starts_with(name))
            .map(|l| format!("{l}\n"))
            .collect();
        let env = env_ok();
        let found = run(&text, &env).unwrap_err();
        assert!(
            found.iter().any(|d| d.code() == Code::AiConfigInvalid
                && at(d) == (None, None, Some(name.to_owned()))),
            "{name}: {found:?}"
        );
        no_env_read(&env);
    }
}

#[test]
fn absent_or_false_enabled_is_ai_disabled_and_reads_no_environment() {
    for (text, field) in [
        ("", None),
        ("schema_version = 1\n", None),
        ("enabled = false\n", Some("enabled")),
    ] {
        let env = env_ok();
        let d = one(run(text, &env), Code::AiDisabled);
        no_env_read(&env);
        assert_eq!(at(&d).2.as_deref(), field);
    }
}

#[test]
fn disabled_configuration_still_rejects_unknown_fields_but_needs_no_required_ones() {
    let d = one(
        run("enabled = false\nbogus = 1\n", &env_ok()),
        Code::AiConfigInvalid,
    );
    assert_eq!(at(&d), (Some(2), Some(1), Some("bogus".to_owned())));
}

#[test]
fn limits_outside_their_ranges_are_rejected_and_boundaries_accepted() {
    let rejected = [
        ("total_deadline_secs = 0", "total_deadline_secs"),
        ("total_deadline_secs = 301", "total_deadline_secs"),
        ("total_deadline_secs = -1", "total_deadline_secs"),
        ("attempt_timeout_secs = 0", "attempt_timeout_secs"),
        (
            "attempt_timeout_secs = 121\ntotal_deadline_secs = 300",
            "attempt_timeout_secs",
        ),
        ("attempt_timeout_secs = 61", "attempt_timeout_secs"),
        ("max_retries = 3", "max_retries"),
        ("max_retries = -1", "max_retries"),
        ("max_retries = 300", "max_retries"),
        ("max_response_bytes = 1023", "max_response_bytes"),
        ("max_response_bytes = 16777217", "max_response_bytes"),
        ("max_clauses_per_request = 0", "max_clauses_per_request"),
        ("max_clauses_per_request = 101", "max_clauses_per_request"),
    ];
    for (line, field) in rejected {
        let env = env_ok();
        let found = run(&format!("{BASE}{line}\n"), &env).unwrap_err();
        let hit = found
            .iter()
            .find(|d| d.code() == Code::AiConfigLimitInvalid)
            .unwrap_or_else(|| panic!("{line}: {found:?}"));
        assert_eq!(hit.severity(), Severity::Error);
        assert_eq!(hit.code().exit_code(), ExitCode::InvalidInput);
        assert_eq!(at(hit).2.as_deref(), Some(field), "{line}");
        assert_eq!(at(hit).0, Some(10), "{line}");
        no_env_read(&env);
    }
    let accepted = [
        "total_deadline_secs = 1\nattempt_timeout_secs = 1",
        "total_deadline_secs = 300\nattempt_timeout_secs = 120",
        "total_deadline_secs = 30\nattempt_timeout_secs = 30",
        "max_retries = 0",
        "max_retries = 2",
        "max_response_bytes = 1024",
        "max_response_bytes = 16777216",
        "max_clauses_per_request = 1",
        "max_clauses_per_request = 100",
    ];
    for line in accepted {
        assert!(run(&format!("{BASE}{line}\n"), &env_ok()).is_ok(), "{line}");
    }
}

#[test]
fn explicit_limits_are_carried_into_the_configuration() {
    let text = format!(
        "{BASE}total_deadline_secs = 90\nattempt_timeout_secs = 30\nmax_retries = 1\nmax_response_bytes = 2048\nmax_clauses_per_request = 5\n"
    );
    let c = run(&text, &env_ok()).unwrap().config;
    let l = c.limits;
    assert_eq!(
        (l.deadline_s, l.attempt_s, l.retries, l.response_bytes),
        (90, 30, 1, 2048)
    );
    assert_eq!(c.max_clauses_per_request, 5);
    assert_eq!(c.policy.deadline().as_secs(), 90);
}

#[test]
fn endpoint_must_be_https_unless_the_host_is_loopback() {
    let insecure = [
        "http://api.example.org",
        "http://localhost.evil.example",
        "http://127.0.0.1.evil.example",
        "http://128.0.0.1",
        "http://[::2]",
        "ftp://localhost",
        "ws://example.org",
    ];
    for url in insecure {
        let env = env_ok();
        let d = one(
            run(&with(("https://api.example.org", url)), &env),
            Code::AiConfigEndpointInsecure,
        );
        assert_eq!(
            at(&d),
            (Some(5), Some(1), Some("endpoint".to_owned())),
            "{url}"
        );
        no_env_read(&env);
    }
    let accepted = [
        "https://api.example.org",
        "HTTPS://API.EXAMPLE.ORG",
        "https://api.example.org:8443/v1",
        "http://localhost:8080",
        "http://LOCALHOST",
        "http://127.0.0.1",
        "http://127.255.255.254:9/x",
        "http://[::1]:9",
        "https://localhost",
    ];
    for url in accepted {
        assert!(
            run(&with(("https://api.example.org", url)), &env_ok()).is_ok(),
            "{url}"
        );
    }
}

#[test]
fn endpoint_host_is_lowercased_without_port() {
    let c = run(
        &with(("https://api.example.org", "HTTPS://Api.Example.ORG:8443/v1")),
        &env_ok(),
    )
    .unwrap()
    .config;
    assert_eq!(c.endpoint.unwrap().host, "api.example.org");
}

#[test]
fn malformed_endpoint_is_invalid_config_not_insecure() {
    for url in [
        "example.org",
        "https://",
        "https://h:99999",
        "https://h:0",
        "https://h\\\\x",
        "://h",
        "https://[::1",
    ] {
        let d = one(
            run(&with(("https://api.example.org", url)), &env_ok()),
            Code::AiConfigInvalid,
        );
        assert_eq!(at(&d).2.as_deref(), Some("endpoint"), "{url}");
    }
}

#[test]
fn unknown_provider_kind_is_reported_at_the_provider_field() {
    let env = env_ok();
    let d = one(
        run(&with(("openai-responses\"\nmodel", "other\"\nmodel")), &env),
        Code::AiProviderUnknown,
    );
    assert_eq!(at(&d), (Some(3), Some(1), Some("provider".to_owned())));
    no_env_read(&env);
}

#[test]
fn check_order_follows_requirement_28_17() {
    let secret = "api_key = 1\n";
    let unknown = "bogus = 1\n";
    let limit = "max_retries = 9\n";
    let insecure = with(("https://api.example.org", "http://example.org"));
    let disabled = with(("enabled = true", "enabled = false"));
    let provider = with(("openai-responses\"\nmodel", "other\"\nmodel"));
    let cases: Vec<(String, Code)> = vec![
        (
            format!("{BASE}{secret}{unknown}{limit}"),
            Code::AiConfigInlineSecret,
        ),
        (format!("{BASE}{unknown}{limit}"), Code::AiConfigInvalid),
        (format!("{insecure}{limit}"), Code::AiConfigLimitInvalid),
        (
            format!("{insecure}max_retries = 1\n"),
            Code::AiConfigEndpointInsecure,
        ),
        (
            with(("enabled = true", "enabled = false"))
                .replace("https://api.example.org", "http://example.org"),
            Code::AiConfigEndpointInsecure,
        ),
        (
            disabled.replace("openai-responses\"\nmodel", "other\"\nmodel"),
            Code::AiDisabled,
        ),
        (provider.clone(), Code::AiProviderUnknown),
    ];
    for (text, code) in cases {
        let env = env_ok();
        let found = load(
            &input(),
            &text,
            &consent(&["other"]),
            &docs(&["a.md"]),
            &env,
        )
        .unwrap_err();
        assert!(found.iter().all(|d| d.code() == code), "{code}: {found:?}");
        assert_eq!(found.first().map(Diagnostic::code), Some(code));
        no_env_read(&env);
    }
}

#[test]
fn unknown_provider_comes_before_consent_and_consent_before_credential() {
    let env = env_ok();
    let provider = with(("openai-responses\"\nmodel", "other\"\nmodel"));
    let found = load(
        &input(),
        &provider,
        &Consent::absent(cli_arg()),
        &docs(&["a.md"]),
        &env,
    )
    .unwrap_err();
    assert_eq!(found[0].code(), Code::AiProviderUnknown);
    let env = env_ok();
    let found = load(
        &input(),
        BASE,
        &Consent::absent(cli_arg()),
        &docs(&["a.md"]),
        &env,
    )
    .unwrap_err();
    assert_eq!(found[0].code(), Code::AiConsentMissing);
    no_env_read(&env);
}

#[test]
fn consent_must_equal_the_documents_of_the_run() {
    let run_set = docs(&["a.md", "b.md"]);
    let cases: Vec<(Consent, bool)> = vec![
        (consent(&["a.md", "b.md"]), true),
        (consent(&["a.md"]), false),
        (consent(&["a.md", "b.md", "c.md"]), false),
        (consent(&["a.md", "c.md"]), false),
        (consent(&[]), false),
        (Consent::absent(cli_arg()), false),
    ];
    for (consent, ok) in cases {
        let env = env_ok();
        let result = load(&input(), BASE, &consent, &run_set, &env);
        if ok {
            assert!(result.is_ok());
        } else {
            let d = &result.unwrap_err()[0];
            assert_eq!(d.code(), Code::AiConsentMissing);
            assert_eq!(d.severity(), Severity::Error);
            assert_eq!(d.code().exit_code(), ExitCode::InvalidInput);
            assert_eq!(d.primary(), &cli_arg());
            no_env_read(&env);
        }
    }
}

#[test]
fn absent_consent_points_at_the_command_line() {
    let at_command = Location::Cli(CliLocation::Command("propose".to_owned()));
    let env = env_ok();
    let found = load(
        &input(),
        BASE,
        &Consent::absent(at_command.clone()),
        &docs(&["a.md"]),
        &env,
    )
    .unwrap_err();
    assert_eq!(found[0].primary(), &at_command);
}

#[test]
fn consent_message_names_missing_and_extra_documents() {
    let env = env_ok();
    let found = load(&input(), BASE, &consent(&["b.md"]), &docs(&["a.md"]), &env).unwrap_err();
    let m = found[0].message();
    assert!(
        m.contains("missing: ['a.md']") && m.contains("not sent: ['b.md']"),
        "{m}"
    );
}

#[test]
fn consent_is_required_even_when_the_provider_is_the_offline_mock() {
    let env = MapEnv::new(&[]);
    let found = load(
        &input(),
        MOCK,
        &Consent::absent(cli_arg()),
        &docs(&["a.md"]),
        &env,
    )
    .unwrap_err();
    assert_eq!(found[0].code(), Code::AiConsentMissing);
}

#[test]
fn allow_document_text_false_refuses_even_with_exact_consent() {
    let env = env_ok();
    let d = one(
        run(
            &with(("allow_document_text = true", "allow_document_text = false")),
            &env,
        ),
        Code::AiConsentMissing,
    );
    assert_eq!(d.primary(), &cli_arg());
    no_env_read(&env);
}

#[test]
fn missing_or_empty_credential_is_reported_at_the_named_field() {
    for env in [
        MapEnv::new(&[]),
        MapEnv::new(&[("PROVIDER_CREDENTIAL", "")]),
    ] {
        let d = one(run(BASE, &env), Code::AiCredentialMissing);
        assert_eq!(
            at(&d),
            (Some(6), Some(1), Some("credential_env_var".to_owned()))
        );
        assert_eq!(env.reads(), vec!["PROVIDER_CREDENTIAL".to_owned()]);
    }
}

#[test]
fn credential_debug_is_redacted() {
    let ok = run(BASE, &env_ok()).unwrap();
    let shown = format!("{ok:?}");
    assert!(!shown.contains(CANARY), "{shown}");
    assert!(shown.contains("[REDACTED]"));
}

#[test]
fn disabled_without_configuration_is_error_at_the_given_location() {
    let at_command = Location::Cli(CliLocation::Command("propose".to_owned()));
    let d = crate::disabled_without_configuration(at_command.clone());
    assert_eq!(
        (d.code(), d.severity()),
        (Code::AiDisabled, Severity::Error)
    );
    assert_eq!(d.code().exit_code(), ExitCode::InvalidInput);
    assert_eq!(d.primary(), &at_command);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    // Consent passes exactly when the two sets are equal.
    #[test]
    fn consent_passes_iff_the_sets_are_equal(
        consented in proptest::collection::btree_set("[a-d]{1,2}", 0..4),
        sent in proptest::collection::btree_set("[a-d]{1,2}", 0..4),
    ) {
        let to_ids = |s: &BTreeSet<String>| -> BTreeSet<DocumentId> { s.iter().map(|d| doc(d)).collect() };
        let env = env_ok();
        let result = load(&input(), BASE, &Consent::named(to_ids(&consented), cli_arg()), &to_ids(&sent), &env);
        prop_assert_eq!(result.is_ok(), consented == sent);
        if result.is_err() {
            prop_assert!(env.reads().is_empty());
        }
    }

    // The environment is read only after every other check passed.
    #[test]
    fn arbitrary_text_reads_the_environment_only_on_success_or_missing_credential(text in "\\PC{0,200}") {
        let env = MapEnv::new(&[]);
        let result = run(&text, &env);
        match result {
            Ok(_) => {}
            Err(found) => {
                let missing = found.iter().any(|d| d.code() == Code::AiCredentialMissing);
                prop_assert_eq!(env.reads().is_empty(), !missing);
            }
        }
    }
}
