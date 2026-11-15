//! Canary tests of the Run_Summary writer and of the diagnostics of the
//! call loop (Req 28.9, 23.19 to 23.23). Each credential is checked as a
//! whole and by every run of eight consecutive characters, so a cut or
//! truncated copy is caught as well. The oracle is a sliding window in
//! `tests_redact_fragments::leaked`.

use crate::tests_memfs::MemFs;
use crate::tests_provenance::doc;
use crate::tests_redact_fragments::{canaries, leaked, redactor};
use crate::tests_rig::{payload, Harness};
use crate::tests_run::cid;
use crate::tests_run_support::VecSink;
use crate::{render_diagnostic, ClauseId, Out, Redactor, RunReport, RunSummary};
use rt_ai_adapter::{
    AiAdapter, AttemptOutcome, AttemptReport, MonotonicClock, ProviderFailure, ProviderRequest,
};
use rt_ai_mock::SimulatedClock;
use rt_foundation::Diagnostic;
use std::collections::VecDeque;
use std::time::Duration;

fn piece(credential: &str, from: usize, len: usize) -> String {
    credential.chars().skip(from).take(len).collect()
}

fn hostile_clauses(credential: &str) -> Vec<ClauseId> {
    [
        format!("{credential}.md"),
        format!("{}.md", piece(credential, 2, 14)),
        format!("doc-{}-x", piece(credential, 5, 9)),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, name)| ClauseId {
        document: doc(&name),
        start: i as u32 * 10,
        end: i as u32 * 10 + 5,
    })
    .collect()
}

fn written(redactor: Redactor, text: &str) -> (Vec<u8>, Vec<u8>) {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let mut out = Out::new(&mut stdout, &mut stderr, redactor);
    out.stdout(text).unwrap();
    drop(out);
    (stdout, stderr)
}

#[test]
fn the_run_summary_holds_no_run_of_eight_credential_characters() {
    for c in canaries() {
        let requested = hostile_clauses(c);
        let summary = RunSummary::new(&requested, &requested[..2], &requested[..1]);
        let text = summary.render(&redactor(c));
        let (stdout, stderr) = written(redactor(c), &text);
        let stdout = String::from_utf8(stdout).unwrap();
        assert_eq!(leaked(&stdout, c), None, "{c}: {stdout}");
        assert!(stderr.is_empty());
        assert!(stdout.starts_with("run summary: incomplete\n"), "{stdout}");
        assert!(stdout.contains("unprocessed:\n  "), "{stdout}");
    }
}

#[test]
fn the_summary_test_detects_a_leak_when_nothing_is_redacted() {
    for c in canaries() {
        let requested = hostile_clauses(c);
        let summary = RunSummary::new(&requested, &[], &[]);
        let text = summary.render(&Redactor::no_credential());
        assert!(leaked(&text, c).is_some(), "{c}: {text}");
    }
}

#[test]
fn the_summary_lists_follow_ascending_start_then_document_then_end() {
    let a = ClauseId {
        document: doc("b.md"),
        start: 5,
        end: 9,
    };
    let b = ClauseId {
        document: doc("a.md"),
        start: 20,
        end: 30,
    };
    let c = ClauseId {
        document: doc("a.md"),
        start: 5,
        end: 9,
    };
    let summary = RunSummary::new(&[b.clone(), a.clone(), c.clone()], &[], &[]);
    assert_eq!(summary.unprocessed(), [c, a, b]);
}

#[test]
fn the_summary_keeps_its_invariants_for_inconsistent_input() {
    let all = [cid(0), cid(1), cid(2)];
    let summary = RunSummary::new(&all, &[cid(0)], &[cid(0), cid(2)]);
    assert_eq!(summary.processed(), [cid(0)]);
    assert_eq!(summary.stored(), [cid(0)]);
    assert_eq!(summary.unprocessed(), [cid(1), cid(2)]);
    assert!(!summary.is_complete());
}

struct Scripted {
    reports: VecDeque<AttemptReport>,
    clock: SimulatedClock,
}

impl AiAdapter for Scripted {
    fn attempt(&mut self, _: &ProviderRequest, _: Duration) -> AttemptReport {
        self.reports.pop_front().unwrap_or(AttemptReport {
            outcome: AttemptOutcome::Failure(ProviderFailure::AuthFailed),
            elapsed: Duration::ZERO,
            detail: None,
        })
    }

    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }
}

fn failure(f: ProviderFailure, detail: String) -> AttemptReport {
    AttemptReport {
        outcome: AttemptOutcome::Failure(f),
        elapsed: Duration::ZERO,
        detail: Some(detail),
    }
}

fn every_text(d: &Diagnostic) -> String {
    format!("{d:?}\n{}\n{}", d.message(), render_diagnostic(d))
}

struct Run {
    report: RunReport,
    log: String,
    store: String,
}

fn run_with_detail(c: &str, redact: bool) -> Run {
    let mut h = Harness::new("");
    h.plan = crate::tests_rig::plan_of(&h.config, &[0]);
    if redact {
        h.redactor = redactor(c);
    }
    let echo = format!("provider said {c} and {}", piece(c, 3, 12));
    let mut p = Scripted {
        reports: [
            failure(ProviderFailure::Unavailable, echo.clone()),
            failure(
                ProviderFailure::Other {
                    http_status: Some(400),
                },
                echo,
            ),
        ]
        .into(),
        clock: SimulatedClock::new(),
    };
    let fs = MemFs::new();
    let sink = VecSink::default();
    let report = h.run(&mut p, &fs, &sink);
    let store = fs
        .files()
        .values()
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .collect::<String>();
    Run {
        report,
        log: sink.text(),
        store,
    }
}

#[test]
fn provider_text_in_diagnostics_holds_no_run_of_eight_credential_characters() {
    for c in canaries() {
        let run = run_with_detail(c, true);
        let codes: Vec<&str> = run
            .report
            .diagnostics
            .iter()
            .map(|d| d.code().id())
            .collect();
        assert_eq!(codes, vec!["RT-AI-0101", "RT-AI-0105"], "{c}");
        for d in &run.report.diagnostics {
            assert_eq!(leaked(&every_text(d), c), None, "{c}: {d:?}");
        }
        assert_eq!(leaked(&run.log, c), None, "{c}");
        assert_eq!(run.store, "", "{c}");
        let text = run.report.summary.render(&redactor(c));
        assert_eq!(leaked(&text, c), None, "{c}");
    }
}

#[test]
fn the_diagnostic_test_detects_a_leak_when_nothing_is_redacted() {
    for c in canaries() {
        let run = run_with_detail(c, false);
        let all: String = run.report.diagnostics.iter().map(every_text).collect();
        assert!(leaked(&all, c).is_some(), "{c}: {all}");
    }
}

#[test]
fn a_response_that_echoes_the_credential_is_refused_and_leaks_nowhere() {
    for c in canaries() {
        let mut h = Harness::new("");
        h.plan = crate::tests_rig::plan_of(&h.config, &[0]);
        h.redactor = redactor(c);
        let echoed = payload(0).replace("clause 0", &format!("echo {c} and {}", piece(c, 1, 10)));
        let mut p = crate::tests_run::provider(vec![crate::tests_run::ok_with(echoed)]);
        let fs = MemFs::new();
        let sink = VecSink::default();
        let report = h.run(&mut p, &fs, &sink);
        assert_eq!(report.exit.value(), 6, "{c}");
        let codes: Vec<&str> = report.diagnostics.iter().map(|d| d.code().id()).collect();
        assert_eq!(codes, vec!["RT-AI-0206"], "{c}");
        for d in &report.diagnostics {
            assert_eq!(leaked(&every_text(d), c), None, "{c}");
        }
        assert!(fs.files().is_empty(), "{c}");
        assert_eq!(leaked(&sink.text(), c), None, "{c}");
    }
}
