//! Step 4 of Req 16.17 and Req 23.33: an unreadable or malformed batch in
//! the existing Proposal_Store is `RT-AI-0302`, exit 2, before any
//! provider call and before any Transmission_Log entry. Expected codes are
//! written by hand from the catalogue.

use crate::tests::MapEnv;
use crate::tests_cli_rig::{run_std, Fixture};
use crate::tests_gate_rig::{run_providers, Steps};
use crate::tests_memfs::{Faults, MemFs};
use crate::tests_provenance::{doc, model, RULES};
use crate::tests_rig::payload;
use crate::tests_run::ok;
use crate::StdFs;
use rt_foundation::ExitCode;
use rt_srcmodel::{print_candidates, CandidateClause};
use std::io::ErrorKind;

const CREDENTIAL: &str = "credential-value-0123456789";

fn garbage(fixture: &Fixture, name: &str, bytes: &str) {
    let path = fixture.path(&format!("store/batches/{name}"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn name(letter: &str) -> String {
    format!("{}.json", letter.repeat(64))
}

fn hosted(label: &str) -> Fixture {
    let fixture = Fixture::new(label);
    fixture.write_config(&fixture.real_config(""));
    std::fs::create_dir_all(fixture.path("work")).unwrap();
    fixture
}

#[test]
fn a_garbage_batch_exits_2_with_rt_ai_0302_and_no_provider_call() {
    let fixture = hosted("store-garbage");
    garbage(&fixture, &name("a"), "{");
    let (providers, probe) = Steps::new(vec![ok(0)]);
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CREDENTIAL)]);
    let ran = run_providers(&fixture.consented(), &env, &providers, &StdFs);
    assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
    assert_eq!(
        ran.shapes(),
        [("error".to_owned(), "RT-AI-0302".to_owned())],
        "{}",
        ran.stderr
    );
    assert_eq!(probe.attempts(), 0);
    assert!(!fixture.path("work/tx.jsonl").exists());
    assert_eq!(ran.stdout, "");
    let batches = std::fs::read_dir(fixture.path("store/batches")).unwrap();
    assert_eq!(batches.count(), 1);
}

#[test]
fn only_the_first_malformed_batch_is_named() {
    let fixture = hosted("store-first");
    garbage(&fixture, &name("b"), "{");
    garbage(&fixture, &name("a"), "[]");
    let (providers, probe) = Steps::new(vec![ok(0)]);
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CREDENTIAL)]);
    let ran = run_providers(&fixture.consented(), &env, &providers, &StdFs);
    assert_eq!(ran.codes(), ["RT-AI-0302"], "{}", ran.stderr);
    assert!(ran.stderr.contains(&name("a")), "{}", ran.stderr);
    assert!(!ran.stderr.contains(&name("b")), "{}", ran.stderr);
    assert_eq!(probe.attempts(), 0);
}

#[test]
fn the_offending_path_is_redacted_from_the_output() {
    let fixture = hosted("store-redact");
    let dir = format!("store-{}", &CREDENTIAL[3..15]);
    let path = fixture.path(&format!("{dir}/batches/{}", name("a")));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "{").unwrap();
    let mut args = fixture.consented();
    let at = args.iter().position(|a| a == "--store").unwrap();
    args[at + 1] = fixture.path(&dir).into_os_string();
    let (providers, probe) = Steps::new(vec![ok(0)]);
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CREDENTIAL)]);
    let ran = run_providers(&args, &env, &providers, &StdFs);
    assert_eq!(ran.codes(), ["RT-AI-0302"], "{}", ran.stderr);
    assert_eq!(probe.attempts(), 0);
    assert!(ran.stderr.contains(crate::REDACTED), "{}", ran.stderr);
    assert!(
        !ran.stderr.contains(&CREDENTIAL[3..15]),
        "credential characters leaked: {}",
        ran.stderr
    );
}

#[test]
fn temp_files_and_foreign_names_are_not_batches() {
    let fixture = Fixture::new("store-ignored");
    fixture.write_script(&payload(0));
    std::fs::create_dir_all(fixture.path("work")).unwrap();
    let temp = fixture.path(&format!("store/.tmp/{}.tmp", "c".repeat(64)));
    std::fs::create_dir_all(temp.parent().unwrap()).unwrap();
    std::fs::write(temp, "{").unwrap();
    garbage(&fixture, "notes.txt", "not a batch");
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert!(ran.shapes().is_empty(), "{}", ran.stderr);
}

#[test]
fn a_valid_batch_of_another_request_is_accepted() {
    let fixture = Fixture::new("store-other");
    std::fs::create_dir_all(fixture.path("work")).unwrap();
    fixture.write_script(&payload(0));
    let first = fixture.request_id();
    assert_eq!(run_std(&fixture.consented()).code, ExitCode::Success);
    let source = model("rules.md", RULES);
    let second = CandidateClause::new(&source, 12, 27, None).unwrap();
    std::fs::write(
        fixture.path("candidates.json"),
        print_candidates(&doc("rules.md"), &[second]),
    )
    .unwrap();
    let id = fixture.request_id();
    assert_ne!(id, first);
    fixture.write_script_for(&id, &payload(1));
    let ran = run_std(&fixture.consented());
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert!(ran.shapes().is_empty(), "{}", ran.stderr);
    let mut found: Vec<String> = std::fs::read_dir(fixture.path("store/batches"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    let mut want = vec![format!("{first}.json"), format!("{id}.json")];
    want.sort();
    assert_eq!(found, want);
}

// A hosted run over an in-memory store that holds `bytes` as one batch.
fn memory_run(
    label: &str,
    bytes: &[u8],
    faults: Faults,
) -> (crate::tests_cli_rig::Ran, usize, MemFs) {
    let fixture = hosted(label);
    let memory = MemFs::new();
    memory.plant(
        &fixture
            .path(&format!("store/batches/{}", name("a")))
            .to_string_lossy(),
        bytes,
    );
    memory.with_faults(faults);
    let (providers, probe) = Steps::new(vec![ok(0)]);
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CREDENTIAL)]);
    let ran = run_providers(&fixture.consented(), &env, &providers, &memory);
    (ran, probe.attempts(), memory)
}

#[test]
fn a_garbage_batch_in_memory_stops_the_run_before_any_write() {
    let (ran, attempts, memory) = memory_run("store-mem-garbage", b"{", Faults::default());
    assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-AI-0302"], "{}", ran.stderr);
    assert_eq!(attempts, 0);
    assert!(memory.ops().iter().all(|op| !op.starts_with("create")));
}

#[test]
fn a_read_error_on_a_batch_is_rt_ai_0302_naming_the_file() {
    let faults = Faults {
        read: Some((name("a"), ErrorKind::PermissionDenied)),
        ..Faults::default()
    };
    let (ran, attempts, _) = memory_run("store-mem-read", b"{}", faults);
    assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-AI-0302"], "{}", ran.stderr);
    assert!(ran.stderr.contains("PermissionDenied"), "{}", ran.stderr);
    assert!(ran.stderr.contains(&name("a")), "{}", ran.stderr);
    assert_eq!(attempts, 0);
}

#[test]
fn a_directory_listing_error_is_rt_ai_0302() {
    let faults = Faults {
        list_dir: Some(ErrorKind::PermissionDenied),
        ..Faults::default()
    };
    let (ran, attempts, _) = memory_run("store-mem-list", b"{}", faults);
    assert_eq!(ran.code, ExitCode::InvalidInput, "{}", ran.stderr);
    assert_eq!(ran.codes(), ["RT-AI-0302"], "{}", ran.stderr);
    assert!(ran.stderr.contains("batches"), "{}", ran.stderr);
    assert_eq!(attempts, 0);
}

#[test]
fn a_store_that_does_not_exist_yet_is_empty() {
    let fixture = hosted("store-mem-empty");
    let memory = MemFs::new();
    let (providers, probe) = Steps::new(vec![ok(0)]);
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", CREDENTIAL)]);
    let ran = run_providers(&fixture.consented(), &env, &providers, &memory);
    assert_eq!(ran.code, ExitCode::Success, "{}", ran.stderr);
    assert_eq!(probe.attempts(), 1);
}
