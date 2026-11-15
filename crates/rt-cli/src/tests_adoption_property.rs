//! Property 16: adoption is all or nothing (design §5; Req 25.1, 26.19,
//! 26.30, 26.31, 23.13; policy sections 2, 5, 6, 7 and 10).
//!
//! Each generated case builds a proposal (new rule or next revision of an
//! existing rule, approve or reject, with or without a ledger), then injects
//! one or two faults of any kind at generated operation numbers of the real
//! `run` over a `FaultFs`. Two faults model a failed rollback or a crash
//! during one. The expected states are the bytes before the command (the old
//! pair) and the bytes of a clean run, which is itself checked field by field
//! against facts derived from the generated parameters, not from the commit
//! code. The generator seed has a fixed default and is printed on every run.

use super::commit_recovery_tests::readers;
use super::commit_tests::{
    adoption_fs, commit_leftovers, decide_args, pair, probe, Clean, Fixture,
};
use super::fault_tests::{Fault, FaultFs, OpRecord};
use super::validate_tests::{rulebook_text, TEXT};
use super::*;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestCaseError, TestRng, TestRunner};
use rt_foundation::{DocumentId, InputId, RevisionNumber, RuleId};
use rt_hash::sha256;
use rt_ir::{draft_hash, parse_draft, parse_rulebook, print_pretty, Identity, Origin};
use rt_proposals::{
    pending_proposals, print_batch, proposal_id, Batch, Completion, Evidence, Proposal,
    ProposalKind, RuleDraft, StoredResponse,
};

const CASES: u32 = 512;
const SEED: [u8; 32] = *b"ruletrace-property-16-seed-0001!";
const JOURNAL: &str = "ledger.jsonl.journal";
const CLAUSE1: &str = "1. A competitor is eligible if age is at least 16 and holds a licence.";

#[derive(Clone, Debug)]
struct Params {
    existing_rule: bool,
    value: u32,
    reject: bool,
    with_ledger: bool,
}

impl Params {
    fn rule(&self) -> &'static str {
        if self.existing_rule {
            "d.a"
        } else {
            "d.b"
        }
    }

    fn revision(&self) -> u32 {
        if self.existing_rule {
            2
        } else {
            1
        }
    }

    fn verb(&self) -> &'static str {
        if self.reject {
            "reject"
        } else {
            "approve"
        }
    }
}

/// A fault kind and a raw position that is reduced to a valid operation
/// number once the length of a clean run is known.
type RawFault = (u8, u16);

fn resolve(raw: RawFault, span: usize) -> Fault {
    let n = usize::from(raw.1) % span + 1;
    match raw.0 % 4 {
        0 => Fault::Fail(n),
        1 => Fault::Partial(n),
        2 => Fault::CrashAfter(n),
        _ => Fault::CrashPartial(n),
    }
}

/// The old Rulebook and the draft hash of the generated rule, computed from the
/// generated JSON and not from the adopted file.
struct Built {
    old: rt_ir::Rulebook,
    draft: rt_hash::Sha256Hex,
}

fn build(p: &Params) -> (Fixture, Built) {
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
    let target = if p.existing_rule { "a" } else { "b" };
    let json = format!(
        r#"{{"id":"{}","kind":"derived","target":"{target}","type":"number","expr":{{"node":"num","t":"number","value":"{}"}},"markers":[],"spans":[{{"document":"rules.md","start":{start},"end":{end},"line":3,"column":1}}]}}"#,
        p.rule(),
        p.value
    );
    let rule = RuleId::new(p.rule()).unwrap();
    let content = parse_draft(
        json.as_bytes(),
        &InputId::new("d").unwrap(),
        RevisionNumber::new(p.revision()).unwrap(),
        rb.revisions[0].origin.clone(),
        1 << 20,
        64,
    )
    .unwrap()
    .into_parts()
    .0
    .content;
    let draft = draft_hash(&rb.ir_version, &rule, &content, &rb.documents).unwrap();
    let request_id = sha256(format!("request-{}", p.value).as_bytes());
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
            rationale: "generated rule".to_owned(),
            evidence: vec![Evidence {
                document: DocumentId::new("rules.md").unwrap(),
                sha256: sha256(TEXT.as_bytes()),
                start: u32::try_from(start).unwrap(),
                end: u32::try_from(end).unwrap(),
                line: 3,
                column: 1,
                quote: CLAUSE1.to_owned(),
            }],
            rule: Some((RuleDraft { id: rule, json }, draft.clone())),
            outcome: Vec::new(),
        }],
    };
    let pending = pending_proposals(std::slice::from_ref(&batch), &rb);
    let hash = rt_review::prospective_adoption(&rb, &pending[0], &here())
        .unwrap()
        .content_hash
        .to_string();
    let fx = Fixture {
        batch: print_batch(&batch).into_bytes(),
        hash,
        proposal_id: id.as_str().to_owned(),
    };
    (fx, Built { old: rb, draft })
}

fn command(p: &Params, fx: &Fixture) -> Vec<OsString> {
    let mut args = decide_args(p.verb(), p.rule(), &fx.hash, &[]);
    let at = args.iter().position(|a| a == "--revision").unwrap();
    args[at + 1] = p.revision().to_string().into();
    if !p.with_ledger {
        args.push("--create-ledger".into());
    }
    args
}

/// The reference run: the same command with no fault, which must succeed.
fn clean_for(p: &Params, fx: &Fixture) -> Clean {
    let fs = adoption_fs(fx, p.with_ledger);
    let before = fs.snapshot();
    let out = run(&command(p, fx), &fs);
    assert_eq!(out.code.value(), 0, "{p:?}: {}", out.stderr);
    assert_eq!(out.stderr, "");
    Clean {
        before,
        after: fs.snapshot(),
        log: fs.log(),
    }
}

fn recovery_probe(p: &Params) -> Vec<OsString> {
    let mut args = probe();
    if !p.with_ledger {
        args.push("--create-ledger".into());
    }
    args
}

macro_rules! ensure {
    ($cond:expr, $($msg:tt)+) => {
        if !$cond {
            return Err(format!($($msg)+));
        }
    };
}

fn text(record: &rt_json::Value<rt_json::Position>, key: &str) -> Option<String> {
    record.get(key)?.value.as_str().map(str::to_owned)
}

/// Facts the new pair must satisfy, taken from the generated parameters: one
/// added ledger line of the right kind for the right revision, and revision
/// numbers 1..n without a gap for every rule.
fn check_new_pair(p: &Params, fx: &Fixture, built: &Built, clean: &Clean) -> Result<(), String> {
    let old = &built.old;
    let new_ledger = String::from_utf8(clean.after[Path::new("ledger.jsonl")].clone()).unwrap();
    let prefix = if p.with_ledger {
        String::from_utf8(clean.before[Path::new("ledger.jsonl")].clone()).unwrap()
    } else {
        format!("{}\n", rt_ledger::HEADER_LINE)
    };
    let Some(added) = new_ledger.strip_prefix(&prefix) else {
        return Err("the old ledger is not a prefix of the new one".to_owned());
    };
    ensure!(
        added.matches('\n').count() == 1 && added.ends_with('\n'),
        "exactly one record line is added: {added:?}"
    );
    let record = rt_json::parse(
        &InputId::new("record").unwrap(),
        added.trim_end().as_bytes(),
        rt_json::ParseLimits::new(1 << 16, 8),
    )
    .map_err(|_| "the added line is not JSON".to_owned())?;
    let kind = if p.reject { "rejection" } else { "approval" };
    ensure!(
        text(&record, "kind").as_deref() == Some(kind),
        "record kind"
    );
    ensure!(
        text(&record, "rule").as_deref() == Some(p.rule()),
        "record rule"
    );
    ensure!(
        record.get("revision").and_then(|r| r.value.as_u64()) == Some(u64::from(p.revision())),
        "record revision"
    );
    ensure!(
        text(&record, "content_hash").as_deref() == Some(fx.hash.as_str()),
        "record content hash"
    );
    let rb = parse_rulebook(
        &clean.after[Path::new("rb.json")],
        &InputId::new("rb.json").unwrap(),
        1 << 22,
        64,
    )
    .map_err(|_| "the new Rulebook does not parse".to_owned())?
    .into_parts()
    .0;
    ensure!(
        rb.revisions.len() == old.revisions.len() + 1,
        "exactly one revision is added"
    );
    let mut per_rule: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for r in &rb.revisions {
        per_rule
            .entry(r.key.id.as_str().to_owned())
            .or_default()
            .push(r.key.revision.get());
    }
    for (rule, mut numbers) in per_rule {
        numbers.sort_unstable();
        let expected: Vec<u32> = (1..=u32::try_from(numbers.len()).unwrap()).collect();
        ensure!(numbers == expected, "rule {rule}: revisions {numbers:?}");
    }
    let added_rev = rb
        .revisions
        .iter()
        .find(|r| r.key.id.as_str() == p.rule() && r.key.revision.get() == p.revision());
    let Some(added_rev) = added_rev else {
        return Err("the adopted revision is missing".to_owned());
    };
    match &added_rev.origin {
        Origin::AiProposal(o) if o.proposal_id == fx.proposal_id => {}
        other => return Err(format!("the adopted revision origin: {other:?}")),
    }
    let adopted = draft_hash(
        &rb.ir_version,
        &added_rev.key.id,
        &added_rev.content,
        &rb.documents,
    )
    .map_err(|_| "the adopted content cannot be hashed".to_owned())?;
    ensure!(
        adopted == built.draft,
        "the adopted content differs from the generated draft"
    );
    check_earlier_revisions_unchanged(old, &rb, added_rev)
}

/// Every revision of the old Rulebook is in the new one with the same key,
/// origin and content, and the new Rulebook without the adopted revision
/// prints to the same bytes as the old one.
fn check_earlier_revisions_unchanged(
    old: &rt_ir::Rulebook,
    new: &rt_ir::Rulebook,
    added: &rt_ir::RuleRevision,
) -> Result<(), String> {
    for before in &old.revisions {
        let found = new.revisions.iter().find(|r| r.key == before.key);
        ensure!(
            found.is_some_and(|r| r.origin == before.origin && r.content == before.content),
            "earlier revision {:?} changed or vanished",
            before.key
        );
    }
    let mut rest = new.clone();
    rest.revisions.retain(|r| r.key != added.key);
    let (Ok(old_bytes), Ok(rest_bytes)) = (print_pretty(old), print_pretty(&rest)) else {
        return Err("a Rulebook cannot be printed".to_owned());
    };
    ensure!(
        old_bytes == rest_bytes,
        "the new Rulebook differs from the old one beyond the adopted revision"
    );
    Ok(())
}

type Files = BTreeMap<PathBuf, Vec<u8>>;

fn has_journal(files: &Files) -> bool {
    files.contains_key(Path::new(JOURNAL))
}

/// While a journal exists the pair may be mixed, so every reader must refuse
/// and change nothing; without one the pair must be whole.
fn observe(fs: &FaultFs, old: &Files, new: &Files, when: &str) -> Result<(), String> {
    let state = fs.snapshot();
    // Req 26.30: a Rulebook that holds the adopted revision never meets a
    // ledger without its record, journal or not.
    let (ledger, rulebook) = pair(&state);
    ensure!(
        rulebook != pair(new).1 || ledger == pair(new).0,
        "{when}: the new Rulebook stands with a ledger that lacks its record"
    );
    if has_journal(&state) {
        for (name, args) in readers() {
            fs.revive();
            let out = run(&args, fs);
            ensure!(
                out.code.value() == 2 && out.stderr.starts_with("error RT-APR-0214:"),
                "{when}: reader {name} did not refuse: {}",
                out.stderr
            );
            ensure!(fs.snapshot() == state, "{when}: reader {name} wrote");
        }
        return Ok(());
    }
    let now = pair(&state);
    ensure!(
        now == pair(old) || now == pair(new),
        "{when}: mixed pair without a journal"
    );
    Ok(())
}

#[derive(Default)]
struct Tally {
    ended_new: Cell<u32>,
    ended_old: Cell<u32>,
    journal_seen: Cell<u32>,
    double: Cell<u32>,
    // Faults that fired, by kind: fail, partial, crash, crash partial.
    fired: Cell<[u32; 4]>,
    recovered_old: Cell<u32>,
    recovered_new: Cell<u32>,
    double_rolled_back: Cell<u32>,
    double_recovered: Cell<u32>,
}

/// The operation number a fault is armed for.
fn at(fault: Fault) -> usize {
    match fault {
        Fault::None => 0,
        Fault::Fail(n) | Fault::Partial(n) | Fault::CrashAfter(n) | Fault::CrashPartial(n) => n,
    }
}

/// True for an operation that only a rollback or a recovery performs: a
/// restore from a backup, or the removal of a ledger the commit created.
fn is_restore(op: &OpRecord) -> bool {
    let from_backup = op.kind == "rename" && op.path.to_string_lossy().ends_with(".old");
    let created_ledger = op.kind == "remove" && op.path == Path::new("ledger.jsonl");
    from_backup || created_ledger
}

fn bump(cell: &Cell<u32>) {
    cell.set(cell.get() + 1);
}

fn case(p: &Params, raw: &[RawFault], tally: &Tally) -> Result<(), String> {
    let (fx, built) = build(p);
    let clean = clean_for(p, &fx);
    let (old, new) = (&clean.before, &clean.after);
    ensure!(pair(old) != pair(new), "the command changes the pair");
    check_new_pair(p, &fx, &built, &clean)?;
    let span = clean.log.len();
    let faults: Vec<Fault> = raw
        .iter()
        .enumerate()
        .map(|(i, r)| resolve(*r, span + 12 * i))
        .collect();
    let fs = adoption_fs(&fx, p.with_ledger);
    fs.set_fault(faults[0]);
    for f in &faults[1..] {
        fs.add_fault(*f);
        bump(&tally.double);
    }
    let args = command(p, &fx);
    let first = run(&args, &fs);
    let label = format!("{faults:?}");
    let ops_run = fs.log();
    let mut fired = tally.fired.get();
    for (fault, r) in faults.iter().zip(raw) {
        if at(*fault) <= ops_run.len() {
            fired[usize::from(r.0 % 4)] += 1;
        }
    }
    tally.fired.set(fired);
    let reached_rollback = ops_run.iter().any(is_restore);
    fs.revive();
    let after_fault = fs.snapshot();
    if first.code.value() == 0 {
        ensure!(
            pair(&after_fault) == pair(new),
            "{label}: success without the new pair"
        );
    }
    if has_journal(&after_fault) {
        bump(&tally.journal_seen);
    }
    observe(&fs, old, new, &format!("{label}: after the faulted run"))?;

    let recovery = run(&recovery_probe(p), &fs);
    let recovered = fs.snapshot();
    if pair(&recovered) == pair(old) {
        bump(&tally.recovered_old);
    } else if pair(&recovered) == pair(new) {
        bump(&tally.recovered_new);
    }
    if faults.len() > 1 {
        if reached_rollback {
            bump(&tally.double_rolled_back);
        }
        if has_journal(&after_fault) && recovered != after_fault {
            bump(&tally.double_recovered);
        }
    }
    let refused = recovery.code.value() == 70;
    if refused {
        ensure!(recovered == after_fault, "{label}: a refusal changed files");
    } else {
        ensure!(
            recovery.code.value() == 2,
            "{label}: recovery exit {}: {}",
            recovery.code.value(),
            recovery.stderr
        );
        ensure!(!has_journal(&recovered), "{label}: journal survived");
    }
    observe(&fs, old, new, &format!("{label}: after recovery"))?;

    let again = run(&args, &fs);
    let end = fs.snapshot();
    if pair(&recovered) == pair(new) {
        ensure!(again.code.value() != 0, "{label}: decided twice");
        ensure!(end == recovered, "{label}: a repeat changed files");
        bump(&tally.ended_new);
    } else {
        ensure!(
            pair(&recovered) == pair(old),
            "{label}: recovered a mixed pair"
        );
        if again.code.value() == 0 {
            ensure!(end == *new, "{label}: the retry differs from a clean run");
            ensure!(commit_leftovers(&end).is_empty(), "{label}: leftovers");
            bump(&tally.ended_new);
        } else {
            ensure!(end == recovered, "{label}: a refused retry changed files");
            bump(&tally.ended_old);
        }
    }
    Ok(())
}

fn strategy() -> impl Strategy<Value = (Params, Vec<RawFault>)> {
    let params = (any::<bool>(), 2u32..900, any::<bool>(), any::<bool>()).prop_map(
        |(existing_rule, value, reject, with_ledger)| Params {
            existing_rule,
            value,
            reject,
            with_ledger,
        },
    );
    let fault = (any::<u8>(), any::<u16>());
    let faults = prop_oneof![
        3 => proptest::collection::vec(fault, 1..=1),
        2 => proptest::collection::vec(fault, 2..=2),
    ];
    (params, faults)
}

/// The generator seed. It is fixed so that a run reproduces; vary it by
/// changing the `SEED` constant.
fn seed() -> ([u8; 32], String) {
    (SEED, "fixed".to_string())
}

/// Property 16 over generated proposals and one or two generated faults.
///
/// **Validates: Requirements 25.1, 26.19, 26.30, 26.31, 23.13**
#[test]
fn adoption_is_all_or_nothing_under_generated_faults() {
    let tally = Tally::default();
    let config = Config {
        cases: CASES,
        failure_persistence: None,
        ..Config::default()
    };
    let (seed, origin) = seed();
    eprintln!("property 16 seed: {origin} ({seed:?})");
    let rng = TestRng::from_seed(RngAlgorithm::ChaCha, &seed);
    let mut runner = TestRunner::new_with_rng(config, rng);
    let result = runner.run(&strategy(), |(p, raw)| {
        case(&p, &raw, &tally).map_err(TestCaseError::fail)
    });
    if let Err(failure) = result {
        panic!("seed {origin} ({seed:?}): {failure}");
    }
    eprintln!(
        "property 16 tally: new {} old {} recovered old {} new {} journal {} double {} \
         rollback {} recovery {} fired {:?}",
        tally.ended_new.get(),
        tally.ended_old.get(),
        tally.recovered_old.get(),
        tally.recovered_new.get(),
        tally.journal_seen.get(),
        tally.double.get(),
        tally.double_rolled_back.get(),
        tally.double_recovered.get(),
        tally.fired.get()
    );
    // Floors sit well below the counts of the default seed (and of any seed
    // seen so far) yet above what a dead code path could reach.
    let floors = [
        ("ended on the new pair", tally.ended_new.get(), 200),
        ("ended on the old pair", tally.ended_old.get(), 1),
        ("recovered to the old pair", tally.recovered_old.get(), 100),
        ("recovered to the new pair", tally.recovered_new.get(), 40),
        (
            "left a journal that readers refused",
            tally.journal_seen.get(),
            40,
        ),
        ("injected two faults", tally.double.get(), 100),
        (
            "double fault reached a rollback",
            tally.double_rolled_back.get(),
            2,
        ),
        (
            "double fault reached a recovery",
            tally.double_recovered.get(),
            15,
        ),
    ];

    for (what, count, floor) in floors {
        assert!(
            count >= floor,
            "seed {origin}: {count} cases {what}, need {floor}"
        );
    }
    for (kind, count) in ["fail", "partial", "crash", "crash partial"]
        .iter()
        .zip(tally.fired.get())
    {
        assert!(count >= 50, "seed {origin}: {count} fired {kind} faults");
    }
}

/// The oracle itself: a mixed pair without a journal is rejected, and a mixed
/// pair under a journal is rejected unless readers refuse.
#[test]
fn the_state_check_rejects_a_mixed_pair_and_an_unguarded_journal() {
    let p = Params {
        existing_rule: false,
        value: 2,
        reject: false,
        with_ledger: true,
    };
    let (fx, _) = build(&p);
    let clean = clean_for(&p, &fx);
    let mut stale_ledger = clean.after.clone();
    stale_ledger.insert(
        PathBuf::from("ledger.jsonl"),
        clean.before[Path::new("ledger.jsonl")].clone(),
    );
    let fs = FaultFs::from_map(stale_ledger);
    assert_eq!(
        observe(&fs, &clean.before, &clean.after, "stale"),
        Err("stale: the new Rulebook stands with a ledger that lacks its record".to_owned())
    );
    let mut mixed = clean.before.clone();
    mixed.insert(
        PathBuf::from("ledger.jsonl"),
        clean.after[Path::new("ledger.jsonl")].clone(),
    );
    let fs = FaultFs::from_map(mixed);
    let verdict = observe(&fs, &clean.before, &clean.after, "mixed");
    assert_eq!(
        verdict,
        Err("mixed: mixed pair without a journal".to_owned())
    );
    let fs = FaultFs::from_map(clean.before.clone());
    assert_eq!(observe(&fs, &clean.before, &clean.after, "old"), Ok(()));
    let fs = FaultFs::from_map(clean.after.clone());
    assert_eq!(observe(&fs, &clean.before, &clean.after, "new"), Ok(()));
}

fn here() -> rt_foundation::Location {
    rt_foundation::Location::Cli(rt_foundation::CliLocation::Command("review".to_owned()))
}
