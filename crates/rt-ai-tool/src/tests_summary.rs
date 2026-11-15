//! Run_Summary tests (design 3.7.7; Req 23.36). Expected text and lists
//! are written by hand from the requirement.

use crate::tests_run::cid;
use crate::{ClauseId, Redactor, RunSummary};

fn render(summary: &RunSummary) -> String {
    summary.render(&Redactor::no_credential())
}

fn all() -> Vec<ClauseId> {
    vec![cid(0), cid(1), cid(2)]
}

#[test]
fn the_unconfirmed_section_appears_only_when_it_is_not_empty() {
    let plain = RunSummary::new(&all(), &[cid(0)], &[cid(0)]);
    assert!(!render(&plain).contains("durability"), "{}", render(&plain));
    let marked = RunSummary::with_unconfirmed(&all(), &[cid(0)], &[cid(0)], &[cid(1)]);
    assert_eq!(
        render(&marked),
        "run summary: incomplete\n\
         processed:\n  rules.md 0 11\n\
         stored:\n  rules.md 0 11\n\
         stored, durability not confirmed:\n  rules.md 12 27\n\
         unprocessed:\n  rules.md 28 32\n"
    );
}

#[test]
fn an_unconfirmed_clause_is_not_unprocessed_and_blocks_complete() {
    let s = RunSummary::with_unconfirmed(&all(), &[cid(0), cid(1)], &[cid(0), cid(1)], &[cid(2)]);
    assert!(s.unprocessed().is_empty());
    assert_eq!(s.unconfirmed(), [cid(2)]);
    assert!(!s.is_complete());
    assert!(render(&s).starts_with("run summary: incomplete\n"));
}

#[test]
fn unconfirmed_clauses_that_are_processed_or_not_requested_are_dropped() {
    let s =
        RunSummary::with_unconfirmed(&[cid(0), cid(1)], &[cid(0)], &[cid(0)], &[cid(0), cid(2)]);
    assert!(s.unconfirmed().is_empty());
    assert_eq!(s.unprocessed(), [cid(1)]);
}

#[test]
fn the_unconfirmed_list_is_sorted_by_start_offset() {
    let s = RunSummary::with_unconfirmed(&all(), &[], &[], &[cid(2), cid(0)]);
    assert_eq!(s.unconfirmed(), [cid(0), cid(2)]);
    assert_eq!(s.unprocessed(), [cid(1)]);
}

#[test]
fn platform_skipped_clauses_move_out_of_stored_into_their_own_section() {
    let plain = RunSummary::new(&all(), &all(), &all());
    assert!(plain.platform_skipped().is_empty());
    assert!(!render(&plain).contains("durability"), "{}", render(&plain));
    let marked = plain.with_platform_skipped(&[cid(2), cid(1)]);
    assert_eq!(marked.platform_skipped(), [cid(1), cid(2)]);
    assert_eq!(marked.stored(), [cid(0)]);
    assert_eq!(marked.processed(), all());
    assert!(marked.is_complete());
    assert_eq!(
        render(&marked),
        "run summary: complete, directory durability not confirmed\n\
         processed:\n  rules.md 0 11\n  rules.md 12 27\n  rules.md 28 32\n\
         stored:\n  rules.md 0 11\n\
         stored, directory durability not confirmed (skipped by platform):\n  \
         rules.md 12 27\n  rules.md 28 32\n\
         unprocessed:\n  (none)\n"
    );
}

#[test]
fn a_platform_skipped_clause_that_was_not_stored_is_dropped() {
    let s = RunSummary::new(&all(), &[cid(0), cid(1)], &[cid(0), cid(1)])
        .with_platform_skipped(&[cid(1), cid(2)]);
    assert_eq!(s.platform_skipped(), [cid(1)]);
    assert_eq!(s.stored(), [cid(0)]);
    assert_eq!(s.unprocessed(), [cid(2)]);
}

#[test]
fn the_platform_skip_and_the_failed_sync_are_two_separate_sections() {
    let s = RunSummary::with_unconfirmed(&all(), &[cid(0)], &[cid(0)], &[cid(1)])
        .with_platform_skipped(&[cid(0)]);
    assert!(!s.is_complete());
    assert!(s.stored().is_empty());
    assert_eq!(s.platform_skipped(), [cid(0)]);
    assert_eq!(s.unconfirmed(), [cid(1)]);
    assert_eq!(
        render(&s),
        "run summary: incomplete\n\
         processed:\n  rules.md 0 11\n\
         stored:\n  (none)\n\
         stored, directory durability not confirmed (skipped by platform):\n  \
         rules.md 0 11\n\
         stored, durability not confirmed:\n  rules.md 12 27\n\
         unprocessed:\n  rules.md 28 32\n"
    );
}
