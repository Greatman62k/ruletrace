//! Directory sync skipped by the platform: the Warning RT-AI-0309, the
//! Run_Summary header and the rerun behavior (Req 7.17, 23.36, 29.19;
//! design 3.7.5, 3.7.7). Expected codes, severities, locations and texts
//! are written by hand from the requirements.

use crate::tests::MapEnv;
use crate::tests_cli_rig::{run_with, Fixture};
use crate::tests_memfs::{Faults, MemFs};
use crate::tests_rig::{payload, Harness};
use crate::tests_run::{cid, fail, ok, ok_with, provider};
use crate::tests_run_support::{blocking_payload, blocking_payload_for, shapes, VecSink};
use crate::Redactor;
use rt_ai_adapter::ProviderFailure;
use rt_foundation::{CliLocation, Diagnostic, ExitCode, Location, Severity};

const SKIP_CODE: &str = "RT-AI-0309";

fn skipping() -> Faults {
    Faults {
        sync_dir_skipped: true,
        ..Faults::default()
    }
}

fn command_propose() -> Location {
    Location::Cli(CliLocation::Command("propose".to_owned()))
}

fn skip_warnings(diagnostics: &[Diagnostic]) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.code().id() == SKIP_CODE)
        .collect()
}

fn sorted_ids(diagnostics: &[Diagnostic]) -> Vec<&'static str> {
    let mut all = diagnostics.to_vec();
    all.sort();
    all.iter().map(|d| d.code().id()).collect()
}

#[test]
fn the_skip_warning_is_one_warning_with_exit_0_at_the_command() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(skipping());
    let mut p = provider(vec![ok(0), ok(1), ok(2)]);
    let report = h.run(&mut p, &fs, &VecSink::default());
    assert_eq!(report.exit, ExitCode::Success);
    assert_eq!(shapes(&report.diagnostics), vec![(SKIP_CODE, "warning")]);
    let warning = &report.diagnostics[0];
    assert_eq!(warning.severity(), Severity::Warning);
    assert_eq!(warning.code().mnemonic(), "AI_STORE_DIRECTORY_SYNC_SKIPPED");
    assert_eq!(warning.code().exit_code(), ExitCode::Success);
    assert_eq!(warning.primary(), &command_propose());
}

#[test]
fn only_a_run_that_stored_a_skipped_batch_warns() {
    let h = Harness::new("");
    let fs = MemFs::new();
    let mut p = provider(vec![ok(0), ok(1), ok(2)]);
    let report = h.run(&mut p, &fs, &VecSink::default());
    assert!(skip_warnings(&report.diagnostics).is_empty());
    assert!(report.summary.platform_skipped().is_empty());
}

#[test]
fn a_mixed_run_warns_once_and_names_the_skipped_clauses_only() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        sync_dir_skipped: true,
        sync_dir_after: 1,
        ..Faults::default()
    });
    let mut p = provider(vec![ok(0), ok(1), ok(2)]);
    let report = h.run(&mut p, &fs, &VecSink::default());
    assert_eq!(shapes(&report.diagnostics), vec![(SKIP_CODE, "warning")]);
    assert_eq!(
        report.summary.render(&Redactor::no_credential()),
        "run summary: complete, directory durability not confirmed\n\
         processed:\n  rules.md 0 11\n  rules.md 12 27\n  rules.md 28 32\n\
         stored:\n  rules.md 0 11\n\
         stored, directory durability not confirmed (skipped by platform):\n  \
         rules.md 12 27\n  rules.md 28 32\n\
         unprocessed:\n  (none)\n"
    );
}

#[test]
fn a_skip_followed_by_a_failure_keeps_the_incomplete_header_and_warns() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(skipping());
    let mut p = provider(vec![ok(0), fail(ProviderFailure::AuthFailed, 100)]);
    let report = h.run(&mut p, &fs, &VecSink::default());
    assert_eq!(report.exit.value(), 5);
    let mut ids: Vec<&str> = report.diagnostics.iter().map(|d| d.code().id()).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["RT-AI-0104", "RT-AI-0306", SKIP_CODE]);
    assert_eq!(skip_warnings(&report.diagnostics).len(), 1);
    assert_eq!(
        report.summary.render(&Redactor::no_credential()),
        "run summary: incomplete\n\
         processed:\n  rules.md 0 11\n\
         stored:\n  (none)\n\
         stored, directory durability not confirmed (skipped by platform):\n  \
         rules.md 0 11\n\
         unprocessed:\n  rules.md 12 27\n  rules.md 28 32\n"
    );
}

#[test]
fn a_failure_before_any_skipped_batch_adds_no_skip_warning() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(skipping());
    let mut p = provider(vec![fail(ProviderFailure::AuthFailed, 100)]);
    let report = h.run(&mut p, &fs, &VecSink::default());
    assert!(skip_warnings(&report.diagnostics).is_empty());
}

#[test]
fn a_rerun_after_a_skipped_store_finds_every_batch_and_changes_nothing() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(skipping());
    let first_sink = VecSink::default();
    let first = h.run(&mut provider(vec![ok(0), ok(1), ok(2)]), &fs, &first_sink);
    assert_eq!(first.exit, ExitCode::Success);
    assert_eq!(first.summary.platform_skipped().len(), 3);
    let before = fs.files();
    let log_before = first_sink.text();
    assert_eq!(first_sink.lines().len(), 3);
    fs.clear_ops();

    let mut p = provider(vec![ok(0), ok(1), ok(2)]);
    let second = h.run(&mut p, &fs, &first_sink);
    assert_eq!(second.exit, ExitCode::Success);
    assert_eq!(
        shapes(&second.diagnostics),
        vec![("RT-AI-0301", "warning"); 3]
    );
    assert_eq!(p.steps_consumed(), 0);
    assert_eq!(first_sink.text(), log_before);
    assert_eq!(fs.files(), before);
    assert!(fs.ops().iter().all(|o| o.starts_with("exists ")));
    assert!(second.summary.platform_skipped().is_empty());
    assert_eq!(second.summary.processed(), [cid(0), cid(1), cid(2)]);
    assert!(second.summary.is_complete());
}

#[test]
fn a_skipped_batch_with_blocking_findings_gets_both_warnings_in_order() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(skipping());
    let mut p = provider(vec![ok(0), ok_with(blocking_payload()), ok(2)]);
    let report = h.run(&mut p, &fs, &VecSink::default());
    assert_eq!(report.exit, ExitCode::Success);
    assert_eq!(
        shapes(&report.diagnostics),
        vec![("RT-AI-0305", "warning"), (SKIP_CODE, "warning")]
    );
    assert_eq!(
        sorted_ids(&report.diagnostics),
        vec!["RT-AI-0305", SKIP_CODE]
    );
    assert!(fs.read(&h.batch_file(1)).is_some());
}

#[test]
fn the_skip_warning_orders_after_the_incomplete_run_warning() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(skipping());
    let mut p = provider(vec![ok(0), fail(ProviderFailure::AuthFailed, 100)]);
    let report = h.run(&mut p, &fs, &VecSink::default());
    let ids = sorted_ids(&report.diagnostics);
    let at = |code: &str| ids.iter().position(|id| *id == code).unwrap();
    assert!(at("RT-AI-0306") < at(SKIP_CODE), "{ids:?}");
}

fn cli_fixture(name: &str, payload: &str) -> Fixture {
    let fixture = Fixture::new(name);
    std::fs::create_dir_all(fixture.path("work")).unwrap();
    fixture.write_script(payload);
    fixture
}

#[test]
fn the_command_writes_the_skip_warning_once_on_standard_error() {
    let fixture = cli_fixture("skip-cli", &payload(0));
    let fs = MemFs::new();
    fs.with_faults(skipping());
    let ran = run_with(&fixture.consented(), &MapEnv::new(&[]), &fs);
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert_eq!(ran.shapes(), [("warning".to_owned(), SKIP_CODE.to_owned())]);
    assert_eq!(ran.stderr.matches(SKIP_CODE).count(), 1, "{}", ran.stderr);
    assert!(!ran.stdout.contains(SKIP_CODE), "{}", ran.stdout);
    assert_eq!(
        ran.stdout,
        "run summary: complete, directory durability not confirmed\n\
         processed:\n  rules.md 0 11\n\
         stored:\n  (none)\n\
         stored, directory durability not confirmed (skipped by platform):\n  \
         rules.md 0 11\n\
         unprocessed:\n  (none)\n"
    );
}

#[test]
fn the_command_orders_the_blocking_warning_before_the_skip_warning() {
    let fixture = cli_fixture("skip-cli-blocking", &blocking_payload_for(0));
    let fs = MemFs::new();
    fs.with_faults(skipping());
    let ran = run_with(&fixture.consented(), &MapEnv::new(&[]), &fs);
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-AI-0305", SKIP_CODE]);
}
