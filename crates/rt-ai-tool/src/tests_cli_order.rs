//! Exit code and Diagnostic of each failing stage before a request is
//! built (Req 16.17 steps 1 to 4; Req 28.17). Every expected code is
//! written by hand from the catalogue.

use crate::tests::MapEnv;
use crate::tests_cli_rig::{run_std, run_with, Fixture};
use crate::StdFs;
use rt_foundation::ExitCode;

const INPUT_FILES: [&str; 5] = [
    "ai.toml",
    "candidates.json",
    "rulebook.json",
    "rules.model.json",
    "script.json",
];

// A refusal before any request leaves the tree as the inputs made it.
fn assert_nothing_written(fixture: &Fixture) {
    let found = fixture.files();
    assert!(
        found.iter().all(|f| INPUT_FILES.contains(&f.as_str())),
        "{found:?}"
    );
}

#[test]
fn step_1_an_unknown_option_exits_2_with_no_other_effect() {
    let fixture = Fixture::new("order-args");
    fixture.write_script("{}");
    let before = fixture.files();
    let ran = run_std(&fixture.args(&["--bogus"]));
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.codes(), ["RT-CLI-0004"]);
    assert_eq!(ran.stdout, "");
    assert_eq!(fixture.files(), before);
}

#[test]
fn step_1_an_unknown_command_exits_2() {
    let ran = run_std(&["review".into(), "list".into()]);
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.codes(), ["RT-CLI-0001"]);
    assert_eq!(ran.stdout, "");
}

#[test]
fn step_1_no_configuration_named_is_ai_disabled() {
    let fixture = Fixture::new("order-noconfig");
    let mut args = fixture.consented();
    args.drain(1..3);
    let ran = run_std(&args);
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.codes(), ["RT-AI-0001"]);
}

#[test]
fn step_1_a_malformed_created_at_exits_2_at_its_argument() {
    let fixture = Fixture::new("order-time");
    let mut args = fixture.args(&["--consent", "rules.md"]);
    let at = args.iter().position(|a| a == "--created-at").unwrap();
    args[at + 1] = "yesterday".into();
    let ran = run_std(&args);
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.codes(), ["RT-CLI-0004"]);
    assert!(
        ran.stderr.contains("argument 12 (--created-at)"),
        "{}",
        ran.stderr
    );
}

#[test]
fn step_2_each_configuration_failure_exits_2_with_its_code() {
    let fixture = Fixture::new("order-config");
    let cases = [
        ("schema_version = 1\nenabled =\n".to_owned(), "RT-AI-0002"),
        (
            fixture
                .mock_config("")
                .replace("enabled = true", "enabled = false"),
            "RT-AI-0001",
        ),
        (fixture.mock_config("api_key = \"x\"\n"), "RT-AI-0003"),
        (
            fixture.real_config("").replace("https://api", "http://api"),
            "RT-AI-0004",
        ),
        (
            fixture.mock_config("max_clauses_per_request = 0\n"),
            "RT-AI-0005",
        ),
    ];
    for (text, code) in cases {
        fixture.write_config(&text);
        let ran = run_std(&fixture.consented());
        assert_eq!(ran.code, ExitCode::InvalidInput, "{code}: {}", ran.stderr);
        assert_eq!(ran.codes(), [code], "{}", ran.stderr);
        assert_eq!(ran.stdout, "");
    }
    assert_nothing_written(&fixture);
}

#[test]
fn step_2_a_missing_configuration_file_exits_2() {
    let fixture = Fixture::new("order-noconfigfile");
    std::fs::remove_file(fixture.path("ai.toml")).unwrap();
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.codes(), ["RT-CLI-0003"]);
}

#[test]
fn step_3_missing_consent_exits_2_and_reads_no_credential() {
    let fixture = Fixture::new("order-consent");
    fixture.write_config(&fixture.real_config(""));
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", "canary-0123456789")]);
    let ran = run_with(&fixture.args(&[]), &env, &StdFs);
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.codes(), ["RT-AI-0008"]);
    assert!(env.reads().is_empty(), "{:?}", env.reads());
    assert_nothing_written(&fixture);
}

#[test]
fn step_3_consent_for_another_document_exits_2() {
    let fixture = Fixture::new("order-consent-other");
    let ran = run_std(&fixture.args(&["--consent", "other.md"]));
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.codes(), ["RT-AI-0008"]);
    assert!(
        ran.stderr.contains("missing: ['rules.md']"),
        "{}",
        ran.stderr
    );
}

#[test]
fn step_3_a_missing_credential_exits_2_after_consent_passed() {
    let fixture = Fixture::new("order-credential");
    fixture.write_config(&fixture.real_config(""));
    let ran = run_with(&fixture.consented(), &MapEnv::new(&[]), &StdFs);
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.codes(), ["RT-AI-0007"]);
    assert_nothing_written(&fixture);
}

#[test]
fn step_4_each_input_failure_exits_2_with_its_code() {
    let fixture = Fixture::new("order-input");
    let good = fixture.text("candidates.json");
    std::fs::remove_file(fixture.path("candidates.json")).unwrap();
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.codes(), ["RT-CLI-0003"]);
    std::fs::write(
        fixture.path("candidates.json"),
        good.replace("\"document\": \"rules.md\"", "\"document\": \"other.md\""),
    )
    .unwrap();
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.codes(), ["RT-AI-0304"]);
    std::fs::write(fixture.path("candidates.json"), good).unwrap();
    std::fs::write(fixture.path("rulebook.json"), "{").unwrap();
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.stdout, "");
    assert!(!ran.codes().is_empty());
    assert!(
        ran.codes().iter().all(|c| c.starts_with("RT-")),
        "{}",
        ran.stderr
    );
    assert_nothing_written(&fixture);
}

#[test]
fn an_earlier_stage_decides_when_a_later_stage_also_fails() {
    let fixture = Fixture::new("order-earliest");
    fixture.write_config("schema_version = 1\nenabled =\n");
    std::fs::remove_file(fixture.path("candidates.json")).unwrap();
    let ran = run_std(&fixture.args(&[]));
    assert_eq!(ran.codes(), ["RT-AI-0002"]);
    fixture.write_config(&fixture.mock_config(""));
    let ran = run_std(&fixture.args(&[]));
    assert_eq!(ran.codes(), ["RT-AI-0008"]);
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.codes(), ["RT-CLI-0003"]);
}

#[test]
fn a_revision_request_is_refused_because_this_build_has_no_revision_proposals() {
    let fixture = Fixture::new("order-revise");
    let ran = run_std(&fixture.args(&[
        "--revise-rule",
        "r.1",
        "--revise-revision",
        "1",
        "--feedback-file",
        "f.txt",
    ]));
    assert_eq!(ran.code, ExitCode::InvalidInput);
    assert_eq!(ran.codes(), ["RT-CLI-0004"]);
    assert_nothing_written(&fixture);
}
