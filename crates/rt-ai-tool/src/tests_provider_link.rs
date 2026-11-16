//! Step 2 of Req 16.17 and 28.17: a provider kind that a build does not
//! link (here the `Unlinked` test build, since `BuiltInProviders` links both
//! kinds and a hosted run would reach a network) is `RT-AI-0006` at configuration time, before consent, credential
//! and input load. Expected codes are written by hand from the catalogue.

use crate::tests::MapEnv;
use crate::tests_cli_rig::Fixture;
use crate::tests_gate_rig::{run_providers, Steps, Unlinked};
use crate::{BuiltInProviders, ProviderKind, Providers, StdFs};
use rt_foundation::ExitCode;

const CREDENTIAL: &str = "credential-value-0123456789";

fn hosted(name: &str) -> Fixture {
    let fixture = Fixture::new(name);
    fixture.write_config(&fixture.real_config(""));
    std::fs::create_dir_all(fixture.path("work")).unwrap();
    fixture
}

#[test]
fn an_unlinked_provider_wins_over_a_missing_consent_and_reads_no_credential() {
    let fixture = hosted("link-consent");
    let before = fixture.files();
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CREDENTIAL)]);
    let ran = run_providers(&fixture.args(&[]), &env, &Unlinked, &StdFs);
    assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-AI-0006"], "{}", ran.stderr);
    assert!(env.reads().is_empty(), "{:?}", env.reads());
    assert_eq!(fixture.files(), before);
}

#[test]
fn an_unlinked_provider_wins_over_a_missing_credential() {
    let fixture = hosted("link-credential");
    let env = MapEnv::new(&[]);
    let ran = run_providers(&fixture.consented(), &env, &Unlinked, &StdFs);
    assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-AI-0006"], "{}", ran.stderr);
    assert!(env.reads().is_empty(), "{:?}", env.reads());
}

#[test]
fn an_unlinked_provider_wins_over_an_unreadable_input() {
    let fixture = hosted("link-input");
    std::fs::remove_file(fixture.path("candidates.json")).unwrap();
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CREDENTIAL)]);
    let ran = run_providers(&fixture.consented(), &env, &Unlinked, &StdFs);
    assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-AI-0006"], "{}", ran.stderr);
}

#[test]
fn the_unlinked_provider_error_is_located_at_the_provider_field() {
    let fixture = hosted("link-location");
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CREDENTIAL)]);
    let ran = run_providers(&fixture.consented(), &env, &Unlinked, &StdFs);
    assert_eq!(ran.codes(), ["RT-AI-0006"], "{}", ran.stderr);
    assert!(
        ran.stderr.contains(" line 3 column 1 field provider\n"),
        "{}",
        ran.stderr
    );
}

#[test]
fn a_linked_provider_still_reports_consent_then_credential() {
    let fixture = hosted("link-linked");
    let (providers, probe) = Steps::new(Vec::new());
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CREDENTIAL)]);
    let ran = run_providers(&fixture.args(&[]), &env, &providers, &StdFs);
    assert_eq!(ran.codes(), ["RT-AI-0008"], "{}", ran.stderr);
    let ran = run_providers(&fixture.consented(), &MapEnv::new(&[]), &providers, &StdFs);
    assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-AI-0007"], "{}", ran.stderr);
    assert_eq!(probe.attempts(), 0);
}

// Task 10.5 removed the interim arm: the Responses integration is linked,
// so RT-AI-0006 is left for a kind that no provider implements.
#[test]
fn the_built_in_providers_link_the_replay_and_the_responses_integration() {
    assert!(BuiltInProviders.is_linked(ProviderKind::MockReplay));
    assert!(BuiltInProviders.is_linked(ProviderKind::OpenAiResponses));
}
