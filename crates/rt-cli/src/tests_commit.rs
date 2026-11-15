//! Tests of the journaled two-file commit, its recovery and the locking of
//! `review approve` and `review reject` on a Pending_Proposal (design §3.8.3;
//! Req 26.19, 26.29 to 26.31, 23.13, 25.1).
//!
//! Every test drives the real `run` entry point over a `FaultFs`. Expected
//! files are byte-exact: either the state before the command or the state of a
//! clean run, whose content is itself checked independently in
//! `clean_adoption_changes_both_files`.

use super::fault_tests::{Fault, FaultFs, OpRecord};
use super::validate_tests::{empty_ledger, model_text, rulebook_text, TEXT, ZERO};
use super::*;
use rt_foundation::{DocumentId, InputId, RevisionNumber, RuleId};
use rt_hash::sha256;
use rt_ir::{draft_hash, parse_draft, parse_rulebook, Identity, Origin};
use rt_json::ParseLimits;
use rt_proposals::{
    pending_proposals, print_batch, proposal_id, Batch, Completion, Evidence, Proposal,
    ProposalKind, RuleDraft, StoredResponse,
};

const TIME: &str = "2026-11-14T09:00:00Z";
const CLAUSE1: &str = "1. A competitor is eligible if age is at least 16 and holds a licence.";
const BATCH_FILE: &str = "store/batches/req.json";

pub(super) struct Fixture {
    pub(super) batch: Vec<u8>,
    pub(super) hash: String,
    pub(super) proposal_id: String,
}

pub(super) fn fixture() -> Fixture {
    let rb = parse_rulebook(
        rulebook_text().as_bytes(),
        &InputId::new("rb.json").unwrap(),
        1 << 22,
        64,
    )
    .unwrap()
    .into_parts()
    .0;
    let start = TEXT.find(CLAUSE1).unwrap();
    let end = start + CLAUSE1.len();
    let json = format!(
        r#"{{"id":"d.b","kind":"derived","target":"b","type":"number","expr":{{"node":"num","t":"number","value":"2"}},"markers":[],"spans":[{{"document":"rules.md","start":{start},"end":{end},"line":3,"column":1}}]}}"#
    );
    let probe = parse_draft(
        json.as_bytes(),
        &InputId::new("d").unwrap(),
        RevisionNumber::new(1).unwrap(),
        rb.revisions[0].origin.clone(),
        1 << 20,
        64,
    )
    .unwrap()
    .into_parts()
    .0
    .content;
    let draft = draft_hash(
        &rb.ir_version,
        &RuleId::new("d.b").unwrap(),
        &probe,
        &rb.documents,
    )
    .unwrap();
    let request_id = sha256(b"request-1");
    let id = proposal_id(&request_id, 0);
    let batch = Batch {
        request_id: request_id.clone(),
        response_sha256: sha256(b"payload"),
        response: StoredResponse {
            completion: Completion::Complete,
            payload: "payload".to_owned(),
        },
        provider: "mock".to_owned(),
        model: "m1".to_owned(),
        adapter_version: "0.1".to_owned(),
        ai_identities: vec![Identity::new("ai-1").unwrap()],
        created_at: "2026-11-14T08:00:00Z".to_owned(),
        proposals: vec![Proposal {
            proposal_id: id.clone(),
            kind: ProposalKind::RuleSuggestion,
            proposer: Identity::new("ai-1").unwrap(),
            rationale: "second rule".to_owned(),
            evidence: vec![Evidence {
                document: DocumentId::new("rules.md").unwrap(),
                sha256: sha256(TEXT.as_bytes()),
                start: u32::try_from(start).unwrap(),
                end: u32::try_from(end).unwrap(),
                line: 3,
                column: 1,
                quote: CLAUSE1.to_owned(),
            }],
            rule: Some((
                RuleDraft {
                    id: RuleId::new("d.b").unwrap(),
                    json,
                },
                draft,
            )),
            outcome: Vec::new(),
        }],
    };
    let pending = pending_proposals(std::slice::from_ref(&batch), &rb);
    let prospective = rt_review::prospective_adoption(&rb, &pending[0], &here()).unwrap();
    Fixture {
        batch: print_batch(&batch).into_bytes(),
        hash: prospective.content_hash.to_string(),
        proposal_id: id.as_str().to_owned(),
    }
}

fn model() -> String {
    model_text(TEXT)
}

pub(super) fn adoption_fs(fx: &Fixture, with_ledger: bool) -> FaultFs {
    let ledger = empty_ledger();
    let model = model();
    let rb = rulebook_text();
    let mut files: Vec<(&str, &[u8])> = vec![
        ("rb.json", rb.as_bytes()),
        ("model.json", model.as_bytes()),
        (BATCH_FILE, fx.batch.as_slice()),
    ];
    if with_ledger {
        files.push(("ledger.jsonl", ledger.as_bytes()));
    }
    FaultFs::with(&files)
}

pub(super) fn decide_args(verb: &str, rule: &str, hash: &str, extra: &[&str]) -> Vec<OsString> {
    let mut a = vec![
        "review",
        verb,
        "--rulebook",
        "rb.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "model.json",
        "--proposals",
        "store",
        "--rule",
        rule,
        "--revision",
        "1",
        "--content-hash",
        hash,
        "--reviewer",
        "A. Reviewer",
        "--time",
        TIME,
    ];
    if verb == "reject" {
        a.extend(["--reason", "wrong threshold"]);
    }
    a.extend_from_slice(extra);
    os(&a)
}

pub(super) fn approve(fx: &Fixture) -> Vec<OsString> {
    decide_args("approve", "d.b", &fx.hash, &[])
}

/// A command that recovers an interrupted commit and then refuses, because the
/// rule does not exist.
pub(super) fn probe() -> Vec<OsString> {
    decide_args("approve", "x.y", ZERO, &[])
}

pub(super) fn commit_leftovers(snapshot: &BTreeMap<PathBuf, Vec<u8>>) -> Vec<String> {
    snapshot
        .keys()
        .map(|p| p.display().to_string())
        .filter(|n| {
            [".new", ".old", ".journal", ".journal.tmp"]
                .iter()
                .any(|s| n.ends_with(s))
        })
        .collect()
}

pub(super) fn pair(snapshot: &BTreeMap<PathBuf, Vec<u8>>) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    (
        snapshot.get(Path::new("ledger.jsonl")).cloned(),
        snapshot.get(Path::new("rb.json")).cloned(),
    )
}

pub(super) fn position(log: &[OpRecord], pred: impl Fn(&OpRecord) -> bool) -> usize {
    log.iter()
        .position(pred)
        .map(|i| i + 1)
        .expect("the operation is in the clean log")
}

pub(super) struct Clean {
    pub(super) before: BTreeMap<PathBuf, Vec<u8>>,
    pub(super) after: BTreeMap<PathBuf, Vec<u8>>,
    pub(super) log: Vec<OpRecord>,
}

pub(super) fn clean_run(fx: &Fixture, with_ledger: bool) -> Clean {
    let fs = adoption_fs(fx, with_ledger);
    let before = fs.snapshot();
    let mut args = approve(fx);
    if !with_ledger {
        args.push("--create-ledger".into());
    }
    let out = run(&args, &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert_eq!(out.stderr, "");
    Clean {
        before,
        after: fs.snapshot(),
        log: fs.log(),
    }
}

pub(super) fn args_for(fx: &Fixture, with_ledger: bool) -> Vec<OsString> {
    let mut args = approve(fx);
    if !with_ledger {
        args.push("--create-ledger".into());
    }
    args
}

#[test]
fn clean_adoption_changes_both_files() {
    let fx = fixture();
    let c = clean_run(&fx, true);
    // Only the two targets differ, and no commit file remains.
    assert!(commit_leftovers(&c.after).is_empty());
    let changed: Vec<&PathBuf> = c
        .after
        .keys()
        .filter(|k| c.before.get(*k) != c.after.get(*k))
        .collect();
    assert_eq!(
        changed,
        vec![Path::new("ledger.jsonl"), Path::new("rb.json")]
    );
    // The ledger is the old file plus exactly one approval line for d.b.
    let old = String::from_utf8(c.before[Path::new("ledger.jsonl")].clone()).unwrap();
    let new = String::from_utf8(c.after[Path::new("ledger.jsonl")].clone()).unwrap();
    let added = new.strip_prefix(&old).expect("the old ledger is a prefix");
    assert_eq!(added.matches('\n').count(), 1);
    let record = rt_json::parse(
        &InputId::new("record").unwrap(),
        added.trim_end().as_bytes(),
        ParseLimits::new(1 << 16, 8),
    )
    .unwrap();
    let text = |key: &str| record.get(key).unwrap().value.as_str().map(str::to_owned);
    assert_eq!(text("kind").as_deref(), Some("approval"));
    assert_eq!(text("rule").as_deref(), Some("d.b"));
    assert_eq!(record.get("revision").unwrap().value.as_u64(), Some(1));
    assert_eq!(text("content_hash").as_deref(), Some(fx.hash.as_str()));
    assert_eq!(text("reviewer").as_deref(), Some("A. Reviewer"));
    assert_eq!(text("time").as_deref(), Some(TIME));
    // The Rulebook keeps d.a and holds d.b revision 1 with the proposal Origin.
    let rb = parse_rulebook(
        &c.after[Path::new("rb.json")],
        &InputId::new("rb.json").unwrap(),
        1 << 22,
        64,
    )
    .unwrap()
    .into_parts()
    .0;
    let ids: Vec<(String, u32)> = rb
        .revisions
        .iter()
        .map(|r| (r.key.id.as_str().to_owned(), r.key.revision.get()))
        .collect();
    assert_eq!(ids, vec![("d.a".to_owned(), 1), ("d.b".to_owned(), 1)]);
    match &rb.revisions[1].origin {
        Origin::AiProposal(o) => assert_eq!(o.proposal_id, fx.proposal_id),
        other => panic!("expected an AI origin, found {other:?}"),
    }
}

#[test]
fn clean_adoption_orders_the_protocol_steps() {
    let fx = fixture();
    let log = clean_run(&fx, true).log;
    let lock = position(&log, |o| o.kind == "try_lock");
    let first_new = position(&log, |o| {
        o.kind == "create_new" && o.path == Path::new("ledger.jsonl.new")
    });
    let backup = position(&log, |o| {
        o.kind == "hard_link" && o.path == Path::new("rb.json")
    });
    let journal = position(&log, |o| {
        o.is_rename("ledger.jsonl.journal.tmp", "ledger.jsonl.journal")
    });
    let ledger = position(&log, |o| o.is_rename("ledger.jsonl.new", "ledger.jsonl"));
    let rulebook = position(&log, |o| o.is_rename("rb.json.new", "rb.json"));
    let drop_journal = position(&log, |o| {
        o.kind == "remove" && o.path == Path::new("ledger.jsonl.journal")
    });
    assert_eq!(lock, 1);
    assert!(lock < first_new && first_new < backup && backup < journal);
    assert!(journal < ledger && ledger < rulebook && rulebook < drop_journal);
}

#[test]
fn in_process_failure_at_every_operation_leaves_both_files_whole() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    let total = clean.log.len();
    let first_new = position(&clean.log, |o| {
        o.kind == "create_new" && o.path == Path::new("ledger.jsonl.new")
    });
    let rulebook = position(&clean.log, |o| o.is_rename("rb.json.new", "rb.json"));
    let publish = position(&clean.log, |o| {
        o.is_rename("ledger.jsonl.journal.tmp", "ledger.jsonl.journal")
    });
    let args = args_for(&fx, true);
    let mut covered = 0;
    for n in 1..=total {
        for fault in [Fault::Fail(n), Fault::Partial(n)] {
            if matches!(fault, Fault::Partial(_)) && clean.log[n - 1].kind != "create_new" {
                continue;
            }
            let fs = adoption_fs(&fx, true);
            fs.set_fault(fault);
            let out = run(&args, &fs);
            let after = fs.snapshot();
            let what = format!("{fault:?} on {:?}: {}", clean.log[n - 1], out.stderr);
            let kind = clean.log[n - 1].kind;
            let first_ledger_read = clean.log[n - 1].kind == "read"
                && clean.log[n - 1].path == Path::new("ledger.jsonl")
                && clean.log[..n - 1]
                    .iter()
                    .all(|o| !(o.kind == "read" && o.path == Path::new("ledger.jsonl")));
            if first_ledger_read {
                // The loader reads the file again to build its diagnostic, and
                // the second read succeeds, so the command completes.
                assert_eq!(out.code.value(), 0, "{what}");
                assert_eq!(after, clean.after, "{what}");
            } else if n < first_new {
                // Before anything is written.
                assert_ne!(out.code.value(), 0, "{what}");
                assert_eq!(after, clean.before, "{what}");
            } else if n <= rulebook + 1 && kind != "hard_link" {
                // A failure before the commit is complete rolls back (26.19).
                assert_eq!(out.code.value(), 70, "{what}");
                assert!(out.stderr.contains("RT-APR-0208"), "{what}");
                // Before the journal nothing was changed; after it the rollback
                // restored both files.
                let words = if n < publish {
                    "no file changed"
                } else {
                    "are unchanged"
                };
                assert!(out.stderr.contains(words), "{what}");
                assert_eq!(after, clean.before, "{what}");
            } else {
                // A failed hard link falls back to a copy; a failed cleanup
                // leaves a complete commit.
                assert_eq!(out.code.value(), 0, "{what}");
                assert_eq!(pair(&after), pair(&clean.after), "{what}");
                if after.contains_key(Path::new("ledger.jsonl.journal")) {
                    assert!(out.stderr.contains("RT-APR-0211"), "{what}");
                }
            }
            covered += 1;
        }
    }
    assert!(covered >= total, "every operation is covered");
}

#[test]
fn crash_after_every_operation_is_recovered_by_the_next_review_command() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    let total = clean.log.len();
    let journal = position(&clean.log, |o| {
        o.is_rename("ledger.jsonl.journal.tmp", "ledger.jsonl.journal")
    });
    let ledger = position(&clean.log, |o| {
        o.is_rename("ledger.jsonl.new", "ledger.jsonl")
    });
    let dropped = position(&clean.log, |o| {
        o.kind == "remove" && o.path == Path::new("ledger.jsonl.journal")
    });
    let args = args_for(&fx, true);
    let creates: Vec<usize> = (1..=total)
        .filter(|n| clean.log[n - 1].kind == "create_new")
        .collect();
    let mut faults: Vec<Fault> = (1..=total).map(Fault::CrashAfter).collect();
    faults.extend(creates.iter().map(|n| Fault::CrashPartial(*n)));
    for fault in faults {
        let (Fault::CrashAfter(n) | Fault::CrashPartial(n)) = fault else {
            unreachable!()
        };
        let fs = adoption_fs(&fx, true);
        fs.set_fault(fault);
        let _ = run(&args, &fs);
        assert!(fs.is_dead(), "{fault:?}: the crash happened");
        // The next process: recovery runs before the refusal of the probe.
        fs.revive();
        let out = run(&probe(), &fs);
        assert_eq!(out.code.value(), 2, "{fault:?}: {}", out.stderr);
        let recovered = fs.snapshot();
        // A crash after the ledger rename is rolled forward, an earlier one
        // leaves the earlier state (the journal, once there, is rolled back).
        let expected = if n >= ledger {
            pair(&clean.after)
        } else {
            pair(&clean.before)
        };
        assert_eq!(pair(&recovered), expected, "{fault:?}");
        if n >= journal {
            assert!(
                commit_leftovers(&recovered).is_empty(),
                "{fault:?}: {:?}",
                commit_leftovers(&recovered)
            );
        }
        // The notice appears exactly when a journal was there to recover.
        let had_journal = n >= journal && n < dropped;
        assert_eq!(
            out.stderr.contains("warning RT-APR-0211"),
            had_journal,
            "{fault:?}: {}",
            out.stderr
        );
        // The same decision again converges on the clean state. A marker torn
        // by the crash cannot be told from a user's file and is refused.
        let again = run(&args, &fs);
        let end = fs.snapshot();
        let torn_marker = matches!(fault, Fault::CrashPartial(_))
            && clean.log[n - 1].path == Path::new("ledger.jsonl.journal.tmp");
        if torn_marker {
            assert_eq!(again.code.value(), 70, "{fault:?}: {}", again.stderr);
            assert!(
                again.stderr.contains("ledger.jsonl.journal.tmp"),
                "{fault:?}: {}",
                again.stderr
            );
            assert_eq!(end, recovered, "{fault:?}");
        } else if n >= ledger {
            assert_ne!(again.code.value(), 0, "{fault:?}: already decided");
            assert_eq!(end, recovered, "{fault:?}");
        } else {
            assert_eq!(again.code.value(), 0, "{fault:?}: {}", again.stderr);
            assert_eq!(end, clean.after, "{fault:?}");
        }
    }
}

#[test]
fn a_held_lock_refuses_with_rt_apr_0212_and_changes_nothing() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let before = fs.snapshot();
    let guard = fs.hold_lock("ledger.jsonl.lock");
    let out = run(&approve(&fx), &fs);
    assert_eq!(out.code.value(), 70, "{}", out.stderr);
    assert!(out.stderr.contains("RT-APR-0212"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), before);
    drop(guard);
    // The lock is released: the same command now succeeds.
    let out = run(&approve(&fx), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    // And it is released again after that command.
    let out = run(&probe(), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
}

#[test]
fn adoption_creates_a_missing_ledger_and_a_crash_after_it_rolls_forward() {
    let fx = fixture();
    let clean = clean_run(&fx, false);
    assert!(!clean.before.contains_key(Path::new("ledger.jsonl")));
    assert!(clean.after.contains_key(Path::new("ledger.jsonl")));
    assert!(commit_leftovers(&clean.after).is_empty());
    let ledger = position(&clean.log, |o| {
        o.is_rename("ledger.jsonl.new", "ledger.jsonl")
    });
    let rulebook = position(&clean.log, |o| o.is_rename("rb.json.new", "rb.json"));
    // A failure at the Rulebook rename removes the ledger that was created.
    let fs = adoption_fs(&fx, false);
    fs.set_fault(Fault::Fail(rulebook));
    let out = run(&args_for(&fx, false), &fs);
    assert_eq!(out.code.value(), 70, "{}", out.stderr);
    assert_eq!(fs.snapshot(), clean.before);
    // A crash after the ledger rename is rolled forward by the next command.
    let fs = adoption_fs(&fx, false);
    fs.set_fault(Fault::CrashAfter(ledger));
    let _ = run(&args_for(&fx, false), &fs);
    fs.revive();
    let mut next = probe();
    next.push("--create-ledger".into());
    let out = run(&next, &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert_eq!(fs.snapshot(), clean.after);
}

#[test]
fn a_cleanup_failure_after_the_commit_is_reported_and_finished_later() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    let drop_journal = position(&clean.log, |o| {
        o.kind == "remove" && o.path == Path::new("ledger.jsonl.journal")
    });
    let fs = adoption_fs(&fx, true);
    fs.set_fault(Fault::Fail(drop_journal));
    let out = run(&approve(&fx), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert!(out.stderr.contains("warning RT-APR-0211"), "{}", out.stderr);
    assert!(
        out.stderr.contains("the commit completed"),
        "{}",
        out.stderr
    );
    assert_eq!(pair(&fs.snapshot()), pair(&clean.after));
    assert!(fs
        .snapshot()
        .contains_key(Path::new("ledger.jsonl.journal")));
    fs.revive();
    let out = run(&probe(), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert_eq!(fs.snapshot(), clean.after);
}

#[test]
fn a_failed_hard_link_falls_back_to_a_copy_and_the_commit_succeeds() {
    let fx = fixture();
    let clean = clean_run(&fx, true);
    let link = position(&clean.log, |o| o.kind == "hard_link");
    let fs = adoption_fs(&fx, true);
    fs.set_fault(Fault::Fail(link));
    let out = run(&approve(&fx), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    assert_eq!(fs.snapshot(), clean.after);
}

pub(super) fn journal_after_crash(
    fx: &Fixture,
    after: impl Fn(&OpRecord) -> bool,
) -> (FaultFs, Clean) {
    let clean = clean_run(fx, true);
    let n = position(&clean.log, after);
    let fs = adoption_fs(fx, true);
    fs.set_fault(Fault::CrashAfter(n));
    let _ = run(&approve(fx), &fs);
    fs.revive();
    (fs, clean)
}

pub(super) fn with_journal_text(fs: &FaultFs, edit: impl Fn(String) -> String) {
    let mut files = fs.snapshot();
    let key = PathBuf::from("ledger.jsonl.journal");
    let text = String::from_utf8(files[&key].clone()).unwrap();
    files.insert(key, edit(text).into_bytes());
    fs.replace_all(files);
}

pub(super) fn refused_with_0208(fs: &FaultFs, label: &str) {
    let frozen = fs.snapshot();
    let out = run(&probe(), fs);
    assert_eq!(out.code.value(), 70, "{label}: {}", out.stderr);
    assert!(
        out.stderr.contains("RT-APR-0208"),
        "{label}: {}",
        out.stderr
    );
    assert_eq!(fs.snapshot(), frozen, "{label}: no file changed");
}

#[test]
fn a_malformed_journal_is_refused_and_nothing_changes() {
    let fx = fixture();
    let (fs, _) = journal_after_crash(&fx, |o| {
        o.is_rename("ledger.jsonl.journal.tmp", "ledger.jsonl.journal")
    });
    with_journal_text(&fs, |t| t[..t.len() / 2].to_owned());
    refused_with_0208(&fs, "truncated journal");
}

#[test]
fn a_journal_naming_other_files_is_refused_and_nothing_changes() {
    let fx = fixture();
    let (fs, _) = journal_after_crash(&fx, |o| {
        o.is_rename("ledger.jsonl.journal.tmp", "ledger.jsonl.journal")
    });
    with_journal_text(&fs, |t| t.replace("\"rb.json\"", "\"other.json\""));
    refused_with_0208(&fs, "foreign path");
}

#[test]
fn a_new_ledger_with_an_old_rulebook_and_no_new_file_is_rolled_back_to_the_old_pair() {
    let fx = fixture();
    let (fs, clean) = journal_after_crash(&fx, |o| o.is_rename("ledger.jsonl.new", "ledger.jsonl"));
    // Without the new Rulebook file the commit cannot be completed. The same
    // state is what an interrupted rollback leaves, and the verified ledger
    // backup restores the old pair; nothing is guessed (Req 26.19).
    let mut files = fs.snapshot();
    files.remove(Path::new("rb.json.new"));
    fs.replace_all(files);
    let out = run(&probe(), &fs);
    assert_eq!(out.code.value(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("warning RT-APR-0211"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), clean.before);
}

#[test]
fn a_new_rulebook_file_that_differs_from_the_journal_is_not_rolled_forward() {
    let fx = fixture();
    let (fs, _) = journal_after_crash(&fx, |o| o.is_rename("ledger.jsonl.new", "ledger.jsonl"));
    let mut files = fs.snapshot();
    files.insert(PathBuf::from("rb.json.new"), b"{}".to_vec());
    fs.replace_all(files);
    refused_with_0208(&fs, "tampered new Rulebook file");
}

#[test]
fn a_rejection_adopts_the_proposal_as_rejected_through_the_same_commit() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let out = run(&decide_args("reject", "d.b", &fx.hash, &[]), &fs);
    assert_eq!(out.code.value(), 0, "{}", out.stderr);
    let after = fs.snapshot();
    assert!(commit_leftovers(&after).is_empty());
    let ledger = String::from_utf8(after[Path::new("ledger.jsonl")].clone()).unwrap();
    assert!(ledger.contains(r#""kind":"rejection""#), "{ledger}");
    let rb = String::from_utf8(after[Path::new("rb.json")].clone()).unwrap();
    assert!(rb.contains(&fx.proposal_id), "the rejected content is kept");
}

#[test]
fn a_refusal_before_the_commit_leaves_every_file_byte_identical() {
    let fx = fixture();
    let fs = adoption_fs(&fx, true);
    let before = fs.snapshot();
    let stale = "1".repeat(64);
    let out = run(&decide_args("approve", "d.b", &stale, &[]), &fs);
    assert_eq!(out.code.value(), 3, "{}", out.stderr);
    assert!(out.stderr.contains("RT-APR-0203"), "{}", out.stderr);
    assert_eq!(fs.snapshot(), before);
}

fn here() -> rt_foundation::Location {
    rt_foundation::Location::Cli(rt_foundation::CliLocation::Command("review".to_owned()))
}
