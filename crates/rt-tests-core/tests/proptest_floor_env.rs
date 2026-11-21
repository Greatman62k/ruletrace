//! `PROPTEST_CASES` cannot lower the case count of a `property_tests!` test
//! (Req 20.2). This file holds one test because it sets a process-wide
//! environment variable.

#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;

use proptest::prelude::*;
use proptest_floor::property_tests;
use std::sync::atomic::{AtomicU32, Ordering};

static LOWERED_RUNS: AtomicU32 = AtomicU32::new(0);
static RAISED_RUNS: AtomicU32 = AtomicU32::new(0);

property_tests! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    fn configured_256(x in 0u32..1000) {
        LOWERED_RUNS.fetch_add(1, Ordering::SeqCst);
        prop_assert!(x < 1000);
    }

    fn configured_256_again(x in 0u32..1000) {
        RAISED_RUNS.fetch_add(1, Ordering::SeqCst);
        prop_assert!(x < 1000);
    }
}

#[test]
fn the_environment_cannot_lower_the_count_but_can_raise_it() {
    std::env::set_var("PROPTEST_CASES", "3");
    configured_256();
    assert_eq!(LOWERED_RUNS.load(Ordering::SeqCst), 256);

    std::env::set_var("PROPTEST_CASES", "300");
    configured_256_again();
    assert_eq!(RAISED_RUNS.load(Ordering::SeqCst), 300);
}
