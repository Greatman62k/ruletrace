//! Property 7: Diagnostic canonical order is total and permutation independent.
//!
//! **Validates: Requirements 7.10, 7.22, 6.12, 17.12**
//!
//! "Rendering" here is the `Debug` text of the sorted list, which prints every
//! field of every diagnostic. The real renderers are later tasks; this test
//! pins the property they rely on, that one multiset of diagnostics has one
//! canonical sequence and so one byte string.

#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;
use proptest::prelude::*;
use proptest_floor::property_tests;
use rt_foundation::{
    CliLocation, Code, Diagnostic, DocumentId, InputId, InputLocation, Location, RevisionNumber,
    RuleId, RuleRef, SourceSpan, SpanSource,
};
use std::cmp::Ordering;

/// A 100 byte ASCII document in which every offset is a boundary and every line
/// has 10 columns.
struct Doc(DocumentId);

impl SpanSource for Doc {
    fn document_id(&self) -> &DocumentId {
        &self.0
    }
    fn len_bytes(&self) -> u64 {
        100
    }
    fn is_char_boundary(&self, _offset: u32) -> bool {
        true
    }
    fn line_column(&self, offset: u32) -> Option<(u32, u32)> {
        (offset <= 100).then(|| (offset / 10 + 1, offset % 10 + 1))
    }
}

// Small value domains make equal and nearly equal diagnostics likely.
const NAMES: [&str; 3] = ["a", "b", "ab"];

fn arb_code() -> impl Strategy<Value = Code> {
    prop::sample::select(Code::ALL.to_vec())
}

fn arb_span() -> impl Strategy<Value = Location> {
    (prop::sample::select(NAMES.to_vec()), 0u32..90, 1u32..10).prop_filter_map(
        "valid span",
        |(n, start, len)| {
            let doc = Doc(DocumentId::new(n).ok()?);
            let (line, column) = doc.line_column(start)?;
            SourceSpan::new(&doc, start, start + len, line, column)
                .ok()
                .map(Location::Source)
        },
    )
}

fn arb_location() -> impl Strategy<Value = Location> {
    let name = prop::sample::select(NAMES.to_vec());
    prop_oneof![
        arb_span(),
        name.clone()
            .prop_filter_map("doc id", |n| DocumentId::new(n)
                .ok()
                .map(Location::Document)),
        (
            name.clone(),
            prop::option::of(1u32..3),
            prop::option::of(1u32..3),
            prop::option::of(name.clone().prop_map(str::to_string)),
        )
            .prop_filter_map("input id", |(n, line, column, field)| {
                Some(Location::Input(InputLocation {
                    input: InputId::new(n).ok()?,
                    line,
                    column,
                    field,
                }))
            }),
        (
            0u32..2,
            prop::option::of(name.clone().prop_map(str::to_string))
        )
            .prop_map(|(position, name)| Location::Cli(CliLocation::Argument { position, name })),
        name.prop_map(|n| Location::Cli(CliLocation::Command(n.to_string()))),
        arb_code().prop_map(Location::None),
    ]
}

fn arb_rule_ref() -> impl Strategy<Value = RuleRef> {
    (
        prop::sample::select(NAMES.to_vec()),
        prop::option::of(1u32..3),
    )
        .prop_filter_map("rule ref", |(n, rev)| {
            Some(RuleRef {
                rule: RuleId::new(n).ok()?,
                revision: match rev {
                    Some(r) => Some(RevisionNumber::new(r).ok()?),
                    None => None,
                },
            })
        })
}

fn arb_diagnostic() -> impl Strategy<Value = Diagnostic> {
    (
        arb_code(),
        prop::sample::select(vec!["", "m", "mm", "n", "é"]),
        arb_location(),
        prop::collection::vec(arb_location(), 0..3),
        prop::collection::vec(arb_rule_ref(), 0..3),
    )
        .prop_map(|(code, message, primary, related, rules)| {
            Diagnostic::new(code, message.to_string(), primary)
                .with_related(related)
                .with_rules(rules)
        })
}

fn render(list: &[Diagnostic]) -> Vec<u8> {
    list.iter()
        .map(|d| format!("{d:?}\n"))
        .collect::<String>()
        .into_bytes()
}

fn sorted(mut list: Vec<Diagnostic>) -> Vec<Diagnostic> {
    list.sort();
    list
}

property_tests! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Every permutation of a generated list sorts to the same sequence and the
    /// same bytes; equal items are retained and rendering twice gives identical
    /// bytes.
    #[test]
    fn permutation_sorts_to_identical_bytes(
        (list, shuffled) in prop::collection::vec(arb_diagnostic(), 0..12)
            .prop_flat_map(|v| (Just(v.clone()), Just(v).prop_shuffle()))
    ) {
        let a = sorted(list.clone());
        let b = sorted(shuffled);
        prop_assert_eq!(a.len(), list.len());
        prop_assert_eq!(&a, &b);
        prop_assert_eq!(render(&a), render(&b));
        prop_assert_eq!(render(&a), render(&a));
        // Sorted output is a permutation of the input: nothing merged, nothing
        // lost.
        for d in &list {
            let want = list.iter().filter(|x| *x == d).count();
            let got = a.iter().filter(|x| *x == d).count();
            prop_assert_eq!(want, got);
        }
        prop_assert!(a.windows(2).all(|w| w[0] <= w[1]));
    }

    /// The order is total, antisymmetric, transitive and consistent with
    /// equality.
    #[test]
    fn order_is_a_total_order(
        a in arb_diagnostic(),
        b in arb_diagnostic(),
        c in arb_diagnostic(),
    ) {
        prop_assert_eq!(a.cmp(&b), b.cmp(&a).reverse());
        prop_assert_eq!(a.cmp(&b) == Ordering::Equal, a == b);
        prop_assert_eq!(a.partial_cmp(&b), Some(a.cmp(&b)));
        prop_assert_eq!(a.cmp(&a), Ordering::Equal);
        if a <= b && b <= c {
            prop_assert!(a <= c);
        }
        if a >= b && b >= c {
            prop_assert!(a >= c);
        }
    }
}

/// Duplicates are kept: two equal diagnostics stay two items after sorting.
#[test]
fn equal_diagnostics_are_both_retained() {
    let d = Diagnostic::new(
        Code::AiDisabled,
        "m".to_string(),
        Location::None(Code::AiDisabled),
    );
    let v = sorted(vec![d.clone(), d.clone()]);
    assert_eq!(v, vec![d.clone(), d]);
}
