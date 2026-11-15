//! `propose` with `--revise-rule`, `--revise-revision` and
//! `--feedback-file` (Req 23.16, 23.1, 23.2, 23.9, 27.4, 28.7, 28.8,
//! 28.14). Expected request bytes are written by hand from design 3.7.2
//! and hashed with `rt_hash::sha256`. One digest comes from the external
//! `sha256sum` tool. Expected codes come from the catalogue.

use crate::revise::Unknown;
use crate::tests::MapEnv;
use crate::tests_cli_rig::{run_std, Fixture, Ran};
use crate::tests_gate_rig::{batch_path, log_lines, work_dir, COMPLETE_ONE};
use crate::tests_plan::{HEAD, VOCAB_RANK};
use crate::tests_provenance::{model, RULES, RULES_HASH};
use crate::tests_redact_fragments::{canaries, leaked};
use crate::tests_rig::{config, evidence, models, payload};
use crate::{
    build_plan, AiConfig, Credential, Out, PlanInput, Providers, Redactor, ReviseContext, RunPlan,
    StdFs, Ud32Option, MAX_FEEDBACK_BYTES,
};
use proptest::prelude::*;
use rt_ai_adapter::{
    AiAdapter, AttemptOutcome, AttemptReport, Completion, MonotonicClock, ProviderMeta,
    ProviderRequest, ProviderResponse,
};
use rt_ai_mock::SimulatedClock;
use rt_foundation::{CliLocation, Diagnostic, ExitCode, InputId, Location, RevisionNumber, RuleId};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, Rulebook};
use rt_srcmodel::CandidateClause;
use std::cell::RefCell;
use std::ffi::OsString;
use std::rc::Rc;
use std::time::Duration;

const CREDENTIAL: &str = "credential-value-0123456789";
pub(crate) const FEEDBACK: &str = "Raise the age to 21.\nCite \"clause 1\".";
pub(crate) const NEW_CONTENT: &str = r#"{"id":"elig.basic","kind":"eligibility","target":"eligible","type":"boolean","expr":{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"21"}},"markers":[],"spans":[{"document":"rules.md","start":0,"end":11,"line":1,"column":1}]}"#;

// sha256sum of the bytes of `expected_bytes(1, FEEDBACK)`, escaped by an
// independent JSON writer.
const GOLDEN_ID: &str = "47b7fbeabd70ccc9d05aad7137870daae658564ac866f0d6e44f6b220f46c5f8";

fn rule_json(revision: u32, value: &str) -> String {
    format!(
        r#"{{"id":"elig.basic","revision":{revision},"origin":{{"kind":"manual","author":null}},"kind":"eligibility","target":"eligible","type":"boolean","expr":{{"node":"cmp","t":"boolean","op":"ge","left":{{"node":"field","t":"number","name":"age"}},"right":{{"node":"num","t":"number","value":"{value}"}}}},"markers":[],"spans":[{{"document":"rules.md","start":0,"end":11,"line":1,"column":1}}]}}"#
    )
}

pub(crate) fn rulebook_text() -> String {
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"n","version":"1","documents":[{{"id":"rules.md","sha256":"{RULES_HASH}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{},{}],"precedence":[]}}"#,
        rule_json(1, "16"),
        rule_json(2, "18")
    )
}

fn rulebook() -> Rulebook {
    parse_rulebook(
        rulebook_text().as_bytes(),
        &InputId::new("rb.json").unwrap(),
        8 << 20,
        64,
    )
    .map(|s| s.into_parts().0)
    .map_err(|r| r.into_diagnostics().as_slice().to_vec())
    .unwrap()
}

// The canonical draft of each stored revision, written by hand from the
// key order of design 4.1.2.
fn content_of(revision: u32) -> String {
    let value = if revision == 1 { "16" } else { "18" };
    format!(
        r#"{{"id":"elig.basic","kind":"eligibility","target":"eligible","type":"boolean","expr":{{"node":"cmp","t":"boolean","op":"ge","left":{{"node":"field","t":"number","name":"age"}},"right":{{"node":"num","t":"number","value":"{value}"}}}},"markers":[],"spans":[{{"document":"rules.md","start":0,"end":11,"line":1,"column":1}}]}}"#
    )
}

// Written independently of `rt-json`: quote, backslash and control
// characters.
fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

const CLAUSE: &str = r#"{"document":"rules.md","sha256":"RULES_HASH","start":0,"end":11,"line":1,"column":1,"text":"Alpha beta."}]"#;
const DECLS: &str = r#","declarations":{"ir_version":"1","inputs":[{"name":"age","type":"number"}],"enums":[],"sets":[],"collections":[],"highest_revision":{"elig.basic":2}},"#;

fn expected_bytes(revision: u32, feedback: &str) -> String {
    format!(
        r#"{HEAD}{}{DECLS}{VOCAB_RANK},"revise":{{"rule":"elig.basic","revision":{revision},"content":{},"feedback":{}}}}}"#,
        CLAUSE.replace("RULES_HASH", RULES_HASH),
        quote(&content_of(revision)),
        quote(feedback)
    )
}

pub(crate) fn expected_id(revision: u32, feedback: &str) -> String {
    sha256(expected_bytes(revision, feedback).as_bytes())
        .as_str()
        .to_owned()
}

pub(crate) fn revised_payload() -> String {
    format!(
        r#"{{"schema_version":"1","proposals":[{{"kind":"rule_suggestion","proposer":"model-1","rationale":"raise the age","evidence":[{}],"rule":{NEW_CONTENT}}}]}}"#,
        evidence(0)
    )
}

pub(crate) fn revise_fixture(name: &str) -> Fixture {
    let fixture = Fixture::new(name);
    std::fs::write(fixture.path("rulebook.json"), rulebook_text()).unwrap();
    std::fs::write(fixture.path("feedback.txt"), FEEDBACK).unwrap();
    work_dir(&fixture);
    fixture
}

pub(crate) fn revise_opts(fixture: &Fixture, rule: &str, revision: &str) -> Vec<String> {
    vec![
        "--revise-rule".to_owned(),
        rule.to_owned(),
        "--revise-revision".to_owned(),
        revision.to_owned(),
        "--feedback-file".to_owned(),
        fixture.path("feedback.txt").display().to_string(),
    ]
}

fn args_with(fixture: &Fixture, extra: &[String]) -> Vec<OsString> {
    let refs: Vec<&str> = extra.iter().map(String::as_str).collect();
    fixture.args(&refs)
}

pub(crate) fn consented_with(fixture: &Fixture, extra: &[String]) -> Vec<OsString> {
    let mut all = extra.to_vec();
    all.push("--consent".to_owned());
    all.push("rules.md".to_owned());
    args_with(fixture, &all)
}

fn input_files(fixture: &Fixture) -> Vec<String> {
    let mut names: Vec<String> = [
        "ai.toml",
        "candidates.json",
        "feedback.txt",
        "rulebook.json",
        "rules.model.json",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    if fixture.path("script.json").exists() {
        names.push("script.json".to_owned());
    }
    names.sort();
    names
}

fn assert_nothing_sent(fixture: &Fixture) {
    assert_eq!(fixture.files(), input_files(fixture));
}

#[test]
fn observable_a_revision_run_sends_the_hand_written_request_and_stores_a_proposal() {
    let fixture = revise_fixture("revise-observable");
    let id = expected_id(1, FEEDBACK);
    fixture.write_script_for(&id, &revised_payload());
    let rulebook_before = std::fs::read(fixture.path("rulebook.json")).unwrap();
    let feedback_before = std::fs::read(fixture.path("feedback.txt")).unwrap();

    let ran = run_std(&consented_with(
        &fixture,
        &revise_opts(&fixture, "elig.basic", "1"),
    ));

    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert_eq!(ran.stdout, COMPLETE_ONE);
    let batch = std::fs::read(fixture.path(&batch_path(&id))).unwrap();
    let parsed = rt_proposals::parse_batch(&InputId::new("batch").unwrap(), &batch).unwrap();
    assert_eq!(parsed.request_id.as_str(), id);
    assert_eq!(parsed.proposals.len(), 1);
    let (draft, _) = parsed.proposals[0].rule.as_ref().unwrap();
    assert_eq!(draft.id.as_str(), "elig.basic");
    assert_eq!(draft.json, NEW_CONTENT);
    let expected_log = format!(
        r#"{{"provider":"mock-replay","model":"model-1","endpoint_host":"","documents":[{{"document":"rules.md","sha256":"{RULES_HASH}"}}],"clauses":1,"request_bytes":{},"request_id":"{id}","attempt":1,"time":"2026-11-15T00:00:00Z"}}"#,
        expected_bytes(1, FEEDBACK).len()
    );
    assert_eq!(log_lines(&fixture), [expected_log]);
    assert_eq!(
        std::fs::read(fixture.path("rulebook.json")).unwrap(),
        rulebook_before
    );
    assert_eq!(
        std::fs::read(fixture.path("feedback.txt")).unwrap(),
        feedback_before
    );
    let mut written = fixture.files();
    written.retain(|f| f.starts_with("store/") || f.starts_with("work/"));
    assert_eq!(written, [batch_path(&id), "work/tx.jsonl".to_owned()]);
    assert!(fixture.files().iter().all(|f| !f.contains("ledger")));
}

#[test]
fn the_digest_of_the_hand_written_request_matches_sha256sum() {
    assert_eq!(expected_id(1, FEEDBACK), GOLDEN_ID);
}

struct Recording {
    seen: Rc<RefCell<Vec<ProviderRequest>>>,
    clock: SimulatedClock,
    payload: String,
}

impl AiAdapter for Recording {
    fn attempt(&mut self, request: &ProviderRequest, _: Duration) -> AttemptReport {
        self.seen.borrow_mut().push(request.clone());
        AttemptReport {
            outcome: AttemptOutcome::Response(ProviderResponse {
                completion: Completion::Complete,
                payload: self.payload.clone().into_bytes(),
                meta: ProviderMeta {
                    provider: "openai-responses".to_owned(),
                    model: "model-1".to_owned(),
                },
            }),
            elapsed: Duration::ZERO,
            detail: None,
        }
    }

    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }
}

struct Hosted {
    seen: Rc<RefCell<Vec<ProviderRequest>>>,
    payload: String,
}

impl Providers for Hosted {
    fn is_linked(&self, _: crate::ProviderKind) -> bool {
        true
    }

    fn adapter<'a>(
        &self,
        _: &AiConfig,
        _: Option<&Credential>,
    ) -> Result<(Box<dyn AiAdapter + 'a>, String), Diagnostic> {
        let adapter = Recording {
            seen: Rc::clone(&self.seen),
            clock: SimulatedClock::new(),
            payload: self.payload.clone(),
        };
        Ok((Box::new(adapter), "recording-1".to_owned()))
    }
}

pub(crate) fn run_hosted(
    fixture: &Fixture,
    credential: &str,
    args: &[OsString],
    payload: String,
) -> (Ran, Vec<ProviderRequest>) {
    fixture.write_config(&fixture.real_config(""));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let providers = Hosted {
        seen: Rc::clone(&seen),
        payload,
    };
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", credential)]);
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = {
        let mut out = Out::new(&mut stdout, &mut stderr, Redactor::no_credential());
        crate::run_with(args, &env, &providers, &StdFs, &mut out)
    };
    let ran = Ran {
        code,
        stdout: String::from_utf8(stdout).unwrap(),
        stderr: String::from_utf8(stderr).unwrap(),
    };
    let requests = seen.borrow().clone();
    (ran, requests)
}

#[test]
fn the_request_context_holds_the_named_revision_and_the_feedback() {
    let fixture = revise_fixture("revise-members");
    let args = consented_with(&fixture, &revise_opts(&fixture, "elig.basic", "2"));
    let (ran, requests) = run_hosted(&fixture, CREDENTIAL, &args, revised_payload());
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert_eq!(requests.len(), 1);
    let revise = requests[0]
        .context
        .iter()
        .find(|(key, _)| key == "revise")
        .map(|(_, v)| v)
        .unwrap();
    let members = revise.as_object().unwrap();
    let keys: Vec<&str> = members.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(keys, ["rule", "revision", "content", "feedback"]);
    assert_eq!(members[0].value.as_str(), Some("elig.basic"));
    assert_eq!(members[1].value.as_u64(), Some(2));
    assert_eq!(members[2].value.as_str(), Some(content_of(2).as_str()));
    assert_eq!(members[3].value.as_str(), Some(FEEDBACK));
    let sent = String::from_utf8(requests[0].canonical_bytes().unwrap()).unwrap();
    assert_eq!(sent, expected_bytes(2, FEEDBACK));
}

#[test]
fn a_run_without_the_options_still_sends_a_null_revise_member() {
    let fixture = revise_fixture("revise-absent");
    let args = fixture.consented();
    let (ran, requests) = run_hosted(&fixture, CREDENTIAL, &args, payload(0));
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    let sent = String::from_utf8(requests[0].canonical_bytes().unwrap()).unwrap();
    assert!(sent.ends_with(r#","revise":null}"#), "{sent}");
}

#[test]
fn a_dry_run_prints_the_identifier_of_the_revision_request_and_not_the_feedback() {
    let fixture = revise_fixture("revise-dry");
    let mut extra = revise_opts(&fixture, "elig.basic", "1");
    extra.push("--dry-run".to_owned());
    let before = fixture.files();
    let ran = run_std(&args_with(&fixture, &extra));
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert_eq!(
        ran.stdout.lines().next(),
        Some(expected_id(1, FEEDBACK).as_str())
    );
    assert!(!ran.stdout.contains("Raise the age"), "{}", ran.stdout);
    assert!(!ran.stderr.contains("Raise the age"), "{}", ran.stderr);
    assert_eq!(fixture.files(), before);
}

#[test]
fn one_or_two_options_are_refused_naming_each_missing_option_and_send_nothing() {
    let names = ["--revise-rule", "--revise-revision", "--feedback-file"];
    for mask in 1..7_u8 {
        let fixture = revise_fixture(&format!("revise-partial-{mask}"));
        let all = revise_opts(&fixture, "elig.basic", "1");
        let mut given = Vec::new();
        let mut missing = Vec::new();
        for (i, name) in names.iter().enumerate() {
            if mask & (1 << i) != 0 {
                given.push(all[2 * i].clone());
                given.push(all[2 * i + 1].clone());
            } else {
                missing.push(format!("required option '{name}' is missing"));
            }
        }
        let ran = run_std(&consented_with(&fixture, &given));
        assert_eq!(ran.code, ExitCode::InvalidInput, "{mask}: {}", ran.stderr);
        let messages: Vec<String> = ran
            .stderr
            .lines()
            .filter(|l| l.starts_with("error RT-CLI-0002: "))
            .map(|l| {
                l.trim_start_matches("error RT-CLI-0002: ")
                    .split(" at ")
                    .next()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        let (mut messages, mut missing) = (messages, missing);
        messages.sort();
        missing.sort();
        assert_eq!(messages, missing, "{mask}");
        assert_eq!(ran.codes(), vec!["RT-CLI-0002"; missing.len()], "{mask}");
        assert_nothing_sent(&fixture);
    }
}

#[test]
fn a_rule_the_rulebook_does_not_contain_is_refused_at_its_argument() {
    let fixture = revise_fixture("revise-unknown-rule");
    let ran = run_std(&consented_with(
        &fixture,
        &revise_opts(&fixture, "elig.other", "1"),
    ));
    assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
    let found = crate::tests_gate_rig::findings(&ran);
    assert_eq!(
        found,
        [(
            "error".to_owned(),
            "RT-AI-0304".to_owned(),
            "--revise-rule names rule 'elig.other', which the Rulebook does not contain".to_owned()
        )]
    );
    assert!(
        ran.stderr.contains("at argument 14 (--revise-rule)"),
        "{}",
        ran.stderr
    );
    assert_nothing_sent(&fixture);
}

#[test]
fn a_revision_the_rule_does_not_have_is_refused_at_its_argument() {
    for number in ["3", "4294967295"] {
        let fixture = revise_fixture(&format!("revise-unknown-revision-{number}"));
        let ran = run_std(&consented_with(
            &fixture,
            &revise_opts(&fixture, "elig.basic", number),
        ));
        assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
        let found = crate::tests_gate_rig::findings(&ran);
        let message = format!(
            "--revise-revision names revision {number} of rule 'elig.basic', which the Rulebook does not contain"
        );
        assert_eq!(
            found,
            [("error".to_owned(), "RT-AI-0304".to_owned(), message)]
        );
        assert!(
            ran.stderr.contains("at argument 16 (--revise-revision)"),
            "{}",
            ran.stderr
        );
        assert_nothing_sent(&fixture);
    }
}

#[test]
fn a_missing_feedback_file_is_refused_as_an_unreadable_input() {
    let fixture = revise_fixture("revise-feedback-missing");
    std::fs::remove_file(fixture.path("feedback.txt")).unwrap();
    let extra = revise_opts(&fixture, "elig.basic", "1");
    let ran = run_std(&consented_with(&fixture, &extra));
    assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-CLI-0003"]);
    let path = fixture.path("feedback.txt").display().to_string();
    assert!(
        ran.stderr
            .contains(&format!("input file '{path}' does not exist")),
        "{}",
        ran.stderr
    );
    assert!(fixture.files().iter().all(|f| !f.starts_with("store/")));
}

#[test]
fn an_unreadable_feedback_path_is_refused_as_an_unreadable_input() {
    let fixture = revise_fixture("revise-feedback-directory");
    std::fs::remove_file(fixture.path("feedback.txt")).unwrap();
    std::fs::create_dir(fixture.path("feedback.txt")).unwrap();
    let extra = revise_opts(&fixture, "elig.basic", "1");
    let ran = run_std(&consented_with(&fixture, &extra));
    assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-CLI-0003"]);
    assert!(ran.stderr.contains("cannot be read"), "{}", ran.stderr);
    assert!(fixture.files().iter().all(|f| !f.starts_with("store/")));
}

#[test]
fn an_empty_or_non_utf8_feedback_is_refused_with_its_own_message() {
    let cases: [(Vec<u8>, &str); 2] = [
        (
            Vec::new(),
            "the feedback file given by --feedback-file is empty",
        ),
        (
            vec![0x66, 0xff, 0x66],
            "the feedback file given by --feedback-file is not valid UTF-8",
        ),
    ];
    for (i, (bytes, message)) in cases.iter().enumerate() {
        let fixture = revise_fixture(&format!("revise-feedback-bad-{i}"));
        std::fs::write(fixture.path("feedback.txt"), bytes).unwrap();
        let extra = revise_opts(&fixture, "elig.basic", "1");
        let ran = run_std(&consented_with(&fixture, &extra));
        assert_eq!(ran.code, ExitCode::InvalidInput, "{i}: {}", ran.stderr);
        let found = crate::tests_gate_rig::findings(&ran);
        assert_eq!(found.len(), 1, "{i}: {found:?}");
        assert_eq!(found[0].1, "RT-AI-0304", "{i}");
        assert!(found[0].2.starts_with(message), "{i}: {}", found[0].2);
        assert_nothing_sent(&fixture);
    }
}

// Req 19.3 and 23.16: an oversize feedback file is RT-LIM-0001 with the
// size that the file system reports, not the size of a bounded read.
#[test]
fn an_oversize_feedback_is_refused_as_rt_lim_0001_with_its_true_size() {
    for size in [MAX_FEEDBACK_BYTES + 1, 20_000, 3_000_000] {
        let fixture = revise_fixture(&format!("revise-feedback-big-{size}"));
        std::fs::write(fixture.path("feedback.txt"), vec![b'a'; size]).unwrap();
        let extra = revise_opts(&fixture, "elig.basic", "1");
        let ran = run_std(&consented_with(&fixture, &extra));
        assert_eq!(ran.code, ExitCode::InvalidInput, "{size}: {}", ran.stderr);
        let found = crate::tests_gate_rig::findings(&ran);
        assert_eq!(found.len(), 1, "{size}: {found:?}");
        assert_eq!(found[0].0, "error", "{size}");
        assert_eq!(found[0].1, "RT-LIM-0001", "{size}");
        let path = fixture.path("feedback.txt").display().to_string();
        let message = format!(
            "input file '{path}' is {size} bytes, larger than the effective limit of 8192 bytes"
        );
        assert!(found[0].2.starts_with(&message), "{size}: {}", found[0].2);
        assert_nothing_sent(&fixture);
    }
}

#[test]
fn a_feedback_of_exactly_the_limit_is_accepted() {
    let fixture = revise_fixture("revise-feedback-limit");
    let text = "x".repeat(MAX_FEEDBACK_BYTES);
    std::fs::write(fixture.path("feedback.txt"), &text).unwrap();
    fixture.write_script_for(&expected_id(1, &text), &revised_payload());
    let extra = revise_opts(&fixture, "elig.basic", "1");
    let ran = run_std(&consented_with(&fixture, &extra));
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
}

#[test]
fn the_feedback_needs_the_consent_of_the_clauses_and_no_other() {
    let extra_names =
        |extra: &[&str]| -> Vec<String> { extra.iter().map(|s| (*s).to_owned()).collect() };
    let cases: [(&str, Vec<String>); 3] = [
        ("none", Vec::new()),
        ("other", extra_names(&["--consent", "other.md"])),
        (
            "feedback-name",
            extra_names(&["--consent", "rules.md", "--consent", "feedback.txt"]),
        ),
    ];
    for (name, consent) in cases {
        let fixture = revise_fixture(&format!("revise-consent-{name}"));
        fixture.write_script_for(&expected_id(1, FEEDBACK), &revised_payload());
        let mut all = revise_opts(&fixture, "elig.basic", "1");
        all.extend(consent);
        let ran = run_std(&args_with(&fixture, &all));
        assert_eq!(ran.code, ExitCode::InvalidInput, "{name}: {}", ran.stderr);
        assert_eq!(ran.codes(), ["RT-AI-0008"], "{name}: {}", ran.stderr);
        assert_nothing_sent(&fixture);
    }
}

#[test]
fn a_revision_request_needs_allow_document_text() {
    let fixture = revise_fixture("revise-allow");
    fixture.write_config(
        &fixture
            .mock_config("")
            .replace("allow_document_text = true", "allow_document_text = false"),
    );
    fixture.write_script_for(&expected_id(1, FEEDBACK), &revised_payload());
    let extra = revise_opts(&fixture, "elig.basic", "1");
    let ran = run_std(&consented_with(&fixture, &extra));
    assert_eq!(ran.codes(), ["RT-AI-0008"], "{}", ran.stderr);
    assert_nothing_sent(&fixture);
}

#[test]
fn a_canary_in_the_feedback_reaches_no_output_log_or_store() {
    let canary = "feedback-canary-91c4e7d2a85b";
    let fixture = revise_fixture("revise-canary");
    let text = format!("{FEEDBACK}\n{canary}");
    std::fs::write(fixture.path("feedback.txt"), &text).unwrap();
    fixture.write_script_for(&expected_id(1, &text), &revised_payload());
    let extra = revise_opts(&fixture, "elig.basic", "1");
    let ran = run_std(&consented_with(&fixture, &extra));
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert!(!ran.stdout.contains(canary), "{}", ran.stdout);
    assert!(!ran.stderr.contains(canary), "{}", ran.stderr);
    for name in fixture.files() {
        if name == "feedback.txt" {
            continue;
        }
        let bytes = std::fs::read(fixture.path(&name)).unwrap();
        let found = String::from_utf8_lossy(&bytes).contains(canary);
        assert!(!found, "{name} holds the feedback canary");
    }
}

// The feedback is part of the request, so a credential in it is refused
// by the request check (RT-AI-0308, exit 2) before any response exists.
#[test]
fn a_credential_in_the_feedback_is_refused_before_any_request() {
    for (i, credential) in canaries().into_iter().enumerate() {
        let fixture = revise_fixture(&format!("revise-credential-{i}"));
        std::fs::write(
            fixture.path("feedback.txt"),
            format!("{FEEDBACK} {credential}"),
        )
        .unwrap();
        let args = consented_with(&fixture, &revise_opts(&fixture, "elig.basic", "1"));
        let (ran, seen) = run_hosted(&fixture, credential, &args, revised_payload());
        assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
        assert_eq!(ran.codes(), ["RT-AI-0308"], "{}", ran.stderr);
        assert!(seen.is_empty(), "{credential}: the provider got a request");
        assert_eq!(leaked(&ran.stdout, credential), None, "{}", ran.stdout);
        assert_eq!(leaked(&ran.stderr, credential), None, "{}", ran.stderr);
        for name in fixture.files() {
            if name == "feedback.txt" {
                continue;
            }
            let bytes = std::fs::read(fixture.path(&name)).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            assert_eq!(leaked(&text, credential), None, "{name}: {text}");
        }
    }
}

// The response-echo case of RT-AI-0206 with a request that holds no
// credential: the feedback is clean and only the response carries it.
#[test]
fn a_credential_only_in_the_response_is_refused_as_rt_ai_0206() {
    for (i, credential) in canaries().into_iter().enumerate() {
        let fixture = revise_fixture(&format!("revise-echo-{i}"));
        let echoing = revised_payload().replace("raise the age", credential);
        let args = consented_with(&fixture, &revise_opts(&fixture, "elig.basic", "1"));
        let (ran, seen) = run_hosted(&fixture, credential, &args, echoing);
        assert_eq!(seen.len(), 1, "{credential}: the request was not sent");
        assert_eq!(ran.code, ExitCode::Response, "{}", ran.stderr);
        assert_eq!(ran.codes(), ["RT-AI-0206"], "{}", ran.stderr);
        assert_eq!(leaked(&ran.stdout, credential), None, "{}", ran.stdout);
        assert_eq!(leaked(&ran.stderr, credential), None, "{}", ran.stderr);
        assert!(fixture.files().iter().all(|f| !f.starts_with("store/")));
        for name in fixture.files() {
            let bytes = std::fs::read(fixture.path(&name)).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            assert_eq!(leaked(&text, credential), None, "{name}: {text}");
        }
    }
}

fn plan_with(context: Option<&ReviseContext>) -> RunPlan {
    let source = model("rules.md", RULES);
    let clause = CandidateClause::new(&source, 0, 11, None).unwrap();
    build_plan(&PlanInput {
        config: &config(""),
        models: &models(),
        candidates: &[clause],
        rulebook: &rulebook(),
        option: Ud32Option::RankReference,
        candidates_at: Location::Cli(CliLocation::Argument {
            position: 5,
            name: Some("--candidates".to_owned()),
        }),
        revise: context,
    })
    .unwrap()
}

fn context(revision: u32, feedback: &str) -> ReviseContext {
    ReviseContext::resolve(
        &rulebook(),
        &RuleId::new("elig.basic").unwrap(),
        RevisionNumber::new(revision).unwrap(),
        feedback.to_owned(),
    )
    .unwrap()
}

#[test]
fn each_change_of_the_revision_request_changes_the_identifier() {
    let base = plan_with(Some(&context(1, FEEDBACK)));
    let id = base.requests()[0].id().as_str().to_owned();
    let variants = [
        plan_with(None),
        plan_with(Some(&context(2, FEEDBACK))),
        plan_with(Some(&context(
            1,
            "Raise the age to 21.\nCite \"clause 2\".",
        ))),
        plan_with(Some(&context(1, &format!("{FEEDBACK} ")))),
    ];
    for variant in &variants {
        assert_ne!(variant.requests()[0].id().as_str(), id);
    }
    let mut ids: Vec<&str> = variants
        .iter()
        .map(|p| p.requests()[0].id().as_str())
        .collect();
    ids.push(&id);
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 5);
}

const ALPHABET: [char; 10] = ['a', 'b', ' ', '"', '\\', '\n', 'é', '中', '\u{1}', '\t'];

fn feedback() -> impl Strategy<Value = String> {
    proptest::collection::vec(proptest::sample::select(ALPHABET.to_vec()), 1..=40)
        .prop_map(|c| c.into_iter().collect())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    // Property 31 for a revision request: the bytes and the identifier
    // equal the hand-written oracle, and one change of the feedback or of
    // the revision changes the identifier.
    #[test]
    fn revision_requests_equal_the_oracle_and_every_change_changes_the_identifier(
        revision in 1_u32..=2,
        text in feedback(),
        at in any::<usize>(),
    ) {
        let plan = plan_with(Some(&context(revision, &text)));
        let want = expected_bytes(revision, &text);
        prop_assert_eq!(plan.requests()[0].bytes(), want.as_bytes());
        let id = sha256(want.as_bytes());
        prop_assert_eq!(plan.requests()[0].id().as_str(), id.as_str());

        let other_revision = 3 - revision;
        let changed = plan_with(Some(&context(other_revision, &text)));
        prop_assert_ne!(changed.requests()[0].id().as_str(), id.as_str());

        let chars: Vec<char> = text.chars().collect();
        let index = at % chars.len();
        let edited: String = chars
            .iter()
            .enumerate()
            .map(|(i, c)| if i == index { '#' } else { *c })
            .collect();
        let altered = plan_with(Some(&context(revision, &edited)));
        prop_assert_ne!(altered.requests()[0].id().as_str(), id.as_str());
        let appended = plan_with(Some(&context(revision, &format!("{text}x"))));
        prop_assert_ne!(appended.requests()[0].id().as_str(), id.as_str());
        let none = plan_with(None);
        prop_assert_ne!(none.requests()[0].id().as_str(), id.as_str());
    }
}

#[test]
fn resolve_names_the_unknown_part() {
    let rule = RuleId::new("elig.basic").unwrap();
    let other = RuleId::new("elig.none").unwrap();
    let three = RevisionNumber::new(3).unwrap();
    let one = RevisionNumber::new(1).unwrap();
    let rb = rulebook();
    let resolve = |id, n| ReviseContext::resolve(&rb, id, n, "f".to_owned());
    assert_eq!(resolve(&other, one), Err(Unknown::Rule));
    assert_eq!(resolve(&rule, three), Err(Unknown::Revision));
    assert!(resolve(&rule, one).is_ok());
}
