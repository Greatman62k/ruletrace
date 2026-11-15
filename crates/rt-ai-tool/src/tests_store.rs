//! Proposal_Store writer tests (design 3.7.5; Req 29.10, 29.11, 29.19).
//! The disk is replaced by an in-memory file system that fails on request,
//! so each failure runs through the real error paths of the writer. The
//! file content is checked by `parse_batch`, a separate decoder that
//! verifies the canonical form and every proposal identifier.

use crate::accept::{accept_batch, AcceptInput, AcceptedBatch};
use crate::tests_memfs::{Faults, Link, MemFs};
use crate::tests_response::LIMIT;
use crate::tests_rig::{payload, Harness, ROOT};
use crate::tests_scratch::{GateFs, ScratchDir};
use crate::{store_batch, FailureOutcome, StoreFailure, StoreOutcome, StoreReport};
use rt_foundation::{Code, Diagnostic, ExitCode, InputId, Location, Severity};
use rt_hash::{sha256, Sha256Hex};
use rt_proposals::parse_batch;
use std::io::ErrorKind;
use std::path::Path;

fn accepted_with(h: &Harness, request: usize, text: &str) -> AcceptedBatch {
    let planned = &h.plan.requests()[request];
    let id = InputId::new("response-1").unwrap();
    let request_id = Sha256Hex::parse(planned.id().as_str()).unwrap();
    accept_batch(&AcceptInput {
        response_id: &id,
        request_id: &request_id,
        completion: rt_ai_adapter::Completion::Complete,
        payload: text.as_bytes(),
        response_limit: LIMIT,
        models: &h.models,
        clauses: planned.clauses(),
        declarations: &h.rulebook,
        redactor: &h.redactor,
        revise_rule: None,
    })
    .unwrap_or_else(|d| panic!("rejected: {d:?}"))
}

fn accepted(h: &Harness, request: usize) -> AcceptedBatch {
    accepted_with(h, request, &payload(request))
}

fn store_full(h: &Harness, fs: &MemFs, batch: &AcceptedBatch) -> Result<StoreReport, StoreFailure> {
    store_batch(fs, Path::new(ROOT), batch, &h.meta)
}

fn store(h: &Harness, fs: &MemFs, batch: &AcceptedBatch) -> Result<StoreOutcome, Diagnostic> {
    store_full(h, fs, batch)
        .map(|report| report.outcome)
        .map_err(|failure| failure.error)
}

fn expect_temp_warning(w: &Diagnostic, temp: &str, kind: &str) {
    assert_eq!(w.code(), Code::AiStoreTempNotRemoved);
    assert_eq!(w.code().id(), "RT-AI-0307");
    assert_eq!(w.code().mnemonic(), "AI_STORE_TEMP_NOT_REMOVED");
    assert_eq!(w.severity(), Severity::Warning);
    assert_eq!(w.code().exit_code(), ExitCode::Success);
    assert_eq!(
        w.message(),
        format!(
            "the temporary file could not be removed ({kind}); \
             it is never read and can be deleted"
        )
    );
    match w.primary() {
        Location::Input(at) => assert_eq!(at.input.as_str(), temp),
        other => panic!("expected an input location, found {other:?}"),
    }
}

fn expect_failure(d: &Diagnostic, final_name: &str) {
    assert_eq!(d.code(), Code::AiStoreWriteFailed);
    assert_eq!(d.severity(), Severity::Error);
    assert_eq!(d.code().id(), "RT-AI-0303");
    assert_eq!(d.code().exit_code(), ExitCode::Internal);
    assert_eq!(d.code().exit_code().value(), 70);
    match d.primary() {
        Location::Input(at) => assert_eq!(at.input.as_str(), final_name),
        other => panic!("expected an input location, found {other:?}"),
    }
}

fn expect_already(outcome: Result<StoreOutcome, Diagnostic>, final_name: &str) {
    let StoreOutcome::AlreadyStored(w) = outcome.expect("no failure") else {
        panic!("expected AlreadyStored");
    };
    assert_eq!(w.code().id(), "RT-AI-0301");
    assert_eq!(w.severity(), Severity::Warning);
    assert_eq!(w.code().exit_code(), ExitCode::Success);
    match w.primary() {
        Location::Input(at) => assert_eq!(at.input.as_str(), final_name),
        other => panic!("expected an input location, found {other:?}"),
    }
}

fn tmp_name(h: &Harness, request: usize, slot: &str) -> String {
    format!("{ROOT}/.tmp/{}{slot}.tmp", h.id(request))
}

#[test]
fn a_stored_batch_is_whole_canonical_and_leaves_no_temporary_file() {
    let h = Harness::new("");
    let fs = MemFs::new();
    let batch = accepted(&h, 0);
    assert_eq!(store(&h, &fs, &batch).unwrap(), StoreOutcome::Stored);
    let name = h.batch_file(0);
    let bytes = fs.read(&name).expect("the batch file exists");
    let parsed = parse_batch(&InputId::new("batch").unwrap(), &bytes).unwrap();
    assert_eq!(parsed.request_id.as_str(), h.id(0));
    assert_eq!(parsed.response.payload, payload(0));
    assert_eq!(parsed.response_sha256, sha256(payload(0).as_bytes()));
    assert_eq!(parsed.provider, "openai-responses");
    assert_eq!(parsed.model, h.config.model);
    assert_eq!(parsed.model, "model-1");
    assert_eq!(parsed.adapter_version, "mock-1");
    assert_eq!(parsed.created_at, "2026-11-15T00:00:00Z");
    assert_eq!(parsed.proposals.len(), 1);
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert_eq!(
        fs.names_under(&format!("{ROOT}/.tmp")),
        Vec::<String>::new()
    );
}

#[test]
fn the_steps_run_in_the_documented_order() {
    let h = Harness::new("");
    let fs = MemFs::new();
    store(&h, &fs, &accepted(&h, 0)).unwrap();
    let tmp = tmp_name(&h, 0, "");
    let ops = fs.ops();
    assert_eq!(ops.len(), 9, "{ops:?}");
    assert_eq!(ops[0], format!("exists {}", h.batch_file(0)));
    assert_eq!(ops[1], format!("create_dir_all {ROOT}/batches"));
    assert_eq!(ops[2], format!("create_dir_all {ROOT}/.tmp"));
    assert_eq!(ops[3], format!("create_new {tmp}"));
    assert!(ops[4].starts_with("write "), "{ops:?}");
    assert_eq!(ops[5], "sync_file");
    assert_eq!(ops[6], format!("link {tmp} {}", h.batch_file(0)));
    assert_eq!(ops[7], format!("sync_dir {ROOT}/batches"));
    assert_eq!(ops[8], format!("remove {tmp}"));
}

#[test]
fn an_existing_batch_is_reported_before_any_write() {
    let h = Harness::new("");
    let fs = MemFs::new();
    let name = h.batch_file(0);
    fs.plant(&name, b"earlier batch bytes");
    fs.clear_ops();
    expect_already(store(&h, &fs, &accepted(&h, 0)), &name);
    assert_eq!(fs.read(&name).unwrap(), b"earlier batch bytes");
    assert_eq!(fs.ops(), vec![format!("exists {name}")]);
    assert_eq!(fs.files().len(), 1);
}

#[test]
fn a_partial_write_removes_the_temporary_file_and_stores_nothing() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        partial_write: Some(17),
        ..Faults::default()
    });
    let err = store(&h, &fs, &accepted(&h, 0)).unwrap_err();
    expect_failure(&err, &h.batch_file(0));
    assert!(
        err.message().contains("writing the temporary file"),
        "{err:?}"
    );
    assert!(fs.files().is_empty(), "{:?}", fs.files().keys());
    assert!(fs
        .ops()
        .contains(&format!("remove {}", tmp_name(&h, 0, ""))));
}

#[test]
fn a_failed_sync_of_the_temporary_file_stores_nothing() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        sync_file: Some(ErrorKind::Other),
        ..Faults::default()
    });
    let err = store(&h, &fs, &accepted(&h, 0)).unwrap_err();
    expect_failure(&err, &h.batch_file(0));
    assert!(fs.files().is_empty());
    assert!(!fs.ops().iter().any(|o| o.starts_with("link ")));
}

#[test]
fn a_crash_between_the_temporary_write_and_the_link_leaves_no_batch_file() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        links: [Link::Crash].into(),
        ..Faults::default()
    });
    let err = store(&h, &fs, &accepted(&h, 0)).unwrap_err();
    expect_failure(&err, &h.batch_file(0));
    let names: Vec<String> = fs.files().into_keys().collect();
    assert_eq!(names, vec![tmp_name(&h, 0, "")]);
    assert!(fs.read(&h.batch_file(0)).is_none());
}

#[test]
fn a_rerun_after_a_crash_stores_the_batch_and_keeps_the_stale_temporary_file() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        links: [Link::Crash].into(),
        ..Faults::default()
    });
    store(&h, &fs, &accepted(&h, 0)).unwrap_err();
    fs.revive();
    assert_eq!(
        store(&h, &fs, &accepted(&h, 0)).unwrap(),
        StoreOutcome::Stored
    );
    let bytes = fs.read(&h.batch_file(0)).unwrap();
    parse_batch(&InputId::new("batch").unwrap(), &bytes).unwrap();
    let left: Vec<String> = fs.names_under(&format!("{ROOT}/.tmp"));
    assert_eq!(left, vec![tmp_name(&h, 0, "")]);
    assert!(fs
        .ops()
        .contains(&format!("create_new {}", tmp_name(&h, 0, ".1"))));
}

#[test]
fn an_unsupported_hard_link_refuses_the_write_and_changes_nothing() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        links: [Link::Fail(ErrorKind::Unsupported)].into(),
        ..Faults::default()
    });
    let err = store(&h, &fs, &accepted(&h, 0)).unwrap_err();
    expect_failure(&err, &h.batch_file(0));
    assert!(err.message().contains("Unsupported"), "{err:?}");
    assert!(fs.files().is_empty(), "{:?}", fs.files().keys());
    let tmp = tmp_name(&h, 0, "");
    let ops = fs.ops();
    let after_link: Vec<&String> = ops.iter().skip_while(|o| !o.starts_with("link ")).collect();
    assert_eq!(after_link.len(), 2, "{ops:?}");
    assert_eq!(after_link[1], &format!("remove {tmp}"));
    assert_eq!(
        ops.iter().filter(|o| o.starts_with("create_new")).count(),
        1
    );
}

#[test]
fn any_other_link_failure_refuses_the_write_in_the_same_way() {
    for kind in [
        ErrorKind::PermissionDenied,
        ErrorKind::Other,
        ErrorKind::NotFound,
    ] {
        let h = Harness::new("");
        let fs = MemFs::new();
        fs.with_faults(Faults {
            links: [Link::Fail(kind)].into(),
            ..Faults::default()
        });
        let err = store(&h, &fs, &accepted(&h, 0)).unwrap_err();
        expect_failure(&err, &h.batch_file(0));
        assert!(fs.files().is_empty(), "{kind:?}");
    }
}

#[test]
fn a_concurrent_winner_gives_the_warning_and_keeps_the_winner_bytes() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        links: [Link::Winner(b"winner bytes".to_vec())].into(),
        ..Faults::default()
    });
    expect_already(store(&h, &fs, &accepted(&h, 0)), &h.batch_file(0));
    assert_eq!(fs.read(&h.batch_file(0)).unwrap(), b"winner bytes");
    assert_eq!(
        fs.names_under(&format!("{ROOT}/.tmp")),
        Vec::<String>::new()
    );
    assert_eq!(fs.files().len(), 1);
}

#[test]
fn a_failed_directory_sync_reports_the_failure_and_keeps_the_whole_file() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        sync_dir: Some(ErrorKind::Other),
        ..Faults::default()
    });
    let failure = store_full(&h, &fs, &accepted(&h, 0)).unwrap_err();
    assert_eq!(failure.outcome, FailureOutcome::KeptUnconfirmed);
    let err = failure.error;
    expect_failure(&err, &h.batch_file(0));
    assert!(err.message().contains("complete batch file stays in place"));
    let bytes = fs.read(&h.batch_file(0)).unwrap();
    parse_batch(&InputId::new("batch").unwrap(), &bytes).unwrap();
    assert_eq!(
        fs.names_under(&format!("{ROOT}/.tmp")),
        Vec::<String>::new()
    );
}

#[test]
fn a_failed_existence_check_or_directory_creation_stores_nothing() {
    for faults in [
        Faults {
            exists: Some(ErrorKind::PermissionDenied),
            ..Faults::default()
        },
        Faults {
            create_dir: Some(ErrorKind::PermissionDenied),
            ..Faults::default()
        },
        Faults {
            create_temp: Some(ErrorKind::PermissionDenied),
            ..Faults::default()
        },
    ] {
        let h = Harness::new("");
        let fs = MemFs::new();
        fs.with_faults(faults);
        let err = store(&h, &fs, &accepted(&h, 0)).unwrap_err();
        expect_failure(&err, &h.batch_file(0));
        assert!(fs.files().is_empty());
    }
}

#[test]
fn a_failed_removal_after_a_failed_write_is_a_warning_beside_the_error() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        partial_write: Some(1),
        remove: Some(ErrorKind::PermissionDenied),
        ..Faults::default()
    });
    let failure = store_full(&h, &fs, &accepted(&h, 0)).unwrap_err();
    assert_eq!(failure.outcome, FailureOutcome::Refused);
    expect_failure(&failure.error, &h.batch_file(0));
    assert_eq!(
        failure.error.message(),
        "the batch was not stored: writing the temporary file failed (WriteZero)"
    );
    assert_eq!(failure.warnings.len(), 1, "{:?}", failure.warnings);
    expect_temp_warning(
        &failure.warnings[0],
        &tmp_name(&h, 0, ""),
        "PermissionDenied",
    );
}

#[test]
fn a_failed_removal_after_a_failed_link_is_a_warning_beside_the_error() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        links: [Link::Fail(ErrorKind::Unsupported)].into(),
        remove: Some(ErrorKind::Other),
        ..Faults::default()
    });
    let failure = store_full(&h, &fs, &accepted(&h, 0)).unwrap_err();
    expect_failure(&failure.error, &h.batch_file(0));
    assert_eq!(failure.warnings.len(), 1);
    expect_temp_warning(&failure.warnings[0], &tmp_name(&h, 0, ""), "Other");
}

#[test]
fn a_failed_removal_after_the_link_keeps_the_batch_and_warns() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        remove: Some(ErrorKind::PermissionDenied),
        ..Faults::default()
    });
    let report = store_full(&h, &fs, &accepted(&h, 0)).unwrap();
    assert_eq!(report.outcome, StoreOutcome::Stored);
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    expect_temp_warning(
        &report.warnings[0],
        &tmp_name(&h, 0, ""),
        "PermissionDenied",
    );
    let bytes = fs.read(&h.batch_file(0)).unwrap();
    parse_batch(&InputId::new("batch").unwrap(), &bytes).unwrap();
}

#[test]
fn a_failed_removal_after_losing_the_race_is_a_warning_not_an_error() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        links: [Link::Winner(b"winner bytes".to_vec())].into(),
        remove: Some(ErrorKind::PermissionDenied),
        ..Faults::default()
    });
    let report = store_full(&h, &fs, &accepted(&h, 0)).unwrap();
    assert!(matches!(report.outcome, StoreOutcome::AlreadyStored(_)));
    assert_eq!(report.warnings.len(), 1);
    expect_temp_warning(
        &report.warnings[0],
        &tmp_name(&h, 0, ""),
        "PermissionDenied",
    );
    assert_eq!(fs.read(&h.batch_file(0)).unwrap(), b"winner bytes");
}

#[test]
fn a_removal_that_finds_no_file_gives_no_warning() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.with_faults(Faults {
        remove: Some(ErrorKind::NotFound),
        ..Faults::default()
    });
    let report = store_full(&h, &fs, &accepted(&h, 0)).unwrap();
    assert_eq!(report.outcome, StoreOutcome::Stored);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn all_temporary_slots_taken_refuses_without_touching_them() {
    let h = Harness::new("");
    let fs = MemFs::new();
    fs.plant(&tmp_name(&h, 0, ""), b"stale");
    for n in 1..8 {
        fs.plant(&tmp_name(&h, 0, &format!(".{n}")), b"stale");
    }
    let failure = store_full(&h, &fs, &accepted(&h, 0)).unwrap_err();
    expect_failure(&failure.error, &h.batch_file(0));
    assert!(failure.warnings.is_empty());
    let stale: Vec<String> = [""]
        .into_iter()
        .map(str::to_owned)
        .chain((1..8).map(|n| format!(".{n}")))
        .map(|slot| tmp_name(&h, 0, &slot))
        .collect();
    assert_eq!(
        failure.error.message(),
        format!(
            "the batch was not stored: finding a free temporary file name \
             failed (AlreadyExists); if no other run is active, delete \
             these stale temporary files: {}",
            stale.join(", ")
        )
    );
    assert_eq!(fs.files().len(), 8);
    assert!(fs.files().values().all(|b| b == b"stale"));
}

#[test]
fn the_stale_file_hint_escapes_control_characters_of_the_store_path() {
    let h = Harness::new("");
    let fs = MemFs::new();
    let root = "st\nore";
    let id = h.id(0);
    fs.plant(&format!("{root}/.tmp/{id}.tmp"), b"stale");
    for n in 1..8 {
        fs.plant(&format!("{root}/.tmp/{id}.{n}.tmp"), b"stale");
    }
    let failure = store_batch(&fs, Path::new(root), &accepted(&h, 0), &h.meta).unwrap_err();
    assert!(
        failure.error.message().contains("st\\u{a}ore/.tmp/"),
        "{}",
        failure.error.message()
    );
    assert!(!failure.error.message().contains('\n'));
}

#[test]
fn stored_bytes_do_not_depend_on_the_run_that_wrote_them() {
    let h = Harness::new("");
    let first = MemFs::new();
    let second = MemFs::new();
    store(&h, &first, &accepted(&h, 1)).unwrap();
    store(&h, &second, &accepted(&h, 1)).unwrap();
    assert_eq!(first.read(&h.batch_file(1)), second.read(&h.batch_file(1)));
}

// Real files. The gate holds the first two links until both runs have
// written a temporary file, so the second link always meets the batch file
// of the first and the `AlreadyExists` path of the writer runs on a disk.
#[test]
fn concurrent_runs_on_a_real_disk_store_one_batch_and_report_the_rest() {
    let h = Harness::new("");
    let scratch = ScratchDir::new("store-race");
    let dir = scratch.path().to_path_buf();
    let gate = GateFs::new();
    let batch = accepted(&h, 0);
    let results: Vec<Result<StoreOutcome, Diagnostic>> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    store_batch(&gate, &dir, &batch, &h.meta)
                        .map(|report| report.outcome)
                        .map_err(|failure| failure.error)
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    let stored = results
        .iter()
        .filter(|r| matches!(r, Ok(StoreOutcome::Stored)))
        .count();
    let already = results
        .iter()
        .filter(|r| matches!(r, Ok(StoreOutcome::AlreadyStored(w)) if w.code() == Code::AiBatchAlreadyStored))
        .count();
    assert_eq!((stored, already), (1, 7), "{results:?}");
    assert!(gate.link_target_existed() >= 1);
    let name = dir.join("batches").join(format!("{}.json", h.id(0)));
    let bytes = std::fs::read(&name).unwrap();
    parse_batch(&InputId::new("batch").unwrap(), &bytes).unwrap();
    let batches = std::fs::read_dir(dir.join("batches")).unwrap().count();
    let temps = std::fs::read_dir(dir.join(".tmp")).unwrap().count();
    assert_eq!((batches, temps), (1, 0));
}
