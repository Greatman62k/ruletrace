//! A revision run accepts only drafts of the named rule (Req 23.16, design
//! 3.7.4). A `rule_suggestion` that names another Rule identifier rejects
//! the whole batch with `RT-AI-0207`, exit 6, as a protocol failure that
//! follows the schema check and precedes provenance. Expected values are
//! written by hand.

use crate::tests_cli_rig::{run_std, Fixture};
use crate::tests_gate_rig::{findings, work_dir};
use crate::tests_revise::{
    consented_with, expected_id, revise_fixture, revise_opts, revised_payload, FEEDBACK,
    NEW_CONTENT,
};
use crate::tests_rig::evidence;
use rt_foundation::ExitCode;

fn other_rule() -> String {
    NEW_CONTENT.replacen(r#""id":"elig.basic""#, r#""id":"elig.other""#, 1)
}

fn naming_other_rule() -> String {
    revised_payload().replace(NEW_CONTENT, &other_rule())
}

fn revise_args(fixture: &Fixture) -> Vec<std::ffi::OsString> {
    consented_with(fixture, &revise_opts(fixture, "elig.basic", "1"))
}

fn assert_nothing_stored(fixture: &Fixture) {
    assert!(
        fixture.files().iter().all(|f| !f.starts_with("store/")),
        "{:?}",
        fixture.files()
    );
}

#[test]
fn a_draft_that_names_another_rule_rejects_the_batch() {
    let fixture = revise_fixture("target-mismatch");
    fixture.write_script_for(&expected_id(1, FEEDBACK), &naming_other_rule());
    let ran = run_std(&revise_args(&fixture));
    assert_eq!(ran.code, ExitCode::Response, "{}", ran.stderr);
    let found = findings(&ran);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "error");
    assert_eq!(found[0].1, "RT-AI-0207");
    assert_eq!(
        found[0].2,
        "a rule suggestion of a revision run names a Rule identifier other than \
         the one given by --revise-rule"
    );
    assert!(
        ran.stderr.contains(" field proposals[0].rule\n"),
        "{}",
        ran.stderr
    );
    assert_nothing_stored(&fixture);
}

#[test]
fn the_first_mismatching_proposal_is_named_by_its_index() {
    let fixture = revise_fixture("target-mismatch-second");
    let second = format!(
        r#"{{"kind":"rule_suggestion","proposer":"model-1","rationale":"other","evidence":[{}],"rule":{}}}"#,
        evidence(0),
        other_rule()
    );
    let mut payload = revised_payload();
    assert!(payload.ends_with("]}"));
    payload.truncate(payload.len() - 2);
    payload.push_str(&format!(",{second}]}}"));
    fixture.write_script_for(&expected_id(1, FEEDBACK), &payload);
    let ran = run_std(&revise_args(&fixture));
    assert_eq!(ran.codes(), ["RT-AI-0207"], "{}", ran.stderr);
    assert!(
        ran.stderr.contains(" field proposals[1].rule\n"),
        "{}",
        ran.stderr
    );
    assert_nothing_stored(&fixture);
}

#[test]
fn a_draft_of_the_named_rule_is_accepted() {
    let fixture = revise_fixture("target-match");
    fixture.write_script_for(&expected_id(1, FEEDBACK), &revised_payload());
    let ran = run_std(&revise_args(&fixture));
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert!(fixture
        .files()
        .iter()
        .any(|f| f.starts_with("store/batches/")));
}

#[test]
fn a_run_without_a_revision_request_accepts_any_rule_identifier() {
    let fixture = Fixture::new("target-no-revise");
    work_dir(&fixture);
    fixture.write_script(&naming_other_rule());
    let ran = run_std(&fixture.consented());
    assert_ne!(ran.codes(), ["RT-AI-0207"], "{}", ran.stderr);
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
}

#[test]
fn the_target_check_precedes_the_provenance_check() {
    let fixture = revise_fixture("target-before-provenance");
    let broken = naming_other_rule().replace(r#""quote":"Alpha beta.""#, r#""quote":"Wrong text""#);
    assert_ne!(broken, naming_other_rule());
    fixture.write_script_for(&expected_id(1, FEEDBACK), &broken);
    let ran = run_std(&revise_args(&fixture));
    assert_eq!(ran.code, ExitCode::Response, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-AI-0207"], "{}", ran.stderr);
}

#[test]
fn a_schema_violation_precedes_the_target_check() {
    let fixture = revise_fixture("target-after-schema");
    let broken = naming_other_rule().replacen(r#""proposer""#, r#""proposers""#, 1);
    fixture.write_script_for(&expected_id(1, FEEDBACK), &broken);
    let ran = run_std(&revise_args(&fixture));
    assert_eq!(ran.code, ExitCode::Response, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-AI-0202"], "{}", ran.stderr);
}
