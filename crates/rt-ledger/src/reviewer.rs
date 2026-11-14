//! The `Reviewer` identity (design §3.4.4, UD-23; Req 26.17).

use rt_foundation::{CliLocation, Code, Diagnostic, Location};
use rt_ir::{Identity, Origin};
use std::collections::BTreeSet;

/// The AI identities of a target revision: proposer, provider, model and every
/// recorded AI identity. Compared with a Reviewer by exact equality of Unicode
/// scalar values, with no normalization, case folding or trimming (UD-23).
/// Names are not authenticated (R-8).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AiIdentitySet(BTreeSet<String>);

impl AiIdentitySet {
    /// Builds a set from names.
    #[must_use]
    pub fn new<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self(names.into_iter().map(Into::into).collect())
    }

    /// The AI identities recorded in an Origin: none for a manual revision.
    #[must_use]
    pub fn from_origin(origin: &Origin) -> Self {
        match origin {
            Origin::Manual { .. } => Self::default(),
            Origin::AiProposal(ai) => {
                let mut set = BTreeSet::new();
                set.insert(ai.proposer.as_str().to_owned());
                set.insert(ai.provider.clone());
                set.insert(ai.model.clone());
                set.extend(ai.ai_identities.iter().map(|i| i.as_str().to_owned()));
                Self(set)
            }
        }
    }

    /// Adds the identities of `other` (for example those of a Proposal_Store
    /// batch).
    #[must_use]
    pub fn union(mut self, other: &Self) -> Self {
        self.0.extend(other.0.iter().cloned());
        self
    }

    /// True when `name` is in the set (exact comparison).
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.0.contains(name)
    }
}

/// A human Reviewer: 1 to 200 Unicode scalar values. The type has no AI variant
/// and its only constructor is [`Reviewer::new`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Reviewer(Identity);

fn reviewer_location() -> Location {
    Location::Cli(CliLocation::Argument {
        position: 0,
        name: Some("--reviewer".to_owned()),
    })
}

impl Reviewer {
    /// Validates the Reviewer name.
    ///
    /// # Errors
    /// `REVIEW_REVIEWER_INVALID` (`RT-APR-0202`) for empty text or more than
    /// 200 scalar values.
    #[allow(clippy::result_large_err)] // The design §3.4.4 signature returns a Diagnostic.
    pub fn new(raw: &str) -> Result<Self, Diagnostic> {
        Identity::new(raw).map(Self).map_err(|e| {
            Diagnostic::new(
                Code::ReviewReviewerInvalid,
                format!("reviewer is invalid: {e:?}"),
                reviewer_location(),
            )
        })
    }

    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Refuses a Reviewer equal to an AI identity.
    ///
    /// # Errors
    /// `REVIEW_SELF_APPROVAL` (`RT-APR-0201`).
    #[allow(clippy::result_large_err)] // The design §3.4.4 signature returns a Diagnostic.
    pub fn check_not_ai(&self, known: &AiIdentitySet) -> Result<(), Diagnostic> {
        if known.contains(self.as_str()) {
            return Err(Diagnostic::new(
                Code::ReviewSelfApproval,
                "the reviewer equals a proposer, provider, model or AI identity of the target"
                    .to_owned(),
                reviewer_location(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewer_length_bounds() {
        assert!(Reviewer::new("").is_err_and(|d| d.code() == Code::ReviewReviewerInvalid));
        assert!(Reviewer::new(&"é".repeat(200)).is_ok());
        assert!(Reviewer::new(&"é".repeat(201)).is_err());
    }

    #[test]
    fn exact_comparison_only() {
        let known = AiIdentitySet::new(["gpt-x"]);
        assert!(Reviewer::new("GPT-X").unwrap().check_not_ai(&known).is_ok());
        assert!(Reviewer::new("gpt-x")
            .unwrap()
            .check_not_ai(&known)
            .is_err_and(|d| d.code() == Code::ReviewSelfApproval));
    }
}
