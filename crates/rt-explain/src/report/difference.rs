//! The closed table of published-difference kinds (Req 13.12).
//!
//! One row per kind: the kind name, its catalogue code and its description.
//! Validation and both writers read this table and no other mapping.

use rt_foundation::Code;

/// One difference kind of a published-result comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DifferenceKindInfo {
    name: &'static str,
    code: Code,
    description: &'static str,
}

impl DifferenceKindInfo {
    /// The kind name as the engine reports it.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The catalogue code of the kind.
    #[must_use]
    pub fn code(&self) -> Code {
        self.code
    }

    /// What the kind means, after Req 13.12.
    #[must_use]
    pub fn description(&self) -> &'static str {
        self.description
    }
}

/// Every supported kind, in the order of Req 13.12. The engine's kinds map
/// one to one onto these rows; a cross-crate test enforces it.
pub const DIFFERENCE_KINDS: &[DifferenceKindInfo] = &[
    DifferenceKindInfo {
        name: "value_differs",
        code: Code::PublishedResultDiffers,
        description: "the published value is not the computed value",
    },
    DifferenceKindInfo {
        name: "tie_broken_without_declared_criterion",
        code: Code::PublishedTieGroupSplit,
        description: "the published result gives members of one computed tie group different positions",
    },
    DifferenceKindInfo {
        name: "tie_not_in_computed_result",
        code: Code::PublishedTieNotComputed,
        description: "the published result gives one position to participants that the computed result separates",
    },
];

/// The row of a kind name, or `None` for a name that is not supported.
#[must_use]
pub fn difference_kind(name: &str) -> Option<&'static DifferenceKindInfo> {
    DIFFERENCE_KINDS.iter().find(|k| k.name == name)
}
