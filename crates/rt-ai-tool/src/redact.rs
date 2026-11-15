//! Redaction of the credential value (Req 28.9).

use crate::credential::Credential;
use rt_foundation::escape_controls;

/// The replacement text.
pub const REDACTED: &str = "[REDACTED]";

/// A byte pattern with its Knuth-Morris-Pratt failure table.
#[derive(Clone)]
struct Needle {
    bytes: Vec<u8>,
    fail: Vec<usize>,
}

impl Needle {
    fn new(text: &str) -> Self {
        let bytes = text.as_bytes().to_vec();
        let mut fail: Vec<usize> = Vec::with_capacity(bytes.len());
        let mut k = 0;
        for (i, byte) in bytes.iter().enumerate() {
            if i == 0 {
                fail.push(0);
                continue;
            }
            while k > 0 && bytes.get(k) != Some(byte) {
                k = fail.get(k - 1).copied().unwrap_or(0);
            }
            if bytes.get(k) == Some(byte) {
                k += 1;
            }
            fail.push(k);
        }
        Self { bytes, fail }
    }

    // Every occurrence in `text`, overlapping ones included, in O(text).
    // Needle and text are valid UTF-8, which is self-synchronizing, so a
    // byte match always starts and ends on character boundaries. After a
    // full match `bytes.get(k)` is `None`, which falls back through `fail`.
    fn find_all(&self, text: &[u8], spans: &mut Vec<(usize, usize)>) {
        let len = self.bytes.len();
        let mut k = 0;
        for (end, byte) in text.iter().enumerate() {
            while k > 0 && self.bytes.get(k) != Some(byte) {
                k = self.fail.get(k - 1).copied().unwrap_or(0);
            }
            if self.bytes.get(k) == Some(byte) {
                k += 1;
            }
            if len > 0 && k == len {
                spans.push((end + 1 - len, end + 1));
            }
        }
    }
}

/// Replaces the credential value, the `Bearer <value>` form and the value in
/// JSON-escaped and control-escaped form with [`REDACTED`].
#[derive(Clone)]
pub struct Redactor {
    needles: Vec<Needle>,
}

fn json_escaped(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out
}

impl Redactor {
    /// A redactor for `credential`. It also matches the JSON-escaped and
    /// control-escaped renderings of the value. [`Credential`] is never
    /// empty, so no needle can match everywhere.
    #[must_use]
    pub fn new(credential: &Credential) -> Self {
        let value = credential.expose();
        let mut forms = vec![
            value.to_owned(),
            json_escaped(value),
            escape_controls(value),
        ];
        forms.sort();
        forms.dedup();
        let needles: Vec<Needle> = forms
            .iter()
            .flat_map(|f| [format!("Bearer {f}"), f.clone()])
            .map(|f| Needle::new(&f))
            .collect();
        Self { needles }
    }

    /// A redactor that changes nothing, for a run that has no credential
    /// (the offline mock provider, or output before a credential is read).
    #[must_use]
    pub fn no_credential() -> Self {
        Self {
            needles: Vec::new(),
        }
    }

    /// True when `text` contains the credential in any form that
    /// [`Redactor::redact`] replaces.
    #[must_use]
    pub fn finds(&self, text: &str) -> bool {
        let mut spans = Vec::new();
        self.needles.iter().any(|n| {
            n.find_all(text.as_bytes(), &mut spans);
            !spans.is_empty()
        })
    }

    /// `text` with every occurrence replaced.
    ///
    /// Matches are found in the original text only, so a marker is never
    /// searched or rewritten. Overlapping matches become one marker.
    #[must_use]
    pub fn redact(&self, text: &str) -> String {
        let mut spans: Vec<(usize, usize)> = Vec::new();
        for needle in &self.needles {
            needle.find_all(text.as_bytes(), &mut spans);
        }
        spans.sort_unstable();
        let mut out = String::with_capacity(text.len());
        let mut copied = 0;
        let mut merged = spans.into_iter().peekable();
        while let Some((start, mut end)) = merged.next() {
            while let Some(&(next_start, next_end)) = merged.peek() {
                // Adjacent spans stay separate; only real overlap merges.
                if next_start >= end {
                    break;
                }
                end = end.max(next_end);
                merged.next();
            }
            out.push_str(&text[copied..start]);
            out.push_str(REDACTED);
            copied = end;
        }
        out.push_str(&text[copied..]);
        out
    }
}

impl std::fmt::Debug for Redactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Redactor([REDACTED])")
    }
}
