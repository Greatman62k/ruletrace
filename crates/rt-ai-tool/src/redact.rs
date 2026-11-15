//! Redaction of the credential value (Req 28.9).

use crate::credential::Credential;
use rt_foundation::escape_controls;

/// The replacement text.
pub const REDACTED: &str = "[REDACTED]";

/// Replaces the credential value, the `Bearer <value>` form and the value in
/// JSON-escaped and control-escaped form with [`REDACTED`].
#[derive(Clone)]
pub struct Redactor {
    needles: Vec<String>,
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
    /// control-escaped renderings of the value.
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
        let mut needles: Vec<String> = forms
            .iter()
            .flat_map(|f| [format!("Bearer {f}"), f.clone()])
            .collect();
        // Longest first so a longer form is never split by a shorter one.
        needles.sort_by_key(|n| std::cmp::Reverse(n.len()));
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

    /// `text` with every occurrence replaced.
    #[must_use]
    pub fn redact(&self, text: &str) -> String {
        self.needles
            .iter()
            .fold(text.to_owned(), |acc, n| acc.replace(n.as_str(), REDACTED))
    }
}

impl std::fmt::Debug for Redactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Redactor([REDACTED])")
    }
}
