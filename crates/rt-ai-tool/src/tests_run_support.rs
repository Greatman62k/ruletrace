//! Run-loop fixtures: a log sink that keeps its bytes, the runner for the
//! shared [`Harness`] and an unsupported-rule response.

use crate::tests_memfs::MemFs;
use crate::tests_rig::{Harness, CLAUSES, ROOT};
use crate::{run_requests, LogSink, RunInput, RunReport, TransmissionLog};
use rt_ai_adapter::AiAdapter;
use rt_foundation::{Diagnostic, InputId, Location};
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

/// A response whose rule suggestion is well formed and unsupported, so the
/// batch is stored with a blocking finding.
pub(crate) fn blocking_payload() -> String {
    blocking_payload_for(1)
}

/// The same response citing clause `index`.
pub(crate) fn blocking_payload_for(index: usize) -> String {
    let (start, end, line, _) = CLAUSES[index];
    let span = format!(
        r#"{{"document":"rules.md","start":{start},"end":{end},"line":{line},"column":1}}"#
    );
    let rule = format!(
        r#"{{"id":"elig.new","kind":"unrecognized","name":"rank_reference","detail":"the top four qualify","target":"eligible2","type":"boolean","markers":[],"spans":[{span}]}}"#
    );
    format!(
        r#"{{"schema_version":"1","proposals":[{{"kind":"rule_suggestion","proposer":"model-1","rationale":"why","evidence":[{}],"rule":{rule}}}]}}"#,
        crate::tests_rig::evidence(index)
    )
}

/// A log sink that keeps every appended byte and can be made to fail.
#[derive(Clone, Default)]
pub(crate) struct VecSink {
    bytes: Rc<RefCell<Vec<u8>>>,
    fail: Rc<RefCell<bool>>,
}

impl VecSink {
    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes.borrow()).into_owned()
    }

    pub(crate) fn lines(&self) -> Vec<String> {
        self.text().lines().map(str::to_owned).collect()
    }

    pub(crate) fn fail_next_appends(&self) {
        *self.fail.borrow_mut() = true;
    }
}

impl LogSink for VecSink {
    fn append(&mut self, line: &[u8]) -> io::Result<()> {
        if *self.fail.borrow() {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        self.bytes.borrow_mut().extend_from_slice(line);
        Ok(())
    }
}

pub(crate) fn log_at() -> Location {
    Location::Input(rt_foundation::InputLocation {
        input: InputId::new("ai.toml").unwrap(),
        line: None,
        column: None,
        field: Some("transmission_log".to_owned()),
    })
}

impl Harness {
    pub(crate) fn run(&self, adapter: &mut dyn AiAdapter, fs: &MemFs, sink: &VecSink) -> RunReport {
        let mut log = TransmissionLog::new(sink.clone(), self.redactor.clone(), log_at());
        let root = std::path::Path::new(ROOT);
        run_requests(
            &RunInput {
                config: &self.config,
                plan: &self.plan,
                models: &self.models,
                declarations: &self.rulebook,
                redactor: &self.redactor,
                store_root: root,
                meta: &self.meta,
                log_time: "2026-11-15T00:00:01Z",
            },
            adapter,
            &mut log,
            fs,
        )
    }
}

/// `(code id, severity text)` of each Diagnostic, in order.
pub(crate) fn shapes(diagnostics: &[Diagnostic]) -> Vec<(&'static str, &'static str)> {
    diagnostics
        .iter()
        .map(|d| {
            let severity = match d.severity() {
                rt_foundation::Severity::Error => "error",
                rt_foundation::Severity::Warning => "warning",
                rt_foundation::Severity::Ambiguity => "ambiguity",
                rt_foundation::Severity::Unsupported => "unsupported",
            };
            (d.code().id(), severity)
        })
        .collect()
}
