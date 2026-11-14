//! Exact numbers for Rule_IR, compilation and execution (class core, std-only).
//!
//! The public API is representation-independent: callers see only the opaque
//! [`ExactNumber`], [`NumericError`], [`RoundingMode`], the [`limits`] constants and the
//! free functions below. The representation confirmed in `tools/decisions/UD-3.md` is a
//! bounded rational (R-128). A change of representation stays inside this crate.
//!
//! No floating-point type appears in any public item. Every operation that can fail
//! returns a [`NumericError`]; nothing wraps, saturates or rounds silently.
//!
//! Status (task 3.2): checked arithmetic, comparison, `sum`, `min`, `max` and `count` are
//! implemented, and so are literal parsing and canonical text (task 3.3) and `round`
//! (task 3.4).
#![forbid(unsafe_code)]

mod text;
mod wide;

use std::cmp::Ordering;
use std::fmt;

/// Bounds from the confirmed `ExactNumber_Limits` record (`tools/decisions/UD-3.md`).
pub mod limits {
    /// Largest numerator magnitude: 2^127 - 1.
    pub const NUMERATOR_MAX: u128 = (1u128 << 127) - 1;
    /// Largest denominator: 2^127 - 1. The smallest is 1.
    pub const DENOMINATOR_MAX: u128 = (1u128 << 127) - 1;
    /// Largest number of significant digits in a decimal literal.
    pub const LITERAL_MAX_DIGITS: usize = 38;
    /// Largest number of fractional digits in a decimal literal.
    pub const LITERAL_MAX_FRACTION_DIGITS: usize = 28;
    /// Largest number of fractional digits in a serialized decimal.
    pub const SERIALIZED_MAX_FRACTION_DIGITS: usize = 126;
    /// Smallest number of rounding places.
    pub const ROUNDING_MIN_PLACES: u32 = 0;
    /// Largest number of rounding places.
    pub const ROUNDING_MAX_PLACES: u32 = 28;
    /// Width in bits of checked intermediate values.
    pub const CHECKED_INTERMEDIATE_BITS: u32 = 256;
}

/// Sign of an [`ExactNumber`]. `Zero` if and only if the magnitude is zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Sign {
    Zero,
    Positive,
    Negative,
}

/// An exact number, always normalized: lowest terms, positive denominator, one zero
/// (`0/1`). Fields are private, so structural equality is value equality.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExactNumber {
    sign: Sign,
    num: u128,
    den: u128,
}

impl ExactNumber {
    /// The number zero.
    pub const ZERO: ExactNumber = ExactNumber {
        sign: Sign::Zero,
        num: 0,
        den: 1,
    };

    /// Whether the value is zero.
    pub fn is_zero(&self) -> bool {
        self.sign == Sign::Zero
    }
}

impl Default for ExactNumber {
    fn default() -> Self {
        Self::ZERO
    }
}

/// Why a numeric literal was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiteralFault {
    /// The text is empty or has no digits.
    Empty,
    /// A character other than digits, one `.` and a leading `-`.
    BadCharacter,
    /// A leading `+`.
    LeadingPlus,
    /// An exponent (`e` or `E`).
    Exponent,
    /// A fraction such as `1/3` where only a decimal is accepted.
    Fraction,
    /// `NaN` or `Infinity`.
    NotFinite,
    /// More than 38 significant digits.
    TooManyDigits,
    /// More than 28 fractional digits.
    TooManyFractionDigits,
}

/// Failure of a numeric operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericError {
    /// A result or intermediate value exceeds the bounds in [`limits`].
    Overflow,
    /// Division by zero.
    DivisionByZero,
    /// A literal or canonical text is not accepted.
    InvalidLiteral(LiteralFault),
    /// Rounding places outside 0 to 28.
    InvalidPlaces,
}

impl fmt::Display for NumericError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NumericError::Overflow => f.write_str("numeric overflow"),
            NumericError::DivisionByZero => f.write_str("division by zero"),
            NumericError::InvalidLiteral(fault) => write!(f, "invalid numeric literal: {fault:?}"),
            NumericError::InvalidPlaces => f.write_str("rounding places outside 0 to 28"),
        }
    }
}

impl std::error::Error for NumericError {}

/// Rounding mode declared by a rule (Rule_IR names `half_even`, `half_away_from_zero`,
/// `toward_zero`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RoundingMode {
    /// Nearest; ties go to the even last digit.
    HalfEven,
    /// Nearest; ties go away from zero.
    HalfAwayFromZero,
    /// Truncate.
    TowardZero,
}

impl RoundingMode {
    /// The Rule_IR name of the mode.
    pub fn name(self) -> &'static str {
        match self {
            RoundingMode::HalfEven => "half_even",
            RoundingMode::HalfAwayFromZero => "half_away_from_zero",
            RoundingMode::TowardZero => "toward_zero",
        }
    }

    /// Parses a Rule_IR mode name. Any other text gives `None`.
    pub fn from_name(name: &str) -> Option<RoundingMode> {
        match name {
            "half_even" => Some(RoundingMode::HalfEven),
            "half_away_from_zero" => Some(RoundingMode::HalfAwayFromZero),
            "toward_zero" => Some(RoundingMode::TowardZero),
            _ => None,
        }
    }
}

/// Builds a number from an already reduced `num/den` (`den > 0`, coprime). Zero gives the
/// single zero; a part beyond the bounds gives [`NumericError::Overflow`].
fn make(negative: bool, num: u128, den: u128) -> Result<ExactNumber, NumericError> {
    if num == 0 {
        return Ok(ExactNumber::ZERO);
    }
    if num > limits::NUMERATOR_MAX || den > limits::DENOMINATOR_MAX || den == 0 {
        return Err(NumericError::Overflow);
    }
    let sign = if negative {
        Sign::Negative
    } else {
        Sign::Positive
    };
    Ok(ExactNumber { sign, num, den })
}

impl ExactNumber {
    fn is_negative(&self) -> bool {
        self.sign == Sign::Negative
    }
}

/// `a + b`, exact or [`NumericError::Overflow`].
///
/// Knuth, TAOCP 4.5.1: with `g = gcd(b, d)` the sum `(a d/g + c b/g) / (b d/g)` reduces
/// only by a divisor of `g`. The numerator needs at most 255 bits; the unreduced
/// denominator is below 2^254, so the bound applies to the reduced result only.
pub fn add(a: &ExactNumber, b: &ExactNumber) -> Result<ExactNumber, NumericError> {
    if a.is_zero() {
        return Ok(b.clone());
    }
    if b.is_zero() {
        return Ok(a.clone());
    }
    let g = wide::gcd(a.den, b.den);
    let (a_den_r, b_den_r) = (a.den / g, b.den / g);
    let t1 = wide::mul_wide(a.num, b_den_r);
    let t2 = wide::mul_wide(b.num, a_den_r);
    let (negative, numerator) = if a.sign == b.sign {
        (
            a.is_negative(),
            t1.checked_add(t2).ok_or(NumericError::Overflow)?,
        )
    } else {
        match t1.cmp(&t2) {
            Ordering::Equal => return Ok(ExactNumber::ZERO),
            Ordering::Greater => (
                a.is_negative(),
                t1.checked_sub(t2).ok_or(NumericError::Overflow)?,
            ),
            Ordering::Less => (
                b.is_negative(),
                t2.checked_sub(t1).ok_or(NumericError::Overflow)?,
            ),
        }
    };
    let (_, rem) = numerator.div_rem_u128(g).ok_or(NumericError::Overflow)?;
    let g2 = wide::gcd(g, rem);
    let (num, _) = numerator.div_rem_u128(g2).ok_or(NumericError::Overflow)?;
    let (den, _) = wide::mul_wide(b_den_r, a.den)
        .div_rem_u128(g2)
        .ok_or(NumericError::Overflow)?;
    match (num.to_u128(), den.to_u128()) {
        (Some(n), Some(d)) => make(negative, n, d),
        _ => Err(NumericError::Overflow),
    }
}

/// `a - b`, exact or [`NumericError::Overflow`].
pub fn sub(a: &ExactNumber, b: &ExactNumber) -> Result<ExactNumber, NumericError> {
    add(a, &neg(b))
}

/// `a * b`, exact or [`NumericError::Overflow`]. Cross-cancels first, so the product is
/// already in lowest terms and only the bound check can fail.
pub fn mul(a: &ExactNumber, b: &ExactNumber) -> Result<ExactNumber, NumericError> {
    if a.is_zero() || b.is_zero() {
        return Ok(ExactNumber::ZERO);
    }
    let g1 = wide::gcd(a.num, b.den);
    let g2 = wide::gcd(b.num, a.den);
    let num = (a.num / g1)
        .checked_mul(b.num / g2)
        .ok_or(NumericError::Overflow)?;
    let den = (a.den / g2)
        .checked_mul(b.den / g1)
        .ok_or(NumericError::Overflow)?;
    make(a.sign != b.sign, num, den)
}

/// `a / b`, exact; a zero divisor gives [`NumericError::DivisionByZero`].
pub fn div(a: &ExactNumber, b: &ExactNumber) -> Result<ExactNumber, NumericError> {
    if b.is_zero() {
        return Err(NumericError::DivisionByZero);
    }
    // The reciprocal of a normalized number is normalized and within the same bounds.
    let reciprocal = ExactNumber {
        sign: b.sign,
        num: b.den,
        den: b.num,
    };
    mul(a, &reciprocal)
}

/// Total order of values. Exact and cannot fail.
pub fn cmp(a: &ExactNumber, b: &ExactNumber) -> Ordering {
    let rank = |s: Sign| match s {
        Sign::Negative => 0u8,
        Sign::Zero => 1,
        Sign::Positive => 2,
    };
    match rank(a.sign).cmp(&rank(b.sign)) {
        Ordering::Equal if a.is_zero() => Ordering::Equal,
        Ordering::Equal => {
            let magnitude = wide::mul_wide(a.num, b.den).cmp(&wide::mul_wide(b.num, a.den));
            if a.is_negative() {
                magnitude.reverse()
            } else {
                magnitude
            }
        }
        other => other,
    }
}

/// `-a`. Sign-and-magnitude makes this exact and infallible.
pub fn neg(a: &ExactNumber) -> ExactNumber {
    let sign = match a.sign {
        Sign::Zero => Sign::Zero,
        Sign::Positive => Sign::Negative,
        Sign::Negative => Sign::Positive,
    };
    ExactNumber {
        sign,
        num: a.num,
        den: a.den,
    }
}

/// Left fold of [`add`] over `values` in the given order; the empty sum is zero.
pub fn sum(values: &[ExactNumber]) -> Result<ExactNumber, NumericError> {
    values
        .iter()
        .try_fold(ExactNumber::ZERO, |acc, v| add(&acc, v))
}

/// The smallest of `values`, or `None` when empty (the caller reports RT-DGN-0005).
pub fn min(values: &[ExactNumber]) -> Option<ExactNumber> {
    let mut it = values.iter();
    let first = it.next()?;
    Some(
        it.fold(
            first,
            |m, v| if cmp(v, m) == Ordering::Less { v } else { m },
        )
        .clone(),
    )
}

/// The largest of `values`, or `None` when empty (the caller reports RT-DGN-0005).
pub fn max(values: &[ExactNumber]) -> Option<ExactNumber> {
    let mut it = values.iter();
    let first = it.next()?;
    Some(
        it.fold(
            first,
            |m, v| if cmp(v, m) == Ordering::Greater { v } else { m },
        )
        .clone(),
    )
}

/// The integer `n` as an exact number, or [`NumericError::Overflow`] beyond the bound.
pub fn count(n: usize) -> Result<ExactNumber, NumericError> {
    let n = u128::try_from(n).map_err(|_| NumericError::Overflow)?;
    make(false, n, 1)
}

/// Rounds `a` to `places` (0 to 28) with `mode`. Other places give
/// [`NumericError::InvalidPlaces`]; a result beyond the bounds gives
/// [`NumericError::Overflow`].
///
/// Design §4.2.3: the scaled numerator `num * 10^places` (below 2^221) is divided by the
/// denominator into a quotient and a remainder `r < den`. `2r` is compared with `den`
/// (it fits in `u128` because `den < 2^127`), the mode decides whether the magnitude
/// goes up by one unit, and the result `q / 10^places` is reduced by `gcd(10^places,
/// q mod 10^places)`. Rounding is applied only where the caller declares it.
pub fn round(
    a: &ExactNumber,
    places: u32,
    mode: RoundingMode,
) -> Result<ExactNumber, NumericError> {
    if places > limits::ROUNDING_MAX_PLACES {
        return Err(NumericError::InvalidPlaces);
    }
    let scale = 10u128
        .checked_pow(places)
        .ok_or(NumericError::InvalidPlaces)?;
    if a.is_zero() {
        return Ok(ExactNumber::ZERO);
    }
    let (mut q, r) = wide::mul_wide(a.num, scale)
        .div_rem_u128(a.den)
        .ok_or(NumericError::Overflow)?;
    // `r < den < 2^127`, so `2r` cannot overflow.
    let twice = r << 1;
    let up = match mode {
        RoundingMode::TowardZero => false,
        RoundingMode::HalfAwayFromZero => twice >= a.den,
        RoundingMode::HalfEven => match twice.cmp(&a.den) {
            Ordering::Greater => true,
            Ordering::Less => false,
            Ordering::Equal => q.lo & 1 == 1,
        },
    };
    if up {
        q = q
            .checked_add(wide::U256 { hi: 0, lo: 1 })
            .ok_or(NumericError::Overflow)?;
    }
    let (_, rem) = q.div_rem_u128(scale).ok_or(NumericError::Overflow)?;
    let g = wide::gcd(scale, rem);
    let (num, _) = q.div_rem_u128(g).ok_or(NumericError::Overflow)?;
    let num = num.to_u128().ok_or(NumericError::Overflow)?;
    make(a.is_negative(), num, scale / g)
}

/// Parses a decimal literal: optional `-`, digits, optional `.` and digits; at most 38
/// significant and 28 fractional digits; no exponent, `+`, `NaN`, `Infinity` or fraction.
/// `-0` and `0.00` give zero. Used for Rule_IR, Competition_Data and Published_Result.
pub fn parse_literal(text: &str) -> Result<ExactNumber, NumericError> {
    text::parse_literal(text)
}

/// The canonical text: a decimal when the denominator has only the factors 2 and 5
/// (no exponent, no trailing fractional zeros, zero is `0`), otherwise `n/d` in lowest
/// terms with the minus sign on the numerator only.
pub fn to_canonical(a: &ExactNumber) -> String {
    text::to_canonical(a)
}

/// Parses canonical text from a serialized artifact: a decimal with up to 126 fractional
/// digits, or `n/d`. A fraction is accepted here only; [`parse_literal`] rejects it. A
/// fraction not in lowest terms is accepted and normalized, so the value is exact.
pub fn parse_canonical(text: &str) -> Result<ExactNumber, NumericError> {
    text::parse_canonical(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_match_the_confirmed_record() {
        assert_eq!(
            limits::NUMERATOR_MAX,
            170141183460469231731687303715884105727
        );
        assert_eq!(limits::DENOMINATOR_MAX, limits::NUMERATOR_MAX);
        assert_eq!(limits::LITERAL_MAX_DIGITS, 38);
        assert_eq!(limits::LITERAL_MAX_FRACTION_DIGITS, 28);
        assert_eq!(limits::SERIALIZED_MAX_FRACTION_DIGITS, 126);
        assert_eq!(limits::ROUNDING_MAX_PLACES, 28);
        assert_eq!(limits::CHECKED_INTERMEDIATE_BITS, 256);
    }

    #[test]
    fn rounding_mode_names_round_trip() {
        for m in [
            RoundingMode::HalfEven,
            RoundingMode::HalfAwayFromZero,
            RoundingMode::TowardZero,
        ] {
            assert_eq!(RoundingMode::from_name(m.name()), Some(m));
        }
        assert_eq!(RoundingMode::from_name("half_up"), None);
    }

    #[test]
    fn zero_is_unique_and_default() {
        assert!(ExactNumber::default().is_zero());
        assert_eq!(ExactNumber::default(), ExactNumber::ZERO);
    }

    const MAX: u128 = limits::NUMERATOR_MAX;

    /// Reduced `±num/den` for tests; panics (test only) when out of bounds.
    fn q(negative: bool, num: u128, den: u128) -> ExactNumber {
        let g = wide::gcd(num, den).max(1);
        make(negative, num / g, den / g).unwrap()
    }

    fn int(v: i128) -> ExactNumber {
        q(v < 0, v.unsigned_abs(), 1)
    }

    #[test]
    fn add_sub_basic_and_normalized() {
        assert_eq!(add(&q(false, 1, 2), &q(false, 1, 3)), Ok(q(false, 5, 6)));
        assert_eq!(add(&q(false, 1, 3), &q(false, 2, 3)), Ok(int(1)));
        assert_eq!(add(&q(false, 1, 2), &q(true, 1, 2)), Ok(ExactNumber::ZERO));
        assert_eq!(sub(&q(false, 1, 4), &q(false, 3, 4)), Ok(q(true, 1, 2)));
        assert_eq!(sub(&int(0), &int(0)), Ok(ExactNumber::ZERO));
        // unrelated signs and a reduction by g2 > 1: 1/6 + 1/10 = 4/15
        assert_eq!(add(&q(false, 1, 6), &q(false, 1, 10)), Ok(q(false, 4, 15)));
    }

    #[test]
    fn add_at_and_beyond_the_bound() {
        let max = int(MAX as i128);
        assert_eq!(add(&int(MAX as i128 - 1), &int(1)), Ok(max.clone()));
        assert_eq!(add(&max, &int(1)), Err(NumericError::Overflow));
        assert_eq!(
            add(&int(-(MAX as i128)), &int(-1)),
            Err(NumericError::Overflow)
        );
        assert_eq!(sub(&max, &int(-1)), Err(NumericError::Overflow));
        assert_eq!(add(&max, &int(-(MAX as i128))), Ok(ExactNumber::ZERO));
        // Denominators MAX and MAX-1 are coprime; the lcm is far beyond the bound.
        let a = q(false, 1, MAX);
        let b = q(false, 1, MAX - 1);
        assert_eq!(add(&a, &b), Err(NumericError::Overflow));
        // The unreduced denominator is beyond 2^128 but the result reduces into bounds.
        let d = 1u128 << 126;
        assert_eq!(
            add(&q(false, 1, d), &q(false, 1, d)),
            Ok(q(false, 1, d / 2))
        );
        let c = MAX; // 1/c + 1/c = 2/c
        assert_eq!(add(&q(false, 1, c), &q(false, 1, c)), Ok(q(false, 2, c)));
    }

    #[test]
    fn mul_div_basic_and_bounds() {
        assert_eq!(mul(&q(false, 2, 3), &q(false, 3, 4)), Ok(q(false, 1, 2)));
        assert_eq!(mul(&q(true, 2, 3), &q(true, 3, 4)), Ok(q(false, 1, 2)));
        assert_eq!(mul(&q(true, 2, 3), &int(0)), Ok(ExactNumber::ZERO));
        assert_eq!(div(&int(1), &int(3)), Ok(q(false, 1, 3)));
        assert_eq!(div(&q(false, 1, 3), &q(true, 1, 3)), Ok(int(-1)));
        assert_eq!(mul(&int(MAX as i128), &int(1)), Ok(int(MAX as i128)));
        assert_eq!(mul(&int(MAX as i128), &int(2)), Err(NumericError::Overflow));
        assert_eq!(
            mul(&q(false, 1, MAX), &q(false, 1, 2)),
            Err(NumericError::Overflow)
        );
        // Cross-cancelling keeps this inside the bound.
        assert_eq!(mul(&q(false, MAX, 3), &q(false, 3, MAX)), Ok(int(1)));
        // Reciprocal of a bound-sized value stays inside the bound.
        assert_eq!(div(&int(1), &int(MAX as i128)), Ok(q(false, 1, MAX)));
        assert_eq!(
            div(&int(MAX as i128), &q(false, 1, 2)),
            Err(NumericError::Overflow)
        );
    }

    #[test]
    fn division_by_zero_is_reported() {
        assert_eq!(
            div(&int(5), &ExactNumber::ZERO),
            Err(NumericError::DivisionByZero)
        );
        assert_eq!(
            div(&ExactNumber::ZERO, &ExactNumber::ZERO),
            Err(NumericError::DivisionByZero)
        );
        assert_eq!(div(&ExactNumber::ZERO, &int(3)), Ok(ExactNumber::ZERO));
    }

    #[test]
    fn cmp_is_exact_at_the_bound() {
        assert_eq!(cmp(&q(true, 1, 2), &ExactNumber::ZERO), Ordering::Less);
        assert_eq!(cmp(&ExactNumber::ZERO, &q(false, 1, 2)), Ordering::Less);
        assert_eq!(cmp(&q(true, 1, 2), &q(true, 1, 3)), Ordering::Less);
        assert_eq!(cmp(&q(false, 1, 2), &q(false, 1, 3)), Ordering::Greater);
        assert_eq!(cmp(&int(-5), &int(-5)), Ordering::Equal);
        let a = q(false, MAX - 1, MAX);
        let b = q(false, MAX - 2, MAX - 1);
        assert_eq!(cmp(&a, &b), Ordering::Greater);
        assert_eq!(cmp(&neg(&a), &neg(&b)), Ordering::Less);
    }

    #[test]
    fn neg_sum_min_max_count() {
        assert_eq!(neg(&ExactNumber::ZERO), ExactNumber::ZERO);
        assert_eq!(neg(&neg(&q(false, 1, 3))), q(false, 1, 3));
        let third = q(false, 1, 3);
        assert_eq!(sum(&[third.clone(), third.clone(), third]), Ok(int(1)));
        assert_eq!(sum(&[]), Ok(ExactNumber::ZERO));
        assert_eq!(
            sum(&[int(MAX as i128), int(1)]),
            Err(NumericError::Overflow)
        );
        let v = [int(3), int(-7), int(3), q(false, 1, 2)];
        assert_eq!(min(&v), Some(int(-7)));
        assert_eq!(max(&v), Some(int(3)));
        assert_eq!(min(&[]), None);
        assert_eq!(max(&[]), None);
        assert_eq!(count(0), Ok(ExactNumber::ZERO));
        assert_eq!(count(42), Ok(int(42)));
    }

    /// Deterministic xorshift generator; no clock or randomness source (core rule).
    struct Xs(u64);
    impl Xs {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn value(&mut self) -> ExactNumber {
            let num = u128::from(self.next() >> 40) | (u128::from(self.next() >> 40) << 24);
            let den = 1 + (u128::from(self.next() >> 38) << 20 | u128::from(self.next() >> 44));
            q(self.next() & 1 == 1, num, den)
        }
    }

    #[test]
    fn algebraic_laws_hold_on_generated_values() {
        let mut g = Xs(0x9E37_79B9_7F4A_7C15);
        for _ in 0..500 {
            let (a, b) = (g.value(), g.value());
            let s = add(&a, &b).unwrap();
            assert_eq!(s, add(&b, &a).unwrap());
            assert_eq!(sub(&s, &b).unwrap(), a);
            assert_eq!(cmp(&a, &b), cmp(&sub(&a, &b).unwrap(), &ExactNumber::ZERO));
            if !b.is_zero() {
                assert_eq!(div(&mul(&a, &b).unwrap(), &b).unwrap(), a);
            }
            // normalized: zero is the single zero, denominator positive and reduced
            if s.is_zero() {
                assert_eq!(s, ExactNumber::ZERO);
            } else {
                assert_eq!(wide::gcd(s.num, s.den), 1);
            }
        }
    }

    fn lit(s: &str) -> Result<ExactNumber, NumericError> {
        parse_literal(s)
    }

    fn bad(f: LiteralFault) -> Result<ExactNumber, NumericError> {
        Err(NumericError::InvalidLiteral(f))
    }

    #[test]
    fn literal_accept_table() {
        assert_eq!(lit("1.5"), Ok(q(false, 3, 2)));
        assert_eq!(lit("2"), Ok(int(2)));
        assert_eq!(lit("0.3"), Ok(q(false, 3, 10)));
        assert_eq!(lit("-2.50"), Ok(q(true, 5, 2)));
        assert_eq!(lit("007.5"), Ok(q(false, 15, 2)));
        for z in ["0", "-0", "0.0", "-0.00", "000"] {
            assert_eq!(lit(z), Ok(ExactNumber::ZERO), "{z}");
        }
        // 38 digits, and 28 fractional digits
        let d38 = "9".repeat(38);
        assert_eq!(lit(&d38), Ok(q(false, 10u128.pow(38) - 1, 1)));
        assert_eq!(
            lit("0.0000000000000000000000000001"),
            Ok(q(false, 1, 10u128.pow(28)))
        );
        assert!(lit(&format!("{}.{}", "9".repeat(10), "9".repeat(28))).is_ok());
        // leading zeros are not significant
        assert!(lit(&format!("{}1", "0".repeat(60))).is_ok());
    }

    #[test]
    fn literal_reject_table() {
        assert_eq!(lit(""), bad(LiteralFault::Empty));
        assert_eq!(lit("-"), bad(LiteralFault::Empty));
        assert_eq!(lit("+1"), bad(LiteralFault::LeadingPlus));
        assert_eq!(lit("NaN"), bad(LiteralFault::NotFinite));
        assert_eq!(lit("-Infinity"), bad(LiteralFault::NotFinite));
        assert_eq!(lit("1e5"), bad(LiteralFault::Exponent));
        assert_eq!(lit("1E-2"), bad(LiteralFault::Exponent));
        assert_eq!(lit("1/3"), bad(LiteralFault::Fraction));
        assert_eq!(lit("-2/3"), bad(LiteralFault::Fraction));
        for s in [
            ".5", "5.", "1.2.3", "--1", "1-", " 1", "1 ", "1,5", "\u{661}", "0x10",
        ] {
            assert_eq!(lit(s), bad(LiteralFault::BadCharacter), "{s:?}");
        }
        assert_eq!(lit(&"9".repeat(39)), bad(LiteralFault::TooManyDigits));
        assert_eq!(
            lit("0.00000000000000000000000000001"),
            bad(LiteralFault::TooManyFractionDigits)
        );
        assert_eq!(lit("1.0000000000000000000000000000"), Ok(int(1)));
        assert_eq!(
            lit("1.00000000000000000000000000000"),
            bad(LiteralFault::TooManyFractionDigits)
        );
    }

    #[test]
    fn canonical_table() {
        let cases: [(ExactNumber, &str); 9] = [
            (q(false, 3, 2), "1.5"),
            (int(2), "2"),
            (q(false, 3, 10), "0.3"),
            (q(false, 1, 3), "1/3"),
            (q(true, 2, 3), "-2/3"),
            (ExactNumber::ZERO, "0"),
            (q(true, 1, 8), "-0.125"),
            (q(false, 1, 1000), "0.001"),
            (int(-40), "-40"),
        ];
        for (v, s) in cases {
            assert_eq!(to_canonical(&v), s);
            assert_eq!(parse_canonical(s), Ok(v));
        }
        assert_eq!(lit("1.50").map(|v| to_canonical(&v)), Ok("1.5".to_owned()));
        assert_eq!(lit("-0.00").map(|v| to_canonical(&v)), Ok("0".to_owned()));
        assert_eq!(to_canonical(&q(false, 1, 1 << 10)), "0.0009765625");
    }

    #[test]
    fn canonical_126_digit_expansion() {
        let v = q(false, 1, 1u128 << 126);
        let s = to_canonical(&v);
        let frac = s.strip_prefix("0.").unwrap();
        assert_eq!(frac.len(), 126);
        assert!(frac.ends_with('5'));
        assert!(frac.starts_with("0000000000000000000000000000000000000"));
        assert_eq!(parse_canonical(&s), Ok(v));
        // 127 fractional digits are beyond the serialized limit
        assert_eq!(
            parse_canonical(&format!("0.{}5", "0".repeat(126))),
            bad(LiteralFault::TooManyFractionDigits)
        );
        // largest numerator over a power of five, a power of two and a power of ten
        for den in [5u128.pow(54), 1u128 << 126, 10u128.pow(28)] {
            let v = q(true, MAX, den);
            assert_eq!(parse_canonical(&to_canonical(&v)), Ok(v));
        }
    }

    #[test]
    fn canonical_parser_accepts_fractions_only_here() {
        assert_eq!(parse_canonical("1/3"), Ok(q(false, 1, 3)));
        assert_eq!(parse_canonical("-2/3"), Ok(q(true, 2, 3)));
        assert_eq!(parse_canonical("2/4"), Ok(q(false, 1, 2)));
        assert_eq!(parse_canonical("0/5"), Ok(ExactNumber::ZERO));
        assert_eq!(parse_canonical("1/0"), bad(LiteralFault::Fraction));
        assert_eq!(parse_canonical("1/"), bad(LiteralFault::Empty));
        assert_eq!(parse_canonical("/3"), bad(LiteralFault::Empty));
        assert_eq!(parse_canonical("1/-3"), bad(LiteralFault::BadCharacter));
        assert_eq!(parse_canonical("1/3/5"), bad(LiteralFault::BadCharacter));
        assert_eq!(parse_canonical("+1/3"), bad(LiteralFault::LeadingPlus));
        assert_eq!(parse_canonical("1e3"), bad(LiteralFault::Exponent));
        assert_eq!(parse_canonical("NaN"), bad(LiteralFault::NotFinite));
        // out of bounds is Overflow, not a wrap
        let over = (MAX + 1).to_string();
        assert_eq!(
            parse_canonical(&format!("{over}/3")),
            Err(NumericError::Overflow)
        );
        assert_eq!(
            parse_canonical(&format!("1/{over}")),
            Err(NumericError::Overflow)
        );
        assert_eq!(
            parse_canonical(&format!("{over}.5")),
            Err(NumericError::Overflow)
        );
        assert_eq!(
            parse_canonical(&"9".repeat(201)),
            bad(LiteralFault::TooManyDigits)
        );
        // the Rule_IR literal parser rejects the same fraction
        assert_eq!(lit("1/3"), bad(LiteralFault::Fraction));
    }

    #[test]
    fn canonical_round_trips_on_generated_values() {
        let mut g = Xs(0x1234_5678_9ABC_DEF1);
        for _ in 0..2000 {
            let v = g.value();
            let s = to_canonical(&v);
            assert_eq!(parse_canonical(&s), Ok(v.clone()), "{s}");
            assert_eq!(to_canonical(&parse_canonical(&s).unwrap()), s);
            assert!(!s.contains('e') && !s.starts_with('+'));
            if let Some((_, frac)) = s.split_once('.') {
                assert!(!frac.ends_with('0'));
            }
        }
    }

    fn rounded(text: &str, places: u32, mode: RoundingMode) -> String {
        let v = lit(text).unwrap();
        to_canonical(&round(&v, places, mode).unwrap())
    }

    #[test]
    fn rounding_tables_of_req_8_28() {
        use RoundingMode::{HalfAwayFromZero as A, HalfEven as E, TowardZero as T};
        let table: [(&str, u32, [&str; 3]); 4] = [
            ("2.5", 0, ["2", "3", "2"]),
            ("-2.5", 0, ["-2", "-3", "-2"]),
            ("3.5", 0, ["4", "4", "3"]),
            ("0.125", 2, ["0.12", "0.13", "0.12"]),
        ];
        for (input, places, expected) in table {
            for (mode, want) in [E, A, T].into_iter().zip(expected) {
                assert_eq!(rounded(input, places, mode), want, "{input} {mode:?}");
            }
        }
    }

    #[test]
    fn rounding_edges() {
        use RoundingMode::{HalfAwayFromZero as A, HalfEven as E, TowardZero as T};
        // non-terminating values, below and above the half
        assert_eq!(rounded("0.5", 0, E), "0");
        assert_eq!(rounded("1.5", 0, E), "2");
        assert_eq!(to_canonical(&round(&q(false, 1, 3), 2, E).unwrap()), "0.33");
        assert_eq!(to_canonical(&round(&q(false, 2, 3), 2, E).unwrap()), "0.67");
        assert_eq!(to_canonical(&round(&q(true, 2, 3), 0, T).unwrap()), "0");
        // a negative value that rounds to zero gives the single zero
        assert_eq!(round(&q(true, 1, 3), 0, A), Ok(ExactNumber::ZERO));
        // places beyond the value's scale leave it unchanged; zero stays zero
        assert_eq!(rounded("1.5", 28, E), "1.5");
        assert_eq!(round(&ExactNumber::ZERO, 5, E), Ok(ExactNumber::ZERO));
        // places range
        assert_eq!(round(&int(1), 29, E), Err(NumericError::InvalidPlaces));
        assert_eq!(
            round(&int(1), u32::MAX, E),
            Err(NumericError::InvalidPlaces)
        );
        assert!(round(&int(1), 0, E).is_ok());
        assert!(round(&int(1), 28, E).is_ok());
    }

    #[test]
    fn rounding_overflow_is_reported() {
        use RoundingMode::{HalfAwayFromZero as A, HalfEven as E, TowardZero as T};
        // MAX/2 = ...863.5 ties up to a numerator above the bound at 0 places
        let half_max = q(false, MAX, 2);
        assert_eq!(round(&half_max, 0, A), Ok(int((MAX / 2 + 1) as i128)));
        assert_eq!(round(&half_max, 0, T), Ok(int((MAX / 2) as i128)));
        // a huge value with a fractional part cannot take 28 places
        let big = q(false, MAX, 3);
        assert_eq!(round(&big, 28, E), Err(NumericError::Overflow));
        // an integer at the bound is exact at any places
        assert_eq!(round(&int(MAX as i128), 0, E), Ok(int(MAX as i128)));
        assert_eq!(round(&int(MAX as i128), 28, T), Ok(int(MAX as i128)));
    }

    #[test]
    fn rounding_agrees_with_integer_reference() {
        use RoundingMode::{HalfAwayFromZero as A, HalfEven as E, TowardZero as T};
        let mut g = Xs(0x0BAD_5EED_1234_5678);
        for _ in 0..2000 {
            let n = i128::from(g.next() % 2_000_001) - 1_000_000;
            let d = i128::from(g.next() % 997) + 1;
            let v = q(n < 0, n.unsigned_abs(), d.unsigned_abs());
            let (quo, rem) = (n.abs() / d, n.abs() % d);
            let sign = if n < 0 { -1 } else { 1 };
            let tie = rem * 2 == d;
            let expected = [
                (E, quo + i128::from(rem * 2 > d || (tie && quo % 2 == 1))),
                (A, quo + i128::from(rem * 2 >= d)),
                (T, quo),
            ];
            for (mode, mag) in expected {
                assert_eq!(round(&v, 0, mode), Ok(int(sign * mag)), "{n}/{d} {mode:?}");
            }
        }
    }

    #[test]
    fn parsers_never_panic_on_arbitrary_text() {
        const ALPHABET: &[char] = &[
            '0', '1', '9', '5', '-', '+', '.', '/', 'e', 'E', 'N', 'a', ' ', '\u{661}', 'é',
        ];
        let mut g = Xs(0xDEAD_BEEF_CAFE_F00D);
        for _ in 0..20000 {
            let len = (g.next() % 12) as usize;
            let s: String = (0..len)
                .map(|_| ALPHABET[(g.next() % ALPHABET.len() as u64) as usize])
                .collect();
            let _ = parse_literal(&s);
            let _ = parse_canonical(&s);
        }
        let long = "9".repeat(5000);
        let _ = parse_literal(&long);
        let _ = parse_canonical(&long);
        let _ = parse_canonical(&format!("0.{long}"));
        let _ = parse_canonical(&format!("{long}/{long}"));
    }
}
