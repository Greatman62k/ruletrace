//! Transmission_Consent (Req 28.7).

use rt_foundation::{escape_controls, Code, Diagnostic, DocumentId, Location};
use std::collections::BTreeSet;

/// The consent argument of one invocation.
#[derive(Clone, Debug)]
pub struct Consent {
    documents: Option<BTreeSet<DocumentId>>,
    at: Location,
}

impl Consent {
    /// No consent argument was given. `at` is the command line.
    #[must_use]
    pub fn absent(at: Location) -> Self {
        Self {
            documents: None,
            at,
        }
    }

    /// A consent argument naming `documents`. `at` is that argument.
    #[must_use]
    pub fn named(documents: BTreeSet<DocumentId>, at: Location) -> Self {
        Self {
            documents: Some(documents),
            at,
        }
    }

    /// `RT-AI-0008` unless document text is allowed and the consent set
    /// equals `run_documents` exactly.
    pub(crate) fn check(
        &self,
        allow_document_text: bool,
        run_documents: &BTreeSet<DocumentId>,
    ) -> Option<Diagnostic> {
        let fail = |message: String| {
            Some(Diagnostic::new(
                Code::AiConsentMissing,
                message,
                self.at.clone(),
            ))
        };
        if !allow_document_text {
            return fail(
                "`allow_document_text` is false, so no document text may be sent".to_owned(),
            );
        }
        let Some(consented) = &self.documents else {
            return fail("no consent argument names the documents to send".to_owned());
        };
        if consented == run_documents {
            return None;
        }
        let list = |a: &BTreeSet<DocumentId>, b: &BTreeSet<DocumentId>| {
            a.difference(b)
                .map(|d| format!("'{}'", escape_controls(d.as_str())))
                .collect::<Vec<_>>()
                .join(", ")
        };
        fail(format!(
            "consent must name exactly the documents sent; missing: [{}]; not sent: [{}]",
            list(run_documents, consented),
            list(consented, run_documents)
        ))
    }
}
