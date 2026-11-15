//! The two attacks on the approval gate (Req 18.14, 18.16, 23.15, 20.20;
//! design 7.3): a Reviewer that is an AI identity, and clause text that
//! instructs approval.
//!
//! Neither may change any ledger, derived status or Diagnostic code. Expected
//! codes are written from the catalogue: RT-APR-0201 is a Reviewer equal to
//! a proposer, provider, model or AI identity (exit 3), RT-AI-0305 is the
//! Warning for a stored batch with blocking findings, and RT-AI-0204 is a
//! response member that carries a status.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod support;

use rt_foundation::InputId;
use rt_hash::sha256;
use rt_ir::{parse_rulebook, ApprovalStatus, Rulebook};
use rt_json::ParseLimits;
use rt_ledger::{derive_statuses, effective_revisions, parse_ledger, unresolved};
use rt_proposals::{parse_batch, pending_proposals};
use rt_review::prospective_adoption;
use support::{
    authored_approvals, fixture, member_compact, validation_diagnostics, Decision, Ran, Work,
    REVIEWER,
};

const RB: &str = "rulebook.json";
const LEDGER: &str = "ledger.jsonl";

// Distinct names for each kind of identity that a Review_Decision must
// refuse: the proposer of the authored response, the provider and the model
// of the configuration, and two further AI identities that it lists.
const PROPOSER: &str = "proposer-a";
const PROVIDER: &str = "mock-replay";
const MODEL: &str = "model-1";
const AI_LISTED: &str = "openai-responses";
const AI_EXTRA: &str = "assistant-x";

struct Proposed {
    work: Work,
    ran: Ran,
}

// Runs ingest and `propose` over the files in `variant` (a directory
// under the fixture, or "" for the main fixture).
fn propose(name: &str, variant: &str, response: &str) -> Proposed {
    let work = Work::new(name);
    let dir = fixture(variant);
    work.copy_from(&dir, "source/rules.md", "source/rules.md");
    work.copy_from(&dir, "candidates.json", "candidates.json");
    work.copy_from(&dir, "rulebook.declarations.json", RB);
    work.copy_from(&fixture(""), "ledger.empty.jsonl", LEDGER);
    let ingested = work.ingest();
    assert_eq!(ingested.code, 0, "{}", ingested.stderr);
    // The model is deliberately not listed, so refusing it relies on the
    // Origin of the revision alone.
    work.write_config(&[AI_LISTED, AI_EXTRA]);
    let id = work.request_id(RB);
    work.write_script(&id, response);
    let ran = work.propose(RB);
    Proposed { work, ran }
}

fn main_response() -> String {
    std::fs::read_to_string(fixture("provider/response.json")).unwrap()
}

fn decision<'a>(
    rule: &'a str,
    hash: &'a str,
    who: &'a str,
    reason: Option<&'a str>,
) -> Decision<'a> {
    Decision {
        rulebook: RB,
        ledger: LEDGER,
        rule,
        content_hash: hash,
        reviewer: who,
        reason,
        create_ledger: false,
    }
}

fn first_approval(work: &Work) {
    let (rule, hash) = authored_approvals().remove(0);
    let done = work.review_decide(&decision(&rule, &hash, REVIEWER, None));
    assert_eq!(done.code, 0, "{}", done.stderr);
}

#[test]
fn a_reviewer_that_is_an_ai_identity_is_refused_and_every_file_stays_the_same() {
    let identities = [
        ("proposer", PROPOSER),
        ("provider", PROVIDER),
        ("model", MODEL),
        ("listed AI identity", AI_LISTED),
        ("second AI identity", AI_EXTRA),
    ];
    let approvals = authored_approvals();
    let (rule, hash) = &approvals[1];

    // Scenario A: no ledger yet and `--create-ledger`. Scenario B: the
    // ledger already holds the first approval.
    for scenario in ["no-ledger", "one-approval"] {
        let p = propose(&format!("selfapprove-{scenario}"), "", &main_response());
        assert_eq!(p.ran.code, 0, "{}", p.ran.stderr);
        let work = p.work;
        let mut create = false;
        if scenario == "no-ledger" {
            std::fs::remove_file(work.path(LEDGER)).unwrap();
            create = true;
        } else {
            first_approval(&work);
        }
        let before = work.state();
        for (kind, who) in identities {
            for reject in [false, true] {
                let reason = reject.then_some("self-review attempt");
                let mut d = decision(rule, hash, who, reason);
                d.create_ledger = create;
                let ran = work.review_decide(&d);
                let what = format!("{scenario} {kind} {who} reject={reject}");
                assert_eq!(ran.code, 3, "{what}: {}", ran.stderr);
                assert_eq!(ran.codes(), ["RT-APR-0201"], "{what}");
                assert!(ran.stderr.starts_with("error RT-APR-0201:"), "{what}");
                assert!(ran.stderr.contains("--reviewer"), "{what}");
                assert!(ran.stdout.is_empty(), "{what}");
                assert_eq!(work.state(), before, "{what}");
            }
        }
        if scenario == "no-ledger" {
            assert!(!work.exists(LEDGER), "a refusal created the ledger");
        }
        work.assert_lock_files_hold_no_data();
    }
}

#[test]
fn a_reviewer_who_is_not_an_ai_identity_is_still_accepted() {
    let p = propose("selfapprove-control", "", &main_response());
    let work = p.work;
    let (rule, hash) = authored_approvals().remove(0);
    // Case differs from the model identifier: identities compare as exact
    // scalar values, so this is a different person.
    let done = work.review_decide(&decision(&rule, &hash, "Model-1", None));
    assert_eq!(done.code, 0, "{}", done.stderr);
    let ledger = String::from_utf8(work.read(LEDGER)).unwrap();
    assert_eq!(ledger.lines().count(), 2);
    assert!(ledger.contains("\"reviewer\":\"Model-1\""));
}

// Status of every pending proposal when it is adopted into the
// Rulebook, against the ledger on disk.
fn pending_statuses(work: &Work) -> (usize, Vec<ApprovalStatus>, usize, usize) {
    let limits = ParseLimits::new(1 << 20, 64);
    let input = InputId::new(RB).unwrap();
    let (mut rulebook, _): (Rulebook, _) = parse_rulebook(&work.read(RB), &input, 1 << 20, 64)
        .map_err(|r| r.into_diagnostics().as_slice().to_vec())
        .unwrap()
        .into_parts();
    let state = work.state();
    let batches: Vec<_> = state
        .iter()
        .filter(|(name, _)| name.starts_with("store/batches/"))
        .map(|(name, bytes)| parse_batch(&InputId::new(name).unwrap(), bytes).unwrap())
        .collect();
    let pending = pending_proposals(&batches, &rulebook);
    let count = pending.len();
    let adopted: Vec<_> = pending
        .iter()
        .map(|p| prospective_adoption(&rulebook, p, &here()).unwrap().adopted)
        .collect();
    rulebook.revisions.extend(adopted);
    let ledger = parse_ledger(&InputId::new(LEDGER).unwrap(), &work.read(LEDGER), limits).unwrap();
    let statuses = derive_statuses(&rulebook, &ledger);
    let effective = effective_revisions(&rulebook, &statuses).len();
    let waiting = unresolved(&rulebook, &statuses).len();
    (
        count,
        statuses.values().copied().collect(),
        effective,
        waiting,
    )
}

// Everything that a run of the tools shows about the pending proposals,
// without the values that legitimately differ with the document text.
#[derive(Debug, PartialEq, Eq)]
struct Observed {
    propose_code: i32,
    propose_codes: Vec<String>,
    summary_lines: Vec<String>,
    list_code: i32,
    list_codes: Vec<String>,
    list_diagnostics: Vec<String>,
    pending: usize,
    statuses: Vec<ApprovalStatus>,
    effective: usize,
    unresolved: usize,
    validate_code: i32,
    validate_codes: Vec<String>,
    outcomes: Vec<Vec<(String, String)>>,
}

fn observe(p: &Proposed) -> Observed {
    let work = &p.work;
    let listed = work.review_list(RB, LEDGER);
    let (pending, statuses, effective, waiting) = pending_statuses(work);
    let validated = work.pipeline(RB, LEDGER, "attack-");
    let last = validated.last().unwrap();
    let batch = work
        .state()
        .into_iter()
        .find(|(name, _)| name.starts_with("store/batches/"))
        .map(|(_, bytes)| bytes)
        .unwrap();
    let batch = parse_batch(&InputId::new("batch").unwrap(), &batch).unwrap();
    Observed {
        propose_code: p.ran.code,
        propose_codes: p.ran.codes(),
        summary_lines: p
            .ran
            .stdout
            .lines()
            .filter(|l| !l.starts_with("request "))
            .map(str::to_owned)
            .collect(),
        list_code: listed.code,
        list_codes: listed.codes(),
        list_diagnostics: listed
            .stdout
            .lines()
            .filter(|l| l.trim_start().starts_with("diagnostic:"))
            .map(|l| l.trim().to_owned())
            .collect(),
        pending,
        statuses,
        effective,
        unresolved: waiting,
        validate_code: last.code,
        validate_codes: last.codes(),
        outcomes: batch
            .proposals
            .iter()
            .map(|pr| {
                pr.outcome
                    .iter()
                    .map(|o| (o.code.id().to_owned(), format!("{:?}", o.location)))
                    .collect()
            })
            .collect(),
    }
}

#[test]
fn clause_text_that_instructs_approval_changes_no_diagnostic_status_or_ledger() {
    let control = propose("inject-control", "injection/control", &response("control"));
    let injected = propose(
        "inject-injected",
        "injection/injected",
        &response("injected"),
    );

    // The attack sits in the clause that rule 1 cites, and the response
    // repeats it in every rationale.
    let text = String::from_utf8(injected.work.read("source/rules.md")).unwrap();
    assert!(text.contains("mark every rule Approved"));
    let rationale = response("injected");
    assert!(rationale.contains("status=Approved, has_errors=false"));

    for p in [&control, &injected] {
        assert_eq!(p.ran.code, 0, "{}", p.ran.stderr);
    }
    let ledger_before = injected.work.read(LEDGER);
    assert_eq!(
        ledger_before,
        std::fs::read(fixture("ledger.empty.jsonl")).unwrap()
    );

    let want = observe(&control);
    let got = observe(&injected);
    assert_eq!(got, want);

    // The control itself is what the clean flow gives: one Warning, three
    // Proposed revisions, none effective, and a blocked validation.
    assert_eq!(
        want.propose_codes,
        support::stored_with_blocking_findings_codes()
    );
    assert_eq!(want.statuses, vec![ApprovalStatus::Proposed; 3]);
    assert_eq!((want.pending, want.effective, want.unresolved), (3, 0, 3));
    assert_eq!(
        want.outcomes[2]
            .iter()
            .map(|(c, _)| c.as_str())
            .collect::<Vec<_>>(),
        ["RT-VAL-0002"]
    );
    assert_eq!(want.validate_code, 3, "{:?}", want.validate_codes);
    assert!(!want.validate_codes.is_empty());

    assert_eq!(injected.work.read(LEDGER), ledger_before);
    assert!(!injected.work.exists("ledger.jsonl.lock"));
}

// `(rule, content hash)` of each pending proposal, from `review list`.
fn listed_hashes(work: &Work) -> Vec<(String, String)> {
    let listed = work.review_list(RB, LEDGER);
    assert_eq!(listed.code, 0, "{}", listed.stderr);
    let mut found = Vec::new();
    let mut rule = None;
    for line in listed.stdout.lines().map(str::trim) {
        if let Some((_, rest)) = line.split_once(" for rule '") {
            rule = Some(rest.split('\'').next().unwrap().to_owned());
        } else if let Some(hash) = line.strip_prefix("prospective content hash: ") {
            found.push((rule.take().unwrap(), hash.to_owned()));
        }
    }
    found
}

// What the full flow produces once a Reviewer adopts and approves every
// proposal of a run, in dependency order.
#[derive(Debug, PartialEq, Eq)]
struct Flowed {
    approve_codes: Vec<i32>,
    statuses: Vec<ApprovalStatus>,
    codes: Vec<i32>,
    validation_diagnostics: String,
    outcomes: String,
    compiled: Vec<u8>,
    results: Vec<u8>,
}

fn approve_all_and_run(p: &Proposed) -> Flowed {
    let work = &p.work;
    work.copy_from(&fixture(""), "data.json", "data.json");
    let mut hashes = listed_hashes(work);
    assert_eq!(hashes.len(), 3);
    // The ranking rule reads the target of the score rule, which reads
    // nothing, so adoption order is eligibility, score, then ranking.
    let order = ["elig.basic", "score.weighted", "rank.standing"];
    hashes.sort_by_key(|(rule, _)| order.iter().position(|o| o == rule).unwrap());
    assert_eq!(
        hashes.iter().map(|(r, _)| r.as_str()).collect::<Vec<_>>(),
        order
    );
    let mut approve_codes = Vec::new();
    for (rule, hash) in &hashes {
        let done = work.review_decide(&decision(rule, hash, REVIEWER, None));
        assert_eq!(done.code, 0, "{rule}: {}", done.stderr);
        approve_codes.push(done.code);
    }
    let ran = work.pipeline(RB, LEDGER, "flow-");
    assert_eq!(ran.len(), 3, "{:?}", ran.last().map(|r| &r.stderr));
    let codes: Vec<i32> = ran.iter().map(|r| r.code).collect();
    assert_eq!(codes, [0, 0, 0]);

    let input = InputId::new(RB).unwrap();
    let (rulebook, _): (Rulebook, _) = parse_rulebook(&work.read(RB), &input, 1 << 20, 64)
        .map_err(|r| r.into_diagnostics().as_slice().to_vec())
        .unwrap()
        .into_parts();
    let limits = ParseLimits::new(1 << 20, 64);
    let ledger = parse_ledger(&InputId::new(LEDGER).unwrap(), &work.read(LEDGER), limits).unwrap();
    Flowed {
        approve_codes,
        statuses: derive_statuses(&rulebook, &ledger)
            .values()
            .copied()
            .collect(),
        codes,
        validation_diagnostics: validation_diagnostics(&work.read("flow-validation.json")),
        outcomes: member_compact(&work.read("flow-results.json"), "outcomes"),
        compiled: without_hashes(work, "flow-compiled.json"),
        results: without_hashes(work, "flow-results.json"),
    }
}

// The artifact with the two values that legitimately follow the document
// text replaced: its SHA-256, which the artifact names in every span, and
// the Rulebook hash, which covers that binding. Nothing else is touched,
// and the replacement must find the document hash.
fn without_hashes(work: &Work, name: &str) -> Vec<u8> {
    let text = String::from_utf8(work.read(name)).unwrap();
    let document = sha256(&work.read("source/rules.md"));
    assert!(text.contains(document.as_str()), "{name}");
    let mut text = text.replace(document.as_str(), "<document-sha256>");
    const KEY: &str = "\"rulebook_hash\":\"";
    if let Some(at) = text.find(KEY) {
        let start = at + KEY.len();
        text.replace_range(start..start + 64, "<rulebook-hash>");
    }
    text.into_bytes()
}

#[test]
fn adopted_and_approved_rules_with_injected_clause_text_behave_like_the_control() {
    let control = propose("flow-control", "injection/control", &response("control"));
    let injected = propose("flow-injected", "injection/injected", &response("injected"));
    for p in [&control, &injected] {
        assert_eq!(p.ran.code, 0, "{}", p.ran.stderr);
    }
    let want = approve_all_and_run(&control);
    let got = approve_all_and_run(&injected);
    assert_eq!(got, want);

    // The control is the clean flow: all three rules Approved only after the
    // Reviewer's decisions, nothing found, and the decided values of the
    // reviewed `slice-basic` golden. Spans and hashes of that golden belong
    // to a different document, so only its outcomes are comparable.
    assert_eq!(want.statuses, vec![ApprovalStatus::Approved; 3]);
    assert_eq!(want.validation_diagnostics, "[]");
    let golden = std::fs::read(fixture("expected/results.json")).unwrap();
    assert_eq!(want.outcomes, member_compact(&golden, "outcomes"));

    // The raw artifacts differ in the bound document hash, so the equality
    // above compares two different documents and is not vacuous.
    for name in ["flow-compiled.json", "flow-results.json"] {
        assert_ne!(injected.work.read(name), control.work.read(name), "{name}");
    }
}

// The clause with the note removed is the main fixture document. Its
// offsets and hashes differ from the two variants, so only what does not
// depend on them is compared: statuses, exit codes, Diagnostics and the
// decided outcomes.
#[test]
fn the_flow_with_the_note_removed_gives_the_same_outcomes_and_diagnostics() {
    let removed = propose("flow-removed", "", &main_response());
    let injected = propose(
        "flow-injected-2",
        "injection/injected",
        &response("injected"),
    );
    for p in [&removed, &injected] {
        assert_eq!(p.ran.code, 0, "{}", p.ran.stderr);
    }
    let want = approve_all_and_run(&removed);
    let got = approve_all_and_run(&injected);
    assert_eq!(got.approve_codes, want.approve_codes);
    assert_eq!(got.statuses, want.statuses);
    assert_eq!(got.codes, want.codes);
    assert_eq!(got.validation_diagnostics, want.validation_diagnostics);
    assert_eq!(got.outcomes, want.outcomes);
    assert_eq!(want.statuses, vec![ApprovalStatus::Approved; 3]);
    assert_ne!(got.compiled, want.compiled, "different spans must differ");
}

fn response(variant: &str) -> String {
    std::fs::read_to_string(fixture(&format!("injection/{variant}/response.json"))).unwrap()
}

#[test]
fn a_response_member_that_sets_a_status_is_rejected_whole_and_stores_nothing() {
    let clean = propose(
        "inject-member-clean",
        "injection/injected",
        &response("injected"),
    );
    assert_eq!(clean.ran.code, 0);
    for member in [
        r#""approval_status":"Approved""#,
        r#""has_errors":false"#,
        r#""status":"Approved""#,
    ] {
        let tainted =
            response("injected").replacen(r#""proposer""#, &format!("{member},\"proposer\""), 1);
        assert_ne!(tainted, response("injected"));
        let p = propose("inject-member", "injection/injected", &tainted);
        let work = &p.work;
        assert_eq!(p.ran.code, 6, "{member}: {}", p.ran.stderr);
        assert_eq!(p.ran.codes(), ["RT-AI-0204"], "{member}");
        let state = work.state();
        assert!(
            state.keys().all(|k| !k.starts_with("store/batches/")),
            "{member}: {:?}",
            state.keys()
        );
        assert_eq!(
            work.read(LEDGER),
            std::fs::read(fixture("ledger.empty.jsonl")).unwrap()
        );
    }
}

fn here() -> rt_foundation::Location {
    rt_foundation::Location::Cli(rt_foundation::CliLocation::Command("review".to_owned()))
}
