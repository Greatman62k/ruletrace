//! `ruletrace-ai models` and `verify_model` end to end against the loopback
//! stub (task 10.16, Req 35.18 to 35.20). Expected codes, exit codes and
//! orders are written by hand from the catalogue and the requirements.

use crate::tests::MapEnv;
use crate::tests_cli_rig::{Fixture, Ran};
use crate::tests_e2e_responses::stub::{http, json_ok, Stub};
use crate::tests_e2e_responses::{
    config_text, errors, exit, store_is_empty, success, warnings, Limits, Rig, Stubbed, CANARY,
    DEFAULT,
};
use crate::tests_gate_rig::run_providers;
use crate::StdFs;
use std::ffi::OsString;
use std::time::Duration;

fn catalogue(ids: &[&str]) -> String {
    let entries: Vec<String> = ids
        .iter()
        .map(|id| format!(r#"{{"id":"{id}","object":"model","created":1,"owned_by":"x"}}"#))
        .collect();
    format!(
        r#"{{"object":"list","extra":true,"data":[{}]}}"#,
        entries.join(",")
    )
}

fn models_args(fixture: &Fixture, extra: &[&str]) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["models".into(), "--config".into()];
    args.push(fixture.path("ai.toml").into());
    args.extend(extra.iter().map(OsString::from));
    args
}

pub(crate) fn run_models(rig: &Rig, extra: &[&str]) -> Ran {
    let env = MapEnv::new(&[("MODEL_API_KEY", CANARY)]);
    let ran = run_providers(
        &models_args(&rig.fixture, extra),
        &env,
        &Stubbed(rig.waits.clone()),
        &StdFs,
    );
    rig.assert_no_canary(&ran);
    ran
}

fn with_verify(rig: &Rig, endpoint: &str, limits: &Limits) {
    let text = config_text(&rig.fixture, endpoint, limits);
    let verified = text.replace(
        "reasoning_effort = \"low\"\n",
        "reasoning_effort = \"low\"\nverify_model = true\n",
    );
    assert_ne!(verified, text);
    rig.fixture.write_config(&verified);
}

#[test]
fn models_prints_the_sorted_identifiers_without_consent_or_documents() {
    let ids = ["muse-spark-1.3", "a-model", "Zed", "muse-ember-1.0", "ä"];
    let stub = Stub::start(vec![json_ok(&catalogue(&ids))]);
    let rig = Rig::new("models-ok", &stub.endpoint(), &DEFAULT);
    let config_before = rig.fixture.text("ai.toml");
    let ran = run_models(&rig, &[]);
    assert_eq!(exit(ran.code), 0, "{}", ran.stderr);
    // Ascending byte order: upper case before lower case, non-ASCII last.
    assert_eq!(
        ran.stdout,
        "Zed\na-model\nmuse-ember-1.0\nmuse-spark-1.3\nä\n"
    );
    assert!(
        errors(&ran).is_empty() && warnings(&ran).is_empty(),
        "{}",
        ran.stderr
    );
    let captured = stub.finish();
    assert_eq!(captured.len(), 1);
    let sent = &captured[0];
    assert_eq!(sent.request_line(), "GET /v1/models HTTP/1.1");
    assert!(sent.body().is_empty(), "{:?}", sent.body());
    let authorization = sent
        .headers()
        .into_iter()
        .find(|(k, _)| k == "authorization")
        .map(|(_, v)| v);
    assert_eq!(authorization, Some(format!("Bearer {CANARY}")));
    let raw = String::from_utf8_lossy(&sent.raw).into_owned();
    assert!(!raw.contains("Alpha"), "{raw}");
    assert!(!raw.contains("rules.md"), "{raw}");
    assert_eq!(rig.fixture.text("ai.toml"), config_before);
    assert!(rig.fixture.files().iter().all(|f| !f.starts_with("store")));
}

#[test]
fn the_models_log_entry_names_the_provider_and_no_document() {
    let stub = Stub::start(vec![json_ok(&catalogue(&["m"]))]);
    let rig = Rig::new("models-log", &stub.endpoint(), &DEFAULT);
    assert_eq!(exit(run_models(&rig, &[]).code), 0);
    assert_eq!(
        rig.log_lines(),
        [
            r#"{"provider":"meta","protocol":"openai-responses","model":"muse-spark-1.3","endpoint_host":"127.0.0.1","kind":"models","documents":[],"attempt":1}"#
        ]
    );
    drop(stub.finish());
}

#[test]
fn an_empty_catalogue_prints_nothing_and_succeeds() {
    let stub = Stub::start(vec![json_ok(r#"{"data":[]}"#)]);
    let rig = Rig::new("models-empty", &stub.endpoint(), &DEFAULT);
    let ran = run_models(&rig, &[]);
    assert_eq!(exit(ran.code), 0, "{}", ran.stderr);
    assert_eq!(ran.stdout, "");
    drop(stub.finish());
}

#[test]
fn control_characters_in_an_identifier_are_escaped_on_one_line() {
    let body = r#"{"data":[{"id":"a\nb\u001b[31m"}]}"#;
    let stub = Stub::start(vec![json_ok(body)]);
    let rig = Rig::new("models-escape", &stub.endpoint(), &DEFAULT);
    let ran = run_models(&rig, &[]);
    assert_eq!(exit(ran.code), 0, "{}", ran.stderr);
    assert_eq!(ran.stdout.lines().count(), 1, "{:?}", ran.stdout);
    assert!(!ran.stdout.contains('\u{1b}'), "{:?}", ran.stdout);
    assert!(ran.stdout.starts_with("a"), "{:?}", ran.stdout);
    drop(stub.finish());
}

// Req 35.18: one identifier per line. Expected text is written by hand.
#[test]
fn every_line_break_in_an_identifier_is_escaped_before_the_terminator() {
    let cases: [(&str, &str); 7] = [
        (r"good\n", "good\\u{a}\n"),
        (r"good\r", "good\\u{d}\n"),
        (r"a\r\nb", "a\\u{d}\\u{a}b\n"),
        (r"a\u2028b", "a\\u{2028}b\n"),
        (r"a\u2029b", "a\\u{2029}b\n"),
        (r"a\u0085b", "a\\u{85}b\n"),
        (r"\n", "\\u{a}\n"),
    ];
    for (id, expected) in cases {
        let body = format!(r#"{{"data":[{{"id":"{id}"}}]}}"#);
        let stub = Stub::start(vec![json_ok(&body)]);
        let rig = Rig::new("models-breaks", &stub.endpoint(), &DEFAULT);
        let ran = run_models(&rig, &[]);
        assert_eq!(exit(ran.code), 0, "{id}: {}", ran.stderr);
        assert_eq!(ran.stdout, expected, "{id}");
        drop(stub.finish());
    }
}

#[test]
fn two_identifiers_one_ending_in_a_line_feed_print_two_lines() {
    let body = r#"{"data":[{"id":"good\n"},{"id":"good"}]}"#;
    let stub = Stub::start(vec![json_ok(body)]);
    let rig = Rig::new("models-pair", &stub.endpoint(), &DEFAULT);
    let ran = run_models(&rig, &[]);
    assert_eq!(exit(ran.code), 0, "{}", ran.stderr);
    // Byte order: "good" sorts before "good\n".
    assert_eq!(ran.stdout, "good\ngood\\u{a}\n");
    drop(stub.finish());
}

#[test]
fn a_model_that_the_catalogue_lacks_is_not_an_error_and_nothing_is_selected() {
    let stub = Stub::start(vec![json_ok(&catalogue(&["other"]))]);
    let rig = Rig::new("models-absent", &stub.endpoint(), &DEFAULT);
    let before = rig.fixture.text("ai.toml");
    let ran = run_models(&rig, &[]);
    assert_eq!(exit(ran.code), 0, "{}", ran.stderr);
    assert_eq!(ran.stdout, "other\n");
    assert_eq!(rig.fixture.text("ai.toml"), before);
    drop(stub.finish());
}

fn discovery_failure(name: &str, reply: crate::tests_e2e_responses::stub::Reply, limits: Limits) {
    let stub = Stub::start(vec![reply]);
    let rig = Rig::new(name, &stub.endpoint(), &limits);
    let ran = run_models(&rig, &[]);
    assert_eq!(exit(ran.code), 5, "{name}: {}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0109"], "{name}: {}", ran.stderr);
    assert_eq!(ran.stdout, "", "{name}");
    assert_eq!(rig.log_lines().len(), 1, "{name}");
    drop(stub.finish());
}

#[test]
fn a_401_malformed_json_a_missing_id_and_an_oversize_body_are_rt_ai_0109_exit_5() {
    let one_try = || Limits {
        retries: 0,
        response_bytes: 1024,
    };
    discovery_failure(
        "models-401",
        http("401 Unauthorized", &[], b"{}"),
        one_try(),
    );
    discovery_failure("models-bad-json", json_ok("{not json"), one_try());
    discovery_failure(
        "models-no-id",
        json_ok(r#"{"data":[{"name":"x"}]}"#),
        one_try(),
    );
    discovery_failure("models-no-data", json_ok(r#"{"models":[]}"#), one_try());
    let big = vec![b' '; 1025];
    discovery_failure("models-big", http("200 OK", &[], &big), one_try());
}

#[test]
fn the_failure_message_names_the_kind_of_failure() {
    let stub = Stub::start(vec![http("401 Unauthorized", &[], b"{}")]);
    let rig = Rig::new("models-kind", &stub.endpoint(), &DEFAULT);
    let ran = run_models(&rig, &[]);
    assert!(
        ran.stderr.contains("authentication failed"),
        "{}",
        ran.stderr
    );
    drop(stub.finish());
}

#[test]
fn a_503_is_retried_on_the_simulated_clock_then_the_list_is_printed() {
    let stub = Stub::start(vec![
        http("503 Service Unavailable", &[], b""),
        json_ok(&catalogue(&["m1"])),
    ]);
    let rig = Rig::new("models-retry", &stub.endpoint(), &DEFAULT);
    let started = std::time::Instant::now();
    let ran = run_models(&rig, &[]);
    assert_eq!(exit(ran.code), 0, "{}", ran.stderr);
    assert_eq!(ran.stdout, "m1\n");
    assert_eq!(warnings(&ran), ["RT-AI-0101"]);
    assert_eq!(rig.waits(), [Duration::from_secs(2)]);
    assert_eq!(rig.log_lines().len(), 2);
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(stub.finish());
}

#[test]
fn exhausted_retries_are_rt_ai_0109_after_three_attempts() {
    let gateway = || http("504 Gateway Timeout", &[], b"");
    let stub = Stub::start(vec![gateway(), gateway(), gateway()]);
    let rig = Rig::new("models-exhausted", &stub.endpoint(), &DEFAULT);
    let ran = run_models(&rig, &[]);
    assert_eq!(exit(ran.code), 5, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0109"], "{}", ran.stderr);
    assert_eq!(stub.connections(), 3);
    drop(stub.finish());
}

#[test]
fn models_without_a_config_or_with_a_foreign_option_exits_2() {
    let fixture = Fixture::new("models-args");
    let env = MapEnv::new(&[]);
    let none = run_providers(
        &[OsString::from("models")],
        &env,
        &crate::tests_gate_rig::Unlinked,
        &StdFs,
    );
    assert_eq!(exit(none.code), 2, "{}", none.stderr);
    assert_eq!(none.codes(), ["RT-AI-0001"], "{}", none.stderr);
    let consent = run_providers(
        &models_args(&fixture, &["--consent", "rules.md"]),
        &env,
        &crate::tests_gate_rig::Unlinked,
        &StdFs,
    );
    assert_eq!(exit(consent.code), 2, "{}", consent.stderr);
    assert!(
        consent.codes().contains(&"RT-CLI-0004".to_owned()),
        "{}",
        consent.stderr
    );
}

#[test]
fn models_refuses_a_disabled_configuration_and_a_missing_credential_before_any_connection() {
    let stub = Stub::start(vec![json_ok(&catalogue(&["m"]))]);
    let rig = Rig::new("models-gates", &stub.endpoint(), &DEFAULT);
    let text = config_text(&rig.fixture, &stub.endpoint(), &DEFAULT);
    rig.fixture
        .write_config(&text.replace("enabled = true", "enabled = false"));
    let ran = run_models(&rig, &[]);
    assert_eq!(exit(ran.code), 2, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0001"], "{}", ran.stderr);
    rig.fixture.write_config(&text);
    let env = MapEnv::new(&[]);
    let ran = run_providers(
        &models_args(&rig.fixture, &[]),
        &env,
        &Stubbed(rig.waits.clone()),
        &StdFs,
    );
    assert_eq!(exit(ran.code), 2, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0007"], "{}", ran.stderr);
    assert_eq!(stub.connections(), 0);
    drop(stub.finish());
}

#[test]
fn models_does_not_negotiate_capabilities() {
    let stub = Stub::start(vec![json_ok(&catalogue(&["m"]))]);
    let rig = Rig::new("models-no-negotiation", &stub.endpoint(), &DEFAULT);
    let text = config_text(&rig.fixture, &stub.endpoint(), &DEFAULT);
    rig.fixture
        .write_config(&text.replace("id = \"meta\"", "id = \"openai\""));
    let ran = run_models(&rig, &[]);
    assert_eq!(exit(ran.code), 0, "{}", ran.stderr);
    assert_eq!(stub.connections(), 1);
    drop(stub.finish());
}

#[test]
fn a_replay_configuration_has_no_catalogue_and_sends_nothing() {
    let fixture = Fixture::new("models-replay");
    let env = MapEnv::new(&[]);
    let ran = run_providers(
        &models_args(&fixture, &[]),
        &env,
        &crate::BuiltInProviders,
        &StdFs,
    );
    assert_eq!(exit(ran.code), 5, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-AI-0109"], "{}", ran.stderr);
}

#[test]
fn verify_model_true_with_a_listed_model_queries_once_then_sends_the_request() {
    let stub = Stub::start(vec![json_ok(&catalogue(&["muse-spark-1.3"])), success("")]);
    let rig = Rig::new("verify-ok", &stub.endpoint(), &DEFAULT);
    with_verify(&rig, &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert_eq!(exit(ran.code), 0, "{}", ran.stderr);
    let captured = stub.finish();
    let lines: Vec<String> = captured.iter().map(|c| c.request_line()).collect();
    assert_eq!(
        lines,
        ["GET /v1/models HTTP/1.1", "POST /v1/responses HTTP/1.1"]
    );
    assert_eq!(rig.log_lines().len(), 2);
    assert!(rig.log_lines()[0].contains(r#""kind":"models""#));
    assert!(rig.batch_path().exists());
}

#[test]
fn verify_model_true_with_an_unlisted_model_is_rt_ai_0110_and_sends_no_request() {
    let stub = Stub::start(vec![json_ok(&catalogue(&["other"])), success("")]);
    let rig = Rig::new("verify-absent", &stub.endpoint(), &DEFAULT);
    with_verify(&rig, &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert_eq!(exit(ran.code), 5, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0110"], "{}", ran.stderr);
    assert!(
        ran.stderr
            .contains(" line 11 column 1 field provider.model"),
        "{}",
        ran.stderr
    );
    let captured = stub.finish();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].request_line(), "GET /v1/models HTTP/1.1");
    assert!(store_is_empty(&rig.fixture));
    assert_eq!(rig.log_lines().len(), 1);
}

#[test]
fn the_catalogue_match_is_exact_and_case_sensitive() {
    for listed in ["Muse-Spark-1.3", "muse-spark-1.3 ", "muse-spark-1.3-x"] {
        let stub = Stub::start(vec![json_ok(&catalogue(&[listed])), success("")]);
        let rig = Rig::new("verify-exact", &stub.endpoint(), &DEFAULT);
        with_verify(&rig, &stub.endpoint(), &DEFAULT);
        let ran = rig.run();
        assert_eq!(errors(&ran), ["RT-AI-0110"], "{listed}: {}", ran.stderr);
        assert_eq!(stub.finish().len(), 1);
    }
}

#[test]
fn a_failed_discovery_before_a_run_is_rt_ai_0109_and_sends_no_request() {
    let stub = Stub::start(vec![http("401 Unauthorized", &[], b"{}"), success("")]);
    let rig = Rig::new("verify-401", &stub.endpoint(), &DEFAULT);
    with_verify(&rig, &stub.endpoint(), &DEFAULT);
    let ran = rig.run();
    assert_eq!(exit(ran.code), 5, "{}", ran.stderr);
    assert_eq!(errors(&ran), ["RT-AI-0109"], "{}", ran.stderr);
    assert_eq!(stub.finish().len(), 1);
    assert!(store_is_empty(&rig.fixture));
}

#[test]
fn without_verify_model_no_catalogue_query_is_made() {
    let stub = Stub::start(vec![success("")]);
    let rig = Rig::new("verify-off", &stub.endpoint(), &DEFAULT);
    assert_eq!(exit(rig.run().code), 0);
    let captured = stub.finish();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].request_line(), "POST /v1/responses HTTP/1.1");
}

#[test]
fn a_dry_run_and_a_missing_consent_make_no_catalogue_query() {
    let stub = Stub::start(vec![json_ok(&catalogue(&["muse-spark-1.3"]))]);
    let rig = Rig::new("verify-gates", &stub.endpoint(), &DEFAULT);
    with_verify(&rig, &stub.endpoint(), &DEFAULT);
    let env = MapEnv::new(&[("MODEL_API_KEY", CANARY)]);
    let dry = run_providers(
        &rig.fixture.args(&["--dry-run"]),
        &env,
        &Stubbed(rig.waits.clone()),
        &StdFs,
    );
    assert_eq!(exit(dry.code), 0, "{}", dry.stderr);
    let unconsented = run_providers(
        &rig.fixture.args(&[]),
        &env,
        &Stubbed(rig.waits.clone()),
        &StdFs,
    );
    assert_eq!(
        errors(&unconsented),
        ["RT-AI-0008"],
        "{}",
        unconsented.stderr
    );
    assert_eq!(stub.connections(), 0);
    drop(stub.finish());
}
