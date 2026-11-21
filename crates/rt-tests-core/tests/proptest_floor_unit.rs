//! The failure behaviour of the case floor (Req 20.2): a configuration below
//! 256 fails unless it is declared below the floor, and a run that passed
//! fewer cases than configured fails.

#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;

use proptest::prelude::*;
use proptest_floor::{assert_cases_ran, effective_config, property_tests};

#[test]
#[should_panic(expected = "configured with 10 cases, below the floor of 256")]
fn a_configuration_of_10_cases_fails() {
    effective_config(ProptestConfig::with_cases(10), false, "t");
}

#[test]
#[should_panic(expected = "configured with 255 cases, below the floor of 256")]
fn a_configuration_of_255_cases_fails() {
    effective_config(ProptestConfig::with_cases(255), false, "t");
}

#[test]
fn the_floor_itself_and_a_declared_below_floor_count_are_accepted() {
    assert_eq!(
        effective_config(ProptestConfig::with_cases(256), false, "t").cases,
        256
    );
    assert_eq!(
        effective_config(ProptestConfig::with_cases(10), true, "t").cases,
        10
    );
}

#[test]
#[should_panic(expected = "generated 255 passing cases, fewer than the 256 required")]
fn a_run_of_255_passing_cases_fails() {
    assert_cases_ran("t", 256, 255);
}

#[test]
fn a_run_of_exactly_the_wanted_cases_passes() {
    assert_cases_ran("t", 256, 256);
}

// The struct form keeps the source scan from reading this deliberate
// negative case as a real property test below the floor.
property_tests! {
    #![proptest_config(ProptestConfig { cases: 10, ..ProptestConfig::default() })]

    #[test]
    #[should_panic(expected = "configured with 10 cases, below the floor of 256")]
    fn a_property_configured_with_10_cases_fails(x in 0u32..10) {
        prop_assert!(x < 10);
    }
}

property_tests! {
    #[test]
    fn without_a_config_the_count_is_the_floor(x in 0u32..10) {
        prop_assert!(x < 10);
    }
}

property_tests! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::with_cases(256)
    })]

    #[test]
    #[should_panic(expected = "Test failed")]
    fn a_failing_property_still_fails(x in 0u32..10) {
        prop_assert!(x > 100);
    }
}
