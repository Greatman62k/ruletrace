//! Property 51: The Coverage_Statement classifies every Candidate_Clause exactly once (task 6.8).
//!
//! Feature: ruletrace-compiler, Property 51: The Coverage_Statement classifies every
//! Candidate_Clause exactly once
//!
//! **Validates: Requirements 33.6, 34.6**
//!
//! The clauses, Rulebook and ledger are those of the stand-in demonstration. A generated subset
//! of the clauses, in a generated order, gets generated store findings and notes. The expected
//! class is computed here from the generated choice, by the order of design §8.11.8.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    missing_docs
)]

use proptest::prelude::*;
use rt_demo::coverage::{
    classify, CoverageClass, CoverageError, CoverageNote, CoverageNotes, PendingEntry,
    StoreBatches, StoreFinding,
};
use rt_foundation::InputId;
use rt_ir::{parse_rulebook, Rulebook};
use rt_json::ParseLimits;
use rt_ledger::{parse_ledger, ApprovalLedger};
use rt_srcmodel::{
    default_load_limits, parse_candidates, CandidateClause, SourceModel, SourceModels,
};
use std::path::{Path, PathBuf};

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo/stand-in-hand")
}

struct World {
    clauses: Vec<CandidateClause>,
    rulebook: Rulebook,
    ledger: ApprovalLedger,
    models: SourceModels,
}

fn world() -> World {
    let art = |p: &str| std::fs::read(dir().join(p)).unwrap();
    let id = |n: &str| InputId::new(n).unwrap();
    let limits = ParseLimits::new(8 * 1024 * 1024, 64);
    let model = SourceModel::load(
        &id("source-model.json"),
        &art("golden/artifacts/source-model.json"),
        default_load_limits(),
    )
    .unwrap();
    let clauses = parse_candidates(
        &id("candidates.json"),
        &art("golden/artifacts/candidates.json"),
        limits,
        &model,
    )
    .unwrap();
    let staged = parse_rulebook(
        &art("rulebook/hand-authored.json"),
        &id("hand-authored.json"),
        8 * 1024 * 1024,
        64,
    )
    .map_err(|_| "rulebook")
    .unwrap();
    let ledger = parse_ledger(
        &id("ledger.jsonl"),
        &art("golden/artifacts/ledger.jsonl"),
        limits,
    )
    .map_err(|_| "ledger")
    .unwrap();
    let mut models = SourceModels::new();
    models.insert(model).map_err(|_| "dup").unwrap();
    World {
        clauses,
        rulebook: staged.into_parts().0,
        ledger,
        models,
    }
}

/// Per clause: whether it is present, a sort key for the input order, and a finding choice.
type Pick = (bool, u32, u8);

fn exact(c: &CandidateClause) -> (rt_foundation::DocumentId, u32, u32) {
    let s = c.span();
    (s.doc().clone(), s.start().get(), s.end().get())
}

fn partition_holds(w: &World, picks: &[Pick], approved: bool) -> Result<(), TestCaseError> {
    let mut chosen: Vec<(u32, &CandidateClause, u8)> = w
        .clauses
        .iter()
        .zip(picks)
        .filter(|(_, p)| p.0)
        .map(|(c, p)| (p.1, c, p.2))
        .collect();
    chosen.sort_by_key(|(k, _, _)| *k);
    let input: Vec<CandidateClause> = chosen.iter().map(|(_, c, _)| (*c).clone()).collect();
    let (mut pending, mut notes) = (Vec::new(), Vec::new());
    for (_, c, choice) in &chosen {
        let (document, start, end) = exact(c);
        if matches!(choice, 1 | 4) {
            pending.push(PendingEntry {
                document: document.clone(),
                start,
                end,
                finding: Some(StoreFinding::Unsupported {
                    capability: "cap".to_owned(),
                }),
            });
        }
        if matches!(choice, 2 | 5) {
            pending.push(PendingEntry {
                document: document.clone(),
                start,
                end,
                finding: Some(StoreFinding::Ambiguity {
                    interpretations: vec!["a".to_owned(), "b".to_owned()],
                }),
            });
        }
        if matches!(choice, 3..=5) {
            notes.push(CoverageNote {
                document,
                start,
                end,
                reason: "authored".to_owned(),
            });
        }
    }
    let ledger = if approved {
        w.ledger.clone()
    } else {
        ApprovalLedger::empty()
    };
    let validation = rt_validate::validate(&w.rulebook, &ledger, &w.models, Some(&input));
    let got = classify(
        &input,
        &w.rulebook,
        &ledger,
        &StoreBatches { pending },
        &validation,
        &CoverageNotes::new(notes),
    );
    let mut ordered = chosen.clone();
    ordered.sort_by_key(|(_, c, _)| c.span().start().get());
    let first_unclassified = ordered.iter().find(|(_, _, ch)| !approved && *ch == 0);
    match (got, first_unclassified) {
        (Err(CoverageError::Unclassified { start, .. }), Some((_, c, _))) => {
            prop_assert_eq!(start, c.span().start().get());
        }
        (Ok(st), None) => {
            prop_assert_eq!(st.entries.len(), ordered.len());
            for (e, (_, c, ch)) in st.entries.iter().zip(&ordered) {
                prop_assert_eq!(e.clause.span(), c.span());
                let want = if approved {
                    "rule_approved"
                } else {
                    match ch {
                        1 | 4 => "unsupported",
                        2 | 5 => "ambiguity",
                        _ => "not_a_rule",
                    }
                };
                prop_assert_eq!(e.class.name(), want);
                if matches!(e.class, CoverageClass::RuleApproved { .. }) {
                    let flagged = validation.state().as_slice().iter().any(|d| {
                        d.code() == rt_foundation::Code::CandidateUnreferenced
                            && matches!(d.primary(), rt_foundation::Location::Source(s) if s == c.span())
                    });
                    prop_assert!(!flagged);
                }
            }
            prop_assert!(st
                .entries
                .windows(2)
                .all(|p| p[0].clause.span().start().get() < p[1].clause.span().start().get()));
        }
        (other, _) => prop_assert!(false, "unexpected result: {other:?}"),
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn every_clause_is_classified_exactly_once(
        picks in prop::collection::vec((any::<bool>(), any::<u32>(), 0u8..6), 8),
        approved in any::<bool>(),
    ) {
        let w = world();
        prop_assume!(w.clauses.len() <= picks.len());
        partition_holds(&w, &picks, approved)?;
    }
}
