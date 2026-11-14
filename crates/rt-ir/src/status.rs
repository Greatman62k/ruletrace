//! The derived approval status.

/// The approval status of a rule revision (Req 5.1).
///
/// The status is derived from a Rulebook and a verified Approval_Ledger and is
/// never stored or read from data. This type therefore has no `FromStr`, no
/// `From` conversion, no `Default`, no deserialization and no function that
/// takes text or a flag. No Rulebook type holds one. A source test in this
/// crate fails if any of these appears.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ApprovalStatus {
    /// No matching decision exists (or only stale approval records).
    Proposed,
    /// A matching approval record exists.
    Approved,
    /// A rejection record names the current content.
    Rejected,
}

impl ApprovalStatus {
    /// The display name. Display only: there is no inverse.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ApprovalStatus::Proposed => "Proposed",
            ApprovalStatus::Approved => "Approved",
            ApprovalStatus::Rejected => "Rejected",
        }
    }
}
