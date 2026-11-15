//! A Provider_Request carries no credential (Req 28.14, 23.33). A credential
//! value, whole or as a run of eight characters, in a clause text, in the
//! feedback or in the revised rule content refuses the run with
//! `RT-AI-0308` before the Transmission_Log, the store and the provider
//! are touched. Expected values are written by hand.

use crate::tests::MapEnv;
use crate::tests_cli_rig::{Fixture, Ran};
use crate::tests_gate_rig::{findings, run_providers, work_dir, Probe, Steps};
use crate::tests_memfs::MemFs;
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

fn batch_name() -> String {
    format!("{}.json", "a".repeat(64))
}

// The credential scan precedes the store check. A store with a malformed
// batch would give RT-AI-0302 if the order were the other way round, and
// the check would list, measure and read the batch.
#[test]
fn the_credential_scan_precedes_the_store_check() {
    let fixture = clause_fixture("req-cred-order", &format!("Alpha {CANARY} b."));
    fixture.write_config(&fixture.real_config(""));
    let batch = fixture
        .path(&format!("store/batches/{}", batch_name()))
        .to_string_lossy()
        .into_owned();
    let memory = MemFs::new();
    memory.plant(&batch, b"{");
    let (providers, probe) = Steps::new(Vec::new());
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CANARY)]);
    let ran = run_providers(&fixture.consented(), &env, &providers, &memory);
    assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
    assert_eq!(
        ran.shapes(),
        [("error".to_owned(), "RT-AI-0308".to_owned())],
        "{}",
        ran.stderr
    );
    assert!(
        ran.stderr.contains(&format!(" field {WHERE_CLAUSE}\n")),
        "{}",
        ran.stderr
    );
    assert_eq!(probe.attempts(), 0);
    let touched: Vec<String> = memory
        .ops()
        .into_iter()
        .filter(|op| op.contains("batches"))
        .collect();
    assert_eq!(touched, Vec::<String>::new());
    assert_eq!(memory.read(&batch), Some(b"{".to_vec()));
}

const FIXED_ADVICE: &str = "the credential or part of it occurs in fixed request text, \
     so it is likely too short or too common";
const DECLARATIONS_ADVICE: &str =
    "the credential or part of it occurs in the Rulebook declarations";
const REVISE_RULE_ADVICE: &str = "the credential or part of it occurs in the revision \
     target given with --revise-rule";

// The message of a refusal whose match is outside a clause, the feedback and
// the revised content: the common text, then the advice of its source.
fn assert_advised(
    got: &(Ran, Probe),
    fixture: &Fixture,
    before: &[String],
    at: &str,
    advice: &str,
) {
    let (ran, probe) = got;
    assert_eq!(ran.code, ExitCode::InvalidInput, "{at}: {}", ran.stderr);
    let found = findings(ran);
    assert_eq!(found.len(), 1, "{at}: {found:?}");
    assert_eq!(found[0].1, "RT-AI-0308", "{at}");
    assert_eq!(
        found[0].2,
        format!(
            "request 1 of 1 contains the credential value or a run of at least 8 \
             consecutive characters of it; {advice}; nothing was sent or written"
        ),
        "{at}"
    );
    assert!(
        ran.stderr.contains(&format!(" field {at}\n")),
        "{at}: {}",
        ran.stderr
    );
    assert_eq!(probe.attempts(), 0);
    assert_eq!(fixture.files(), before, "{at}: a file changed");
}

// Text that RuleTrace writes has no member of the input to name, so the
// message points at the credential. A key of the declarations is such text.
#[test]
fn a_credential_in_fixed_request_text_says_it_is_too_short_or_too_common() {
    let cases = [
        ("response_bytes", "requests[0]"),
        ("highest_revision", "requests[0].declarations"),
        ("half_away_from_zero", "requests[0].vocabulary"),
    ];
    for (i, (credential, at)) in cases.into_iter().enumerate() {
        let fixture = clause_fixture(&format!("req-cred-fixed-{i}"), "Alpha beta.");
        let before = fixture.files();
        let got = run_hosted(&fixture, credential, &fixture.consented());
        assert_advised(&got, &fixture, &before, at, FIXED_ADVICE);
    }
}

#[test]
fn a_credential_in_the_rulebook_declarations_names_the_declarations() {
    let fixture = clause_fixture("req-cred-declarations", "Alpha beta.");
    let rulebook = std::fs::read_to_string(fixture.path("rulebook.json")).unwrap();
    let renamed = rulebook.replacen(r#""name":"age""#, r#""name":"participant_age""#, 1);
    assert_ne!(renamed, rulebook, "the fixture declares the input age");
    std::fs::write(fixture.path("rulebook.json"), renamed).unwrap();
    let before = fixture.files();
    let got = run_hosted(&fixture, "participant_age", &fixture.consented());
    assert_advised(
        &got,
        &fixture,
        &before,
        "requests[0].declarations",
        DECLARATIONS_ADVICE,
    );
}

// The rule identifier is also a key of `highest_revision`, but the user gave
// it with `--revise-rule`, so that member is the one that is named.
#[test]
fn a_credential_in_the_revision_target_rule_names_the_revise_rule_option() {
    let fixture = revise_fixture("req-cred-revise-rule");
    let before = fixture.files();
    let args = consented_with(&fixture, &revise_opts(&fixture, "elig.basic", "1"));
    let got = run_hosted(&fixture, "elig.basic", &args);
    assert_advised(
        &got,
        &fixture,
        &before,
        "requests[0].revise.rule",
        REVISE_RULE_ADVICE,
    );
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

const MODEL_ADVICE: &str = "the credential or part of it occurs in the model name set \
     in the AI configuration";

fn run_with_model(
    name: &str,
    model: &str,
    credential: &str,
) -> (Fixture, Vec<String>, (Ran, Probe)) {
    let fixture = clause_fixture(name, "Alpha beta.");
    let config = fixture.real_config("").replace("model-1", model);
    assert!(config.contains(&format!("model = \"{model}\"")));
    fixture.write_config(&config);
    let before = fixture.files();
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", credential)]);
    let (providers, probe) = Steps::new(Vec::new());
    let ran = run_providers(&fixture.consented(), &env, &providers, &StdFs);
    (fixture, before, (ran, probe))
}

// The model name comes from the AI configuration, so the advice about
// fixed request text, which suggests a credential that is too short or too
// common, would point at the wrong cause.
#[test]
fn a_credential_in_the_configured_model_name_names_the_model() {
    let cases = [
        ("acme-reasoner-2025-01", "acme-reasoner-2025-01"),
        ("acme-reasoner-2025-01", "sk-acme-reasoner-2025-01-tail"),
    ];
    for (i, (model, credential)) in cases.into_iter().enumerate() {
        let (fixture, before, got) =
            run_with_model(&format!("req-cred-model-{i}"), model, credential);
        assert_advised(&got, &fixture, &before, "requests[0].model", MODEL_ADVICE);
    }
}

// The type tokens are text that RuleTrace writes. Only the name after
// `enum:` is the user's.
#[test]
fn a_credential_equal_to_a_type_token_is_fixed_text_not_a_declaration() {
    let fixture = clause_fixture("req-cred-type-token", "Alpha beta.");
    let before = fixture.files();
    let got = run_hosted(&fixture, "number", &fixture.consented());
    assert_advised(
        &got,
        &fixture,
        &before,
        "requests[0].declarations",
        FIXED_ADVICE,
    );
}
