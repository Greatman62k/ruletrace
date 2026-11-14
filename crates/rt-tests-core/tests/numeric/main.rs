//! Differential numeric harness: `rt-numeric` against an independent exact-rational model
//! (task 3.5, Req 20.22, 8.11). The model is test-only code in `reference.rs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    missing_docs,
    dead_code
)]

mod harness;
mod laws;
mod reference;
mod rounding;
mod serialization;

use harness::*;
use reference::{Kind, Rat};

fn corpus(seed: u64) -> Vec<Rat> {
    let mut r = Rng::new(seed);
    let mut v = edge_values();
    v.extend((0..CASES).map(|_| operand(&mut r)));
    v
}

fn pairs(seed: u64) -> Vec<(Rat, Rat)> {
    let edges = edge_values();
    let mut out = Vec::new();
    for a in &edges {
        for b in &edges {
            out.push((a.clone(), b.clone()));
        }
    }
    let mut r = Rng::new(seed);
    out.extend((0..CASES).map(|_| (operand(&mut r), operand(&mut r))));
    out
}

#[test]
fn operands_round_trip_through_canonical_text() {
    for v in corpus(SEED) {
        to_rt(&v).unwrap();
    }
}

#[test]
fn binary_operations_and_comparison_agree_on_seed_corpus() {
    let (mut add, mut mul, mut div) = (Seen::default(), Seen::default(), Seen::default());
    for (a, b) in pairs(SEED) {
        check_pair(&a, &b).unwrap();
        add.note(&a.add(&b));
        mul.note(&a.mul(&b));
        div.note(&a.div(&b));
    }
    for seen in [&add, &mul, &div] {
        assert!(
            seen.ok > 100 && seen.overflow > 20,
            "corpus too narrow: {seen:?}"
        );
    }
}

#[test]
fn rounding_agrees_on_seed_corpus_for_all_places() {
    let mut r = Rng::new(SEED ^ 1);
    for v in corpus(SEED ^ 2) {
        check_round(&v, r.below(29) as u32).unwrap();
    }
    for v in edge_values() {
        for places in [0, 1, 2, 27, 28, 29, 100] {
            check_round(&v, places).unwrap();
        }
    }
}

#[test]
fn aggregation_agrees_on_seed_corpus() {
    let mut r = Rng::new(SEED ^ 4);
    let mut overflow = 0;
    check_aggregate(&[]).unwrap();
    for _ in 0..CASES {
        let len = r.below(9) as usize;
        let mut values: Vec<Rat> = (0..len).map(|_| operand(&mut r)).collect();
        if r.below(3) == 0 {
            // Force the running sum past the bound.
            let big = edge_values()[13].clone();
            values.insert(0, big.clone());
            values.push(big);
        }
        if Rat::sum(&values) == Err(Kind::Overflow) {
            overflow += 1;
        }
        check_aggregate(&values).unwrap();
    }
    assert!(overflow > 5, "no aggregate reached the bound: {overflow}");
}

#[test]
fn count_agrees_for_representable_sizes() {
    for n in [0usize, 1, 2, 1000, usize::MAX >> 1] {
        let got = rt_numeric::count(n).map(|c| rt_numeric::to_canonical(&c));
        assert_eq!(got, Ok(n.to_string()));
    }
}

#[test]
fn text_codecs_agree_on_tables_and_generated_literals() {
    let max = reference::bound().to_string();
    let over = (reference::bound() + 1u8).to_string();
    let mut texts: Vec<String> = [
        "0",
        "-0",
        "0.00",
        "1.5",
        "2",
        "0.3",
        "-2.5",
        "1/3",
        "-2/3",
        "2/4",
        "1/0",
        "0/5",
        "+1",
        "1e5",
        "1E5",
        "NaN",
        "-Infinity",
        "inf",
        "",
        "-",
        ".5",
        "5.",
        "1.2.3",
        "1 ",
        "--1",
        "1/-3",
        "1/3/4",
        "- 1",
        "0.1234567890123456789012345678",
        "0.12345678901234567890123456789",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    texts.push("9".repeat(38));
    texts.push("9".repeat(39));
    texts.push(format!("-{}.{}", "9".repeat(10), "9".repeat(28)));
    texts.push(max.clone());
    texts.push(over.clone());
    texts.push(format!("{max}/{max}"));
    texts.push(format!("1/{max}"));
    texts.push(format!("1/{over}"));
    texts.push(format!("{over}/3"));
    texts.push(format!("0.{}", "5".repeat(126)));
    texts.push(format!("0.{}", "5".repeat(127)));
    texts.push(format!("0.{}1", "0".repeat(125)));
    texts.push("0.00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001".to_owned());
    let mut r = Rng::new(SEED ^ 5);
    for v in corpus(SEED ^ 6) {
        texts.push(v.canonical());
    }
    for _ in 0..CASES {
        let int_len = r.below(40) as usize + 1;
        let frac_len = r.below(32) as usize;
        let digits = |r: &mut Rng, n: usize| -> String {
            (0..n)
                .map(|_| char::from(b'0' + r.below(10) as u8))
                .collect()
        };
        let mut int = digits(&mut r, int_len);
        if int.len() > 1 {
            int = int.trim_start_matches('0').to_owned();
            if int.is_empty() {
                int.push('0');
            }
        }
        let sign = if r.below(2) == 0 { "-" } else { "" };
        let frac = if frac_len == 0 {
            String::new()
        } else {
            format!(".{}", digits(&mut r, frac_len))
        };
        texts.push(format!("{sign}{int}{frac}"));
    }
    for text in &texts {
        check_text(text).unwrap();
    }
}
