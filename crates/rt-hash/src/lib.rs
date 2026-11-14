//! SHA-256 wrapper and the `Sha256Hex` newtype (class core).
//!
//! The only item that touches the external `sha2` crate is [`sha256`], so the
//! implementation can be replaced without changing callers.
#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::fmt;

/// A SHA-256 digest as exactly 64 lowercase hexadecimal characters.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sha256Hex(String);

impl Sha256Hex {
    /// Parses a digest. Accepts only 64 lowercase hex characters (`0-9a-f`).
    pub fn parse(text: &str) -> Option<Self> {
        let ok = text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        ok.then(|| Self(text.to_owned()))
    }

    /// The 64-character lowercase hex text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Sha256Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha256Hex({})", self.0)
    }
}

/// Computes the SHA-256 digest of `bytes` (FIPS 180-4) as lowercase hex.
pub fn sha256(bytes: &[u8]) -> Sha256Hex {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest.iter() {
        out.extend(char::from_digit(u32::from(b >> 4), 16));
        out.extend(char::from_digit(u32::from(b & 0x0f), 16));
    }
    Sha256Hex(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nist_empty() {
        assert_eq!(
            sha256(b"").as_str(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn nist_abc() {
        assert_eq!(
            sha256(b"abc").as_str(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn nist_448_bit() {
        let m = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(m.len() * 8, 448);
        assert_eq!(
            sha256(m).as_str(),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn nist_million_a() {
        let m = vec![b'a'; 1_000_000];
        assert_eq!(
            sha256(&m).as_str(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn parse_accepts_only_64_lowercase_hex() {
        let h = sha256(b"abc");
        assert_eq!(Sha256Hex::parse(h.as_str()), Some(h.clone()));
        assert_eq!(Sha256Hex::parse(&h.as_str().to_uppercase()), None);
        assert_eq!(Sha256Hex::parse("abc"), None);
        assert_eq!(Sha256Hex::parse(&"g".repeat(64)), None);
        assert_eq!(h.to_string(), h.as_str());
    }
}
