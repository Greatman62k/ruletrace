//! Gate-level failure tests of `ruletrace-ai propose` with the mock
//! providers, one test per failure class (Req 23.17, 23.33, 23.38, 29.14).
//!
//! Every run goes through `run_with`, the entry point of the executable,
//! over files in a scratch directory. The expected exit code, Diagnostic
//! codes and severities, Run_Summary text, store files and
//! Transmission_Log lines are written from the requirements and the
//! catalogue. The simulated timer matrix, the cross product of failures and
//! concurrent runs belong to the Phase 10 tests.

use crate::tests::MapEnv;
use crate::tests_cli_rig::{run_std, Fixture};
use crate::tests_gate_rig::{
    announced_size, announcements, batch_path, findings, inputs_and_log, log_line, log_lines,
    request_ids, run_providers, shapes_of, three_requests, work_dir, write_script_entries, Probe,
    Steps, COMPLETE_ONE, NONE_PROCESSED,
};
use crate::tests_provenance::doc;
use crate::tests_rig::payload;
use crate::tests_run::{fail, ok};
use crate::tests_run_support::blocking_payload_for;
use crate::StdFs;
use rt_ai_adapter::ProviderFailure;
use rt_foundation::{Code, ExitCode, InputId, Severity};
use rt_proposals::{parse_batch, StoredLocation};

type Finding = (String, String, String);

fn finding(severity: &str, code: &str, message: &str) -> Finding {
    (severity.to_owned(), code.to_owned(), message.to_owned())
}

fn pair(severity: &str, code: &str) -> (String, String) {
    (severity.to_owned(), code.to_owned())
}

fn run_steps(
    name: &str,
    steps: Vec<rt_ai_mock::Step>,
) -> (Fixture, String, crate::tests_cli_rig::Ran, Probe) {
    let fixture = Fixture::new(name);
    work_dir(&fixture);
    let id = fixture.request_id();
    let (providers, probe) = Steps::new(steps);
    let ran = run_providers(&fixture.consented(), &MapEnv::new(&[]), &providers, &StdFs);
    (fixture, id, ran, probe)
}

// Every attempt of one request sends the same bytes (Req 29.16), so the
// announcements and the Transmission_Log agree on one size.
fn log_of_attempts(id: &str, ran: &crate::tests_cli_rig::Ran, count: u32) -> Vec<String> {
    let lines = announcements(ran);
    assert_eq!(lines.len(), count as usize, "{}", ran.stderr);
    let size = announced_size(&lines[0]);
    for line in &lines {
        assert!(
            line.starts_with(&format!("request {id}: provider mock-replay, ")),
            "{line}"
        );
        assert_eq!(announced_size(line), size, "{line}");
    }
    (1..=count).map(|n| log_line(id, size, n)).collect()
}

fn assert_nothing_stored(fixture: &Fixture) {
    assert!(!fixture.path("store").exists());
    assert_eq!(fixture.files(), inputs_and_log(fixture, &[]));
}

#[test]
fn transport_failure_then_success_stores_one_batch_and_exits_0() {
    let (fixture, id, ran, probe) = run_steps(
        "gate-retry-success",
        vec![fail(ProviderFailure::Unavailable, 0), ok(0)],
    );
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert_eq!(
        findings(&ran),
        [finding(
            "warning",
            "RT-AI-0101",
            "attempt 1 failed: AI_PROVIDER_UNAVAILABLE"
        )]
    );
    assert_eq!(ran.stdout, COMPLETE_ONE);
    assert_eq!(probe.attempts(), 2);
    // Req 29.3: a fixed wait of 2 s, then the 100 ms of the second step.
    assert_eq!(probe.clock_ms(), 2_100);
    assert_eq!(log_lines(&fixture), log_of_attempts(&id, &ran, 2));
    assert_eq!(
        fixture.files(),
        inputs_and_log(&fixture, &[&batch_path(&id)])
    );

    let baseline = Fixture::new("gate-retry-baseline");
    work_dir(&baseline);
    baseline.write_script(&payload(0));
    let clean = run_std(&baseline.consented());
    assert_eq!(clean.code, ExitCode::Success, "{}", clean.stderr);
    assert!(clean.stderr.lines().all(|l| l.starts_with("request ")));
    assert_eq!(
        std::fs::read(fixture.path(&batch_path(&id))).unwrap(),
        std::fs::read(baseline.path(&batch_path(&id))).unwrap(),
        "Req 29.18: the same stored bytes after any number of retries"
    );
}

#[test]
fn retry_exhaustion_exits_5_with_three_warnings_and_one_error() {
    let (fixture, id, ran, probe) = run_steps(
        "gate-exhausted",
        vec![
            fail(ProviderFailure::Unavailable, 0),
            fail(ProviderFailure::Unavailable, 0),
            fail(ProviderFailure::Unavailable, 0),
            ok(0),
        ],
    );
    assert_eq!(ran.code, ExitCode::Provider, "{}", ran.stderr);
    assert_eq!(ran.code.value(), 5);
    let found = findings(&ran);
    let expected: Vec<Finding> = (1..=3)
        .map(|n| {
            finding(
                "warning",
                "RT-AI-0101",
                &format!("attempt {n} failed: AI_PROVIDER_UNAVAILABLE"),
            )
        })
        .collect();
    assert_eq!(found[..3], expected[..]);
    assert_eq!(found.len(), 4, "{}", ran.stderr);
    assert_eq!(found[3].0, "error");
    assert_eq!(found[3].1, "RT-AI-0106");
    // Req 29.6: the attempt count and the last Warning's name.
    assert!(found[3].2.contains('3'), "{}", found[3].2);
    assert!(
        found[3].2.contains("AI_PROVIDER_UNAVAILABLE"),
        "{}",
        found[3].2
    );
    assert_eq!(ran.stdout, NONE_PROCESSED);
    assert_eq!(probe.attempts(), 3);
    // Req 29.3 and 29.4: waits of 2 s and 8 s, no wait after the last.
    assert_eq!(probe.clock_ms(), 10_000);
    assert_eq!(log_lines(&fixture), log_of_attempts(&id, &ran, 3));
    assert_nothing_stored(&fixture);
}

#[test]
fn a_non_transient_failure_exits_5_after_one_attempt() {
    let cases = [
        (ProviderFailure::AuthFailed, "RT-AI-0104"),
        (ProviderFailure::TlsFailed, "RT-AI-0108"),
        (
            ProviderFailure::Other {
                http_status: Some(400),
            },
            "RT-AI-0105",
        ),
    ];
    for (n, (failure, code)) in cases.into_iter().enumerate() {
        let (fixture, id, ran, probe) =
            run_steps(&format!("gate-final-{n}"), vec![fail(failure, 0), ok(0)]);
        assert_eq!(ran.code, ExitCode::Provider, "{code}: {}", ran.stderr);
        assert_eq!(shapes_of(&ran), [pair("error", code)], "{}", ran.stderr);
        assert_eq!(probe.attempts(), 1, "{code}");
        assert_eq!(probe.clock_ms(), 0, "{code}");
        assert_eq!(ran.stdout, NONE_PROCESSED);
        assert_eq!(log_lines(&fixture), log_of_attempts(&id, &ran, 1));
        assert_nothing_stored(&fixture);
    }
}

#[test]
fn a_non_transient_failure_after_a_retry_keeps_the_earlier_warning() {
    let (fixture, id, ran, probe) = run_steps(
        "gate-final-after-retry",
        vec![
            fail(ProviderFailure::Timeout, 20_000),
            fail(ProviderFailure::AuthFailed, 0),
            ok(0),
        ],
    );
    assert_eq!(ran.code, ExitCode::Provider, "{}", ran.stderr);
    assert_eq!(
        shapes_of(&ran),
        [pair("warning", "RT-AI-0102"), pair("error", "RT-AI-0104")]
    );
    assert_eq!(probe.attempts(), 2);
    assert_eq!(log_lines(&fixture), log_of_attempts(&id, &ran, 2));
    assert_nothing_stored(&fixture);
}

fn bad_span() -> String {
    let text = payload(0).replace("\"end\":11,", "\"end\":99,");
    assert_ne!(text, payload(0));
    text
}

fn quote_mismatch() -> String {
    let text = payload(0).replace("\"quote\":\"Alpha beta.\"", "\"quote\":\"Alpha gamma.\"");
    assert_ne!(text, payload(0));
    text
}

// A response that fails after one attempt: the exit code and the one
// Error of the class, the incomplete summary, one logged attempt and no
// store (Req 29.12, 29.17, 30.1).
fn assert_rejected_once(name: &str, text: &str, exit: u8, code: &str) {
    let fixture = Fixture::new(name);
    work_dir(&fixture);
    fixture.write_script(text);
    let id = fixture.request_id();
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code.value(), exit, "{code}: {}", ran.stderr);
    assert_eq!(shapes_of(&ran), [pair("error", code)], "{}", ran.stderr);
    assert_eq!(ran.stdout, NONE_PROCESSED);
    assert_eq!(log_lines(&fixture), log_of_attempts(&id, &ran, 1));
    assert_nothing_stored(&fixture);
}

#[test]
fn a_malformed_response_exits_6_with_rt_ai_0201_and_stores_nothing() {
    assert_rejected_once("gate-malformed", "{ not json", 6, "RT-AI-0201");
}

#[test]
fn a_schema_violation_exits_6_with_rt_ai_0202_and_stores_nothing() {
    let text = r#"{"schema_version":"1","proposals":"x"}"#;
    assert_rejected_once("gate-schema", text, 6, "RT-AI-0202");
}

#[test]
fn a_bad_span_exits_3_with_rt_prv_0101_and_stores_nothing() {
    assert_rejected_once("gate-span", &bad_span(), 3, "RT-PRV-0101");
}

#[test]
fn a_quote_mismatch_exits_3_with_rt_prv_0104_and_stores_nothing() {
    assert_rejected_once("gate-quote", &quote_mismatch(), 3, "RT-PRV-0104");
}

#[test]
fn a_store_write_failure_exits_70_and_leaves_the_store_files_as_they_were() {
    let fixture = Fixture::new("gate-store-write");
    work_dir(&fixture);
    fixture.write_script(&payload(0));
    let id = fixture.request_id();
    // A regular file where the store needs its temporary directory.
    std::fs::create_dir_all(fixture.path("store")).unwrap();
    std::fs::write(fixture.path("store/.tmp"), "planted").unwrap();
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::Internal, "{}", ran.stderr);
    assert_eq!(ran.code.value(), 70);
    assert_eq!(shapes_of(&ran), [pair("error", "RT-AI-0303")]);
    assert_eq!(ran.stdout, NONE_PROCESSED);
    assert_eq!(log_lines(&fixture), log_of_attempts(&id, &ran, 1));
    assert_eq!(fixture.files(), inputs_and_log(&fixture, &["store/.tmp"]));
    assert_eq!(fixture.text("store/.tmp"), "planted");
    let batches = fixture.path("store/batches");
    if batches.exists() {
        assert_eq!(std::fs::read_dir(batches).unwrap().count(), 0);
    }
}

#[test]
fn a_well_formed_unsupported_clause_is_stored_as_a_non_executable_proposal() {
    let fixture = Fixture::new("gate-unsupported");
    work_dir(&fixture);
    fixture.write_script(&blocking_payload_for(0));
    let id = fixture.request_id();
    let rulebook = std::fs::read(fixture.path("rulebook.json")).unwrap();
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert_eq!(ran.code.value(), 0);
    assert_eq!(shapes_of(&ran), [pair("warning", "RT-AI-0305")]);
    assert_eq!(ran.stdout, COMPLETE_ONE);
    assert_eq!(log_lines(&fixture), log_of_attempts(&id, &ran, 1));
    assert_eq!(
        fixture.files(),
        inputs_and_log(&fixture, &[&batch_path(&id)])
    );
    assert_eq!(
        std::fs::read(fixture.path("rulebook.json")).unwrap(),
        rulebook
    );

    let stored = std::fs::read(fixture.path(&batch_path(&id))).unwrap();
    let batch = parse_batch(&InputId::new("batch").unwrap(), &stored).unwrap();
    assert_eq!(batch.proposals.len(), 1);
    let proposal = &batch.proposals[0];
    assert_eq!(proposal.outcome.len(), 1, "{:?}", proposal.outcome);
    let found = &proposal.outcome[0];
    assert_eq!(found.code, Code::ValUnsupportedConstruct);
    assert_eq!(found.code.id(), "RT-VAL-0003");
    assert_eq!(found.code.severity(), Severity::Unsupported);
    assert_eq!(
        found.location,
        StoredLocation::Source {
            document: doc("rules.md"),
            start: 0,
            end: 11
        }
    );
    assert!(proposal.rule.is_some());
}

#[test]
fn a_failing_second_request_keeps_the_first_batch_and_never_sends_the_third() {
    let good = Fixture::new("gate-three-good");
    work_dir(&good);
    three_requests(&good);
    let ids = request_ids(&good);
    assert_eq!(ids.len(), 3);
    let all: Vec<(String, String)> = (0..3).map(|i| (ids[i].clone(), payload(i))).collect();
    write_script_entries(&good, &all);
    let clean = run_std(&good.consented());
    assert_eq!(clean.code, ExitCode::Success, "{}", clean.stderr);
    assert_eq!(
        clean.stdout,
        "run summary: complete\n\
         processed:\n  rules.md 0 11\n  rules.md 12 27\n  rules.md 28 32\n\
         stored:\n  rules.md 0 11\n  rules.md 12 27\n  rules.md 28 32\n\
         unprocessed:\n  (none)\n"
    );
    let first = std::fs::read(good.path(&batch_path(&ids[0]))).unwrap();

    let bad = Fixture::new("gate-three-bad");
    work_dir(&bad);
    three_requests(&bad);
    let bad_ids = request_ids(&bad);
    let broken = payload(1).replace("\"end\":27,", "\"end\":99,");
    assert_ne!(broken, payload(1));
    write_script_entries(
        &bad,
        &[
            (bad_ids[0].clone(), payload(0)),
            (bad_ids[1].clone(), broken),
            (bad_ids[2].clone(), payload(2)),
        ],
    );
    let ran = run_std(&bad.consented());
    assert_eq!(ran.code, ExitCode::Blocked, "{}", ran.stderr);
    assert_eq!(ran.code.value(), 3);
    assert_eq!(
        shapes_of(&ran),
        [pair("error", "RT-PRV-0101"), pair("warning", "RT-AI-0306")]
    );
    assert_eq!(
        ran.stdout,
        "run summary: incomplete\n\
         processed:\n  rules.md 0 11\n\
         stored:\n  rules.md 0 11\n\
         unprocessed:\n  rules.md 12 27\n  rules.md 28 32\n"
    );
    let sent = announcements(&ran);
    assert_eq!(sent.len(), 2, "{}", ran.stderr);
    let expected: Vec<String> = (0..2)
        .map(|i| log_line(&bad_ids[i], announced_size(&sent[i]), 1))
        .collect();
    assert_eq!(log_lines(&bad), expected);
    assert!(!bad.text("work/tx.jsonl").contains(&bad_ids[2]));
    assert_eq!(
        bad.files(),
        inputs_and_log(&bad, &[&batch_path(&bad_ids[0])])
    );
    assert_eq!(
        std::fs::read(bad.path(&batch_path(&bad_ids[0]))).unwrap(),
        first
    );
}
