//! Kleene three-valued logic (design §3.6.3, Req 11.4).

use std::ops::Not;

/// True, False or Unknown. Faults are outside this type; the engine merges them separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tri {
    /// Known true.
    True,
    /// Known false.
    False,
    /// Unknown because information is missing.
    Unknown,
}

impl Tri {
    /// The known value of a Boolean.
    #[must_use]
    pub fn from_bool(value: bool) -> Self {
        if value {
            Tri::True
        } else {
            Tri::False
        }
    }

    /// `Some` for a known value, `None` for Unknown.
    #[must_use]
    pub fn to_bool(self) -> Option<bool> {
        match self {
            Tri::True => Some(true),
            Tri::False => Some(false),
            Tri::Unknown => None,
        }
    }

    /// Kleene AND: false if either side is false, true only if both are true, else Unknown.
    #[must_use]
    pub fn and(self, other: Tri) -> Tri {
        match (self, other) {
            (Tri::False, _) | (_, Tri::False) => Tri::False,
            (Tri::True, Tri::True) => Tri::True,
            _ => Tri::Unknown,
        }
    }

    /// Kleene OR: true if either side is true, false only if both are false, else Unknown.
    #[must_use]
    pub fn or(self, other: Tri) -> Tri {
        match (self, other) {
            (Tri::True, _) | (_, Tri::True) => Tri::True,
            (Tri::False, Tri::False) => Tri::False,
            _ => Tri::Unknown,
        }
    }
}

impl Not for Tri {
    type Output = Tri;

    /// Kleene NOT: Unknown stays Unknown.
    fn not(self) -> Tri {
        match self {
            Tri::True => Tri::False,
            Tri::False => Tri::True,
            Tri::Unknown => Tri::Unknown,
        }
    }
}
