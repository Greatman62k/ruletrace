//! Behaviour of the generated [`Code`] enum that is not generated.

use crate::Code;
use std::cmp::Ordering;
use std::fmt;

impl Code {
    /// True when the code's severity is blocking (Error, Ambiguity or
    /// Unsupported).
    #[must_use]
    pub fn is_blocking(self) -> bool {
        self.severity().is_blocking()
    }
}

/// Codes order by their identifier bytes (`RT-<family>-<number>`), as Req 7.10
/// requires for the "Diagnostic code in ascending lexicographic order" and for
/// a No_Location reason code.
impl Ord for Code {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id().as_bytes().cmp(other.id().as_bytes())
    }
}

impl PartialOrd for Code {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}
