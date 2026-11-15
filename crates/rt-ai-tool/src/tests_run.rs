//! Call-loop tests (design 3.7.6, 3.7.7; Req 23.35 to 23.38, 29.10,
//! 29.11, 29.17, 29.19). Providers are mock scripts on a simulated clock,
//! and the store is the in-memory file system that fails on request.
//! Expected exit codes, codes, severities and clause lists are written by
//! hand from the requirements.

use crate::tests_memfs::{Faults, Link, MemFs};
use crate::tests_provenance::doc;
use crate::tests_rig::{payload, Harness, CLAUSES};
use crate::tests_run_support::{blocking_payload, shapes, VecSink};
use crate::{ClauseId, Redactor, RunReport};
use rt_ai_adapter::{Completion, ProviderFailure};
use rt_ai_mock::{MockProvider, MockScript, ScriptResult, Step};
use rt_foundation::{CliLocation, Code, ExitCode, InputId, Location, Severity};
use rt_proposals::parse_batch;
use std::io::ErrorKind;
use std::time::Duration;

pub(crate) fn cid(index: usize) -> ClauseId {
    let (start, end, _, _) = CLAUSES[index];
    ClauseId {
        document: doc("rules.md"),
        start,
        end,
    }
}

pub(crate) fn ok_with(text: String) -> Step {
    Step {
        elapsed: Duration::from_millis(100),
        result: ScriptResult::Response {
            completion: Completion::Complete,
            payload: text.into_bytes(),
        },
    }
}

pub(crate) fn ok_with_ms(text: String, ms: u64) -> Step {
    Step {
        elapsed: Duration::from_millis(ms),
        result: ScriptResult::Response {
            completion: Completion::Complete,
            payload: text.into_bytes(),
        },
    }
}

pub(crate) fn ok(index: usize) -> Step {
    ok_with(payload(index))
}

pub(crate) fn fail(failure: ProviderFailure, ms: u64) -> Step {
    Step {
        elapsed: Duration::from_millis(ms),
        result: ScriptResult::Failure(failure),
    }
}

pub(crate) fn provider(steps: Vec<Step>) -> MockProvider {
    MockProvider::new(MockScript::from_steps(steps))
}

fn stored_names(fs: &MemFs) -> Vec<String> {
    fs.names_under("store/batches/")
}

fn command_propose() -> Location {
    Location::Cli(CliLocation::Command("propose".to_owned()))
}

const INCOMPLETE_AFTER_FIRST: &str = "run summary: incomplete\n\
processed:\n  rules.md 0 11\n\
stored:\n  rules.md 0 11\n\
unprocessed:\n  rules.md 12 27\n  rules.md 28 32\n";

#[test]
fn a_fully_successful_run_is_complete_with_no_warning() {
    let h = Harness::new("");
    let fs = MemFs::new();
    let sink = VecSink::default();
    let mut p = provider(vec![ok(0), ok(1), ok(2)]);
    let report = h.run(&mut p, &fs, &sink);
    assert_eq!(report.exit, ExitCode::Success);
    assert_eq!(report.exit.value(), 0);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert!(report.summary.is_complete());
    assert_eq!(report.summary.processed(), [cid(0), cid(1), cid(2)]);
    assert_eq!(report.summary.stored(), [cid(0), cid(1), cid(2)]);
    assert!(report.summary.unprocessed().is_empty());
    assert_eq!(
        report.summary.render(&Redactor::no_credential()),
        "run summary: complete\n\
         processed:\n  rules.md 0 11\n  rules.md 12 27\n  rules.md 28 32\n\
         stored:\n  rules.md 0 11\n  rules.md 12 27\n  rules.md 28 32\n\
         unprocessed:\n  (none)\n"
    );
    assert_eq!(stored_names(&fs).len(), 3);
    for i in 0..3 {
        let bytes = fs.read(&h.batch_file(i)).unwrap();
        let batch = parse_batch(&InputId::new("b").unwrap(), &bytes).unwrap();
        assert_eq!(batch.request_id.as_str(), h.id(i));
    }
    assert_eq!(p.steps_consumed(), 3);
}

#[test]
fn a_second_request_that_fails_keeps_the_first_batch_and_claims_nothing_more() {
    let h = Harness::new("");
    let fs = MemFs::new();
    let sink = VecSink::default();
    let mut p = provider(vec![ok(0), fail(ProviderFailure::AuthFailed, 10), ok(2)]);
    let report = h.run(&mut p, &fs, &sink);
    assert_eq!(report.exit.value(), 5);
    assert_eq!(
        shapes(&report.diagnostics),
        vec![("RT-AI-0104", "error"), ("RT-AI-0306", "warning")]
    );
    assert_eq!(stored_names(&fs), vec![h.batch_file(0)]);
    assert_eq!(report.summary.processed(), [cid(0)]);
    assert_eq!(report.summary.stored(), [cid(0)]);
    assert_eq!(report.summary.unprocessed(), [cid(1), cid(2)]);
    assert!(!report.summary.is_complete());
    assert_eq!(
        report.summary.render(&Redactor::no_credential()),
        INCOMPLETE_AFTER_FIRST
    );
    assert_eq!(p.steps_consumed(), 2);
    assert_eq!(sink.lines().len(), 2);
}

struct Variant {
    code: &'static str,
    exit: u8,
    attempts: usize,
    warnings: Vec<&'static str>,
}

// Steps of the failing request, the matching store faults and the
// expectation, for each failure that Req 23.38 names.
fn failing(kind: &str, index: usize) -> (Vec<Step>, Faults, Variant) {
    let mut faults = Faults::default();
    let (steps, variant) = match kind {
        "provenance" => (
            vec![ok_with(
                payload(index).replace("\"rules.md\"", "\"nope.md\""),
            )],
            Variant {
                code: "RT-PRV-0101",
                exit: 3,
                attempts: 1,
                warnings: vec![],
            },
        ),
        "exhausted" => (
            (0..3)
                .map(|_| fail(ProviderFailure::Unavailable, 0))
                .collect(),
            Variant {
                code: "RT-AI-0106",
                exit: 5,
                attempts: 3,
                warnings: vec!["RT-AI-0101"; 3],
            },
        ),
        "schema" => (
            vec![ok_with(
                r#"{"schema_version":"1","proposals":[{"bogus":1}]}"#.to_owned(),
            )],
            Variant {
                code: "RT-AI-0202",
                exit: 6,
                attempts: 1,
                warnings: vec![],
            },
        ),
        _ => {
            faults.links = (0..index)
                .map(|_| Link::Normal)
                .chain([Link::Fail(ErrorKind::Unsupported)])
                .collect();
            (
                vec![ok(index)],
                Variant {
                    code: "RT-AI-0303",
                    exit: 70,
                    attempts: 1,
                    warnings: vec![],
                },
            )
        }
    };
    (steps, faults, variant)
}

fn script_for(kind: &str, at: usize) -> (MockProvider, Faults, Variant) {
    let mut steps: Vec<Step> = (0..at).map(ok).collect();
    let (failing_steps, faults, variant) = failing(kind, at);
    steps.extend(failing_steps);
    steps.extend((at + 1..3).map(ok));
    (provider(steps), faults, variant)
}

#[test]
fn a_failing_second_request_keeps_exactly_the_first_batch_for_each_named_code() {
    for kind in ["provenance", "exhausted", "schema", "write"] {
        let h = Harness::new("");
        let fs = MemFs::new();
        let sink = VecSink::default();
        let (mut p, faults, v) = script_for(kind, 1);
        fs.with_faults(faults);
        let report = h.run(&mut p, &fs, &sink);
        let expected_codes: Vec<&str> = v
            .warnings
            .iter()
            .copied()
            .chain([v.code, "RT-AI-0306"])
            .collect();
        let codes: Vec<&str> = shapes(&report.diagnostics).iter().map(|s| s.0).collect();
        assert_eq!(codes, expected_codes, "{kind}");
        assert_eq!(report.exit.value(), v.exit, "{kind}");
        assert_eq!(stored_names(&fs), vec![h.batch_file(0)], "{kind}");
        assert_eq!(
            fs.names_under("store/.tmp/"),
            Vec::<String>::new(),
            "{kind}"
        );
        assert_eq!(p.steps_consumed(), 1 + v.attempts, "{kind}");
        assert_eq!(sink.lines().len(), 1 + v.attempts, "{kind}");
        assert!(!sink.text().contains(&h.id(2)), "{kind}");
        assert_eq!(
            report.summary.render(&Redactor::no_credential()),
            INCOMPLETE_AFTER_FIRST,
            "{kind}"
        );
        let last = report.diagnostics.last().unwrap();
        assert_eq!(last.severity(), Severity::Warning, "{kind}");
        assert_eq!(last.primary(), &command_propose(), "{kind}");
        assert_eq!(last.code(), Code::AiRunIncomplete, "{kind}");
        let first = fs.read(&h.batch_file(0)).unwrap();
        let first = parse_batch(&InputId::new("b").unwrap(), &first).unwrap();
        assert_eq!(first.response.payload, payload(0), "{kind}");
    }
}

#[test]
fn a_failing_first_request_stores_nothing_and_leaves_the_store_as_it_was() {
    for kind in ["provenance", "exhausted", "schema", "write"] {
        let h = Harness::new("");
        let fs = MemFs::new();
        fs.plant("store/batches/earlier.json", b"an earlier batch");
        let before = fs.files();
        let sink = VecSink::default();
        let (mut p, faults, v) = script_for(kind, 0);
        fs.with_faults(faults);
        let report = h.run(&mut p, &fs, &sink);
        let codes: Vec<&str> = shapes(&report.diagnostics).iter().map(|s| s.0).collect();
        let expected: Vec<&str> = v.warnings.iter().copied().chain([v.code]).collect();
        assert_eq!(codes, expected, "{kind}");
        assert_eq!(report.exit.value(), v.exit, "{kind}");
        assert_eq!(fs.files(), before, "{kind}");
        assert!(report.summary.processed().is_empty(), "{kind}");
        assert!(report.summary.stored().is_empty(), "{kind}");
        assert_eq!(
            report.summary.unprocessed(),
            [cid(0), cid(1), cid(2)],
            "{kind}"
        );
        assert!(!report.summary.is_complete(), "{kind}");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.code() != Code::AiRunIncomplete),
            "{kind}"
        );
        assert_eq!(p.steps_consumed(), v.attempts, "{kind}");
    }
}

// Req 23.36, 23.38 and 29.19 name one exception: a batch whose directory
// sync failed after the link is whole in the store and is listed as stored
// with durability not confirmed. The second request is the one that fails.
#[test]
fn a_failed_directory_sync_of_the_second_request_keeps_that_batch_listed_as_unconfirmed() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        sync_dir: Some(ErrorKind::Other),
        sync_dir_after: 1,
        ..Faults::default()
    });
    let sink = VecSink::default();
    let mut p = provider(vec![ok(0), ok(1), ok(2)]);
    let report = h.run(&mut p, &fs, &sink);
    assert_eq!(report.exit, ExitCode::Internal);
    assert_eq!(report.exit.value(), 70);
    assert_eq!(
        shapes(&report.diagnostics),
        vec![("RT-AI-0303", "error"), ("RT-AI-0306", "warning")]
    );
    assert_eq!(
        report.diagnostics[0].message(),
        "the batch was not stored: making the batch durable failed (Other); \
         the complete batch file stays in place"
    );
    let mut both = vec![h.batch_file(0), h.batch_file(1)];
    both.sort();
    assert_eq!(stored_names(&fs), both);
    for i in 0..2 {
        let bytes = fs.read(&h.batch_file(i)).unwrap();
        let batch = parse_batch(&InputId::new("b").unwrap(), &bytes).unwrap();
        assert_eq!(batch.request_id.as_str(), h.id(i));
        assert_eq!(batch.response.payload, payload(i));
    }
    assert_eq!(fs.names_under("store/.tmp/"), Vec::<String>::new());
    assert_eq!(p.steps_consumed(), 2);
    assert_eq!(sink.lines().len(), 2);
    assert!(!sink.text().contains(&h.id(2)));
    assert_eq!(report.summary.processed(), [cid(0)]);
    assert_eq!(report.summary.stored(), [cid(0)]);
    assert_eq!(report.summary.unconfirmed(), [cid(1)]);
    assert_eq!(report.summary.unprocessed(), [cid(2)]);
    assert!(!report.summary.is_complete());
    assert_eq!(
        report.summary.render(&Redactor::no_credential()),
        "run summary: incomplete\n\
         processed:\n  rules.md 0 11\n\
         stored:\n  rules.md 0 11\n\
         stored, durability not confirmed:\n  rules.md 12 27\n\
         unprocessed:\n  rules.md 28 32\n"
    );
}

#[test]
fn a_failed_directory_sync_of_the_first_request_adds_no_rt_ai_0306() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        sync_dir: Some(ErrorKind::Other),
        ..Faults::default()
    });
    let mut p = provider(vec![ok(0), ok(1), ok(2)]);
    let report = h.run(&mut p, &fs, &VecSink::default());
    assert_eq!(report.exit.value(), 70);
    assert_eq!(shapes(&report.diagnostics), vec![("RT-AI-0303", "error")]);
    assert_eq!(stored_names(&fs), vec![h.batch_file(0)]);
    assert_eq!(p.steps_consumed(), 1);
    assert_eq!(
        report.summary.render(&Redactor::no_credential()),
        "run summary: incomplete\n\
         processed:\n  (none)\n\
         stored:\n  (none)\n\
         stored, durability not confirmed:\n  rules.md 0 11\n\
         unprocessed:\n  rules.md 12 27\n  rules.md 28 32\n"
    );
}

#[test]
fn an_unconfirmed_last_batch_never_makes_the_run_complete() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        sync_dir: Some(ErrorKind::Other),
        sync_dir_after: 2,
        ..Faults::default()
    });
    let mut p = provider(vec![ok(0), ok(1), ok(2)]);
    let report = h.run(&mut p, &fs, &VecSink::default());
    assert_eq!(report.exit.value(), 70);
    assert!(report.summary.unprocessed().is_empty());
    assert_eq!(report.summary.unconfirmed(), [cid(2)]);
    assert!(!report.summary.is_complete());
    let text = report.summary.render(&Redactor::no_credential());
    assert!(text.starts_with("run summary: incomplete\n"), "{text}");
    assert!(text.ends_with("unprocessed:\n  (none)\n"), "{text}");
}

#[test]
fn an_unconfirmed_batch_with_blocking_findings_still_gets_rt_ai_0305() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        sync_dir: Some(ErrorKind::Other),
        sync_dir_after: 1,
        ..Faults::default()
    });
    let mut p = provider(vec![ok(0), blocking_resp(), ok(2)]);
    let report = h.run(&mut p, &fs, &VecSink::default());
    assert_eq!(report.exit.value(), 70);
    assert_eq!(
        shapes(&report.diagnostics),
        vec![
            ("RT-AI-0305", "warning"),
            ("RT-AI-0303", "error"),
            ("RT-AI-0306", "warning")
        ]
    );
    assert!(fs.read(&h.batch_file(1)).is_some());
}

#[test]
fn a_temporary_file_that_cannot_be_removed_warns_and_the_run_completes() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        remove: Some(ErrorKind::PermissionDenied),
        ..Faults::default()
    });
    let sink = VecSink::default();
    let mut p = provider(vec![ok(0), ok(1), ok(2)]);
    let report = h.run(&mut p, &fs, &sink);
    assert_eq!(report.exit, ExitCode::Success);
    assert_eq!(
        shapes(&report.diagnostics),
        vec![("RT-AI-0307", "warning"); 3]
    );
    assert!(report.summary.is_complete());
    assert_eq!(report.summary.stored(), [cid(0), cid(1), cid(2)]);
    assert_eq!(stored_names(&fs).len(), 3);
}

#[test]
fn a_third_request_that_fails_after_two_stored_lists_only_its_own_clauses() {
    let h = Harness::new("");
    let fs = MemFs::new();
    let sink = VecSink::default();
    let mut p = provider(vec![ok(0), ok(1), fail(ProviderFailure::TlsFailed, 0)]);
    let report = h.run(&mut p, &fs, &sink);
    assert_eq!(report.exit.value(), 5);
    assert_eq!(report.summary.stored(), [cid(0), cid(1)]);
    assert_eq!(report.summary.unprocessed(), [cid(2)]);
    assert_eq!(stored_names(&fs).len(), 2);
}

fn text_claims_completeness(text: &str) -> bool {
    text.matches("complete").count() != text.matches("incomplete").count()
}

#[test]
fn no_output_of_an_incomplete_run_claims_completeness() {
    let h = Harness::new("");
    let fs = MemFs::new();
    let sink = VecSink::default();
    let mut p = provider(vec![ok(0), fail(ProviderFailure::AuthFailed, 0)]);
    let report = h.run(&mut p, &fs, &sink);
    let text = report.summary.render(&Redactor::no_credential());
    assert!(!text_claims_completeness(&text), "{text}");
    assert!(text.starts_with("run summary: incomplete\n"));
    for d in &report.diagnostics {
        let line = crate::render_diagnostic(d).to_lowercase();
        assert!(!line.contains("all clauses"), "{line}");
        assert!(!text_claims_completeness(&line), "{line}");
    }
}

#[test]
fn a_repeated_run_sends_nothing_and_leaves_the_store_byte_identical() {
    let h = Harness::new("");
    let fs = MemFs::new();
    let first = h.run(
        &mut provider(vec![ok(0), ok(1), ok(2)]),
        &fs,
        &VecSink::default(),
    );
    assert_eq!(first.exit, ExitCode::Success);
    let before = fs.files();
    fs.clear_ops();
    let sink = VecSink::default();
    let mut p = provider(vec![]);
    let second = h.run(&mut p, &fs, &sink);
    assert_eq!(second.exit.value(), 0);
    assert_eq!(
        shapes(&second.diagnostics),
        vec![("RT-AI-0301", "warning"); 3]
    );
    assert_eq!(p.steps_consumed(), 0);
    assert_eq!(sink.text(), "");
    assert_eq!(fs.files(), before);
    assert!(fs.ops().iter().all(|o| o.starts_with("exists ")));
    assert!(second.summary.is_complete());
    assert_eq!(second.summary.processed(), [cid(0), cid(1), cid(2)]);
    assert!(second.summary.stored().is_empty());
    for (i, d) in second.diagnostics.iter().enumerate() {
        match d.primary() {
            Location::Input(at) => assert_eq!(at.input.as_str(), h.batch_file(i)),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn a_run_that_loses_the_race_for_one_batch_reports_it_and_stores_the_rest() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        links: [Link::Normal, Link::Winner(b"winner".to_vec())].into(),
        ..Faults::default()
    });
    let report = h.run(
        &mut provider(vec![ok(0), blocking_resp(), ok(2)]),
        &fs,
        &VecSink::default(),
    );
    assert_eq!(report.exit.value(), 0);
    assert_eq!(shapes(&report.diagnostics), vec![("RT-AI-0301", "warning")]);
    assert_eq!(fs.read(&h.batch_file(1)).unwrap(), b"winner");
    assert_eq!(report.summary.processed(), [cid(0), cid(1), cid(2)]);
    assert_eq!(report.summary.stored(), [cid(0), cid(2)]);
    assert!(report.summary.is_complete());
}

fn blocking_resp() -> Step {
    ok_with(blocking_payload())
}

#[test]
fn rt_ai_0305_follows_a_successful_write_and_only_that() {
    let h = Harness::new("");
    let fs = MemFs::new();
    let report = h.run(
        &mut provider(vec![ok(0), blocking_resp(), ok(2)]),
        &fs,
        &VecSink::default(),
    );
    assert_eq!(report.exit.value(), 0);
    assert_eq!(shapes(&report.diagnostics), vec![("RT-AI-0305", "warning")]);
    match report.diagnostics[0].primary() {
        Location::Input(at) => {
            assert_eq!(at.input.as_str(), format!("response-{}", h.id(1)), "{at:?}")
        }
        other => panic!("{other:?}"),
    }
    assert!(fs.read(&h.batch_file(1)).is_some());
}

#[test]
fn a_failed_write_emits_no_rt_ai_0305() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        links: [Link::Normal, Link::Fail(ErrorKind::Unsupported)].into(),
        ..Faults::default()
    });
    let report = h.run(
        &mut provider(vec![ok(0), blocking_resp(), ok(2)]),
        &fs,
        &VecSink::default(),
    );
    assert_eq!(report.exit.value(), 70);
    assert_eq!(
        shapes(&report.diagnostics),
        vec![("RT-AI-0303", "error"), ("RT-AI-0306", "warning")]
    );
    assert!(fs.read(&h.batch_file(1)).is_none());
}

#[test]
fn a_lost_race_emits_no_rt_ai_0305() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        links: [Link::Normal, Link::Winner(b"w".to_vec())].into(),
        ..Faults::default()
    });
    let report = h.run(
        &mut provider(vec![ok(0), blocking_resp(), ok(2)]),
        &fs,
        &VecSink::default(),
    );
    assert!(report
        .diagnostics
        .iter()
        .all(|d| d.code() != Code::AiBatchHasBlockingProposals));
}

#[test]
fn a_failed_log_append_sends_nothing_and_ends_the_run_with_exit_70() {
    let h = Harness::new("");
    let fs = MemFs::new();
    let sink = VecSink::default();
    sink.fail_next_appends();
    let mut p = provider(vec![ok(0), ok(1), ok(2)]);
    let report = h.run(&mut p, &fs, &sink);
    assert_eq!(report.exit.value(), 70);
    assert_eq!(shapes(&report.diagnostics), vec![("RT-AI-0010", "error")]);
    assert_eq!(p.steps_consumed(), 0);
    assert!(fs.files().is_empty());
    assert_eq!(report.summary.unprocessed(), [cid(0), cid(1), cid(2)]);
}

#[test]
fn every_attempt_is_logged_with_the_same_request_identifier() {
    let h = Harness::new("");
    let fs = MemFs::new();
    let sink = VecSink::default();
    let mut p = provider(vec![
        fail(ProviderFailure::Unavailable, 0),
        fail(ProviderFailure::Unavailable, 0),
        ok(0),
        ok(1),
        ok(2),
    ]);
    h.run(&mut p, &fs, &sink);
    let lines = sink.lines();
    assert_eq!(lines.len(), 5);
    let id = h.id(0);
    for (n, line) in lines.iter().take(3).enumerate() {
        assert!(line.contains(&format!("\"request_id\":\"{id}\"")), "{line}");
        assert!(line.contains(&format!("\"attempt\":{}", n + 1)), "{line}");
    }
    assert!(lines[3].contains("\"attempt\":1"));
}

#[test]
fn two_runs_on_one_real_disk_store_each_batch_once() {
    use crate::tests_scratch::{GateFs, ScratchDir};
    use crate::{run_requests, RunInput, TransmissionLog};
    struct Null;
    impl crate::LogSink for Null {
        fn append(&mut self, _: &[u8]) -> std::io::Result<()> {
            Ok(())
        }
    }
    use rt_ai_adapter::{
        AiAdapter, AttemptOutcome, AttemptReport, MonotonicClock, ProviderMeta, ProviderRequest,
        ProviderResponse,
    };
    use rt_ai_mock::SimulatedClock;
    // Answers by request identifier, because a run that finds a batch
    // already stored skips that request and the script order would shift.
    struct ByRequest {
        answers: Vec<(String, String)>,
        clock: SimulatedClock,
    }
    impl AiAdapter for ByRequest {
        fn attempt(&mut self, request: &ProviderRequest, _: Duration) -> AttemptReport {
            let id = request.request_id().map(|i| i.as_str().to_owned());
            let found = self.answers.iter().find(|(k, _)| Some(k) == id.as_ref());
            let outcome = match found {
                Some((_, text)) => AttemptOutcome::Response(ProviderResponse {
                    completion: Completion::Complete,
                    payload: text.clone().into_bytes(),
                    meta: ProviderMeta {
                        provider: "mock".to_owned(),
                        model: request.model.clone(),
                    },
                }),
                None => AttemptOutcome::Failure(ProviderFailure::AuthFailed),
            };
            AttemptReport {
                outcome,
                elapsed: Duration::ZERO,
                detail: None,
            }
        }
        fn clock(&self) -> &dyn MonotonicClock {
            &self.clock
        }
    }
    let h = Harness::new("");
    let scratch = ScratchDir::new("run-race");
    let dir = scratch.path().to_path_buf();
    let gate = GateFs::new();
    let reports: Vec<RunReport> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    let mut log = TransmissionLog::new(
                        Null,
                        h.redactor.clone(),
                        crate::tests_run_support::log_at(),
                    );
                    let mut p = ByRequest {
                        answers: (0..3).map(|i| (h.id(i), payload(i))).collect(),
                        clock: SimulatedClock::new(),
                    };
                    run_requests(
                        &RunInput {
                            config: &h.config,
                            plan: &h.plan,
                            models: &h.models,
                            declarations: &h.rulebook,
                            redactor: &h.redactor,
                            store_root: &dir,
                            meta: &h.meta,
                            log_time: "t",
                        },
                        &mut p,
                        &mut log,
                        &gate,
                    )
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    for r in &reports {
        assert_eq!(r.exit, ExitCode::Success);
        assert!(r.summary.is_complete());
        assert!(r
            .diagnostics
            .iter()
            .all(|d| d.code() == Code::AiBatchAlreadyStored));
    }
    let mut stored: Vec<ClauseId> = reports
        .iter()
        .flat_map(|r| r.summary.stored().iter().cloned())
        .collect();
    stored.sort();
    assert_eq!(stored, vec![cid(0), cid(1), cid(2)]);
    let files = std::fs::read_dir(dir.join("batches")).unwrap().count();
    let temps = std::fs::read_dir(dir.join(".tmp")).unwrap().count();
    assert_eq!((files, temps), (3, 0));
    assert!(gate.link_target_existed() >= 1);
}
