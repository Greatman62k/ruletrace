//! The engine's difference kinds and the Explainer's table of kinds agree.
//!
//! rt-explain may not depend on rt-engine, so its table
//! (`rt_explain::report::DIFFERENCE_KINDS`) is checked here, where both crates
//! are visible. A new `DifferenceKind` variant fails the build of this file
//! through the exhaustive matches below, and a kind without a table row, a
//! row without an engine kind, a wrong code or a code that the catalogue does
//! not describe with that kind fails a test (Req 13.12, RN-26).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs
)]

use rt_engine::DifferenceKind;
use rt_explain::report::{difference_kind, DIFFERENCE_KINDS};
use rt_foundation::Code;
use std::collections::BTreeSet;
use std::path::Path;

/// Every engine kind. The `ordinal` match below does not compile when a
/// variant is added, and `the_list_of_engine_kinds_is_complete` fails when the
/// variant is added here but not there.
const ALL: [DifferenceKind; 3] = [
    DifferenceKind::ValueDiffers,
    DifferenceKind::TieBrokenWithoutDeclaredCriterion,
    DifferenceKind::TieNotInComputedResult,
];

/// Expected name and code of each kind, written from Req 13.12 and not read
/// from the table under test.
fn expected(kind: DifferenceKind) -> (&'static str, Code) {
    match kind {
        DifferenceKind::ValueDiffers => ("value_differs", Code::PublishedResultDiffers),
        DifferenceKind::TieBrokenWithoutDeclaredCriterion => (
            "tie_broken_without_declared_criterion",
            Code::PublishedTieGroupSplit,
        ),
        DifferenceKind::TieNotInComputedResult => {
            ("tie_not_in_computed_result", Code::PublishedTieNotComputed)
        }
    }
}

fn ordinal(kind: DifferenceKind) -> usize {
    match kind {
        DifferenceKind::ValueDiffers => 0,
        DifferenceKind::TieBrokenWithoutDeclaredCriterion => 1,
        DifferenceKind::TieNotInComputedResult => 2,
    }
}

#[test]
fn the_list_of_engine_kinds_is_complete() {
    let ordinals: BTreeSet<usize> = ALL.iter().map(|k| ordinal(*k)).collect();
    assert_eq!(ordinals, (0..ALL.len()).collect::<BTreeSet<usize>>());
    let max = ALL.iter().map(|k| ordinal(*k)).max().unwrap();
    assert_eq!(max + 1, ALL.len());
}

#[test]
fn every_engine_kind_has_a_row_with_its_name_and_code() {
    for kind in ALL {
        let (name, code) = expected(kind);
        assert_eq!(kind.as_str(), name);
        let row = difference_kind(kind.as_str())
            .unwrap_or_else(|| panic!("no row for the engine kind {name}"));
        assert_eq!(row.name(), name);
        assert_eq!(row.code(), code);
        assert!(!row.description().is_empty());
    }
}

#[test]
fn the_table_has_no_row_without_an_engine_kind() {
    let engine: BTreeSet<&str> = ALL.iter().map(|k| k.as_str()).collect();
    let rows: BTreeSet<&str> = DIFFERENCE_KINDS.iter().map(|k| k.name()).collect();
    assert_eq!(rows, engine);
    assert_eq!(DIFFERENCE_KINDS.len(), ALL.len());
}

#[test]
fn the_catalogue_describes_each_code_with_its_kind() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../catalogue/diagnostics.v1.json");
    let text = std::fs::read_to_string(path).unwrap();
    for row in DIFFERENCE_KINDS {
        let line = text
            .lines()
            .find(|l| l.contains(&format!("\"code\": \"{}\"", row.code().id())))
            .unwrap_or_else(|| panic!("{} is not in the catalogue", row.code().id()));
        assert!(
            line.contains(&format!("(kind {})", row.name())),
            "catalogue row of {} does not name kind {}: {line}",
            row.code().id(),
            row.name()
        );
    }
}
