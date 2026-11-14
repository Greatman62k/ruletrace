//! Property 9: approval status is a pure function of the Rulebook and the
//! ledger.
//!
//! **Validates: Requirements 5.1, 5.3, 5.16, 25.11, 26.10, 26.11**
//!
//! A Rulebook is generated as text and parsed by `rt-ir`. A ledger is generated
//! as a plan per revision (a matching approval, stale approvals with a wrong
//! content hash, a wrong `ir_version` or a wrong binding, and a rejection),
//! written in canonical form with an independently computed hash chain and
//! verified by `parse_ledger`. An independent oracle states the expected status
//! of every revision from the plan alone. The derived statuses, Effective
//! revisions and unresolved revisions must equal the oracle, be the same when
//! derived twice, and not change when the interleaving of records of different
//! revisions, the Origin of revisions or the order of the revision list
//! changes.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use proptest::collection::vec;
use proptest::prelude::*;
use rt_foundation::InputId;
use rt_hash::sha256;
use rt_ir::{
    parse_rulebook, rule_content_hash, ApprovalStatus, Identity, Origin, RuleRevision, Rulebook,
};
use rt_json::ParseLimits;
use rt_ledger::{
    derive_statuses, effective_revisions, parse_ledger, unresolved, ApprovalLedger, HEADER_LINE,
    ZERO_HASH,
};
use std::collections::{BTreeMap, BTreeSet};

const DOC_A: &str = "eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a";
const DOC_B: &str = "1111111111111111111111111111111111111111111111111111111111111111";

/// What a revision's record plan holds for the slot that uses the true content
/// hash. Only one such record per revision is allowed: a second decision with
/// the same hash is contradictory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrueSlot {
    None,
    Match,
    BadIrVersion,
    BadBinding,
}

#[derive(Clone, Debug)]
struct Revision {
    value: u32,
    /// (use annex document, start, length)
    spans: Vec<(bool, u32, u32)>,
    true_slot: TrueSlot,
    /// Approvals of other (wrong) content hashes, by distinct seed.
    wrong_hashes: u8,
    wrong_first: bool,
    rejected: bool,
}

#[derive(Clone, Debug)]
struct Plan {
    /// One entry per rule, revisions 1.. in order.
    rules: Vec<Vec<Revision>>,
    interleave_seed: u64,
    other_seed: u64,
}

fn revision() -> impl Strategy<Value = Revision> {
    (
        0u32..1000,
        vec((any::<bool>(), 0u32..10_000, 1u32..500), 1..4),
        prop_oneof![
            Just(TrueSlot::None),
            Just(TrueSlot::Match),
            Just(TrueSlot::Match),
            Just(TrueSlot::BadIrVersion),
            Just(TrueSlot::BadBinding),
        ],
        0u8..3,
        any::<bool>(),
        prop_oneof![Just(false), Just(false), Just(true)],
    )
        .prop_map(
            |(value, spans, true_slot, wrong_hashes, wrong_first, rejected)| Revision {
                value,
                spans,
                true_slot,
                wrong_hashes,
                wrong_first,
                rejected,
            },
        )
}

fn plan() -> impl Strategy<Value = Plan> {
    (vec(vec(revision(), 1..4), 1..4), any::<u64>(), any::<u64>()).prop_map(
        |(rules, interleave_seed, other_seed)| Plan {
            rules,
            interleave_seed,
            other_seed,
        },
    )
}

fn rule_id(i: usize) -> String {
    format!("elig.r{i}")
}

fn hex(seed: &str) -> String {
    sha256(seed.as_bytes()).as_str().to_owned()
}

fn rulebook_text(plan: &Plan) -> String {
    let mut revs = Vec::new();
    for (i, rule) in plan.rules.iter().enumerate() {
        for (j, r) in rule.iter().enumerate() {
            let spans: Vec<String> = r
                .spans
                .iter()
                .map(|(annex, start, len)| {
                    format!(
                        r#"{{"document":"{}","start":{start},"end":{},"line":1,"column":1}}"#,
                        if *annex { "annex.md" } else { "rules.md" },
                        start + len
                    )
                })
                .collect();
            revs.push(format!(
                r#"{{"id":"{}","revision":{},"origin":{{"kind":"manual","author":null}},"kind":"eligibility","target":"eligible","type":"boolean","expr":{{"node":"cmp","t":"boolean","op":"ge","left":{{"name":"age","t":"number","node":"field"}},"right":{{"value":"{}","t":"number","node":"num"}}}},"markers":[],"spans":[{}]}}"#,
                rule_id(i),
                j + 1,
                r.value,
                spans.join(",")
            ));
        }
    }
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"spring-open","version":"2026.1","documents":[{{"id":"rules.md","sha256":"{DOC_A}"}},{{"id":"annex.md","sha256":"{DOC_B}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{}],"precedence":[]}}"#,
        revs.join(",")
    )
}

fn parse_book(text: &str) -> Rulebook {
    let input = InputId::new("rulebook.json").expect("input id");
    match parse_rulebook(text.as_bytes(), &input, 8 << 20, 64) {
        Ok(s) => s.into_parts().0,
        Err(r) => panic!(
            "Rulebook rejected: {:?}\n{text}",
            r.into_diagnostics().as_slice()
        ),
    }
}

/// The binding JSON of a revision: sorted, de-duplicated (document, declared
/// hash).
fn binding_json(r: &Revision, corrupt: bool) -> String {
    let docs: BTreeSet<&str> = r
        .spans
        .iter()
        .map(|(a, _, _)| if *a { "annex.md" } else { "rules.md" })
        .collect();
    let items: Vec<String> = docs
        .iter()
        .enumerate()
        .map(|(k, d)| {
            let declared = if *d == "rules.md" { DOC_A } else { DOC_B };
            let sha = if corrupt && k == 0 {
                hex("not-the-declared-hash")
            } else {
                declared.to_owned()
            };
            format!(r#"{{"document":"{d}","sha256":"{sha}"}}"#)
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// One record to be written, without sequence and chain fields.
#[derive(Clone, Debug)]
struct Draft {
    rejection: bool,
    rule: String,
    revision: u32,
    content_hash: String,
    ir_version: &'static str,
    binding: String,
}

/// The records of one revision in their required order. A rejection is last
/// (Rejected is terminal, a later record would be contradictory).
fn drafts_of(book: &Rulebook, i: usize, j: usize, r: &Revision) -> Vec<Draft> {
    let rev = find(book, i, j);
    let true_hash = rule_content_hash(&book.ir_version, rev, &book.documents)
        .expect("hash")
        .as_str()
        .to_owned();
    let make = |rejection: bool, hash: String, ir: &'static str, corrupt: bool| Draft {
        rejection,
        rule: rule_id(i),
        revision: u32::try_from(j + 1).expect("small"),
        content_hash: hash,
        ir_version: ir,
        binding: binding_json(r, corrupt),
    };
    let wrong: Vec<Draft> = (0..r.wrong_hashes)
        .map(|k| make(false, hex(&format!("wrong-{i}-{j}-{k}")), "1", false))
        .collect();
    let slot = match r.true_slot {
        TrueSlot::None => None,
        TrueSlot::Match => Some(make(false, true_hash.clone(), "1", false)),
        TrueSlot::BadIrVersion => Some(make(false, true_hash.clone(), "2", false)),
        TrueSlot::BadBinding => Some(make(false, true_hash.clone(), "1", true)),
    };
    let mut out = Vec::new();
    if r.wrong_first {
        out.extend(wrong.clone());
    }
    out.extend(slot);
    if !r.wrong_first {
        out.extend(wrong);
    }
    if r.rejected {
        out.push(make(true, hex(&format!("rejected-{i}-{j}")), "1", false));
    }
    out
}

fn find(book: &Rulebook, i: usize, j: usize) -> &RuleRevision {
    book.revisions
        .iter()
        .find(|rv| rv.key.id.as_str() == rule_id(i) && rv.key.revision.get() as usize == j + 1)
        .expect("revision")
}

/// Merges the per-revision queues in an order given by `seed`, keeping each
/// queue's order.
fn interleave(queues: Vec<Vec<Draft>>, seed: u64) -> Vec<Draft> {
    let mut order: Vec<usize> = queues
        .iter()
        .enumerate()
        .flat_map(|(q, items)| std::iter::repeat_n(q, items.len()))
        .collect();
    if seed != 0 {
        let mut state = seed;
        for k in (1..order.len()).rev() {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let m = usize::try_from((state >> 33) % (k as u64 + 1)).unwrap_or(0);
            order.swap(k, m);
        }
    }
    let mut iters: Vec<_> = queues.into_iter().map(Vec::into_iter).collect();
    order
        .into_iter()
        .filter_map(|q| iters.get_mut(q).and_then(Iterator::next))
        .collect()
}

fn ledger_bytes(drafts: &[Draft]) -> Vec<u8> {
    let mut prev = ZERO_HASH.to_owned();
    let mut out = String::from(HEADER_LINE);
    out.push('\n');
    for (n, d) in drafts.iter().enumerate() {
        let reason = if d.rejection {
            "\"not accepted\""
        } else {
            "null"
        };
        let head = format!(
            r#"{{"seq":{},"kind":"{}","rule":"{}","revision":{},"content_hash":"{}","ir_version":"{}","binding":{},"reviewer":"A. Reviewer","time":"2026-11-14T09:00:00Z","reason":{},"prev":"{}""#,
            n + 1,
            if d.rejection { "rejection" } else { "approval" },
            d.rule,
            d.revision,
            d.content_hash,
            d.ir_version,
            d.binding,
            reason,
            prev
        );
        let hash = sha256(format!("{head}}}").as_bytes()).as_str().to_owned();
        out.push_str(&format!(r#"{head},"hash":"{hash}"}}"#));
        out.push('\n');
        prev = hash;
    }
    out.into_bytes()
}

fn ledger_for(book: &Rulebook, plan: &Plan, seed: u64) -> ApprovalLedger {
    let mut queues = Vec::new();
    for (i, rule) in plan.rules.iter().enumerate() {
        for (j, r) in rule.iter().enumerate() {
            queues.push(drafts_of(book, i, j, r));
        }
    }
    let bytes = ledger_bytes(&interleave(queues, seed));
    let input = InputId::new("approvals.ledger").expect("input id");
    match parse_ledger(&input, &bytes, ParseLimits::new(1 << 22, 64)) {
        Ok(l) => l,
        Err(e) => panic!("generated ledger rejected: {e:?}"),
    }
}

type Key = (String, u32);

/// The oracle: status of each revision from the plan alone (Req 26.10).
fn oracle(plan: &Plan) -> BTreeMap<Key, ApprovalStatus> {
    let mut out = BTreeMap::new();
    for (i, rule) in plan.rules.iter().enumerate() {
        for (j, r) in rule.iter().enumerate() {
            let status = if r.rejected {
                ApprovalStatus::Rejected
            } else if r.true_slot == TrueSlot::Match {
                ApprovalStatus::Approved
            } else {
                ApprovalStatus::Proposed
            };
            out.insert((rule_id(i), u32::try_from(j + 1).expect("small")), status);
        }
    }
    out
}

fn derived(book: &Rulebook, ledger: &ApprovalLedger) -> BTreeMap<Key, ApprovalStatus> {
    derive_statuses(book, ledger)
        .into_iter()
        .map(|(k, v)| ((k.id.as_str().to_owned(), k.revision.get()), v))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Statuses equal the oracle, deriving twice gives equal results, and the
    /// Effective and unresolved revisions follow from them.
    ///
    /// **Validates: Requirements 5.1, 5.3, 25.11, 26.10, 26.11**
    #[test]
    fn statuses_effective_and_unresolved_follow_the_oracle(p in plan()) {
        let book = parse_book(&rulebook_text(&p));
        let ledger = ledger_for(&book, &p, p.interleave_seed);
        let expected = oracle(&p);

        let statuses = derive_statuses(&book, &ledger);
        prop_assert_eq!(&statuses, &derive_statuses(&book, &ledger));
        prop_assert_eq!(statuses.len(), book.revisions.len());
        prop_assert_eq!(derived(&book, &ledger), expected.clone());

        // Effective revision: the highest Approved revision of each rule; none
        // if no revision of the rule is Approved.
        let effective: BTreeMap<String, Key> = effective_revisions(&book, &statuses)
            .into_iter()
            .map(|(id, k)| (id.as_str().to_owned(), (k.id.as_str().to_owned(), k.revision.get())))
            .collect();
        let mut want_effective: BTreeMap<String, Key> = BTreeMap::new();
        for ((id, rev), s) in &expected {
            if *s == ApprovalStatus::Approved {
                want_effective.insert(id.clone(), (id.clone(), *rev));
            }
        }
        prop_assert_eq!(effective, want_effective);

        // Unresolved: Proposed with no higher Approved or Rejected revision of
        // the same rule.
        let got: Vec<Key> = unresolved(&book, &statuses)
            .into_iter()
            .map(|k| (k.id.as_str().to_owned(), k.revision.get()))
            .collect();
        let mut want: Vec<Key> = expected
            .iter()
            .filter(|((id, rev), s)| {
                **s == ApprovalStatus::Proposed
                    && !expected.iter().any(|((oid, orev), os)| {
                        oid == id
                            && orev > rev
                            && matches!(os, ApprovalStatus::Approved | ApprovalStatus::Rejected)
                    })
            })
            .map(|(k, _)| k.clone())
            .collect();
        want.sort();
        prop_assert_eq!(got, want);
    }

    /// Nothing but the Rulebook content and the ledger decisions changes a
    /// status: another interleaving of the records of different revisions, a
    /// different Origin, different span positions and a different order of the
    /// revision list leave the statuses unchanged.
    ///
    /// **Validates: Requirements 5.3, 5.16, 26.10, 26.11**
    #[test]
    fn no_other_input_changes_the_statuses(
        p in plan(),
        author in "[a-z]{1,8}",
        line in 1u32..10_000,
    ) {
        let book = parse_book(&rulebook_text(&p));
        let base = derived(&book, &ledger_for(&book, &p, p.interleave_seed));

        let other_order = derived(&book, &ledger_for(&book, &p, p.other_seed));
        prop_assert_eq!(&other_order, &base);

        let mut changed = book.clone();
        for rev in &mut changed.revisions {
            rev.origin = Origin::Manual { author: Some(Identity::new(author.clone()).expect("identity")) };
            for span in &mut rev.content.spans {
                span.line = line;
            }
        }
        changed.revisions.reverse();
        let ledger = ledger_for(&book, &p, p.interleave_seed);
        prop_assert_eq!(derived(&changed, &ledger), base.clone());

        // The empty ledger leaves every revision Proposed.
        let empty: BTreeSet<ApprovalStatus> = derived(&book, &ApprovalLedger::empty())
            .into_values()
            .collect();
        prop_assert_eq!(empty, BTreeSet::from([ApprovalStatus::Proposed]));
    }
}
