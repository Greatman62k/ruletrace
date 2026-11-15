//! Search for a canary credential, whole or in part, in captured bytes.
//!
//! The acceptance tests use only a canary string, never a real credential.
//! A fragment is a run of [`FRAGMENT_BYTES`] consecutive bytes of the canary,
//! the same width that the `ruletrace-ai` redactor treats as a fragment.

use std::collections::BTreeSet;

/// The width in bytes of the smallest canary fragment that counts as a leak.
pub const FRAGMENT_BYTES: usize = 8;

/// The offset of the first place in `haystack` where a run of
/// [`FRAGMENT_BYTES`] bytes of `secret` occurs, or the whole of `secret` when
/// it is shorter than that. `None` when no such run occurs.
///
/// A leak of the whole secret is also a leak of its fragments, so one search
/// covers both.
#[must_use]
pub fn first_leak(secret: &[u8], haystack: &[u8]) -> Option<usize> {
    if secret.is_empty() {
        return None;
    }
    let width = FRAGMENT_BYTES.min(secret.len());
    let windows: BTreeSet<&[u8]> = secret.windows(width).collect();
    // A small table of window hashes skips most positions without a lookup.
    let mut table = [false; 4096];
    for w in &windows {
        if let Some(entry) = table.get_mut(slot(pack(w))) {
            *entry = true;
        }
    }
    if width < FRAGMENT_BYTES {
        return haystack.windows(width).position(|w| windows.contains(w));
    }
    let mut rolling: u64 = 0;
    for (index, byte) in haystack.iter().enumerate() {
        rolling = (rolling << 8) | u64::from(*byte);
        if index + 1 < FRAGMENT_BYTES || !table.get(slot(rolling)).copied().unwrap_or(true) {
            continue;
        }
        let start = index + 1 - FRAGMENT_BYTES;
        if haystack
            .get(start..=index)
            .is_some_and(|w| windows.contains(w))
        {
            return Some(start);
        }
    }
    None
}

fn pack(window: &[u8]) -> u64 {
    window.iter().fold(0, |acc, b| (acc << 8) | u64::from(*b))
}

fn slot(packed: u64) -> usize {
    usize::try_from(packed.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 52).unwrap_or(0)
}
