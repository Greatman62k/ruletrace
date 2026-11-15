//! `review list` before the first approval (user decision on Req 26.6 and
//! 33.7). A missing ledger is an empty, uninitialized ledger for `list`, which
//! then creates and changes nothing. Approve and reject still need
//! `--create-ledger`, and a ledger that exists is verified as before.
//!
//! The expected texts are derived from the requirement. The fixture holds `d.a`
//! revision 1 (Proposed) in the Rulebook and one pending proposal for the new
//! rule `d.b`, whose adoption into an empty ledger gives revision 1.

use super::commit_tests::{adoption_fs, approve, fixture};
use super::fault_tests::FaultFs;
use super::validate_tests::{model_text, rulebook_text, TEXT};
use super::*;

fn list_args() -> Vec<OsString> {
    os(&[
        "review",
        "list",
        "--rulebook",
        "rb.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "model.json",
        "--proposals",
        "store",
    ])
}

/// Operations that change a file or take the exclusive lock.
const WRITING: [&str; 5] = ["create_new", "rename", "remove", "hard_link", "try_lock"];

fn assert_no_write(fs: &FaultFs) {
    let writes: Vec<_> = fs
        .log()
        .into_iter()
        .filter(|op| WRITING.contains(&op.kind))
        .collect();
    assert!(writes.is_empty(), "list wrote: {writes:?}");
}

#[test]
fn list_with_a_missing_ledger_shows_prospective_revisions_and_changes_nothing() {
    let fx = fixture();
    let fs = adoption_fs(&fx, false);
    let before = fs.snapshot();
    let out = run(&list_args(), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert_eq!(out.stderr, "");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with("rule 'd.a' revision 1 status Proposed"),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "pending proposal '{}' for rule 'd.b' prospective revision 1\n",
            fx.proposal_id
        )),
        "{text}"
    );
    assert!(
        text.contains(&format!("  prospective content hash: {}\n", fx.hash)),
        "{text}"
    );
    assert_eq!(fs.snapshot(), before, "no file created or changed");
    assert!(!fs.snapshot().contains_key(Path::new("ledger.jsonl")));
    assert!(!fs
        .snapshot()
        .keys()
        .any(|p| p.to_string_lossy().contains("ledger.jsonl.")));
    assert_no_write(&fs);
}

#[test]
fn list_with_a_missing_ledger_equals_list_with_an_empty_ledger() {
    let fx = fixture();
    let missing = run(&list_args(), &adoption_fs(&fx, false));
    let empty = run(&list_args(), &adoption_fs(&fx, true));
    assert_eq!(missing, empty);
}

#[test]
fn list_without_a_proposal_store_or_pending_proposal_works_on_a_missing_ledger() {
    let fs = FaultFs::with(&[
        ("rb.json", rulebook_text().as_bytes()),
        ("model.json", model_text(TEXT).as_bytes()),
    ]);
    let before = fs.snapshot();
    let out = run(
        &os(&[
            "review",
            "list",
            "--rulebook",
            "rb.json",
            "--ledger",
            "ledger.jsonl",
            "--source-model",
            "model.json",
        ]),
        &fs,
    );
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with("rule 'd.a' revision 1 status Proposed"),
        "{text}"
    );
    assert_eq!(fs.snapshot(), before);
    assert_no_write(&fs);
}

#[test]
fn approve_after_list_gives_the_revision_that_list_displayed() {
    let fx = fixture();
    let fs = adoption_fs(&fx, false);
    let listed = run(&list_args(), &fs);
    assert_eq!(listed.code.value(), 0, "{}", listed.stderr);
    let text = String::from_utf8(listed.stdout).unwrap();
    assert!(text.contains("for rule 'd.b' prospective revision 1\n"));
    let mut args = approve(&fx);
    args.push(OsString::from("--create-ledger"));
    let out = run(&args, &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    let said = String::from_utf8(out.stdout).unwrap();
    assert!(
        said.starts_with("approved rule 'd.b' revision 1 (ledger record "),
        "{said}"
    );
    assert!(fs.snapshot().contains_key(Path::new("ledger.jsonl")));
}

#[test]
fn approve_and_reject_without_create_ledger_still_fail_on_a_missing_ledger() {
    let fx = fixture();
    for verb in ["approve", "reject"] {
        let fs = adoption_fs(&fx, false);
        let before = fs.snapshot();
        let args = super::commit_tests::decide_args(verb, "d.b", &fx.hash, &[]);
        let out = run(&args, &fs);
        assert_eq!(out.code.value(), 2, "{verb}: {}", out.stderr);
        assert!(out.stderr.contains("RT-CLI-0003"), "{verb}: {}", out.stderr);
        assert!(out.stdout.is_empty());
        assert_eq!(fs.snapshot(), before, "{verb} changed a file");
    }
}

/// The diagnostic codes an existing but invalid ledger gives, which this
/// change must not alter.
fn refusal(fs: &FaultFs) -> (u8, String) {
    let out = run(&list_args(), fs);
    (out.code.value(), out.stderr)
}

#[test]
fn list_still_rejects_a_malformed_ledger() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let mut files = fs.snapshot();
    files.insert(PathBuf::from("ledger.jsonl"), b"not a ledger\n".to_vec());
    fs.replace_all(files);
    let before = fs.snapshot();
    let (code, stderr) = refusal(&fs);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("RT-APR-0101"), "{stderr}");
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn list_still_rejects_a_hash_chain_tampered_ledger() {
    let fx = fixture();
    let fs = adoption_fs(&fx, false);
    let mut args = approve(&fx);
    args.push(OsString::from("--create-ledger"));
    assert_eq!(run(&args, &fs).code.value(), 0);
    let mut files = fs.snapshot();
    let ledger = files.get_mut(Path::new("ledger.jsonl")).unwrap();
    let text = String::from_utf8(ledger.clone()).unwrap();
    assert!(text.contains("A. Reviewer"));
    *ledger = text.replace("A. Reviewer", "B. Reviewer").into_bytes();
    fs.replace_all(files);
    let (code, stderr) = refusal(&fs);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("RT-APR-0102"), "{stderr}");
}

#[test]
fn list_treats_an_existing_empty_file_as_malformed_not_missing() {
    // Only NotFound means missing: a zero-byte ledger has no header.
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let mut files = fs.snapshot();
    files.insert(PathBuf::from("ledger.jsonl"), Vec::new());
    fs.replace_all(files);
    let (code, stderr) = refusal(&fs);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("RT-APR-"), "{stderr}");
}
