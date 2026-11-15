//! `StoreBatches::from_batches`: how stored proposal batches reach the
//! Coverage_Statement (design §8.11.8, rows 3 and 4).
//!
//! The inputs are the blessed Proposal_Store batches and Rulebook of the `ai`
//! demonstration. Expected findings are written out here, not read back from
//! the generator.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use rt_demo::coverage::{PendingEntry, StoreBatches, StoreFinding};
use rt_foundation::{Code, InputId};
use rt_ir::{parse_rulebook, Rulebook};
use rt_proposals::{parse_batch, Batch};
use std::path::{Path, PathBuf};

fn art(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("demo")
        .join("stand-in-ai")
        .join("golden")
        .join("artifacts")
        .join(rel)
}

fn batches() -> Vec<Batch> {
    let dir = art("proposal-store/batches");
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            parse_batch(
                &InputId::new("batch.json").unwrap(),
                &std::fs::read(p).unwrap(),
            )
            .map_err(|d| d[0].message().to_owned())
            .unwrap()
        })
        .collect()
}

fn rulebook(name: &str) -> Rulebook {
    let bytes = std::fs::read(name).unwrap();
    parse_rulebook(&bytes, &InputId::new("rulebook.json").unwrap(), 8 << 20, 64)
        .map_err(|_| "rulebook rejected")
        .unwrap()
        .into_parts()
        .0
}

fn seed() -> Rulebook {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("demo")
        .join("stand-in-ai")
        .join("rulebook")
        .join("seed.json");
    rulebook(path.to_str().unwrap())
}

#[test]
fn with_no_adoption_every_rule_suggestion_is_pending_with_its_finding() {
    let store = StoreBatches::from_batches(&batches(), &seed());
    let spans: Vec<(u32, u32)> = store.pending.iter().map(|e| (e.start, e.end)).collect();
    let mut expected = vec![(161, 241), (242, 320), (321, 426), (427, 500), (501, 546)];
    let mut found = spans.clone();
    found.sort_unstable();
    expected.sort_unstable();
    assert_eq!(found, expected);
    let unsupported: Vec<&PendingEntry> = store
        .pending
        .iter()
        .filter(|e| e.finding.is_some())
        .collect();
    assert_eq!(unsupported.len(), 1);
    assert_eq!((unsupported[0].start, unsupported[0].end), (427, 500));
    assert_eq!(
        unsupported[0].finding,
        Some(StoreFinding::Unsupported {
            capability: "prize_allocation".to_owned()
        })
    );
}

#[test]
fn an_adopted_proposal_is_not_pending() {
    let adopted = rulebook(art("rulebook.json").to_str().unwrap());
    let store = StoreBatches::from_batches(&batches(), &adopted);
    let spans: Vec<(u32, u32)> = store.pending.iter().map(|e| (e.start, e.end)).collect();
    assert_eq!(spans, [(427, 500)]);
}

#[test]
fn an_ambiguity_outcome_keeps_its_message_as_the_interpretation() {
    let mut all = batches();
    let target = all
        .iter_mut()
        .flat_map(|b| b.proposals.iter_mut())
        .find(|p| !p.outcome.is_empty() && p.outcome[0].code == Code::ValUnsupportedConstruct)
        .unwrap();
    target.outcome[0].code = Code::ValAmbiguousInterpretation;
    target.outcome[0].message = "read as a ratio, or as shares of the total".to_owned();
    let store = StoreBatches::from_batches(&all, &seed());
    let found: Vec<&StoreFinding> = store
        .pending
        .iter()
        .filter_map(|e| e.finding.as_ref())
        .collect();
    assert_eq!(
        found,
        [&StoreFinding::Ambiguity {
            interpretations: vec!["read as a ratio, or as shares of the total".to_owned()]
        }]
    );
}

#[test]
fn a_proposal_without_a_blocking_outcome_has_no_finding() {
    let mut all = batches();
    for p in all.iter_mut().flat_map(|b| b.proposals.iter_mut()) {
        p.outcome.clear();
    }
    let store = StoreBatches::from_batches(&all, &seed());
    assert_eq!(store.pending.len(), 5);
    assert!(store.pending.iter().all(|e| e.finding.is_none()));
}
