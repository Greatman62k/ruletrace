//! Small std-only helpers: randomness, Base64, constant-time compare, UTC time.

use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

/// Returns `bytes` random bytes from the OS as lower-case hex.
///
/// Unix only: reads `/dev/urandom`. Other platforms get `ErrorKind::Unsupported`.
pub fn random_hex(bytes: usize) -> io::Result<String> {
    #[cfg(unix)]
    {
        use std::io::Read;
        let mut buf = vec![0u8; bytes];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
        Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
    }
    #[cfg(not(unix))]
    {
        let _ = bytes;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "OS randomness is implemented for Unix only in this spike",
        ))
    }
}

/// Compares two byte strings without an early exit on the first difference.
///
/// The length is not treated as secret: a length difference is folded into
/// the result and the loop still covers the longer input.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    let n = a.len().max(b.len());
    let mut diff = (a.len() ^ b.len()) as u64;
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= u64::from(x ^ y);
    }
    diff == 0
}

/// Strict standard-alphabet Base64 decoder with padding (RFC 4648).
pub fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let bytes = input.as_bytes();
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::new();
    let chunks = bytes.len() / 4;
    for (i, chunk) in bytes.chunks(4).enumerate() {
        let last = i + 1 == chunks;
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && !last) {
            return None;
        }
        let mut acc: u32 = 0;
        for &c in &chunk[..4 - pad] {
            let v = match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return None,
            };
            acc = (acc << 6) | u32::from(v);
        }
        acc <<= 6 * pad as u32;
        let triple = acc.to_be_bytes();
        out.extend_from_slice(&triple[1..4 - pad]);
    }
    Some(out)
}

/// Formats a time as `YYYY-MM-DDTHH:MM:SS.mmmZ` (UTC).
pub fn iso_utc(t: SystemTime) -> String {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs();
    let millis = d.subsec_millis();
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil-from-days (H. Hinnant), valid for the proleptic Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn ct_eq_distinguishes_value_and_length() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"abcd"));
        assert!(!ct_eq(b"", b"a"));
        assert!(ct_eq(b"", b""));
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        let cases: [(&str, &[u8]); 5] = [
            ("Zg==", b"f"),
            ("Zm8=", b"fo"),
            ("Zm9v", b"foo"),
            ("Zm9vYg==", b"foob"),
            ("Zm9vYmFy", b"foobar"),
        ];
        for (enc, dec) in cases {
            assert_eq!(base64_decode(enc).as_deref(), Some(dec), "{enc}");
        }
        assert_eq!(
            base64_decode("bGFiOnNlY3JldA==").as_deref(),
            Some(&b"lab:secret"[..])
        );
    }

    #[test]
    fn base64_rejects_malformed_input() {
        for bad in ["", "Zg=", "Zg", "Z===", "Zg==Zm9v", "Zm9!", "Zm 9"] {
            assert_eq!(base64_decode(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn iso_utc_matches_known_instants() {
        let at = |s: u64, ms: u64| UNIX_EPOCH + Duration::from_millis(s * 1000 + ms);
        assert_eq!(iso_utc(at(0, 0)), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_utc(at(1_700_000_000, 42)), "2023-11-14T22:13:20.042Z");
        // 2024-02-29 12:00:00 UTC, a leap day.
        assert_eq!(iso_utc(at(1_709_208_000, 0)), "2024-02-29T12:00:00.000Z");
    }

    #[cfg(unix)]
    #[test]
    fn random_hex_has_requested_length_and_differs() {
        let a = random_hex(32).unwrap();
        let b = random_hex(32).unwrap();
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
