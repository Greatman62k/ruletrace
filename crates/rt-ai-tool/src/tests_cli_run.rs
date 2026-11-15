//! End-to-end runs of `ruletrace-ai propose` with the recorded replay
//! provider over files on disk (Req 16.16, 16.17, 23.1, 27.1 to 27.4, 27.8).
//! Expected codes come from the catalogue and the requirements; the stored
//! batch is read back through the Proposal_Store parser.

use crate::tests::MapEnv;
use crate::tests_cli_rig::{run_std, run_with, Fixture, CREATED_AT};
use crate::tests_memfs::{Faults, MemFs};
use crate::tests_provenance::RULES_HASH;
use crate::tests_rig::{evidence, payload};
use crate::tests_run_support::blocking_payload_for;
use crate::StdFs;
use rt_foundation::{ExitCode, InputId};
use std::io::ErrorKind;

fn with_work_dir(fixture: &Fixture) {
    std::fs::create_dir_all(fixture.path("work")).unwrap();
}

fn batch_path(id: &str) -> String {
    format!("store/batches/{id}.json")
}

fn bad_hash_payload() -> String {
    let good = evidence(0);
    payload(0).replace(&good, &good.replace(RULES_HASH, &"0".repeat(64)))
}

fn log_lines(fixture: &Fixture) -> Vec<String> {
    std::fs::read_to_string(fixture.path("work/tx.jsonl"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

// The byte count of the first announcement line on standard error.
fn announced_bytes(stderr: &str) -> u64 {
    let line = stderr.lines().find(|l| l.contains(": provider ")).unwrap();
    let tail = line.rsplit_once("bytes ").unwrap().1;
    tail.parse().unwrap()
}

#[test]
fn observable_a_mock_replay_run_stores_a_batch_and_a_log_entry_and_exits_0() {
    let fixture = Fixture::new("run-observable");
    with_work_dir(&fixture);
    fixture.write_script(&payload(0));
    let id = fixture.request_id();
    let rulebook_before = std::fs::read(fixture.path("rulebook.json")).unwrap();

    let ran = run_std(&fixture.consented());

    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert!(ran.shapes().is_empty(), "{}", ran.stderr);
    assert_eq!(
        ran.stdout,
        "run summary: complete\nprocessed:\n  rules.md 0 11\nstored:\n  rules.md 0 11\nunprocessed:\n  (none)\n"
    );
    let batch = std::fs::read(fixture.path(&batch_path(&id))).unwrap();
    let parsed = rt_proposals::parse_batch(&InputId::new("batch").unwrap(), &batch).unwrap();
    assert_eq!(parsed.request_id.as_str(), id);
    assert_eq!(parsed.provider, "mock-replay");
    assert_eq!(parsed.model, "model-1");
    assert_eq!(parsed.adapter_version, "mock-1");
    assert_eq!(parsed.created_at, CREATED_AT);
    assert_eq!(parsed.response.payload, payload(0));
    assert_eq!(parsed.proposals.len(), 1);

    let bytes = announced_bytes(&ran.stderr);
    let expected = format!(
        r#"{{"provider":"mock-replay","model":"model-1","endpoint_host":"","documents":[{{"document":"rules.md","sha256":"{RULES_HASH}"}}],"clauses":1,"request_bytes":{bytes},"request_id":"{id}","attempt":1,"time":"{CREATED_AT}"}}"#
    );
    assert_eq!(log_lines(&fixture), [expected]);
    assert_eq!(
        std::fs::read(fixture.path("rulebook.json")).unwrap(),
        rulebook_before
    );
    let mut written = fixture.files();
    written.retain(|f| f.starts_with("store/") || f.starts_with("work/"));
    assert_eq!(written, [batch_path(&id), "work/tx.jsonl".to_owned()]);
}

#[test]
fn a_run_opens_no_ledger_file_and_writes_no_file_but_the_store_and_the_log() {
    let fixture = Fixture::new("run-writes");
    with_work_dir(&fixture);
    fixture.write_script(&payload(0));
    let id = fixture.request_id();
    let before = fixture.files();
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    let mut expected = before;
    expected.push(batch_path(&id));
    expected.push("work/tx.jsonl".to_owned());
    expected.sort();
    assert_eq!(fixture.files(), expected);
    assert!(fixture.files().iter().all(|f| !f.contains("ledger")));
}

#[test]
fn the_command_sources_name_no_ledger_and_the_crate_depends_on_no_approval_crate() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/");
    for file in ["cli.rs", "command.rs", "inputs.rs", "main.rs"] {
        let text = std::fs::read_to_string(format!("{dir}{file}")).unwrap();
        let code: Vec<&str> = text
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect();
        for line in code {
            assert!(!line.to_lowercase().contains("ledger"), "{file}: {line}");
        }
    }
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    let normal = manifest.split("[dev-dependencies]").next().unwrap();
    for banned in [
        "rt-review",
        "rt-compile",
        "rt-engine",
        "rt-compiled",
        "rt-ledger",
        "rt-cli",
    ] {
        assert!(!normal.contains(&format!("{banned} =")), "{banned}");
    }
}

#[test]
fn step_5_an_unwritable_transmission_log_exits_70_and_nothing_is_sent() {
    let fixture = Fixture::new("run-log");
    fixture.write_script(&payload(0));
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::Internal);
    assert_eq!(ran.codes(), ["RT-AI-0010"]);
    assert!(!ran.stderr.contains("request "), "{}", ran.stderr);
    assert!(!fixture.path("store").exists());
    assert!(
        ran.stdout.contains("run summary: incomplete"),
        "{}",
        ran.stdout
    );
    assert!(
        ran.stdout.contains("unprocessed:\n  rules.md 0 11\n"),
        "{}",
        ran.stdout
    );
}

#[test]
fn step_6_a_provider_failure_exits_5_after_one_logged_attempt() {
    let fixture = Fixture::new("run-provider");
    with_work_dir(&fixture);
    fixture.write_script_for(&"1".repeat(64), &payload(0));
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::Provider);
    assert_eq!(ran.codes(), ["RT-AI-0105"]);
    assert_eq!(log_lines(&fixture).len(), 1);
    assert!(!fixture.path("store/batches").exists());
}

#[test]
fn steps_7_and_8_a_rejected_response_exits_6_and_stores_nothing() {
    let cases = [
        ("{ not json", "RT-AI-0201"),
        (r#"{"schema_version":"1","proposals":"x"}"#, "RT-AI-0202"),
        (
            r#"{"schema_version":"1","proposals":[],"approval_status":"approved"}"#,
            "RT-AI-0204",
        ),
    ];
    for (text, code) in cases {
        let fixture = Fixture::new("run-response");
        with_work_dir(&fixture);
        fixture.write_script(text);
        let ran = run_std(&fixture.consented());
        assert_eq!(ran.code, ExitCode::Response, "{code}: {}", ran.stderr);
        assert_eq!(ran.codes(), [code], "{}", ran.stderr);
        assert!(!fixture.path("store/batches").exists());
    }
}

#[test]
fn step_9_a_provenance_failure_exits_3_and_stores_nothing() {
    let fixture = Fixture::new("run-provenance");
    with_work_dir(&fixture);
    fixture.write_script(&bad_hash_payload());
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::Blocked);
    assert_eq!(ran.codes(), ["RT-PRV-0102"]);
    assert!(!fixture.path("store/batches").exists());
}

#[test]
fn step_11_a_failed_store_write_exits_70() {
    let fixture = Fixture::new("run-store");
    with_work_dir(&fixture);
    fixture.write_script(&payload(0));
    let fs = MemFs::new();
    fs.with_faults(Faults {
        create_temp: Some(ErrorKind::PermissionDenied),
        ..Faults::default()
    });
    let ran = run_with(&fixture.consented(), &MapEnv::new(&[]), &fs);
    assert_eq!(ran.code, ExitCode::Internal);
    assert_eq!(ran.codes(), ["RT-AI-0303"]);
    assert!(fs.files().is_empty());
}

#[test]
fn a_stored_batch_with_blocking_findings_is_a_warning_and_exit_0() {
    let fixture = Fixture::new("run-blocking");
    with_work_dir(&fixture);
    fixture.write_script(&blocking_payload_for(0));
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert_eq!(
        ran.shapes(),
        [("warning".to_owned(), "RT-AI-0305".to_owned())]
    );
    assert!(fixture.path(&batch_path(&fixture.request_id())).exists());
}

#[test]
fn a_warning_never_changes_the_exit_code() {
    let fixture = Fixture::new("run-warning");
    with_work_dir(&fixture);
    fixture.write_script(&payload(0));
    use std::os::unix::fs::PermissionsExt;
    let open = std::fs::Permissions::from_mode(0o644);
    std::fs::set_permissions(fixture.path("ai.toml"), open).unwrap();
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::Success);
    assert_eq!(
        ran.shapes(),
        [("warning".to_owned(), "RT-AI-0009".to_owned())]
    );
}

#[test]
fn a_second_run_finds_the_batch_sends_nothing_and_exits_0() {
    let fixture = Fixture::new("run-again");
    with_work_dir(&fixture);
    fixture.write_script(&payload(0));
    assert_eq!(run_std(&fixture.consented()).code, ExitCode::Success);
    let batch = std::fs::read(fixture.path(&batch_path(&fixture.request_id()))).unwrap();
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::Success);
    assert_eq!(
        ran.shapes(),
        [("warning".to_owned(), "RT-AI-0301".to_owned())]
    );
    assert_eq!(log_lines(&fixture).len(), 1);
    assert_eq!(
        std::fs::read(fixture.path(&batch_path(&fixture.request_id()))).unwrap(),
        batch
    );
}

#[test]
fn a_failed_run_leaves_the_rulebook_and_the_store_unchanged() {
    let fixture = Fixture::new("run-failed-unchanged");
    with_work_dir(&fixture);
    fixture.write_script(&bad_hash_payload());
    let rulebook = std::fs::read(fixture.path("rulebook.json")).unwrap();
    let before = fixture.files();
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::Blocked);
    assert_eq!(
        std::fs::read(fixture.path("rulebook.json")).unwrap(),
        rulebook
    );
    let mut expected = before;
    expected.push("work/tx.jsonl".to_owned());
    expected.sort();
    assert_eq!(fixture.files(), expected);
}

#[test]
fn dry_run_prints_each_request_identifier_and_sends_nothing() {
    let fixture = Fixture::new("run-dry");
    with_work_dir(&fixture);
    fixture.write_config(&fixture.real_config(""));
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", "canary-0123456789abcdef")]);
    let before = fixture.files();
    let ran = run_with(&fixture.args(&["--dry-run"]), &env, &StdFs);
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert_eq!(ran.stdout.len(), 65);
    assert!(ran.stdout.ends_with('\n'));
    let id = ran.stdout.trim_end();
    assert!(id
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    let bytes = announced_bytes(&ran.stderr);
    assert_eq!(
        ran.stderr,
        format!(
            "dry run, nothing is sent: request {id}: provider openai-responses, endpoint host api.example.org, documents ['rules.md'], clauses 1, bytes {bytes}\n"
        )
    );
    assert!(env.reads().is_empty(), "{:?}", env.reads());
    assert_eq!(fixture.files(), before);
}

#[test]
fn dry_run_needs_no_consent_and_does_not_open_the_script_or_the_log() {
    let fixture = Fixture::new("run-dry-noconsent");
    with_work_dir(&fixture);
    let before = fixture.files();
    let ran = run_std(&fixture.args(&["--dry-run"]));
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert_eq!(ran.stdout.lines().count(), 1);
    assert_eq!(fixture.files(), before);
    assert!(!fixture.path("store").exists());
}

#[test]
fn dry_run_still_reports_a_configuration_failure_at_step_2() {
    let fixture = Fixture::new("run-dry-config");
    fixture.write_config("schema_version = 1\nenabled =\n");
    let ran = run_std(&fixture.args(&["--dry-run"]));
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.codes(), ["RT-AI-0002"]);
    assert_eq!(ran.stdout, "");
}

#[test]
fn dry_run_prints_one_identifier_per_request_in_plan_order() {
    let fixture = Fixture::new("run-dry-two");
    let source = crate::tests_provenance::model("rules.md", crate::tests_provenance::RULES);
    let clauses: Vec<_> = [(0, 11), (12, 27)]
        .iter()
        .map(|(s, e)| rt_srcmodel::CandidateClause::new(&source, *s, *e, None).unwrap())
        .collect();
    let document = rt_foundation::DocumentId::new("rules.md").unwrap();
    std::fs::write(
        fixture.path("candidates.json"),
        rt_srcmodel::print_candidates(&document, &clauses),
    )
    .unwrap();
    fixture.write_config(&fixture.mock_config("max_clauses_per_request = 1\n"));
    let ran = run_std(&fixture.args(&["--dry-run"]));
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    let ids: Vec<&str> = ran.stdout.lines().collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    assert_eq!(ran.stderr.lines().count(), 2);
}
