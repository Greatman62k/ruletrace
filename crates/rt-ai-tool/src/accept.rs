//! The accepted batch: the only value the Proposal_Store writer takes
//! (design 3.7.4 steps 1 to 4, UD-26, UD-36; Req 23.8, 23.10, 30.1 to 30.5,
//! 30.7, 30.9).
//!
//! [`accept_batch`] runs the stages in one fixed order: parse and schema,
//! provenance, credential echo, semantic validation. A failure of any of
//! the first three rejects the whole batch. Semantic findings are data and
//! never reject it.

use crate::out::render_diagnostic;
use crate::plan::ClauseId;
use crate::provenance::verify_provenance;
use crate::redact::Redactor;
use crate::response::{
    parse_response, ParsedEvidence, ParsedProposal, ParsedResponse, ParsedRule, ProposalKind,
};
use rt_ai_adapter::Completion;
use rt_foundation::{Code, Diagnostic, InputId, InputLocation, Location};
use rt_hash::{sha256, Sha256Hex};
use rt_ir::{draft_hash, print_draft, DocumentDecl, Loc, Rulebook};
use rt_proposals::{
    proposal_id, Completion as StoredCompletion, Evidence, Proposal, ProposalKind as StoredKind,
    RuleDraft as StoredDraft, StoredDiagnostic, StoredLocation,
};
use rt_srcmodel::SourceModels;
use rt_validate::{validate_content, RuleDraft};

/// Everything [`accept_batch`] needs. All inputs are explicit values.
pub struct AcceptInput<'a> {
    /// Names the response in every Diagnostic location.
    pub response_id: &'a InputId,
    /// The Request_Identifier of the Provider_Request that was answered.
    pub request_id: &'a Sha256Hex,
    /// How the provider ended the response.
    pub completion: Completion,
    /// The untrusted payload bytes.
    pub payload: &'a [u8],
    /// The configured response size limit in bytes.
    pub response_limit: u64,
    /// The pre-loaded Source_Models.
    pub models: &'a SourceModels,
    /// The Candidate_Clauses of the Provider_Request.
    pub clauses: &'a [ClauseId],
    /// The Rulebook whose declarations the proposed rules are checked in.
    pub declarations: &'a Rulebook,
    /// Matches the credential value. Use [`Redactor::no_credential`] for a
    /// run that has none.
    pub redactor: &'a Redactor,
}

/// A response that passed the protocol, schema, provenance and credential
/// checks, with the Semantic_Outcome of every proposal.
///
/// The fields are private and [`accept_batch`] is the only constructor, so
/// a value of this type is proof that the checks ran in order. The caller
/// is the Proposal_Store writer, which accepts no other type. Every stored
/// proposal is non-executable data. No flag comes from the provider, and
/// the blocking state is derived from the stored findings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptedBatch {
    response_id: InputId,
    request_id: Sha256Hex,
    payload: String,
    response_sha256: Sha256Hex,
    proposals: Vec<Proposal>,
}

impl AcceptedBatch {
    /// The Request_Identifier of the answered request.
    #[must_use]
    pub fn request_id(&self) -> &Sha256Hex {
        &self.request_id
    }

    /// The exact accepted payload text, for the stored response.
    #[must_use]
    pub fn payload(&self) -> &str {
        &self.payload
    }

    /// The SHA-256 of the payload bytes.
    #[must_use]
    pub fn response_sha256(&self) -> &Sha256Hex {
        &self.response_sha256
    }

    /// How the stored response ended.
    #[must_use]
    pub fn completion(&self) -> StoredCompletion {
        StoredCompletion::Complete
    }

    /// The proposals in response order. Only a rule suggestion has a draft.
    #[must_use]
    pub fn proposals(&self) -> &[Proposal] {
        &self.proposals
    }

    /// True when any proposal has a blocking finding. Derived from the
    /// stored findings only.
    #[must_use]
    pub fn has_blocking_findings(&self) -> bool {
        self.proposals
            .iter()
            .flat_map(|p| p.outcome.iter())
            .any(|d| d.code.severity().is_blocking())
    }

    /// The Warning `RT-AI-0305`, once for a batch with blocking findings
    /// (UD-26). It never changes an exit code.
    #[must_use]
    pub fn blocking_warning(&self) -> Option<Diagnostic> {
        self.has_blocking_findings().then(|| {
            Diagnostic::new(
                Code::AiBatchHasBlockingProposals,
                "the stored batch has proposals with blocking findings".to_owned(),
                plain(&self.response_id, None),
            )
        })
    }
}

/// Parses, checks and validates one provider response.
///
/// No `Err` value carries the credential. Every rejection passes through
/// one scrub: if the message, the location or the rendered form of any
/// Diagnostic contains the credential, the whole list is replaced by a
/// single `RT-AI-0206` that names the nearest credential-free field path.
/// The batch is rejected either way, so replacing is simpler than
/// rewriting text in each stage's Diagnostics, and it holds for stages
/// added later.
///
/// # Errors
/// The Diagnostics of the earliest failing stage, which rejects the whole
/// batch: one Diagnostic from [`parse_response`] (`RT-AI-0201` to
/// `RT-AI-0205`, exit 6), every provenance Diagnostic (`RT-PRV-0101` to
/// `RT-PRV-0104`, exit 3), or `RT-AI-0206` (exit 6) when the payload or any
/// text of the batch contains the credential value.
pub fn accept_batch(input: &AcceptInput<'_>) -> Result<AcceptedBatch, Vec<Diagnostic>> {
    accept_unscrubbed(input).map_err(|found| scrub(input, found))
}

fn accept_unscrubbed(input: &AcceptInput<'_>) -> Result<AcceptedBatch, Vec<Diagnostic>> {
    let parsed = parse_response(
        input.response_id,
        input.completion,
        input.payload,
        input.response_limit,
    )
    .map_err(|d| vec![d])?;
    let provenance = verify_provenance(input.response_id, &parsed, input.models, input.clauses);
    if !provenance.is_empty() {
        return Err(provenance);
    }
    // The payload is UTF-8 because `parse_response` accepted it.
    let payload = String::from_utf8_lossy(input.payload).into_owned();
    let leak = Leak {
        redactor: input.redactor,
        id: input.response_id,
    };
    if let Some(d) = leak
        .in_payload(&payload)
        .or_else(|| leak.in_parsed(&parsed))
    {
        return Err(vec![d]);
    }
    let proposals = parsed
        .proposals
        .iter()
        .enumerate()
        .map(|(index, p)| stored(input, p, index))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|d| vec![d])?;
    if let Some(d) = leak
        .in_drafts(&proposals)
        .or_else(|| leak.in_outcomes(&proposals))
    {
        return Err(vec![d]);
    }
    Ok(AcceptedBatch {
        response_id: input.response_id.clone(),
        request_id: input.request_id.clone(),
        response_sha256: sha256(input.payload),
        payload,
        proposals,
    })
}

// Replaces a rejection that echoes the credential. Stage Diagnostics copy
// unknown keys, kinds and document ids from the untrusted response.
fn scrub(input: &AcceptInput<'_>, found: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let Some(leaking) = found.iter().find(|d| echoes(input.redactor, d)) else {
        return found;
    };
    let field = match leaking.primary() {
        Location::Input(at) => at.field.as_deref(),
        _ => None,
    };
    let leak = Leak {
        redactor: input.redactor,
        id: input.response_id,
    };
    vec![leak.found(clean_ancestor(input.redactor, field))]
}

fn echoes(redactor: &Redactor, d: &Diagnostic) -> bool {
    [
        format!("{d:?}"),
        d.message().to_owned(),
        render_diagnostic(d),
    ]
    .iter()
    .any(|text| redactor.finds(text))
}

// The longest prefix of `field` that ends before a `.` or `[` and does not
// contain the credential, or the fixed name `response`.
fn clean_ancestor(redactor: &Redactor, field: Option<&str>) -> String {
    let mut path = field.unwrap_or("response");
    while redactor.finds(path) {
        match path.rfind(['.', '[']) {
            Some(cut) => path = &path[..cut],
            None => return "response".to_owned(),
        }
    }
    path.to_owned()
}

fn stored(
    input: &AcceptInput<'_>,
    p: &ParsedProposal,
    index: usize,
) -> Result<Proposal, Diagnostic> {
    let (rule, outcome) = match &p.kind {
        ProposalKind::RuleSuggestion(r) => {
            let draft = RuleDraft {
                id: r.id.clone(),
                content: (*r.content).clone(),
            };
            let findings = validate_content(input.declarations, &draft, input.models);
            let documents = verified_documents(r, input.models);
            let hash = draft_hash(
                &input.declarations.ir_version,
                &r.id,
                &r.content,
                &documents,
            )
            // Unreachable: provenance verified every cited document. Reject
            // the batch rather than store a draft without its hash.
            .map_err(|e| {
                Diagnostic::new(
                    Code::AiResponseSchemaViolation,
                    format!("the rule draft cannot be hashed: {e:?}"),
                    plain(input.response_id, Some(format!("proposals[{index}].rule"))),
                )
            })?;
            let stored = StoredDraft {
                id: r.id.clone(),
                json: print_draft(&r.id, &r.content),
            };
            let outcome = findings.as_slice().iter().map(stored_diagnostic).collect();
            (Some((stored, hash)), outcome)
        }
        _ => (None, Vec::new()),
    };
    Ok(Proposal {
        proposal_id: proposal_id(input.request_id, index),
        kind: stored_kind(&p.kind),
        proposer: p.proposer.clone(),
        rationale: p.rationale.clone(),
        evidence: p.evidence.iter().map(stored_evidence).collect(),
        rule,
        outcome,
    })
}

// The declared hash of each document the draft cites is the hash of the
// verified Source_Model, so no hash is guessed. Adoption recomputes the
// Draft hash against the Rulebook and refuses a mismatch.
fn verified_documents(rule: &ParsedRule, models: &SourceModels) -> Vec<DocumentDecl> {
    let mut out: Vec<DocumentDecl> = Vec::new();
    for span in &rule.content.spans {
        let known = out.iter().any(|d| d.id == span.document);
        if let (false, Some(m)) = (known, models.get(&span.document)) {
            out.push(DocumentDecl {
                loc: Loc::NONE,
                id: span.document.clone(),
                sha256: m.sha256().clone(),
            });
        }
    }
    out
}

fn stored_kind(kind: &ProposalKind) -> StoredKind {
    match kind {
        ProposalKind::ClauseInterpretation => StoredKind::ClauseInterpretation,
        ProposalKind::RuleSuggestion(_) => StoredKind::RuleSuggestion,
        ProposalKind::ClauseRelationship => StoredKind::ClauseRelationship,
        ProposalKind::ConflictHighlight => StoredKind::ConflictHighlight,
        ProposalKind::CandidateTest => StoredKind::CandidateTest,
    }
}

fn stored_evidence(e: &ParsedEvidence) -> Evidence {
    Evidence {
        document: e.document.clone(),
        sha256: e.sha256.clone(),
        start: e.start,
        end: e.end,
        line: e.line,
        column: e.column,
        quote: e.quote.clone(),
    }
}

fn stored_diagnostic(d: &Diagnostic) -> StoredDiagnostic {
    let location = match d.primary() {
        Location::Source(span) => StoredLocation::Source {
            document: span.doc().clone(),
            start: span.start().get(),
            end: span.end().get(),
        },
        Location::Document(doc) => StoredLocation::Document(doc.clone()),
        Location::Input(at) => StoredLocation::Field(
            at.field
                .as_ref()
                .map_or_else(|| "rule".to_owned(), |f| format!("rule.{f}")),
        ),
        Location::Cli(_) | Location::None(_) => StoredLocation::Field("rule".to_owned()),
    };
    StoredDiagnostic {
        code: d.code(),
        message: d.message().to_owned(),
        location,
    }
}

fn plain(id: &InputId, field: Option<String>) -> Location {
    Location::Input(InputLocation {
        input: id.clone(),
        line: None,
        column: None,
        field,
    })
}

// Finds the credential in everything the store would keep. The Diagnostic
// names the field and never the value.
struct Leak<'a> {
    redactor: &'a Redactor,
    id: &'a InputId,
}

impl Leak<'_> {
    fn found(&self, field: String) -> Diagnostic {
        Diagnostic::new(
            Code::AiResponseCredentialEcho,
            "the response contains the credential value and is not stored".to_owned(),
            plain(self.id, Some(field)),
        )
    }

    fn in_payload(&self, payload: &str) -> Option<Diagnostic> {
        self.redactor
            .finds(payload)
            .then(|| self.found("payload".to_owned()))
    }

    // Decoded text catches a value that the payload spells with JSON
    // escapes, which the raw bytes do not contain.
    fn in_parsed(&self, parsed: &ParsedResponse) -> Option<Diagnostic> {
        parsed.proposals.iter().enumerate().find_map(|(i, p)| {
            let mut texts: Vec<(String, &str)> = vec![
                (format!("proposals[{i}].proposer"), p.proposer.as_str()),
                (format!("proposals[{i}].rationale"), p.rationale.as_str()),
            ];
            for (j, e) in p.evidence.iter().enumerate() {
                texts.push((format!("proposals[{i}].evidence[{j}].quote"), &e.quote));
            }
            if let ProposalKind::RuleSuggestion(r) = &p.kind {
                texts.push((format!("proposals[{i}].rule"), &r.json));
            }
            texts
                .into_iter()
                .find(|(_, t)| self.redactor.finds(t))
                .map(|(field, _)| self.found(field))
        })
    }

    // The stored draft is the canonical print, not the provider's text.
    fn in_drafts(&self, proposals: &[Proposal]) -> Option<Diagnostic> {
        proposals.iter().enumerate().find_map(|(i, p)| {
            let (draft, _) = p.rule.as_ref()?;
            self.redactor
                .finds(&draft.json)
                .then(|| self.found(format!("proposals[{i}].rule")))
        })
    }

    fn in_outcomes(&self, proposals: &[Proposal]) -> Option<Diagnostic> {
        proposals.iter().enumerate().find_map(|(i, p)| {
            p.outcome
                .iter()
                .position(|d| self.redactor.finds(&d.message))
                .map(|k| self.found(format!("proposals[{i}].outcome[{k}]")))
        })
    }
}
