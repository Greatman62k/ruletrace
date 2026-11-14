//! Deterministic generators and comparison functions of the differential
//! numeric harness.
//!
//! Operands are built from the reference model only, then converted through the
//! canonical text. Every conversion also checks that `rt-numeric` prints the
//! operand back to the same text, so a codec fault cannot hide inside operand
//! construction. Tasks 3.6 and 3.7 reuse these functions with their own case
//! counts.

use crate::reference::{bound, Kind, Rat};
use num_bigint::BigInt;
use rt_numeric as rn;
use rt_numeric::{ExactNumber, RoundingMode};
use std::cmp::Ordering;

/// Fixed seed of the corpus. Changing it changes the corpus, so do it
/// deliberately.
pub const SEED: u64 = 0x5254_4E55_4D31_0001;
/// Cases per operation (Req 20.22).
pub const CASES: usize = 256;
pub const MODES: [RoundingMode; 3] = [
    RoundingMode::HalfEven,
    RoundingMode::HalfAwayFromZero,
    RoundingMode::TowardZero,
];

/// SplitMix64: a small fixed generator, so the corpus is identical on every
/// platform.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-enough value in `0..n` (`n > 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    pub fn u127(&mut self) -> u128 {
        let v = (u128::from(self.next_u64()) << 64) | u128::from(self.next_u64());
        v >> 1
    }
}

fn signed(r: &mut Rng, v: BigInt) -> BigInt {
    if r.below(2) == 0 {
        v
    } else {
        -v
    }
}

/// Values at the edges of the limits and the worked examples of Req 8.
pub fn edge_values() -> Vec<Rat> {
    let max = bound();
    let one = BigInt::from(1u8);
    let m = |d: u128| BigInt::from(d);
    let mut v = vec![
        Rat::from_i128(0),
        Rat::from_i128(1),
        Rat::from_i128(-1),
        Rat::from_i128(2),
        Rat::frac(1, 2),
        Rat::frac(-1, 3),
        Rat::frac(2, 3),
        Rat::frac(5, 2),
        Rat::frac(-5, 2),
        Rat::frac(7, 2),
        Rat::frac(1, 8),
        Rat::frac(1, 10_i128.pow(28)),
        Rat::frac(10_i128.pow(37) - 1, 10_i128.pow(28)),
    ];
    for k in [0u128, 1, 2] {
        let big_n = &max - m(k);
        v.push(Rat::from_parts(big_n.clone(), one.clone()));
        v.push(Rat::from_parts(-big_n.clone(), one.clone()));
        v.push(Rat::from_parts(one.clone(), big_n.clone()));
        v.push(Rat::from_parts(-one.clone(), big_n.clone()));
        v.push(Rat::from_parts(&max - m(k + 1), &max - m(k)));
        v.push(Rat::from_parts(&max - m(k), &max - m(k + 1)));
    }
    // 2^126 and 5^54 denominators reach the 126-digit serialized expansion.
    v.push(Rat::from_parts(one.clone(), BigInt::from(1u8) << 126u32));
    v.push(Rat::from_parts(max.clone(), BigInt::from(1u8) << 126u32));
    v.push(Rat::from_parts(one, BigInt::from(5u8).pow(54)));
    v
}

/// One random in-range operand, biased toward the limits.
pub fn operand(r: &mut Rng) -> Rat {
    let max = bound();
    loop {
        let candidate = match r.below(9) {
            0 => Rat::from_i128(i128::from(r.below(41)) - 20),
            1 => {
                let n = i128::from(r.next_u64() >> 20) - (1i128 << 43);
                Rat::frac(n, i128::from(r.below(60)) + 1)
            }
            2 => {
                let int = r.below(10u64.pow(9));
                let frac = r.next_u64() % 10u64.pow(18);
                let places = r.below(19) as u32;
                let sign = if r.below(2) == 0 { "-" } else { "" };
                let tail = if places == 0 {
                    String::new()
                } else {
                    format!(
                        ".{:0>width$}",
                        frac % 10u64.pow(places),
                        width = places as usize
                    )
                };
                crate::reference::parse_literal(&format!("{sign}{int}{tail}"))
                    .unwrap_or_else(|_| Rat::from_i128(0))
            }
            3 => {
                let n = &max - BigInt::from(r.below(1000));
                let d = BigInt::from(r.below(30) + 1);
                Rat::from_parts(signed(r, n), d)
            }
            4 => {
                let n = BigInt::from(r.below(1000) + 1);
                let d = &max - BigInt::from(r.below(1000));
                Rat::from_parts(signed(r, n), d)
            }
            5 => {
                let n = BigInt::from(r.u127());
                let d = BigInt::from(r.u127() | 1);
                Rat::from_parts(signed(r, n), d)
            }
            6 => {
                let (a, b) = (r.below(60) as u32, r.below(40) as u32);
                let d = BigInt::from(2u8).pow(a) * BigInt::from(5u8).pow(b);
                let n = BigInt::from(r.u127());
                Rat::from_parts(signed(r, n), d)
            }
            7 => {
                let n = &max - BigInt::from(r.below(50));
                Rat::from_parts(signed(r, n), BigInt::from(1u8))
            }
            _ => {
                let n = BigInt::from(r.next_u64());
                let d = BigInt::from(r.next_u64() | 1);
                Rat::from_parts(signed(r, n), d)
            }
        };
        if candidate.in_range() {
            return candidate;
        }
    }
}

/// Converts a model value to an `rt-numeric` value and checks the round trip of
/// text.
pub fn to_rt(v: &Rat) -> Result<ExactNumber, String> {
    let text = v.canonical();
    let n = rn::parse_canonical(&text).map_err(|e| format!("parse_canonical({text}): {e}"))?;
    let back = rn::to_canonical(&n);
    if back != text {
        return Err(format!("to_canonical(parse_canonical({text})) = {back}"));
    }
    Ok(n)
}

fn outcome(r: Result<ExactNumber, rn::NumericError>) -> Result<String, Kind> {
    r.map(|n| rn::to_canonical(&n)).map_err(Kind::from)
}

fn expect(r: Result<Rat, Kind>) -> Result<String, Kind> {
    r.map(|v| v.canonical())
}

/// Compares an `rt-numeric` result with the model result, by canonical text and
/// by structural equality with the parsed model text (one zero form,
/// normalized).
fn agree(
    what: &str,
    got: Result<ExactNumber, rn::NumericError>,
    want: Result<Rat, Kind>,
) -> Result<(), String> {
    if let (Ok(g), Ok(w)) = (&got, &want) {
        let parsed = to_rt(w)?;
        if *g != parsed {
            return Err(format!(
                "{what}: structural inequality for {}",
                w.canonical()
            ));
        }
    }
    let (g, w) = (outcome(got), expect(want));
    if g == w {
        Ok(())
    } else {
        Err(format!("{what}: implementation {g:?}, reference {w:?}"))
    }
}

/// Checks `add`, `sub`, `mul`, `div`, `cmp` and `neg` on one pair.
pub fn check_pair(a: &Rat, b: &Rat) -> Result<(), String> {
    let (x, y) = (to_rt(a)?, to_rt(b)?);
    let tag = |op: &str| format!("{op}({}, {})", a.canonical(), b.canonical());
    agree(&tag("add"), rn::add(&x, &y), a.add(b))?;
    agree(&tag("sub"), rn::sub(&x, &y), a.sub(b))?;
    agree(&tag("mul"), rn::mul(&x, &y), a.mul(b))?;
    agree(&tag("div"), rn::div(&x, &y), a.div(b))?;
    let (got, want): (Ordering, Ordering) = (rn::cmp(&x, &y), a.cmp(b));
    if got != want {
        return Err(format!(
            "{}: implementation {got:?}, reference {want:?}",
            tag("cmp")
        ));
    }
    agree(
        &format!("neg({})", a.canonical()),
        Ok(rn::neg(&x)),
        Ok(a.neg()),
    )
}

/// Checks `round` for every mode at `places` (which may be out of range).
pub fn check_round(a: &Rat, places: u32) -> Result<(), String> {
    let x = to_rt(a)?;
    for mode in MODES {
        agree(
            &format!("round({}, {places}, {mode:?})", a.canonical()),
            rn::round(&x, places, mode),
            a.round(places, mode),
        )?;
    }
    Ok(())
}

/// Checks `sum`, `min`, `max` over a list.
pub fn check_aggregate(values: &[Rat]) -> Result<(), String> {
    let xs = values.iter().map(to_rt).collect::<Result<Vec<_>, _>>()?;
    let list = values
        .iter()
        .map(Rat::canonical)
        .collect::<Vec<_>>()
        .join(",");
    agree(&format!("sum([{list}])"), rn::sum(&xs), Rat::sum(values))?;
    for (name, got, want) in [
        ("min", rn::min(&xs), Rat::min(values)),
        ("max", rn::max(&xs), Rat::max(values)),
    ] {
        match (got, want) {
            (None, None) => {}
            (Some(g), Some(w)) => agree(&format!("{name}([{list}])"), Ok(g), Ok(w))?,
            (g, w) => return Err(format!("{name}([{list}]): {g:?} against {w:?}")),
        }
    }
    Ok(())
}

/// Checks `parse_literal` and `parse_canonical` on one text: same acceptance,
/// same value, same failure class.
pub fn check_text(text: &str) -> Result<(), String> {
    agree(
        &format!("parse_literal({text:?})"),
        rn::parse_literal(text),
        crate::reference::parse_literal(text),
    )?;
    agree(
        &format!("parse_canonical({text:?})"),
        rn::parse_canonical(text),
        crate::reference::parse_canonical(text),
    )
}

/// Counts of outcomes seen, so a test can prove the corpus reaches both sides
/// of the bound.
#[derive(Default, Debug)]
pub struct Seen {
    pub ok: usize,
    pub overflow: usize,
}

impl Seen {
    pub fn note(&mut self, r: &Result<Rat, Kind>) {
        match r {
            Ok(_) => self.ok += 1,
            Err(Kind::Overflow) => self.overflow += 1,
            Err(_) => {}
        }
    }
}
