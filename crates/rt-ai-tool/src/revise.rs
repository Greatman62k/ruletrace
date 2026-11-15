//! The `revise` member of a Provider_Request (Req 23.16, design 3.7.2).
//!
//! A revision request carries the content of the named Rule_Revision and
//! the Reviewer feedback as additional context. Both are plain data for the
//! provider. The feedback is untrusted and travels only inside the request,
//! so it is sent under the consent of the clauses and is never logged.

use rt_foundation::{RevisionNumber, RuleId};
use rt_ir::{print_draft, RuleKey, Rulebook};
use rt_json::Value;

/// Largest feedback text, in bytes. A longer text refuses the run.
pub const MAX_FEEDBACK_BYTES: usize = 8_192;

/// Why a revision request names no Rule_Revision of the Rulebook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unknown {
    /// No revision of the rule exists.
    Rule,
    /// The rule exists but not this revision.
    Revision,
}

/// The named Rule_Revision and the Reviewer feedback of one run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviseContext {
    rule: RuleId,
    revision: RevisionNumber,
    content: String,
    feedback: String,
}

impl ReviseContext {
    /// Resolves `rule` and `revision` in `rulebook`.
    ///
    /// The Rulebook is read as declarations and revisions only. No
    /// approval state is consulted, so the named revision may be in any
    /// state.
    pub(crate) fn resolve(
        rulebook: &Rulebook,
        rule: &RuleId,
        revision: RevisionNumber,
        feedback: String,
    ) -> Result<Self, Unknown> {
        if !rulebook.revisions.iter().any(|r| &r.key.id == rule) {
            return Err(Unknown::Rule);
        }
        let key = RuleKey {
            id: rule.clone(),
            revision,
        };
        let found = rulebook.revision(&key).ok_or(Unknown::Revision)?;
        Ok(Self {
            rule: rule.clone(),
            revision,
            content: print_draft(rule, &found.content),
            feedback,
        })
    }

    /// The member value: rule, revision, canonical content and feedback.
    pub(crate) fn value(&self) -> Option<Value<()>> {
        Value::object(vec![
            ("rule".to_owned(), Value::string(self.rule.as_str())),
            ("revision".to_owned(), Value::integer(self.revision.get())),
            ("content".to_owned(), Value::string(self.content.as_str())),
            ("feedback".to_owned(), Value::string(self.feedback.as_str())),
        ])
        .ok()
    }
}
