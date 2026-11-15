//! A Provider_Request carries no credential (Req 28.14, 23.33). A credential
//! value, whole or as a run of eight characters, in a clause text, in the
//! feedback or in the revised rule content refuses the run with
//! `RT-AI-0308` before the Transmission_Log, the store and the provider
//! are touched. Expected values are written by hand.

use crate::tests::MapEnv;
use crate::tests_cli_rig::{Fixture, Ran};
use crate::tests_gate_rig::{findings, run_providers, work_dir, Probe, Steps};
use crate::tests_provenance::{doc, model};
use crate::tests_redact_fragments::{canaries, leaked, CANARY};
use crate::tests_revise::{consented_with, revise_fixture, revise_opts, rulebook_text, FEEDBACK};
use crate::{EnvReader, Out, Redactor, StdFs};
use rt_foundation::ExitCode;
use rt_srcmodel::{print_candidates, CandidateClause};

const WHERE_CLAUSE: &str = "requests[0].clauses[0]";
const WHERE_FEEDBACK: &str = "requests[0].revise.feedback";
const WHERE_CONTENT: &str = "requests[0].revise.content";

// Eight consecutive characters of `CANARY` and of nothing else in a request.
fn window() -> String {
    CANARY.chars().skip(3).take(8).collect()
}

// Every credential form of the tests, then an eight-character window alone.
fn secrets() -> Vec<(String, String)> {
    let mut all: Vec<(String, String)> = canaries()
        .iter()
        .map(|c| ((*c).to_owned(), (*c).to_owned()))
        .collect();
    all.push((CANARY.to_owned(), window()));
    all
}

fn clause_fixture(name: &str, text: &str) -> Fixture {
    let fixture = Fixture::new(name);
    let source = model("rules.md", text);
    let end = u32::try_from(text.len()).unwrap();
    let clause = CandidateClause::new(&source, 0, end, None).unwrap();
    std::fs::write(fixture.path("rules.model.json"), source.print()).unwrap();
    std::fs::write(
        fixture.path("candidates.json"),
        print_candidates(&doc("rules.md"), &[clause]),
    )
    .unwrap();
    work_dir(&fixture);
    fixture
}

fn run_hosted(fixture: &Fixture, credential: &str, args: &[std::ffi::OsString]) -> (Ran, Probe) {
    fixture.write_config(&fixture.real_config(""));
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", credential)]);
    let (providers, probe) = Steps::new(Vec::new());
    (run_providers(args, &env, &providers, &StdFs), probe)
}

fn assert_refused(fixture: &Fixture, before: &[String], got: (Ran, Probe), at: &str, secret: &str) {
    let (ran, probe) = got;
    assert_eq!(ran.code, ExitCode::InvalidInput, "{at}: {}", ran.stderr);
    let found = findings(&ran);
    assert_eq!(found.len(), 1, "{at}: {found:?}");
    assert_eq!(found[0].0, "error", "{at}");
    assert_eq!(found[0].1, "RT-AI-0308", "{at}");
    assert_eq!(
        found[0].2,
        "request 1 of 1 contains the credential value or a run of at least 8 \
         consecutive characters of it; nothing was sent or written",
        "{at}"
    );
    assert!(
        ran.stderr.contains(&format!(" field {at}\n")),
        "{at}: {}",
        ran.stderr
    );
    assert_eq!(probe.attempts(), 0, "{at}: the provider was called");
    assert!(
        !fixture.path("work/tx.jsonl").exists(),
        "{at}: a log line was written"
    );
    assert_eq!(fixture.files(), before, "{at}: a file changed");
    for text in [&ran.stdout, &ran.stderr] {
        assert_eq!(leaked(text, secret), None, "{at}: {text}");
    }
}

#[test]
fn a_credential_in_a_clause_text_is_refused_before_anything_is_sent() {
    for (i, (credential, planted)) in secrets().into_iter().enumerate() {
        let fixture = clause_fixture(
            &format!("req-cred-clause-{i}"),
            &format!("Alpha {planted} b."),
        );
        let before = fixture.files();
        let args = fixture.consented();
        let got = run_hosted(&fixture, &credential, &args);
        assert_refused(&fixture, &before, got, WHERE_CLAUSE, &credential);
    }
}

#[test]
fn a_credential_in_the_feedback_is_refused_before_anything_is_sent() {
    for (i, (credential, planted)) in secrets().into_iter().enumerate() {
        let fixture = revise_fixture(&format!("req-cred-feedback-{i}"));
        std::fs::write(
            fixture.path("feedback.txt"),
            format!("{FEEDBACK} {planted}"),
        )
        .unwrap();
        let before = fixture.files();
        let args = consented_with(&fixture, &revise_opts(&fixture, "elig.basic", "1"));
        let got = run_hosted(&fixture, &credential, &args);
        assert_refused(&fixture, &before, got, WHERE_FEEDBACK, &credential);
    }
}

#[test]
fn a_credential_in_the_revised_rule_content_is_refused_before_anything_is_sent() {
    for (i, (credential, planted)) in secrets().into_iter().enumerate() {
        let fixture = revise_fixture(&format!("req-cred-content-{i}"));
        let marker =
            format!(r#""markers":[{{"kind":"ambiguous","interpretations":["{planted}"]}}]"#);
        let text = rulebook_text().replacen(r#""markers":[]"#, &marker, 1);
        std::fs::write(fixture.path("rulebook.json"), text).unwrap();
        let before = fixture.files();
        let args = consented_with(&fixture, &revise_opts(&fixture, "elig.basic", "1"));
        let got = run_hosted(&fixture, &credential, &args);
        assert_refused(&fixture, &before, got, WHERE_CONTENT, &credential);
    }
}

#[test]
fn a_request_without_the_credential_still_reaches_the_provider() {
    let fixture = clause_fixture("req-cred-clean", "Alpha beta.");
    let args = fixture.consented();
    let (ran, probe) = run_hosted(&fixture, CANARY, &args);
    assert_eq!(probe.attempts(), 1, "{}", ran.stderr);
    assert!(findings(&ran).iter().all(|f| f.1 != "RT-AI-0308"));
}

struct NoRead;

impl EnvReader for NoRead {
    fn get(&self, name: &str) -> Option<String> {
        panic!("a dry run read the environment variable {name}");
    }
}

#[test]
fn a_dry_run_reads_no_credential_and_is_not_refused() {
    let fixture = clause_fixture("req-cred-dry", &format!("Alpha {CANARY} b."));
    fixture.write_config(&fixture.real_config(""));
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = {
        let mut out = Out::new(&mut stdout, &mut stderr, Redactor::no_credential());
        crate::run(&fixture.args(&["--dry-run"]), &NoRead, &StdFs, &mut out)
    };
    let stdout = String::from_utf8(stdout).unwrap();
    let stderr = String::from_utf8(stderr).unwrap();
    assert_eq!(code, ExitCode::Success, "{stderr}");
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
}
