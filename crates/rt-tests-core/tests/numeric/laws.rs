//! Property 17: exact arithmetic laws and differential equivalence (task 3.6).
//!
//! **Validates: Requirements 8.1, 8.8, 8.9, 8.11, 8.12, 8.17, 8.18, 8.22, 8.23, 20.22**
//!
//! Operands come from the harness generators (edge values at, next to and one step
//! inside the bound, plus 256 seeded random cases per operation). Results beyond the
//! bound arise from the operations themselves, because every operand is in range.

use crate::harness::{
    check_pair, check_round, edge_values, operand, to_rt, Rng, CASES, MODES, SEED,
};
use crate::pairs;
use crate::reference::{bound, parse_canonical, Kind, Rat};
use num_bigint::BigInt;
use rt_numeric as rn;
use std::cmp::Ordering;

type Op = fn(&Rat, &Rat) -> Result<Rat, Kind>;
type RtOp = fn(&rn::ExactNumber, &rn::ExactNumber) -> Result<rn::ExactNumber, rn::NumericError>;

fn triples(seed: u64) -> Vec<(Rat, Rat, Rat)> {
    let edges = edge_values();
    let mut r = Rng::new(seed);
    let mut out = Vec::new();
    for _ in 0..CASES {
        let pick = |r: &mut Rng| edges[r.below(edges.len() as u64) as usize].clone();
        out.push((pick(&mut r), pick(&mut r), pick(&mut r)));
        out.push((operand(&mut r), operand(&mut r), operand(&mut r)));
        out.push((operand(&mut r), pick(&mut r), operand(&mut r)));
    }
    out
}

/// Exact result of an operation without any bound, as an unreduced-then-normalized pair.
fn exact(op: &str, a: &Rat, b: &Rat) -> Option<Rat> {
    let (an, ad, bn, bd) = (
        a.numerator(),
        a.denominator(),
        b.numerator(),
        b.denominator(),
    );
    match op {
        "add" => Rat::new(an * bd + bn * ad, ad * bd),
        "sub" => Rat::new(an * bd - bn * ad, ad * bd),
        "mul" => Rat::new(an * bn, ad * bd),
        _ => Rat::new(an * bd, ad * bn),
    }
}

#[test]
fn overflow_is_reported_exactly_when_the_normalized_result_is_outside_the_limits() {
    let ops: [(&str, RtOp); 4] = [
        ("add", rn::add),
        ("sub", rn::sub),
        ("mul", rn::mul),
        ("div", rn::div),
    ];
    let (mut inside, mut outside) = (0usize, 0usize);
    for (a, b) in pairs(SEED ^ 0x17) {
        let (x, y) = (to_rt(&a).unwrap(), to_rt(&b).unwrap());
        for (name, f) in ops {
            let got = f(&x, &y);
            match exact(name, &a, &b) {
                None => assert_eq!(
                    got,
                    Err(rn::NumericError::DivisionByZero),
                    "{name}({}, {})",
                    a.canonical(),
                    b.canonical()
                ),
                Some(want) if want.in_range() => {
                    inside += 1;
                    let got = got.unwrap_or_else(|e| {
                        panic!("{name}({}, {}): {e:?}", a.canonical(), b.canonical())
                    });
                    assert_eq!(rn::to_canonical(&got), want.canonical());
                    assert_eq!(got, to_rt(&want).unwrap());
                }
                Some(_) => {
                    outside += 1;
                    assert_eq!(
                        got,
                        Err(rn::NumericError::Overflow),
                        "{name}({}, {}) must overflow",
                        a.canonical(),
                        b.canonical()
                    );
                }
            }
        }
    }
    assert!(inside > 500 && outside > 200, "{inside} in, {outside} out");
}

#[test]
fn every_operation_agrees_with_the_reference_model() {
    // The harness check covers add, sub, mul, div, cmp and neg; 256 random pairs per run
    // plus every edge pair.
    for (a, b) in pairs(SEED ^ 0x18) {
        check_pair(&a, &b).unwrap();
    }
}

#[test]
fn rounding_agrees_for_every_mode_and_place() {
    let mut r = Rng::new(SEED ^ 0x19);
    let mut values = edge_values();
    values.extend((0..CASES).map(|_| operand(&mut r)));
    for v in &values {
        for places in [0u32, 1, 2, 5, 14, 27, 28, 29, 40] {
            check_round(v, places).unwrap();
        }
        check_round(v, r.below(29) as u32).unwrap();
    }
    assert_eq!(MODES.len(), 3);
}

#[test]
fn add_then_subtract_returns_the_original() {
    // Req 8.8, over pairs where a + b and (a + b) - b are both within the limits.
    let mut checked = 0;
    for (a, b) in pairs(SEED ^ 0x20) {
        let (x, y) = (to_rt(&a).unwrap(), to_rt(&b).unwrap());
        let Ok(s) = a.add(&b) else {
            assert_eq!(rn::add(&x, &y), Err(rn::NumericError::Overflow));
            continue;
        };
        if s.sub(&b).is_err() {
            continue;
        }
        let sum = rn::add(&x, &y).unwrap();
        assert_eq!(
            rn::sub(&sum, &y).unwrap(),
            x,
            "{} {}",
            a.canonical(),
            b.canonical()
        );
        checked += 1;
    }
    assert!(checked > 300, "{checked}");
}

#[test]
fn addition_is_commutative_and_associative() {
    let (mut commuted, mut associated) = (0, 0);
    for (a, b, c) in triples(SEED ^ 0x21) {
        let (x, y, z) = (to_rt(&a).unwrap(), to_rt(&b).unwrap(), to_rt(&c).unwrap());
        // Commutativity holds for results and for overflow alike.
        assert_eq!(rn::add(&x, &y), rn::add(&y, &x));
        assert_eq!(rn::mul(&x, &y), rn::mul(&y, &x));
        commuted += 1;
        // Req 8.9: any grouping or order, when every intermediate sum is in range.
        let orders = [
            (&a, &b, &c, (&x, &y, &z)),
            (&a, &c, &b, (&x, &z, &y)),
            (&b, &c, &a, (&y, &z, &x)),
            (&c, &b, &a, (&z, &y, &x)),
        ];
        let left = |p: (&Rat, &Rat, &Rat)| p.0.add(p.1).and_then(|s| s.add(p.2));
        let right = |p: (&Rat, &Rat, &Rat)| p.1.add(p.2).and_then(|s| p.0.add(&s));
        let all_ok = orders
            .iter()
            .all(|o| left((o.0, o.1, o.2)).is_ok() && right((o.0, o.1, o.2)).is_ok());
        if !all_ok {
            continue;
        }
        let want = to_rt(&left((&a, &b, &c)).unwrap()).unwrap();
        for (.., (p, q, s)) in orders {
            let l = rn::add(p, q).and_then(|t| rn::add(&t, s)).unwrap();
            let r = rn::add(q, s).and_then(|t| rn::add(p, &t)).unwrap();
            assert_eq!(l, want);
            assert_eq!(r, want);
        }
        let list = [x.clone(), y.clone(), z.clone()];
        assert_eq!(rn::sum(&list).unwrap(), want);
        associated += 1;
    }
    assert!(commuted > 500 && associated > 50, "{associated}");
}

#[test]
fn comparison_is_by_value_and_equal_values_have_one_form() {
    // The worked example of Req 8.11 and 8.22.
    let half = rn::parse_canonical("0.5").unwrap();
    let two_fourths = rn::div(
        &rn::parse_literal("2").unwrap(),
        &rn::parse_literal("4").unwrap(),
    )
    .unwrap();
    let one_two = rn::div(
        &rn::parse_literal("1").unwrap(),
        &rn::parse_literal("2").unwrap(),
    )
    .unwrap();
    assert_eq!(half, two_fourths);
    assert_eq!(half, one_two);
    assert_eq!(rn::cmp(&half, &two_fourths), Ordering::Equal);
    assert_eq!(
        rn::parse_literal("1.0").unwrap(),
        rn::parse_literal("1.00").unwrap()
    );
    // Unreduced fractions normalize to lowest terms with a positive denominator.
    let mut r = Rng::new(SEED ^ 0x22);
    let mut values = edge_values();
    values.extend((0..CASES).map(|_| operand(&mut r)));
    let mut scaled_cases = 0;
    for v in &values {
        let k = BigInt::from(r.below(1000) + 2);
        let (n, d) = (v.numerator() * &k, v.denominator() * &k);
        if n.magnitude() > bound().magnitude() || d > bound() {
            // Unreduced text beyond the limits is the parser's concern (task 3.7).
            continue;
        }
        let text = format!("{n}/{d}");
        let want = to_rt(v).unwrap();
        let in_range = parse_canonical(&text).is_ok();
        let got = rn::parse_canonical(&text);
        if in_range {
            // Only reducible texts inside the limits are accepted; they equal the value.
            let got = got.unwrap();
            assert_eq!(got, want, "{text}");
            assert_eq!(rn::cmp(&got, &want), Ordering::Equal);
            assert_eq!(rn::to_canonical(&got), v.canonical());
            scaled_cases += 1;
        } else {
            assert_eq!(got.err(), Some(rn::NumericError::Overflow), "{text}");
        }
        // Value order is total and antisymmetric against the reference.
        let w = &values[r.below(values.len() as u64) as usize];
        let (p, q) = (to_rt(w).unwrap(), want.clone());
        assert_eq!(rn::cmp(&p, &q), w.cmp(v));
        assert_eq!(rn::cmp(&q, &p), rn::cmp(&p, &q).reverse());
        assert_eq!(p == q, w.cmp(v) == Ordering::Equal);
    }
    assert!(scaled_cases > 50, "{scaled_cases}");
}

#[test]
fn zero_has_one_form_everywhere() {
    // Req 8.17: the four spellings, and zero reached by every operation.
    let zero = rn::parse_literal("0").unwrap();
    for text in ["0", "0.0", "-0", "-0.00"] {
        let n = rn::parse_literal(text).unwrap();
        assert_eq!(n, zero, "{text}");
        assert_eq!(rn::to_canonical(&n), "0");
        assert_eq!(rn::cmp(&n, &zero), Ordering::Equal);
    }
    assert_eq!(rn::to_canonical(&rn::parse_canonical("-0/5").unwrap()), "0");
    assert_eq!(rn::parse_canonical("-0/5").unwrap(), zero);
    let mut r = Rng::new(SEED ^ 0x23);
    for _ in 0..CASES {
        let v = operand(&mut r);
        let x = to_rt(&v).unwrap();
        let zeros = [
            rn::sub(&x, &x).unwrap(),
            rn::mul(&x, &zero).unwrap(),
            rn::mul(&zero, &rn::neg(&x)).unwrap(),
            rn::add(&x, &rn::neg(&x)).unwrap(),
            rn::neg(&zero),
            rn::div(&zero, &x).unwrap_or_else(|_| zero.clone()),
        ];
        for z in zeros {
            assert_eq!(z, zero);
            assert_eq!(rn::to_canonical(&z), "0");
        }
        // Rounding a value toward zero to nothing also yields the one zero.
        let tiny = rn::round(
            &rn::neg(&rn::parse_literal("0.4").unwrap()),
            0,
            rn::RoundingMode::TowardZero,
        )
        .unwrap();
        assert_eq!(tiny, zero);
        assert_eq!(rn::to_canonical(&tiny), "0");
    }
}

#[test]
fn results_are_normalized_within_the_limits() {
    // Req 8.22 and 8.23: every Ok result is in lowest terms with a positive denominator,
    // and the outcome depends only on the exact normalized result, never on the size of
    // an intermediate value (the operands here make cross products up to ~2^254).
    let max = bound();
    let big = Rat::from_parts(max.clone(), BigInt::from(1u8));
    let near = Rat::from_parts(&max - 1u8, max.clone());
    let ops: [(&str, Op, RtOp); 3] = [
        ("add", Rat::add, rn::add),
        ("sub", Rat::sub, rn::sub),
        ("mul", Rat::mul, rn::mul),
    ];
    for (name, model, real) in ops {
        for (a, b) in [(&big, &near), (&near, &near), (&big, &big), (&near, &big)] {
            let got = real(&to_rt(a).unwrap(), &to_rt(b).unwrap());
            match model(a, b) {
                Ok(w) => assert_eq!(rn::to_canonical(&got.unwrap()), w.canonical(), "{name}"),
                Err(k) => assert_eq!(Kind::from(got.unwrap_err()), k, "{name}"),
            }
        }
    }
    let mut r = Rng::new(SEED ^ 0x24);
    for _ in 0..CASES {
        let (a, b) = (operand(&mut r), operand(&mut r));
        if let Ok(w) = a.mul(&b) {
            let got = rn::mul(&to_rt(&a).unwrap(), &to_rt(&b).unwrap()).unwrap();
            // Canonical text of a lowest-terms value is itself in lowest terms.
            assert_eq!(rn::to_canonical(&got), w.canonical());
            assert!(!rn::to_canonical(&got).ends_with("/0"));
        }
    }
}

#[test]
fn operands_beyond_the_bound_are_rejected_not_wrapped() {
    let over = (bound() + 1u8).to_string();
    for text in [
        over.clone(),
        format!("-{over}"),
        format!("1/{over}"),
        format!("{over}/1"),
    ] {
        assert_eq!(
            rn::parse_canonical(&text).err(),
            Some(rn::NumericError::Overflow),
            "{text}"
        );
        assert_eq!(parse_canonical(&text).err(), Some(Kind::Overflow));
    }
}
