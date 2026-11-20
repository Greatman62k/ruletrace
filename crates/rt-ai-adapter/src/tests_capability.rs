//! The truth table and the properties of `negotiate` (UD-37, Req 35.2,
//! 35.3). Expected values come from a hand-written table of what one
//! requirement does with one value, not from the function under test.

use crate::proptest_floor::property_tests;
use crate::{negotiate, Capabilities, Required, Tri, Unmet};
use proptest::prelude::*;

const VALUES: [Tri; 3] = [Tri::Unknown, Tri::Yes, Tri::No];

// (required, available) to the unmet value, or None when it is met.
fn row(required: bool, value: Tri) -> Option<Tri> {
    match (required, value) {
        (false, _) | (true, Tri::Yes) => None,
        (true, Tri::No) => Some(Tri::No),
        (true, Tri::Unknown) => Some(Tri::Unknown),
    }
}

fn caps(structured_output: Tri, reasoning_effort: Tri, usage_reporting: Tri) -> Capabilities {
    Capabilities {
        structured_output,
        reasoning_effort,
        usage_reporting,
    }
}

#[test]
fn every_combination_gives_the_exact_unmet_list_in_name_order() {
    let mut rows = 0;
    for structured in [false, true] {
        for effort in [false, true] {
            for so in VALUES {
                for re in VALUES {
                    for usage in VALUES {
                        let required = Required {
                            structured_output: structured,
                            reasoning_effort: effort,
                        };
                        let mut expected = Vec::new();
                        // `reasoning_effort` sorts before `structured_output`.
                        if let Some(value) = row(effort, re) {
                            expected.push(Unmet {
                                capability: "reasoning_effort",
                                value,
                            });
                        }
                        if let Some(value) = row(structured, so) {
                            expected.push(Unmet {
                                capability: "structured_output",
                                value,
                            });
                        }
                        let got = negotiate(&required, &caps(so, re, usage));
                        let want = if expected.is_empty() {
                            Ok(())
                        } else {
                            Err(expected)
                        };
                        assert_eq!(got, want, "{required:?} {so:?} {re:?} {usage:?}");
                        rows += 1;
                    }
                }
            }
        }
    }
    assert_eq!(rows, 4 * 27);
}

#[test]
fn an_unknown_required_capability_fails_closed() {
    let need_output = Required {
        structured_output: true,
        reasoning_effort: false,
    };
    assert_eq!(
        negotiate(&need_output, &Capabilities::UNKNOWN),
        Err(vec![Unmet {
            capability: "structured_output",
            value: Tri::Unknown
        }])
    );
}

#[test]
fn nothing_required_is_met_by_any_capabilities() {
    let nothing = Required {
        structured_output: false,
        reasoning_effort: false,
    };
    assert_eq!(negotiate(&nothing, &Capabilities::UNKNOWN), Ok(()));
}

#[test]
fn both_unmet_are_listed_with_the_effort_first() {
    let both = Required {
        structured_output: true,
        reasoning_effort: true,
    };
    let got = negotiate(&both, &caps(Tri::No, Tri::Unknown, Tri::Yes)).unwrap_err();
    let names: Vec<&str> = got.iter().map(|u| u.capability).collect();
    assert_eq!(names, ["reasoning_effort", "structured_output"]);
    assert!(names[0].as_bytes() < names[1].as_bytes());
    assert_eq!(got[0].value, Tri::Unknown);
    assert_eq!(got[1].value, Tri::No);
}

fn tri() -> impl Strategy<Value = Tri> {
    prop::sample::select(VALUES.to_vec())
}

property_tests! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn unmet_entries_are_exactly_the_required_capabilities_that_are_not_yes(
        structured in any::<bool>(),
        effort in any::<bool>(),
        so in tri(),
        re in tri(),
        usage in tri(),
    ) {
        let required = Required { structured_output: structured, reasoning_effort: effort };
        let available = caps(so, re, usage);
        let first = negotiate(&required, &available);
        prop_assert_eq!(&first, &negotiate(&required, &available));
        let want_effort = effort && re != Tri::Yes;
        let want_output = structured && so != Tri::Yes;
        match first {
            Ok(()) => prop_assert!(!want_effort && !want_output),
            Err(list) => {
                prop_assert!(!list.is_empty());
                prop_assert_eq!(
                    list.iter().any(|u| u.capability == "reasoning_effort"),
                    want_effort
                );
                prop_assert_eq!(
                    list.iter().any(|u| u.capability == "structured_output"),
                    want_output
                );
                prop_assert!(list.iter().all(|u| u.value != Tri::Yes));
                prop_assert!(list.windows(2).all(|w| w[0].capability.as_bytes() < w[1].capability.as_bytes()));
            }
        }
    }

    #[test]
    fn usage_reporting_never_changes_the_result(
        structured in any::<bool>(),
        effort in any::<bool>(),
        so in tri(),
        re in tri(),
        a in tri(),
        b in tri(),
    ) {
        let required = Required { structured_output: structured, reasoning_effort: effort };
        prop_assert_eq!(
            negotiate(&required, &caps(so, re, a)),
            negotiate(&required, &caps(so, re, b))
        );
    }
}
