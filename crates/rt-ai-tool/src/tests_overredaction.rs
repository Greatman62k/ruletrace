//! Documents that redaction removes any run of eight characters of the
//! credential, also from ordinary text (Req 23.23, catalogue RT-AI-0206).
//! The rule trades readability for safety: a prefix such as `sk-proj-`
//! that a credential shares with unrelated text is hidden as well. These
//! tests pin the exact output so that a change in either direction is
//! noticed and decided on.

use crate::tests::MapEnv;
use crate::tests_cli_rig::{Fixture, Ran};
use crate::tests_gate_rig::{findings, run_providers, work_dir, NONE_PROCESSED};
use crate::{AiConfig, Credential, Providers, StdFs};
use rt_ai_adapter::{
    AiAdapter, AttemptOutcome, AttemptReport, MonotonicClock, ProviderFailure, ProviderRequest,
};
use rt_ai_mock::SimulatedClock;
use rt_foundation::{Diagnostic, ExitCode};
use std::time::Duration;

const CREDENTIAL: &str = "sk-proj-Qz81mT4vLw93Xc";

struct Failing {
    clock: SimulatedClock,
    detail: String,
}

impl AiAdapter for Failing {
    fn attempt(&mut self, _: &ProviderRequest, _: Duration) -> AttemptReport {
        AttemptReport {
            outcome: AttemptOutcome::Failure(ProviderFailure::Other { http_status: None }),
            elapsed: Duration::ZERO,
            detail: Some(self.detail.clone()),
        }
    }

    fn clock(&self) -> &dyn MonotonicClock {
        &self.clock
    }
}

struct Detail(String);

impl Providers for Detail {
    fn adapter<'a>(
        &self,
        _: &AiConfig,
        _: Option<&Credential>,
    ) -> Result<(Box<dyn AiAdapter + 'a>, String), Diagnostic> {
        let failing = Failing {
            clock: SimulatedClock::new(),
            detail: self.0.clone(),
        };
        Ok((Box::new(failing), "scripted-1".to_owned()))
    }
}

fn run_detail(name: &str, detail: &str) -> Ran {
    let fixture = Fixture::new(name);
    fixture.write_config(&fixture.real_config(""));
    work_dir(&fixture);
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CREDENTIAL)]);
    run_providers(
        &fixture.consented(),
        &env,
        &Detail(detail.to_owned()),
        &StdFs,
    )
}

#[test]
fn ordinary_text_with_an_eight_character_window_of_the_credential_is_redacted() {
    let ran = run_detail("overredact-window", "rotate your sk-proj- key first");
    assert_eq!(ran.code, ExitCode::Provider, "{}", ran.stderr);
    assert_eq!(
        findings(&ran),
        [(
            "error".to_owned(),
            "RT-AI-0105".to_owned(),
            "AI_PROVIDER_ERROR (rotate your [REDACTED] key first)".to_owned()
        )]
    );
    assert!(!ran.stderr.contains("sk-proj-"), "{}", ran.stderr);
    assert_eq!(ran.stdout, NONE_PROCESSED);
}

#[test]
fn text_with_only_seven_characters_of_the_credential_is_left_alone() {
    let ran = run_detail("overredact-seven", "the sk-proj docs say so");
    assert_eq!(ran.code, ExitCode::Provider, "{}", ran.stderr);
    assert_eq!(
        findings(&ran),
        [(
            "error".to_owned(),
            "RT-AI-0105".to_owned(),
            "AI_PROVIDER_ERROR (the sk-proj docs say so)".to_owned()
        )]
    );
}

#[test]
fn the_whole_credential_and_a_window_in_one_message_become_two_markers() {
    let detail = format!("sent {CREDENTIAL}, saw sk-proj- again");
    let ran = run_detail("overredact-both", &detail);
    assert_eq!(
        findings(&ran),
        [(
            "error".to_owned(),
            "RT-AI-0105".to_owned(),
            "AI_PROVIDER_ERROR (sent [REDACTED], saw [REDACTED] again)".to_owned()
        )]
    );
}
