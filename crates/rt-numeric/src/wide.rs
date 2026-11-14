//! Private 256-bit unsigned helper and binary gcd (design §4.2.3).
//!
//! Only the operations the checked arithmetic needs. Nothing here allocates or
//! panics: every operation is total, or documents the precondition the caller
//! upholds.

use std::cmp::Ordering;

/// Unsigned 256-bit value, `hi * 2^128 + lo`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct U256 {
    pub(crate) hi: u128,
    pub(crate) lo: u128,
}

impl U256 {
    pub(crate) const ZERO: U256 = U256 { hi: 0, lo: 0 };

    #[cfg(test)]
    pub(crate) fn from_u128(v: u128) -> U256 {
        U256 { hi: 0, lo: v }
    }

    /// The value as a `u128`, or `None` when it needs more than 128 bits.
    pub(crate) fn to_u128(self) -> Option<u128> {
        if self.hi == 0 {
            Some(self.lo)
        } else {
            None
        }
    }

    /// Bit `i` (0 is least significant). Positions of 256 or more read as 0.
    fn bit(self, i: u32) -> bool {
        if i >= 256 {
            false
        } else if i >= 128 {
            (self.hi >> (i - 128)) & 1 == 1
        } else {
            (self.lo >> i) & 1 == 1
        }
    }

    fn set_bit(&mut self, i: u32) {
        if i >= 256 {
        } else if i >= 128 {
            self.hi |= 1u128 << (i - 128);
        } else {
            self.lo |= 1u128 << i;
        }
    }

    /// `self + other`, or `None` on carry out of 256 bits.
    pub(crate) fn checked_add(self, other: U256) -> Option<U256> {
        let (lo, c) = self.lo.overflowing_add(other.lo);
        let hi = self.hi.checked_add(other.hi)?.checked_add(u128::from(c))?;
        Some(U256 { hi, lo })
    }

    /// `self - other`, or `None` when `other > self`.
    pub(crate) fn checked_sub(self, other: U256) -> Option<U256> {
        let (lo, b) = self.lo.overflowing_sub(other.lo);
        let hi = self.hi.checked_sub(other.hi)?.checked_sub(u128::from(b))?;
        Some(U256 { hi, lo })
    }

    /// Quotient and remainder of `self / d`. Requires `0 < d < 2^127`, so the
    /// running remainder (below `d`) can be doubled without leaving 128 bits.
    /// Any other `d` gives `None`.
    pub(crate) fn div_rem_u128(self, d: u128) -> Option<(U256, u128)> {
        if d == 0 || d >= (1u128 << 127) {
            return None;
        }
        let mut q = U256::ZERO;
        let mut r: u128 = 0;
        for i in (0..256u32).rev() {
            r = (r << 1) | u128::from(self.bit(i));
            if r >= d {
                r -= d;
                q.set_bit(i);
            }
        }
        Some((q, r))
    }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.hi.cmp(&other.hi).then(self.lo.cmp(&other.lo))
    }
}

/// Full 256-bit product of two `u128` values. Cannot overflow.
pub(crate) fn mul_wide(a: u128, b: u128) -> U256 {
    const MASK: u128 = u64::MAX as u128;
    let (a1, a0) = (a >> 64, a & MASK);
    let (b1, b0) = (b >> 64, b & MASK);
    // Each partial product is below 2^128; the sums below cannot overflow.
    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;
    let mid = (p00 >> 64) + (p01 & MASK) + (p10 & MASK);
    let lo = (p00 & MASK) | (mid << 64);
    let hi = p11 + (p01 >> 64) + (p10 >> 64) + (mid >> 64);
    U256 { hi, lo }
}

/// Binary (Stein) greatest common divisor. `gcd(0, x) = x`, `gcd(0, 0) = 0`.
pub(crate) fn gcd(mut a: u128, mut b: u128) -> u128 {
    if a == 0 {
        return b;
    }
    if b == 0 {
        return a;
    }
    let shift = (a | b).trailing_zeros();
    a >>= a.trailing_zeros();
    loop {
        b >>= b.trailing_zeros();
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        b -= a;
        if b == 0 {
            return a << shift;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_wide_extremes() {
        assert_eq!(mul_wide(0, u128::MAX), U256::ZERO);
        // (2^128 - 1)^2 = 2^256 - 2^129 + 1
        assert_eq!(
            mul_wide(u128::MAX, u128::MAX),
            U256 {
                hi: u128::MAX - 1,
                lo: 1
            }
        );
        assert_eq!(mul_wide(1 << 64, 1 << 64), U256 { hi: 1, lo: 0 });
    }

    #[test]
    fn div_rem_inverts_mul() {
        let m = (1u128 << 127) - 1;
        let p = mul_wide(m, m - 2);
        assert_eq!(p.div_rem_u128(m), Some((U256::from_u128(m - 2), 0)));
        let p1 = p.checked_add(U256::from_u128(5)).unwrap_or(U256::ZERO);
        assert_eq!(p1.div_rem_u128(m), Some((U256::from_u128(m - 2), 5)));
        assert_eq!(p.div_rem_u128(0), None);
        assert_eq!(p.div_rem_u128(1 << 127), None);
    }

    #[test]
    fn add_sub_carry_and_borrow() {
        let a = U256 {
            hi: 0,
            lo: u128::MAX,
        };
        let one = U256::from_u128(1);
        let s = a.checked_add(one);
        assert_eq!(s, Some(U256 { hi: 1, lo: 0 }));
        assert_eq!(s.and_then(|s| s.checked_sub(one)), Some(a));
        assert_eq!(one.checked_sub(a), None);
        let top = U256 {
            hi: u128::MAX,
            lo: u128::MAX,
        };
        assert_eq!(top.checked_add(one), None);
    }

    #[test]
    fn gcd_table() {
        assert_eq!(gcd(0, 0), 0);
        assert_eq!(gcd(0, 7), 7);
        assert_eq!(gcd(12, 18), 6);
        assert_eq!(gcd(1 << 100, 1 << 90), 1 << 90);
        let m = (1u128 << 127) - 1;
        assert_eq!(gcd(m, m - 1), 1);
        assert_eq!(gcd(m, m), m);
    }
}
