//! Property 19: rounding at declared points only (task 3.8, Req 8.3, 8.4, 8.14, 8.28).
//!
//! Limitation: the engine and the validator do not exist yet, so this runs at the
//! `rt-numeric` level. It covers (a) `round` equals the reference rounding for every mode
//! and places 0 to 28, (b) places outside 0 to 28 are refused, never clamped, and
//! (c) no other operation changes the exact value of a result: add, sub, mul, div, neg,
//! sum, min and max return the exact reference value, so no rounding happens implicitly.
//! The validator diagnostics for missing or out-of-range declarations (Req 8.3, 8.4) are
//! not testable until the validator exists.
//!
//! **Validates: Requirements 8.3, 8.4, 8.14, 8.28**

use crate::harness::{check_aggregate, check_pair, check_round, edge_values, operand, to_rt};
use crate::harness::{Rng, CASES, MODES, SEED};
use crate::reference::Rat;
use num_bigint::BigInt;
use rt_numeric as rn;
use rt_numeric::{NumericError, RoundingMode};
use std::cmp::Ordering;

fn values(seed: u64) -> Vec<Rat> {
    let mut r = Rng::new(seed);
    let mut v = edge_values();
    v.extend((0..CASES).map(|_| operand(&mut r)));
    v
}

fn abs(v: &Rat) -> Rat {
    if v.cmp(&Rat::from_i128(0)) == Ordering::Less {
        v.neg()
    } else {
        v.clone()
    }
}

#[test]
fn round_equals_reference_for_every_mode_and_declared_places() {
    for v in values(SEED ^ 0x19) {
        for places in 0..=28 {
            check_round(&v, places).unwrap();
        }
    }
}

#[test]
fn places_outside_zero_to_28_are_refused_for_every_mode() {
    for v in values(SEED ^ 0x20).into_iter().take(80) {
        let x = to_rt(&v).unwrap();
        for places in [29u32, 30, 100, u32::MAX] {
            for mode in MODES {
                assert_eq!(
                    rn::round(&x, places, mode),
                    Err(NumericError::InvalidPlaces)
                );
            }
        }
    }
}

#[test]
fn rounding_is_idempotent_and_within_one_unit_of_the_exact_value() {
    for v in values(SEED ^ 0x21) {
        let x = to_rt(&v).unwrap();
        for places in [0u32, 1, 2, 7, 28] {
            let unit = Rat::from_parts(BigInt::from(1u8), BigInt::from(10u8).pow(places));
            let half = Rat::from_parts(
                BigInt::from(1u8),
                BigInt::from(2u8) * BigInt::from(10u8).pow(places),
            );
            for mode in MODES {
                let Ok(y) = rn::round(&x, places, mode) else {
                    continue;
                };
                assert_eq!(rn::round(&y, places, mode), Ok(y.clone()), "idempotence");
                let y_rat = crate::reference::parse_canonical(&rn::to_canonical(&y)).unwrap();
                let Ok(diff) = y_rat.sub(&v) else { continue };
                let bound = if mode == RoundingMode::TowardZero {
                    &unit
                } else {
                    &half
                };
                let within = match mode {
                    RoundingMode::TowardZero => abs(&diff).cmp(bound) == Ordering::Less,
                    _ => abs(&diff).cmp(bound) != Ordering::Greater,
                };
                assert!(
                    within,
                    "{} at {places} {mode:?} -> {}",
                    v.canonical(),
                    y_rat.canonical()
                );
                if mode == RoundingMode::TowardZero {
                    assert_ne!(abs(&y_rat).cmp(&abs(&v)), Ordering::Greater, "toward zero");
                }
            }
        }
    }
}

#[test]
fn no_operation_other_than_round_changes_the_exact_value() {
    // `check_pair` compares each result with the exact reference value, so any implicit
    // rounding, truncation or scaling in add, sub, mul, div or neg would disagree.
    let vals = values(SEED ^ 0x22);
    let mut r = Rng::new(SEED ^ 0x23);
    for v in edge_values() {
        for w in edge_values() {
            check_pair(&v, &w).unwrap();
        }
    }
    for v in &vals {
        let w = &vals[r.below(vals.len() as u64) as usize];
        check_pair(v, w).unwrap();
    }
    for chunk in vals.chunks(7) {
        check_aggregate(chunk).unwrap();
    }
}

#[test]
fn non_terminating_results_stay_exact_until_a_round_node() {
    let (one, three) = (
        to_rt(&Rat::from_i128(1)).unwrap(),
        to_rt(&Rat::from_i128(3)).unwrap(),
    );
    let third = rn::div(&one, &three).unwrap();
    assert_eq!(rn::to_canonical(&third), "1/3");
    assert_eq!(rn::mul(&third, &three), Ok(one.clone()));
    let sum = rn::add(&rn::add(&third, &third).unwrap(), &third).unwrap();
    assert_eq!(sum, one);
    // The declared rounding is the only place the value changes.
    let rounded = rn::round(&third, 2, RoundingMode::HalfEven).unwrap();
    assert_eq!(rn::to_canonical(&rounded), "0.33");
    assert_ne!(rounded, third);
}
