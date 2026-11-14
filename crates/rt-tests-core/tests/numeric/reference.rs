//! Independent exact-rational reference model (design §7.3, Req 20.22).
//!
//! Shares no code with `rt-numeric`: values are `BigInt` pairs, gcd is Euclid's algorithm,
//! and every bound is read from `rt_numeric::limits`, the confirmed UD-3 constants. The
//! model never overflows internally. A result outside the limits is reported as
//! [`Kind::Overflow`], exactly as the specification states (Req 8.11).

use num_bigint::{BigInt, Sign};
use rt_numeric::{limits, NumericError, RoundingMode};
use std::cmp::Ordering;

/// Failure class shared by the model and the implementation, for comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Overflow,
    DivisionByZero,
    InvalidLiteral,
    InvalidPlaces,
}

impl From<NumericError> for Kind {
    fn from(e: NumericError) -> Kind {
        match e {
            NumericError::Overflow => Kind::Overflow,
            NumericError::DivisionByZero => Kind::DivisionByZero,
            NumericError::InvalidLiteral(_) => Kind::InvalidLiteral,
            NumericError::InvalidPlaces => Kind::InvalidPlaces,
        }
    }
}

/// A rational in lowest terms with a positive denominator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rat {
    n: BigInt,
    d: BigInt,
}

fn zero() -> BigInt {
    BigInt::from(0u8)
}

fn is_zero(x: &BigInt) -> bool {
    x.sign() == Sign::NoSign
}

fn abs(x: &BigInt) -> BigInt {
    if x.sign() == Sign::Minus {
        -x
    } else {
        x.clone()
    }
}

fn gcd(a: &BigInt, b: &BigInt) -> BigInt {
    let (mut a, mut b) = (abs(a), abs(b));
    while !is_zero(&b) {
        let r = &a % &b;
        a = b;
        b = r;
    }
    a
}

fn pow10(p: u32) -> BigInt {
    BigInt::from(10u8).pow(p)
}

/// The numerator and denominator bound of the confirmed limits.
pub fn bound() -> BigInt {
    let n = BigInt::from(limits::NUMERATOR_MAX);
    debug_assert_eq!(n, BigInt::from(limits::DENOMINATOR_MAX));
    n
}

impl Rat {
    /// Normalizes `n / d`. `None` when `d` is zero.
    pub fn new(n: BigInt, d: BigInt) -> Option<Rat> {
        if is_zero(&d) {
            return None;
        }
        let (n, d) = if d.sign() == Sign::Minus {
            (-n, -d)
        } else {
            (n, d)
        };
        let g = gcd(&n, &d);
        if is_zero(&n) {
            return Some(Rat {
                n: zero(),
                d: BigInt::from(1u8),
            });
        }
        Some(Rat {
            n: &n / &g,
            d: &d / &g,
        })
    }

    pub fn from_i128(v: i128) -> Rat {
        Rat {
            n: BigInt::from(v),
            d: BigInt::from(1u8),
        }
    }

    /// `n / d` from machine integers; `d` must be nonzero.
    pub fn frac(n: i128, d: i128) -> Rat {
        Rat::new(BigInt::from(n), BigInt::from(d)).expect("nonzero denominator")
    }

    pub fn from_parts(n: BigInt, d: BigInt) -> Rat {
        Rat::new(n, d).expect("nonzero denominator")
    }

    pub fn numerator(&self) -> &BigInt {
        &self.n
    }

    pub fn denominator(&self) -> &BigInt {
        &self.d
    }

    pub fn is_zero(&self) -> bool {
        is_zero(&self.n)
    }

    /// Whether both parts are within the confirmed limits.
    pub fn in_range(&self) -> bool {
        let b = bound();
        abs(&self.n) <= b && self.d <= b
    }

    fn checked(self) -> Result<Rat, Kind> {
        if self.in_range() {
            Ok(self)
        } else {
            Err(Kind::Overflow)
        }
    }

    pub fn add(&self, o: &Rat) -> Result<Rat, Kind> {
        Rat::new(&self.n * &o.d + &o.n * &self.d, &self.d * &o.d)
            .ok_or(Kind::Overflow)?
            .checked()
    }

    pub fn sub(&self, o: &Rat) -> Result<Rat, Kind> {
        self.add(&o.neg())
    }

    pub fn mul(&self, o: &Rat) -> Result<Rat, Kind> {
        Rat::new(&self.n * &o.n, &self.d * &o.d)
            .ok_or(Kind::Overflow)?
            .checked()
    }

    pub fn div(&self, o: &Rat) -> Result<Rat, Kind> {
        if o.is_zero() {
            return Err(Kind::DivisionByZero);
        }
        Rat::new(&self.n * &o.d, &self.d * &o.n)
            .ok_or(Kind::Overflow)?
            .checked()
    }

    pub fn neg(&self) -> Rat {
        Rat {
            n: -&self.n,
            d: self.d.clone(),
        }
    }

    pub fn cmp(&self, o: &Rat) -> Ordering {
        (&self.n * &o.d).cmp(&(&o.n * &self.d))
    }

    /// Left fold of `add` from zero, as the specification of aggregation states.
    pub fn sum(values: &[Rat]) -> Result<Rat, Kind> {
        values
            .iter()
            .try_fold(Rat::from_i128(0), |acc, v| acc.add(v))
    }

    pub fn min(values: &[Rat]) -> Option<Rat> {
        values
            .iter()
            .fold(None::<&Rat>, |m, v| match m {
                Some(m) if m.cmp(v) != Ordering::Greater => Some(m),
                _ => Some(v),
            })
            .cloned()
    }

    pub fn max(values: &[Rat]) -> Option<Rat> {
        values
            .iter()
            .fold(None::<&Rat>, |m, v| match m {
                Some(m) if m.cmp(v) != Ordering::Less => Some(m),
                _ => Some(v),
            })
            .cloned()
    }

    /// Rounds to `places` decimal places (0 to 28) with the declared mode.
    pub fn round(&self, places: u32, mode: RoundingMode) -> Result<Rat, Kind> {
        if places > limits::ROUNDING_MAX_PLACES {
            return Err(Kind::InvalidPlaces);
        }
        let scale = pow10(places);
        let scaled = abs(&self.n) * &scale;
        let q = &scaled / &self.d;
        let r = &scaled % &self.d;
        let twice = &r * BigInt::from(2u8);
        let up = match mode {
            RoundingMode::TowardZero => false,
            RoundingMode::HalfAwayFromZero => twice >= self.d,
            RoundingMode::HalfEven => match twice.cmp(&self.d) {
                Ordering::Greater => true,
                Ordering::Less => false,
                Ordering::Equal => &q % BigInt::from(2u8) == BigInt::from(1u8),
            },
        };
        let q = if up { q + BigInt::from(1u8) } else { q };
        let signed = if self.n.sign() == Sign::Minus { -q } else { q };
        Rat::new(signed, scale).ok_or(Kind::Overflow)?.checked()
    }

    /// Canonical text: a decimal when the denominator has only the factors 2 and 5,
    /// otherwise `n/d`; zero is `0`.
    pub fn canonical(&self) -> String {
        if self.is_zero() {
            return "0".to_owned();
        }
        let (mut twos, mut fives) = (0u32, 0u32);
        let mut rest = self.d.clone();
        let (two, five) = (BigInt::from(2u8), BigInt::from(5u8));
        while is_zero(&(&rest % &two)) {
            rest = &rest / &two;
            twos += 1;
        }
        while is_zero(&(&rest % &five)) {
            rest = &rest / &five;
            fives += 1;
        }
        let sign = if self.n.sign() == Sign::Minus {
            "-"
        } else {
            ""
        };
        if rest != BigInt::from(1u8) {
            return format!("{}/{}", self.n, self.d);
        }
        let k = twos.max(fives);
        let digits = (abs(&self.n) * pow10(k) / &self.d).to_string();
        let k = k as usize;
        let padded = if digits.len() <= k {
            format!("{}{}", "0".repeat(k + 1 - digits.len()), digits)
        } else {
            digits
        };
        let (int, frac) = padded.split_at(padded.len() - k);
        let frac = frac.trim_end_matches('0');
        if frac.is_empty() {
            format!("{sign}{int}")
        } else {
            format!("{sign}{int}.{frac}")
        }
    }
}

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn big(s: &str) -> BigInt {
    s.parse::<BigInt>().expect("digits only")
}

/// Splits `-?digits(.digits)?` into sign, integer digits and fraction digits.
fn split_decimal(text: &str) -> Option<(bool, &str, &str)> {
    let (neg, body) = match text.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, text),
    };
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i, f),
        None => (body, ""),
    };
    let point_ok = !body.contains('.') || !frac.is_empty();
    (all_digits(int) && point_ok && (frac.is_empty() || all_digits(frac)))
        .then_some((neg, int, frac))
}

fn decimal_value(neg: bool, int: &str, frac: &str) -> Rat {
    let m = big(&format!("{int}{frac}"));
    let m = if neg { -m } else { m };
    Rat::from_parts(m, pow10(frac.len() as u32))
}

/// Reference for `parse_literal` (Req 8.27): at most 38 significant digits and 28
/// fractional digits, no `+`, exponent, fraction, `NaN` or `Infinity`.
pub fn parse_literal(text: &str) -> Result<Rat, Kind> {
    let (neg, int, frac) = split_decimal(text).ok_or(Kind::InvalidLiteral)?;
    let significant = format!("{int}{frac}").trim_start_matches('0').len();
    if significant > limits::LITERAL_MAX_DIGITS || frac.len() > limits::LITERAL_MAX_FRACTION_DIGITS
    {
        return Err(Kind::InvalidLiteral);
    }
    decimal_value(neg, int, frac).checked()
}

/// Reference for `parse_canonical` (Req 8.16, 8.24, 8.25): a decimal with at most 126
/// fractional digits, or `n/d` with a nonzero `d`.
pub fn parse_canonical(text: &str) -> Result<Rat, Kind> {
    if let Some((n, d)) = text.split_once('/') {
        let (neg, n) = match n.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, n),
        };
        if !all_digits(n) || !all_digits(d) || is_zero(&big(d)) {
            return Err(Kind::InvalidLiteral);
        }
        let n = if neg { -big(n) } else { big(n) };
        return Rat::new(n, big(d)).ok_or(Kind::InvalidLiteral)?.checked();
    }
    let (neg, int, frac) = split_decimal(text).ok_or(Kind::InvalidLiteral)?;
    if frac.len() > limits::SERIALIZED_MAX_FRACTION_DIGITS {
        return Err(Kind::InvalidLiteral);
    }
    decimal_value(neg, int, frac).checked()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(s: &str) -> Rat {
        parse_literal(s).unwrap()
    }

    #[test]
    fn rounding_tables_of_req_8_28() {
        let modes = [
            RoundingMode::HalfEven,
            RoundingMode::HalfAwayFromZero,
            RoundingMode::TowardZero,
        ];
        let table: [(&str, u32, [&str; 3]); 4] = [
            ("2.5", 0, ["2", "3", "2"]),
            ("-2.5", 0, ["-2", "-3", "-2"]),
            ("3.5", 0, ["4", "4", "3"]),
            ("0.125", 2, ["0.12", "0.13", "0.12"]),
        ];
        for (input, places, expected) in table {
            for (mode, want) in modes.iter().zip(expected) {
                let got = lit(input).round(places, *mode).unwrap().canonical();
                assert_eq!(got, want, "{input} {places} {mode:?}");
            }
        }
    }

    #[test]
    fn canonical_forms() {
        assert_eq!(Rat::frac(1, 3).canonical(), "1/3");
        assert_eq!(Rat::frac(-2, 3).canonical(), "-2/3");
        assert_eq!(Rat::frac(1, 8).canonical(), "0.125");
        assert_eq!(Rat::frac(7, 1).canonical(), "7");
        assert_eq!(Rat::frac(-1, 2).canonical(), "-0.5");
        assert_eq!(lit("-0").canonical(), "0");
        assert_eq!(lit("0.00").canonical(), "0");
    }

    #[test]
    fn bounds_and_literals() {
        let max = Rat::from_parts(bound(), BigInt::from(1u8));
        assert!(max.add(&Rat::from_i128(1)).is_err());
        assert!(max.in_range());
        assert!(parse_literal("+1").is_err());
        assert!(parse_literal("1e5").is_err());
        assert!(parse_literal("1/3").is_err());
        assert!(parse_canonical("1/3").is_ok());
        assert!(parse_literal(&format!("0.{}", "1".repeat(29))).is_err());
        assert!(parse_literal(&"9".repeat(38)).is_ok());
        assert!(parse_literal(&"9".repeat(39)).is_err());
    }
}
