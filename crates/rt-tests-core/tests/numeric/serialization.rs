//! Property 18: numeric serialization round trip and literal limits (task 3.7).
//!
//! **Validates: Requirements 8.16, 8.24, 8.25, 8.27, 3.6, 4.4, 8.7**
//!
//! Same deterministic corpus style as the other numeric modules: a fixed seed, 256 random
//! cases per property plus the edge values, compared with the independent reference model.
//!
//! Decision on unreduced canonical fractions (open point of task 3.6): `parse_canonical`
//! reduces to lowest terms first and applies the bound to the reduced value, matching the
//! reference. The codec holds each part of a fraction in a `u128`, so a text part wider
//! than `u128` is `Overflow` even if the reduced value would be in range. That width limit
//! (39 digits) is part of the codec's bounded text domain; canonical text never needs it,
//! because every in-range value in lowest terms has parts below 2^127.

use crate::harness::{to_rt, Rng, CASES, SEED};
use crate::reference::{self, bound, Kind, Rat};
use crate::{corpus, edge_values};
use num_bigint::BigInt;
use rt_numeric as rn;
use rt_numeric::{LiteralFault, NumericError};

fn fault(f: LiteralFault) -> Result<rn::ExactNumber, NumericError> {
    Err(NumericError::InvalidLiteral(f))
}

fn digits(r: &mut Rng, n: usize) -> String {
    (0..n)
        .map(|_| char::from(b'0' + r.below(10) as u8))
        .collect()
}

/// Req 8.16, 8.24: serialize then parse returns the same value, and the text is unique.
#[test]
fn canonical_text_round_trips_and_is_unique() {
    let values = corpus(SEED ^ 0x31);
    let mut seen: std::collections::BTreeMap<String, Rat> = std::collections::BTreeMap::new();
    for v in &values {
        let n = to_rt(v).unwrap();
        let text = rn::to_canonical(&n);
        assert_eq!(text, v.canonical());
        assert_eq!(rn::parse_canonical(&text).unwrap(), n, "{text}");
        // The text is unique: equal text means equal value (and the reverse by `canonical`).
        if let Some(prev) = seen.insert(text.clone(), v.clone()) {
            assert_eq!(&prev, v, "two values share the text {text}");
        }
    }
    for a in &values {
        for b in values.iter().take(40) {
            let (ta, tb) = (a.canonical(), b.canonical());
            assert_eq!(a == b, ta == tb, "{ta} against {tb}");
        }
    }
}

/// Req 8.16: non-canonical spellings of a value parse to it and print canonically.
#[test]
fn non_canonical_spellings_normalize() {
    let mut r = Rng::new(SEED ^ 0x32);
    for v in corpus(SEED ^ 0x33) {
        let want = to_rt(&v).unwrap();
        let canon = v.canonical();
        let mut spellings = vec![];
        if !canon.contains('/') {
            let (sign, body) = match canon.strip_prefix('-') {
                Some(b) => ("-", b),
                None => ("", canon.as_str()),
            };
            let frac_len = body.split_once('.').map_or(0, |(_, f)| f.len());
            let zeros = "0".repeat(1 + r.below(3) as usize);
            let tail = if body.contains('.') {
                zeros
            } else {
                format!(".{zeros}")
            };
            // Trailing zeros must stay within the 126 fractional digit limit.
            if frac_len + 3 <= 126 {
                spellings.push(format!("{sign}{body}{tail}"));
            }
            spellings.push(format!(
                "{sign}{}{body}",
                "0".repeat(1 + r.below(3) as usize)
            ));
        }
        for s in spellings {
            let got = rn::parse_canonical(&s).unwrap();
            assert_eq!(got, want, "{s}");
            assert_eq!(rn::to_canonical(&got), canon);
        }
    }
    for zero in ["0", "-0", "0.0", "-0.00", "00", "0/5", "-0/5"] {
        let z = rn::parse_canonical(zero).unwrap();
        assert_eq!(rn::to_canonical(&z), "0", "{zero}");
        assert_eq!(z, rn::ExactNumber::ZERO);
    }
}

/// Unreduced fractions whose parts exceed the bound before reduction, but not `u128`.
#[test]
fn unreduced_fractions_reduce_before_the_bound() {
    let mut r = Rng::new(SEED ^ 0x34);
    let cap = BigInt::from(u128::MAX);
    let mut beyond_bound = 0;
    let mut values = edge_values();
    values.extend(corpus(SEED ^ 0x35));
    for v in values {
        let (n, d) = (v.numerator().clone(), v.denominator().clone());
        let abs_n = if n < BigInt::from(0u8) {
            -n.clone()
        } else {
            n.clone()
        };
        let widest = if abs_n > d { abs_n } else { d.clone() };
        let max_k = &cap / &widest;
        if max_k < BigInt::from(2u8) {
            continue;
        }
        let k = BigInt::from(2u8) + BigInt::from(r.u127()) % (&max_k - BigInt::from(1u8));
        let text = format!("{}/{}", &n * &k, &d * &k);
        if (&d * &k) > bound() {
            beyond_bound += 1;
        }
        let want = to_rt(&v).unwrap();
        assert_eq!(rn::parse_canonical(&text), Ok(want), "{text}");
        assert_eq!(reference::parse_canonical(&text), Ok(v.clone()), "{text}");
    }
    assert!(
        beyond_bound > 20,
        "corpus never exceeded the bound: {beyond_bound}"
    );
}

/// The documented width limit: a fraction part wider than `u128` is `Overflow`.
#[test]
fn fraction_parts_wider_than_u128_are_overflow() {
    let wide = (BigInt::from(u128::MAX) + 1u8).to_string();
    for text in [
        format!("{wide}/{wide}"),
        format!("1/{wide}"),
        format!("{wide}/1"),
    ] {
        assert_eq!(
            rn::parse_canonical(&text),
            Err(NumericError::Overflow),
            "{text}"
        );
    }
}

/// Req 8.27, 3.6, 4.4, 8.7: a literal is accepted exactly when it is an optionally
/// negative decimal with at most 38 significant digits and 28 fractional digits.
#[test]
fn literals_are_accepted_exactly_within_the_limits() {
    let mut r = Rng::new(SEED ^ 0x36);
    let (mut accepted, mut too_many, mut too_long_frac) = (0, 0, 0);
    for _ in 0..CASES * 4 {
        let int_len = r.below(42) as usize + 1;
        let frac_len = r.below(33) as usize;
        let lead = r.below(3) as usize;
        let mut int = format!("{}{}", "0".repeat(lead), digits(&mut r, int_len));
        if r.below(4) == 0 {
            int = "0".to_owned();
        }
        let frac = digits(&mut r, frac_len);
        let sign = if r.below(2) == 0 { "-" } else { "" };
        let dot = if frac_len == 0 {
            String::new()
        } else {
            format!(".{frac}")
        };
        let text = format!("{sign}{int}{dot}");
        let sig = format!("{int}{frac}").trim_start_matches('0').len();
        let result = rn::parse_literal(&text);
        if sig <= 38 && frac_len <= 28 {
            accepted += 1;
            let want = reference::parse_literal(&text).unwrap();
            let got = result.unwrap_or_else(|e| panic!("{text}: {e:?}"));
            assert_eq!(rn::to_canonical(&got), want.canonical(), "{text}");
            assert_eq!(rn::parse_canonical(&rn::to_canonical(&got)).unwrap(), got);
        } else if sig > 38 {
            too_many += 1;
            assert_eq!(result, fault(LiteralFault::TooManyDigits), "{text}");
        } else {
            too_long_frac += 1;
            assert_eq!(result, fault(LiteralFault::TooManyFractionDigits), "{text}");
        }
    }
    assert!(accepted > 100 && too_many > 100 && too_long_frac > 20);
}

#[test]
fn literal_limit_boundaries() {
    let max_digits = "9".repeat(38);
    assert!(rn::parse_literal(&max_digits).is_ok());
    assert_eq!(
        rn::parse_literal(&"9".repeat(39)),
        fault(LiteralFault::TooManyDigits)
    );
    let frac28 = format!("0.{}", "1".repeat(28));
    assert!(rn::parse_literal(&frac28).is_ok());
    assert_eq!(
        rn::parse_literal(&format!("0.{}", "1".repeat(29))),
        fault(LiteralFault::TooManyFractionDigits)
    );
    // 38 digits with 28 fractional digits (10 integer digits).
    let both = format!("{}.{}", "9".repeat(10), "9".repeat(28));
    assert!(rn::parse_literal(&both).is_ok());
    assert!(rn::parse_literal(&format!("-{both}")).is_ok());
}

/// Req 8.25, 8.27: no fraction, exponent, plus, `NaN` or `Infinity` in a literal, while
/// the canonical parser accepts the fraction form.
#[test]
fn literals_reject_other_forms_and_canonical_accepts_fractions() {
    let mut r = Rng::new(SEED ^ 0x37);
    for v in corpus(SEED ^ 0x38) {
        let text = v.canonical();
        if text.contains('/') {
            assert_eq!(rn::parse_literal(&text), fault(LiteralFault::Fraction));
            assert_eq!(rn::parse_canonical(&text), Ok(to_rt(&v).unwrap()));
        } else {
            let plus = format!("+{}", text.trim_start_matches('-'));
            assert_eq!(rn::parse_literal(&plus), fault(LiteralFault::LeadingPlus));
            let exp = format!("{text}e{}", r.below(5));
            assert_eq!(rn::parse_literal(&exp), fault(LiteralFault::Exponent));
        }
    }
    for t in ["NaN", "-NaN", "Infinity", "-Infinity", "inf"] {
        assert_eq!(rn::parse_literal(t), fault(LiteralFault::NotFinite), "{t}");
    }
    for t in [
        "", "-", ".5", "5.", "1.2.3", " 1", "1 ", "--1", "0x10", "1_0",
    ] {
        assert!(rn::parse_literal(t).is_err(), "{t:?}");
    }
}

/// Req 8.24: serialized decimals carry at most 126 fractional digits; one more is rejected.
#[test]
fn serialized_decimal_fraction_limit() {
    let at = format!("0.{}1", "0".repeat(125));
    let over = format!("0.{}1", "0".repeat(126));
    // Within the digit limit, but 10^-126 has a denominator beyond the bound.
    assert_eq!(rn::parse_canonical(&at), Err(NumericError::Overflow));
    assert_eq!(
        rn::parse_canonical(&over),
        fault(LiteralFault::TooManyFractionDigits)
    );
    let twos = Rat::from_parts(BigInt::from(1u8), BigInt::from(1u8) << 126u32);
    assert_eq!(twos.canonical().len(), 128);
    assert_eq!(rn::to_canonical(&to_rt(&twos).unwrap()), twos.canonical());
    assert_eq!(Kind::from(NumericError::Overflow), Kind::Overflow);
}
