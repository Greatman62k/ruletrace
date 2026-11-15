//! Canary credentials through whole `propose` runs of a hosted-provider
//! configuration (Req 28.9, 28.15, 23.22, 23.23). A scripted provider
//! echoes the credential, whole and cut, in failure text and in a response.
//! No run of eight characters of the credential may appear on standard
//! output, standard error or in any file the run wrote. The credentials
//! include one longer than 60 characters, a dotted one and a bracketed one.

use crate::tests::MapEnv;
use crate::tests_cli_rig::{Fixture, Ran};
use crate::tests_memfs::{Faults, MemFs};
use crate::tests_redact_fragments::{canaries, leaked};
use crate::tests_rig::payload;
use crate::{run_with, AiConfig, Credential, Out, Providers, Redactor, StdFs};
use rt_ai_adapter::{
    AiAdapter, AttemptOutcome, AttemptReport, Completion, MonotonicClock, ProviderFailure,
    ProviderMeta, ProviderRequest, ProviderResponse,
};
use rt_ai_mock::SimulatedClock;
use rt_foundation::{Diagnostic, ExitCode};
use std::ffi::OsString;
use std::io::ErrorKind;
use std::time::Duration;

struct Echo {
    clock: SimulatedClock,
    report: AttemptReport,
}

impl AiAdapter for Echo {
    fn attempt(&mut self, _: &ProviderRequest, _: Duration) -> AttemptReport {
        self.report.clone()
    }

    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }
}

struct Scripted(AttemptReport);

impl Providers for Scripted {
    fn is_linked(&self, _: crate::ProviderKind) -> bool {
        true
    }

    fn adapter<'a>(
        &self,
        _: &AiConfig,
        credential: Option<&Credential>,
    ) -> Result<(Box<dyn AiAdapter + 'a>, String), Diagnostic> {
        assert!(
            credential.is_some(),
            "a hosted provider run has a credential"
        );
        let echo = Echo {
            clock: SimulatedClock::new(),
            report: self.0.clone(),
        };
        Ok((Box::new(echo), "scripted-1".to_owned()))
    }
}

fn piece(credential: &str, from: usize, len: usize) -> String {
    credential.chars().skip(from).take(len).collect()
}

fn echoed(credential: &str) -> String {
    let n = credential.chars().count();
    format!(
        "{credential} | {} | {} | {}",
        piece(credential, 0, n - 3),
        piece(credential, 3, 12),
        piece(credential, n - 10, 10)
    )
}

fn failure(f: ProviderFailure, detail: &str) -> AttemptReport {
    AttemptReport {
        outcome: AttemptOutcome::Failure(f),
        elapsed: Duration::ZERO,
        detail: Some(detail.to_owned()),
    }
}

fn response(payload: String) -> AttemptReport {
    AttemptReport {
        outcome: AttemptOutcome::Response(ProviderResponse {
            completion: Completion::Complete,
            payload: payload.into_bytes(),
            meta: ProviderMeta {
                provider: "openai-responses".to_owned(),
                model: "model-1".to_owned(),
            },
        }),
        elapsed: Duration::ZERO,
        detail: None,
    }
}

fn run_hosted(fixture: &Fixture, credential: &str, report: AttemptReport) -> Ran {
    fixture.write_config(&fixture.real_config(""));
    std::fs::create_dir_all(fixture.path("work")).unwrap();
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", credential)]);
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = {
        let mut out = Out::new(&mut stdout, &mut stderr, Redactor::no_credential());
        run_with(
            &fixture.consented(),
            &env,
            &Scripted(report),
            &StdFs,
            &mut out,
        )
    };
    Ran {
        code,
        stdout: String::from_utf8(stdout).unwrap(),
        stderr: String::from_utf8(stderr).unwrap(),
    }
}

fn assert_no_leak(fixture: &Fixture, ran: &Ran, credential: &str) {
    assert_eq!(leaked(&ran.stdout, credential), None, "{}", ran.stdout);
    assert_eq!(leaked(&ran.stderr, credential), None, "{}", ran.stderr);
    for name in fixture.files() {
        let bytes = std::fs::read(fixture.path(&name)).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert_eq!(leaked(&text, credential), None, "{name}: {text}");
    }
}

#[test]
fn provider_failure_text_that_echoes_the_credential_is_removed_everywhere() {
    for (i, c) in canaries().into_iter().enumerate() {
        let fixture = Fixture::new(&format!("hosted-failure-{i}"));
        let detail = echoed(c);
        let ran = run_hosted(
            &fixture,
            c,
            failure(ProviderFailure::Other { http_status: None }, &detail),
        );
        assert_eq!(ran.code, ExitCode::Provider, "{c}: {}", ran.stderr);
        assert_eq!(ran.codes(), ["RT-AI-0105"], "{c}: {}", ran.stderr);
        assert!(ran.stderr.contains("[REDACTED]"), "{c}: {}", ran.stderr);
        assert_no_leak(&fixture, &ran, c);
        assert!(
            ran.stdout.contains("run summary: incomplete"),
            "{}",
            ran.stdout
        );
    }
}

#[test]
fn an_authentication_failure_that_echoes_the_credential_is_removed_everywhere() {
    for (i, c) in canaries().into_iter().enumerate() {
        let fixture = Fixture::new(&format!("hosted-auth-{i}"));
        let ran = run_hosted(
            &fixture,
            c,
            failure(ProviderFailure::AuthFailed, &echoed(c)),
        );
        assert_eq!(ran.code, ExitCode::Provider, "{c}: {}", ran.stderr);
        assert_eq!(ran.codes(), ["RT-AI-0104"], "{c}: {}", ran.stderr);
        assert_no_leak(&fixture, &ran, c);
    }
}

#[test]
fn a_response_that_echoes_the_credential_is_rejected_and_never_stored() {
    for (i, c) in canaries().into_iter().enumerate() {
        let fixture = Fixture::new(&format!("hosted-response-{i}"));
        let echo = echoed(c).replace('"', "'");
        let text = payload(0).replace("clause 0", &echo);
        let ran = run_hosted(&fixture, c, response(text));
        assert_eq!(ran.code, ExitCode::Response, "{c}: {}", ran.stderr);
        assert_eq!(ran.codes(), ["RT-AI-0206"], "{c}: {}", ran.stderr);
        assert!(!fixture.path("store/batches").exists());
        assert_no_leak(&fixture, &ran, c);
    }
}

#[test]
fn a_clean_hosted_run_writes_no_credential_character_run_anywhere() {
    for (i, c) in canaries().into_iter().enumerate() {
        let fixture = Fixture::new(&format!("hosted-clean-{i}"));
        let ran = run_hosted(&fixture, c, response(payload(0)));
        assert_eq!(ran.code, ExitCode::Success, "{c}: {}", ran.stderr);
        assert!(fixture.path("store/batches").exists());
        assert_eq!(
            ran.stderr
                .lines()
                .filter(|l| l.contains("endpoint host api.example.org"))
                .count(),
            1
        );
        assert_no_leak(&fixture, &ran, c);
    }
}

#[test]
fn the_built_in_providers_link_no_hosted_provider_and_make_no_request() {
    let fixture = Fixture::new("hosted-unavailable");
    fixture.write_config(&fixture.real_config(""));
    std::fs::create_dir_all(fixture.path("work")).unwrap();
    let before = fixture.files();
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", canaries()[1])]);
    let ran = crate::tests_cli_rig::run_with(&fixture.consented(), &env, &StdFs);
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.codes(), ["RT-AI-0006"]);
    assert_eq!(fixture.files(), before);
    assert_no_leak(&fixture, &ran, canaries()[1]);
}

// A hosted run whose Proposal_Store directory name copies part of the
// credential, over an in-memory store that fails as `faults` says.
fn run_with_store_faults(fixture: &Fixture, credential: &str, faults: Faults) -> Ran {
    fixture.write_config(&fixture.real_config(""));
    std::fs::create_dir_all(fixture.path("work")).unwrap();
    let name = format!("store-{}", piece(credential, 2, 12));
    let mut args = fixture.consented();
    let at = args.iter().position(|a| a == "--store").unwrap();
    args[at + 1] = OsString::from(fixture.path(&name));
    let memory = MemFs::new();
    memory.with_faults(faults);
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", credential)]);
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = {
        let mut out = Out::new(&mut stdout, &mut stderr, Redactor::no_credential());
        let provider = Scripted(response(payload(0)));
        run_with(&args, &env, &provider, &memory, &mut out)
    };
    Ran {
        code,
        stdout: String::from_utf8(stdout).unwrap(),
        stderr: String::from_utf8(stderr).unwrap(),
    }
}

#[test]
fn a_store_write_failure_naming_a_path_with_credential_characters_is_redacted() {
    for (i, c) in canaries().into_iter().enumerate() {
        let fixture = Fixture::new(&format!("hosted-store-{i}"));
        let faults = Faults {
            create_temp: Some(ErrorKind::PermissionDenied),
            ..Faults::default()
        };
        let ran = run_with_store_faults(&fixture, c, faults);
        assert_eq!(ran.code, ExitCode::Internal, "{c}: {}", ran.stderr);
        assert_eq!(ran.codes(), ["RT-AI-0303"], "{c}: {}", ran.stderr);
        assert!(ran.stderr.contains("[REDACTED]"), "{c}: {}", ran.stderr);
        assert_no_leak(&fixture, &ran, c);
    }
}

#[test]
fn a_temporary_file_warning_naming_a_path_with_credential_characters_is_redacted() {
    for (i, c) in canaries().into_iter().enumerate() {
        let fixture = Fixture::new(&format!("hosted-temp-{i}"));
        let faults = Faults {
            remove: Some(ErrorKind::PermissionDenied),
            ..Faults::default()
        };
        let ran = run_with_store_faults(&fixture, c, faults);
        assert_eq!(ran.code, ExitCode::Success, "{c}: {}", ran.stderr);
        assert_eq!(ran.codes(), ["RT-AI-0307"], "{c}: {}", ran.stderr);
        assert!(ran.stderr.contains("[REDACTED]"), "{c}: {}", ran.stderr);
        assert_no_leak(&fixture, &ran, c);
    }
}
