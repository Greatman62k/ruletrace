//! Diagnostic severity.

/// Severity of a diagnostic. Fixed per catalogue code, never chosen by a caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    /// A violation that blocks compilation.
    Error,
    /// A non-blocking observation. Never changes compilability.
    Warning,
    /// More than one interpretation exists. Blocking.
    Ambiguity,
    /// A construct outside the supported boundary. Blocking.
    Unsupported,
}

impl Severity {
    /// True for `Error`, `Ambiguity` and `Unsupported`; false for `Warning`.
    #[must_use]
    pub fn is_blocking(self) -> bool {
        !matches!(self, Severity::Warning)
    }
}

#[cfg(test)]
mod tests {
    use super::Severity;

    #[test]
    fn only_warning_is_non_blocking() {
        assert!(Severity::Error.is_blocking());
        assert!(Severity::Ambiguity.is_blocking());
        assert!(Severity::Unsupported.is_blocking());
        assert!(!Severity::Warning.is_blocking());
    }
}
