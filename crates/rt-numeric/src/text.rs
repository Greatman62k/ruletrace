//! Literal parsing, canonical serialization and the bounded digit-vector codec
//! (design §4.2.4, Req 8.16, 8.17, 8.24, 8.25, 8.27).
//!
//! A decimal expansion of a value in lowest terms can have 126 fractional
//! digits, which does not fit in 256 bits. The private [`Digits`] vector (base
//! 10^9 limbs, at most 200 digits) multiplies and divides by 2 and 5 so that no
//! big-integer type is needed. Every function here is total: invalid input is
//! an `Err`, never a panic.

use crate::wide::gcd;
use crate::{limits, make, ExactNumber, LiteralFault, NumericError, Sign};

/// Most decimal digits the codec holds. The widest value is below 2^253 (77
/// digits).
const MAX_DIGITS: usize = 200;
const LIMB_BASE: u64 = 1_000_000_000;
const LIMB_DIGITS: usize = 9;

/// Bounded unsigned decimal digit vector, little-endian base 10^9 limbs, no
/// high zero limb.
#[derive(Debug)]
struct Digits {
    limbs: Vec<u32>,
}

impl Digits {
    fn from_u128(mut v: u128) -> Digits {
        let mut limbs = Vec::new();
        let base = u128::from(LIMB_BASE);
        while v > 0 {
            // `v % base` is below 10^9, so it fits in `u32`.
            limbs.push(u32::try_from(v % base).unwrap_or(0));
            v /= base;
        }
        Digits { limbs }
    }

    /// From ASCII digits without leading zeros. `None` beyond [`MAX_DIGITS`] or
    /// on a non-digit.
    fn from_ascii(digits: &str) -> Option<Digits> {
        let bytes = digits.as_bytes();
        if bytes.len() > MAX_DIGITS {
            return None;
        }
        let mut limbs = Vec::new();
        for chunk in bytes.rchunks(LIMB_DIGITS) {
            let mut limb: u32 = 0;
            for &b in chunk {
                if !b.is_ascii_digit() {
                    return None;
                }
                limb = limb * 10 + u32::from(b - b'0');
            }
            limbs.push(limb);
        }
        let mut d = Digits { limbs };
        d.trim();
        Some(d)
    }

    fn trim(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }

    fn digit_len(&self) -> usize {
        match self.limbs.last() {
            None => 0,
            Some(top) => (self.limbs.len() - 1) * LIMB_DIGITS + top.to_string().len(),
        }
    }

    /// Multiplies by `m` (below 10^9). `false` when the result exceeds
    /// [`MAX_DIGITS`].
    fn mul_small(&mut self, m: u32) -> bool {
        let mut carry: u64 = 0;
        for limb in &mut self.limbs {
            let t = u64::from(*limb) * u64::from(m) + carry;
            *limb = u32::try_from(t % LIMB_BASE).unwrap_or(0);
            carry = t / LIMB_BASE;
        }
        if carry > 0 {
            self.limbs.push(u32::try_from(carry).unwrap_or(0));
        }
        self.trim();
        self.digit_len() <= MAX_DIGITS
    }

    /// Divides by `m` (nonzero, below 10^9) when exact and returns `true`;
    /// otherwise leaves the value unchanged and returns `false`.
    fn div_small_exact(&mut self, m: u32) -> bool {
        if m == 0 {
            return false;
        }
        let m = u64::from(m);
        let mut rem: u64 = 0;
        for limb in self.limbs.iter().rev() {
            rem = (rem * LIMB_BASE + u64::from(*limb)) % m;
        }
        if rem != 0 {
            return false;
        }
        let mut rem: u64 = 0;
        for limb in self.limbs.iter_mut().rev() {
            let cur = rem * LIMB_BASE + u64::from(*limb);
            *limb = u32::try_from(cur / m).unwrap_or(0);
            rem = cur % m;
        }
        self.trim();
        true
    }

    fn to_u128(&self) -> Option<u128> {
        let base = u128::from(LIMB_BASE);
        self.limbs.iter().rev().try_fold(0u128, |acc, limb| {
            acc.checked_mul(base)?.checked_add(u128::from(*limb))
        })
    }

    fn to_decimal_string(&self) -> String {
        let mut out = String::new();
        for (i, limb) in self.limbs.iter().rev().enumerate() {
            if i == 0 {
                out.push_str(&limb.to_string());
            } else {
                out.push_str(&format!("{limb:09}"));
            }
        }
        if out.is_empty() {
            out.push('0');
        }
        out
    }
}

/// A scanned decimal: sign and the digit strings before and after the point.
struct Scanned<'a> {
    negative: bool,
    int: &'a str,
    frac: &'a str,
}

fn fault(f: LiteralFault) -> NumericError {
    NumericError::InvalidLiteral(f)
}

/// Checks the shape `-?digits(.digits)?` and classifies the first fault.
fn scan_decimal(text: &str) -> Result<Scanned<'_>, LiteralFault> {
    if text.is_empty() {
        return Err(LiteralFault::Empty);
    }
    if text.starts_with('+') {
        return Err(LiteralFault::LeadingPlus);
    }
    let (negative, rest) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    if rest.is_empty() {
        return Err(LiteralFault::Empty);
    }
    if rest.eq_ignore_ascii_case("nan")
        || rest.eq_ignore_ascii_case("infinity")
        || rest.eq_ignore_ascii_case("inf")
    {
        return Err(LiteralFault::NotFinite);
    }
    if rest.contains('/') {
        return Err(LiteralFault::Fraction);
    }
    if rest.contains(['e', 'E']) {
        return Err(LiteralFault::Exponent);
    }
    let (int, frac) = rest.split_once('.').unwrap_or((rest, ""));
    let has_point = rest.contains('.');
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if int.is_empty() || !digits(int) || !digits(frac) || (has_point && frac.is_empty()) {
        return Err(LiteralFault::BadCharacter);
    }
    Ok(Scanned {
        negative,
        int,
        frac,
    })
}

/// Significant digits (leading zeros removed) of `int` followed by `frac`.
fn significant(s: &Scanned<'_>) -> String {
    s.int
        .chars()
        .chain(s.frac.chars())
        .skip_while(|c| *c == '0')
        .collect()
}

/// `10^n` for `n` up to 38.
fn pow10(n: usize) -> Option<u128> {
    (0..n).try_fold(1u128, |acc, _| acc.checked_mul(10))
}

/// Parses a Rule_IR, Competition_Data or Published_Result literal (Req 8.27).
pub(crate) fn parse_literal(text: &str) -> Result<ExactNumber, NumericError> {
    let s = scan_decimal(text).map_err(fault)?;
    let sig = significant(&s);
    if sig.len() > limits::LITERAL_MAX_DIGITS {
        return Err(fault(LiteralFault::TooManyDigits));
    }
    if s.frac.len() > limits::LITERAL_MAX_FRACTION_DIGITS {
        return Err(fault(LiteralFault::TooManyFractionDigits));
    }
    // At most 38 digits: below 10^38, which fits in `u128`.
    let m = sig.bytes().try_fold(0u128, |acc, b| {
        acc.checked_mul(10)?.checked_add(u128::from(b - b'0'))
    });
    let (Some(m), Some(den)) = (m, pow10(s.frac.len())) else {
        return Err(NumericError::Overflow);
    };
    let g = gcd(m, den).max(1);
    make(s.negative, m / g, den / g)
}

/// Canonical text (Req 8.16, 8.24): a decimal when the denominator has only the
/// factors 2 and 5, otherwise `n/d`.
pub(crate) fn to_canonical(a: &ExactNumber) -> String {
    if a.sign == Sign::Zero {
        return "0".to_owned();
    }
    let sign = if a.sign == Sign::Negative { "-" } else { "" };
    match decimal_digits(a) {
        Some(body) => format!("{sign}{body}"),
        None => format!("{sign}{}/{}", a.num, a.den),
    }
}

/// The unsigned decimal expansion, or `None` when the denominator has another
/// prime factor (or, unreachably, when the codec bound is exceeded).
fn decimal_digits(a: &ExactNumber) -> Option<String> {
    let twos = a.den.trailing_zeros();
    let mut rest = a.den >> twos;
    let mut fives: u32 = 0;
    // `checked_rem` rather than `is_multiple_of`, which is newer than the 1.89
    // floor.
    while rest.checked_rem(5) == Some(0) {
        rest /= 5;
        fives += 1;
    }
    if rest != 1 {
        return None;
    }
    let k = twos.max(fives);
    let mut d = Digits::from_u128(a.num);
    for _ in 0..(k - twos) {
        if !d.mul_small(2) {
            return None;
        }
    }
    for _ in 0..(k - fives) {
        if !d.mul_small(5) {
            return None;
        }
    }
    let k = usize::try_from(k).ok()?;
    let mut s = d.to_decimal_string();
    if k == 0 {
        return Some(s);
    }
    if s.len() <= k {
        s = format!("{}{s}", "0".repeat(k + 1 - s.len()));
    }
    let point = s.len() - k;
    let int = s.get(..point)?;
    let frac = s.get(point..)?.trim_end_matches('0');
    Some(if frac.is_empty() {
        int.to_owned()
    } else {
        format!("{int}.{frac}")
    })
}

/// Parses canonical artifact text: a decimal with up to 126 fractional digits,
/// or `n/d`.
pub(crate) fn parse_canonical(text: &str) -> Result<ExactNumber, NumericError> {
    match text.split_once('/') {
        Some((n, d)) => parse_fraction(n, d),
        None => parse_wide_decimal(text),
    }
}

fn parse_wide_decimal(text: &str) -> Result<ExactNumber, NumericError> {
    let s = scan_decimal(text).map_err(fault)?;
    if s.frac.len() > limits::SERIALIZED_MAX_FRACTION_DIGITS {
        return Err(fault(LiteralFault::TooManyFractionDigits));
    }
    let sig = significant(&s);
    let Some(mut d) = Digits::from_ascii(&sig) else {
        return Err(fault(LiteralFault::TooManyDigits));
    };
    if d.limbs.is_empty() {
        return Ok(ExactNumber::ZERO);
    }
    // M / 10^F = M / (2^F 5^F): cancel factors of 2 and 5 while M has them.
    let (mut twos, mut fives) = (s.frac.len(), s.frac.len());
    while twos > 0 && d.div_small_exact(2) {
        twos -= 1;
    }
    while fives > 0 && d.div_small_exact(5) {
        fives -= 1;
    }
    let den = (0..twos)
        .try_fold(1u128, |acc, _| acc.checked_mul(2))
        .and_then(|v| (0..fives).try_fold(v, |acc, _| acc.checked_mul(5)));
    match (d.to_u128(), den) {
        (Some(num), Some(den)) => make(s.negative, num, den),
        _ => Err(NumericError::Overflow),
    }
}

fn parse_fraction(n: &str, d: &str) -> Result<ExactNumber, NumericError> {
    let (negative, n) = match n.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, n),
    };
    let num = parse_u128(n)?;
    let den = parse_u128(d)?;
    if den == 0 {
        return Err(fault(LiteralFault::Fraction));
    }
    let g = gcd(num, den).max(1);
    make(negative, num / g, den / g)
}

/// Unsigned integer from ASCII digits. Overflow of `u128` is
/// [`NumericError::Overflow`].
fn parse_u128(digits: &str) -> Result<u128, NumericError> {
    if digits.is_empty() {
        return Err(fault(LiteralFault::Empty));
    }
    if digits.starts_with('+') {
        return Err(fault(LiteralFault::LeadingPlus));
    }
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(fault(LiteralFault::BadCharacter));
    }
    digits
        .bytes()
        .try_fold(0u128, |acc, b| {
            acc.checked_mul(10)?.checked_add(u128::from(b - b'0'))
        })
        .ok_or(NumericError::Overflow)
}
