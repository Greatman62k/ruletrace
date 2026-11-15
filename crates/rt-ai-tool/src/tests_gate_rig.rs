//! Fixtures of the gate-level failure tests: a scripted provider that
//! reports how often it was called and when, a runner that takes any
//! [`Providers`], a recorded script with several entries and readers of
//! the text a run writes.

use crate::tests::MapEnv;
use crate::tests_cli_rig::{Fixture, Ran, CREATED_AT};
use crate::tests_provenance::{doc, model, RULES, RULES_HASH};
use crate::{run_with, AiConfig, Credential, Out, Providers, Redactor, StoreFs};
use rt_ai_adapter::{AiAdapter, AttemptReport, MonotonicClock, ProviderRequest};
use rt_ai_mock::{MockProvider, MockScript, Step};
use rt_foundation::Diagnostic;
use rt_hash::sha256;
use rt_json::{write_compact, Value};
use rt_srcmodel::{print_candidates, CandidateClause};
use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::rc::Rc;
use std::time::Duration;

/// What the provider saw, shared with the test that built it.
#[derive(Clone, Default)]
pub(crate) struct Probe {
    attempts: Rc<Cell<usize>>,
    clock_ms: Rc<Cell<u128>>,
}

impl Probe {
    /// Calls that reached the provider.
    pub(crate) fn attempts(&self) -> usize {
        self.attempts.get()
    }

    /// Simulated milliseconds after the last call.
    pub(crate) fn clock_ms(&self) -> u128 {
        self.clock_ms.get()
    }
}

struct Counting {
    inner: MockProvider,
    probe: Probe,
}

impl AiAdapter for Counting {
    fn attempt(&mut self, request: &ProviderRequest, timeout: Duration) -> AttemptReport {
        let report = self.inner.attempt(request, timeout);
        self.probe.attempts.set(self.probe.attempts.get() + 1);
        let now = self.inner.simulated_clock().now();
        self.probe.clock_ms.set(now.as_millis());
        report
    }

    fn clock(&self) -> &dyn MonotonicClock {
        self.inner.clock()
    }
}

/// Providers whose adapter plays `steps` in order, whatever the configured
/// provider kind is.
pub(crate) struct Steps {
    steps: RefCell<Option<Vec<Step>>>,
    probe: Probe,
}

impl Steps {
    pub(crate) fn new(steps: Vec<Step>) -> (Self, Probe) {
        let probe = Probe::default();
        let providers = Self {
            steps: RefCell::new(Some(steps)),
            probe: probe.clone(),
        };
        (providers, probe)
    }
}

impl Providers for Steps {
    fn is_linked(&self, _: crate::ProviderKind) -> bool {
        true
    }

    fn adapter<'a>(
        &self,
        _: &AiConfig,
        _: Option<&Credential>,
    ) -> Result<(Box<dyn AiAdapter + 'a>, String), Diagnostic> {
        let steps = self.steps.borrow_mut().take().unwrap_or_default();
        let inner = MockProvider::new(MockScript::from_steps(steps));
        let probe = self.probe.clone();
        Ok((Box::new(Counting { inner, probe }), "mock-1".to_owned()))
    }
}

pub(crate) fn run_providers(
    args: &[OsString],
    env: &MapEnv,
    providers: &dyn Providers,
    fs: &dyn StoreFs,
) -> Ran {
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let code = {
        let mut out = Out::new(&mut stdout, &mut stderr, Redactor::no_credential());
        run_with(args, env, providers, fs, &mut out)
    };
    Ran {
        code,
        stdout: String::from_utf8(stdout).unwrap(),
        stderr: String::from_utf8(stderr).unwrap(),
    }
}

/// `(severity, code id, message)` of each Diagnostic on standard error.
pub(crate) fn findings(ran: &Ran) -> Vec<(String, String, String)> {
    let known = ["error", "warning", "ambiguity", "unsupported"];
    ran.stderr
        .lines()
        .filter_map(|line| {
            let (severity, rest) = line.split_once(' ')?;
            let (id, message) = rest.split_once(": ")?;
            (known.contains(&severity) && id.starts_with("RT-"))
                .then(|| (severity.to_owned(), id.to_owned(), message.to_owned()))
        })
        .collect()
}

/// The `(severity, code id)` pairs of [`findings`].
pub(crate) fn shapes_of(ran: &Ran) -> Vec<(String, String)> {
    findings(ran).into_iter().map(|(s, c, _)| (s, c)).collect()
}

/// The provider announcements on standard error, one per attempt.
pub(crate) fn announcements(ran: &Ran) -> Vec<String> {
    ran.stderr
        .lines()
        .filter(|l| l.starts_with("request "))
        .map(str::to_owned)
        .collect()
}

/// The byte count that an announcement states.
pub(crate) fn announced_size(line: &str) -> u64 {
    line.rsplit_once("bytes ").unwrap().1.parse().unwrap()
}

/// The Transmission_Log line of attempt `attempt` of one request.
pub(crate) fn log_line(id: &str, bytes: u64, attempt: u32) -> String {
    format!(
        r#"{{"provider":"mock-replay","model":"model-1","endpoint_host":"","documents":[{{"document":"rules.md","sha256":"{RULES_HASH}"}}],"clauses":1,"request_bytes":{bytes},"request_id":"{id}","attempt":{attempt},"time":"{CREATED_AT}"}}"#
    )
}

pub(crate) fn log_lines(fixture: &Fixture) -> Vec<String> {
    fixture
        .text("work/tx.jsonl")
        .lines()
        .map(str::to_owned)
        .collect()
}

pub(crate) fn batch_path(id: &str) -> String {
    format!("store/batches/{id}.json")
}

pub(crate) fn work_dir(fixture: &Fixture) {
    std::fs::create_dir_all(fixture.path("work")).unwrap();
}

/// The files that the inputs of a fixture name, as `Fixture::files` lists
/// them before any run.
pub(crate) fn inputs_and_log(fixture: &Fixture, extra: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = [
        "ai.toml",
        "candidates.json",
        "rulebook.json",
        "rules.model.json",
        "work/tx.jsonl",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    if fixture.path("script.json").exists() {
        names.push("script.json".to_owned());
    }
    names.extend(extra.iter().map(|s| (*s).to_owned()));
    names.sort();
    names
}

pub(crate) const NONE_PROCESSED: &str = "run summary: incomplete\n\
    processed:\n  (none)\n\
    stored:\n  (none)\n\
    unprocessed:\n  rules.md 0 11\n";

pub(crate) const COMPLETE_ONE: &str = "run summary: complete\n\
    processed:\n  rules.md 0 11\n\
    stored:\n  rules.md 0 11\n\
    unprocessed:\n  (none)\n";

/// Rewrites the candidates of a fixture to the three clauses of
/// `rules.md`, one request each.
pub(crate) fn three_requests(fixture: &Fixture) {
    let source = model("rules.md", RULES);
    let clauses: Vec<CandidateClause> = [(0, 11), (12, 27), (28, 32)]
        .iter()
        .map(|(s, e)| CandidateClause::new(&source, *s, *e, None).unwrap())
        .collect();
    std::fs::write(
        fixture.path("candidates.json"),
        print_candidates(&doc("rules.md"), &clauses),
    )
    .unwrap();
    fixture.write_config(&fixture.mock_config("max_clauses_per_request = 1\n"));
}

/// The Request_Identifiers of a fixture, in plan order.
pub(crate) fn request_ids(fixture: &Fixture) -> Vec<String> {
    let ran = crate::tests_cli_rig::run_std(&fixture.args(&["--dry-run"]));
    ran.stdout.lines().map(str::to_owned).collect()
}

/// Writes a recorded script that answers each `(request id, payload)`.
pub(crate) fn write_script_entries(fixture: &Fixture, entries: &[(String, String)]) {
    let list: Vec<_> = entries
        .iter()
        .map(|(id, payload)| {
            Value::object(vec![
                ("request_id".to_owned(), Value::string(id)),
                ("origin".to_owned(), Value::string("authored")),
                ("completion".to_owned(), Value::string("complete")),
                (
                    "response_sha256".to_owned(),
                    Value::string(sha256(payload.as_bytes()).as_str()),
                ),
                ("payload".to_owned(), Value::string(payload)),
            ])
            .unwrap()
        })
        .collect();
    let script = Value::object(vec![
        (
            "format".to_owned(),
            Value::string("ruletrace-recorded-script"),
        ),
        ("version".to_owned(), Value::integer(1)),
        (
            "provider".to_owned(),
            Value::object(vec![
                ("kind".to_owned(), Value::string("mock-replay")),
                ("model".to_owned(), Value::string("model-1")),
                ("adapter_version".to_owned(), Value::string("mock-1")),
            ])
            .unwrap(),
        ),
        ("captured".to_owned(), Value::string("2026-11-15")),
        ("entries".to_owned(), Value::array(list)),
    ])
    .unwrap();
    std::fs::write(fixture.path("script.json"), write_compact(&script)).unwrap();
}
