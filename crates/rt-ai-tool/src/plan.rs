//! Request building and the Request_Identifier (design 3.7.2, Req 23.2,
//! 23.3, 23.32, 29.7, 29.16).

use crate::config::AiConfig;
use crate::decls::declarations;
use crate::revise::ReviseContext;
use crate::txlog::LogContext;
use crate::vocab::{vocabulary, Ud32Option};
use rt_ai_adapter::{ProviderRequest, RequestId};
use rt_foundation::{escape_controls, Code, Diagnostic, DocumentId, Location};
use rt_hash::Sha256Hex;
use rt_ir::Rulebook;
use rt_json::{write_compact, Value};
use rt_srcmodel::{CandidateClause, SourceModels};
use std::collections::{BTreeMap, BTreeSet};

/// Largest clause text, in bytes. A longer clause rejects the whole run.
pub const MAX_CLAUSE_BYTES: usize = 8_192;

/// Largest canonical request, in bytes (256 KiB).
pub const MAX_REQUEST_BYTES: usize = 262_144;

/// The task name of every request of this tool.
pub const TASK: &str = "propose_rules";

/// Identifies a Candidate_Clause in a Run_Summary.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClauseId {
    /// Document identifier.
    pub document: DocumentId,
    /// Start byte offset.
    pub start: u32,
    /// End byte offset (exclusive).
    pub end: u32,
}

/// Everything the builder reads. All of it is explicit input.
pub struct PlanInput<'a> {
    /// The validated configuration.
    pub config: &'a AiConfig,
    /// Source_Models of the documents the clauses come from.
    pub models: &'a SourceModels,
    /// The supplied Candidate_Clauses, in any order.
    pub candidates: &'a [CandidateClause],
    /// The Rulebook whose declarations are sent.
    pub rulebook: &'a Rulebook,
    /// The UD-32 option that selects the vocabulary.
    pub option: Ud32Option,
    /// Location of the candidates input, for `RT-AI-0304`.
    pub candidates_at: Location,
    /// The revision request, put into every request of the run.
    pub revise: Option<&'a ReviseContext>,
}

/// One Provider_Request with its canonical bytes and identifier.
#[derive(Clone, Debug)]
pub struct PlannedRequest {
    request: ProviderRequest,
    bytes: Vec<u8>,
    id: RequestId,
    clauses: Vec<ClauseId>,
    log: LogContext,
}

impl PlannedRequest {
    /// The provider-neutral request.
    #[must_use]
    pub fn request(&self) -> &ProviderRequest {
        &self.request
    }

    /// The canonical bytes, computed once.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// SHA-256 of the canonical bytes, computed once.
    #[must_use]
    pub fn id(&self) -> &RequestId {
        &self.id
    }

    /// The clauses of the request in ascending order.
    #[must_use]
    pub fn clauses(&self) -> &[ClauseId] {
        &self.clauses
    }

    /// The Transmission_Log context of the request.
    #[must_use]
    pub fn log_context(&self) -> &LogContext {
        &self.log
    }
}

/// The ordered requests of one run.
#[derive(Clone, Debug)]
pub struct RunPlan {
    requests: Vec<PlannedRequest>,
}

impl RunPlan {
    /// The requests in ascending order of their first clause.
    #[must_use]
    pub fn requests(&self) -> &[PlannedRequest] {
        &self.requests
    }

    /// The documents whose text the run sends, for the consent check.
    #[must_use]
    pub fn documents(&self) -> BTreeSet<DocumentId> {
        self.requests
            .iter()
            .flat_map(|r| r.clauses.iter().map(|c| c.document.clone()))
            .collect()
    }
}

fn invalid(at: &Location, message: String) -> Diagnostic {
    Diagnostic::new(Code::AiRequestInvalid, message, at.clone())
}

fn describe(clause: &CandidateClause) -> String {
    let span = clause.span();
    format!(
        "clause {}:{}..{}",
        escape_controls(span.doc().as_str()),
        span.start().get(),
        span.end().get()
    )
}

struct Checked<'a> {
    clause: &'a CandidateClause,
    sha256: Sha256Hex,
}

fn check_clauses<'a>(input: &'a PlanInput<'_>) -> Result<Vec<Checked<'a>>, Vec<Diagnostic>> {
    let at = &input.candidates_at;
    if input.candidates.is_empty() {
        return Err(vec![invalid(at, "no Candidate_Clause was supplied".into())]);
    }
    let mut found = Vec::new();
    let mut checked = Vec::new();
    let mut seen: BTreeSet<ClauseId> = BTreeSet::new();
    for clause in input.candidates {
        let span = clause.span();
        // A repeated clause is an input fault; it is never deduplicated
        // silently, so what is sent is exactly what was supplied.
        let id = ClauseId {
            document: span.doc().clone(),
            start: span.start().get(),
            end: span.end().get(),
        };
        if !seen.insert(id) {
            found.push(invalid(
                at,
                format!("{} is a duplicate Candidate_Clause", describe(clause)),
            ));
            continue;
        }
        if clause.text().len() > MAX_CLAUSE_BYTES {
            found.push(invalid(
                at,
                format!(
                    "{} has {} bytes, above the limit of {MAX_CLAUSE_BYTES}",
                    describe(clause),
                    clause.text().len()
                ),
            ));
            continue;
        }
        let Some(model) = input.models.get(span.doc()) else {
            found.push(invalid(
                at,
                format!("{} names a document with no Source_Model", describe(clause)),
            ));
            continue;
        };
        if model.span_text(span) != Some(clause.text()) {
            found.push(invalid(
                at,
                format!("{} differs from its Source_Model", describe(clause)),
            ));
            continue;
        }
        checked.push(Checked {
            clause,
            sha256: model.sha256().clone(),
        });
    }
    if found.is_empty() {
        Ok(checked)
    } else {
        Err(found)
    }
}

fn clause_value(c: &Checked<'_>) -> Option<Value<()>> {
    let span = c.clause.span();
    Value::object(vec![
        ("document".to_owned(), Value::string(span.doc().as_str())),
        ("sha256".to_owned(), Value::string(c.sha256.as_str())),
        ("start".to_owned(), Value::integer(span.start().get())),
        ("end".to_owned(), Value::integer(span.end().get())),
        ("line".to_owned(), Value::integer(span.line())),
        ("column".to_owned(), Value::integer(span.column())),
        ("text".to_owned(), Value::string(c.clause.text())),
    ])
    .ok()
}

struct Shared {
    config: AiConfig,
    declarations: Value<()>,
    vocabulary: Value<()>,
    revise: Value<()>,
}

impl Shared {
    fn request(&self, clauses: Vec<Value<()>>) -> ProviderRequest {
        ProviderRequest {
            task: TASK.to_owned(),
            model: self.config.model.clone(),
            limits: self.config.limits,
            context: vec![
                ("clauses".to_owned(), Value::array(clauses)),
                ("declarations".to_owned(), self.declarations.clone()),
                ("vocabulary".to_owned(), self.vocabulary.clone()),
                ("revise".to_owned(), self.revise.clone()),
            ],
        }
    }

    fn finish(&self, chunk: &[(Checked<'_>, Value<()>)]) -> Option<PlannedRequest> {
        let request = self.request(chunk.iter().map(|(_, v)| v.clone()).collect());
        let bytes = request.canonical_bytes()?;
        let id = request.request_id()?;
        let clauses = chunk
            .iter()
            .map(|(c, _)| {
                let s = c.clause.span();
                ClauseId {
                    document: s.doc().clone(),
                    start: s.start().get(),
                    end: s.end().get(),
                }
            })
            .collect();
        let mut documents: BTreeMap<DocumentId, Sha256Hex> = BTreeMap::new();
        for (c, _) in chunk {
            documents.insert(c.clause.span().doc().clone(), c.sha256.clone());
        }
        let log = LogContext {
            provider: self.config.provider.name().to_owned(),
            model: self.config.model.clone(),
            endpoint_host: self
                .config
                .endpoint
                .as_ref()
                .map(|e| e.host.clone())
                .unwrap_or_default(),
            documents: documents.into_iter().collect(),
            clauses: u32::try_from(chunk.len()).ok()?,
        };
        Some(PlannedRequest {
            request,
            bytes,
            id,
            clauses,
            log,
        })
    }
}

fn cannot_build(at: &Location) -> Vec<Diagnostic> {
    vec![invalid(at, "the request could not be built".into())]
}

/// Builds the requests of a run.
///
/// Candidates are sorted by (start, document id bytes, end) and split into
/// consecutive chunks of at most `max_clauses_per_request` clauses and at
/// most [`MAX_REQUEST_BYTES`] canonical bytes. The identifier of each request
/// is the SHA-256 of its canonical bytes. Nothing is sent and no file is
/// touched.
///
/// # Errors
/// `RT-AI-0304` for each invalid or duplicate clause (same document,
/// start and end) or when no request fits the size limit, in clause order.
pub fn build_plan(input: &PlanInput<'_>) -> Result<RunPlan, Vec<Diagnostic>> {
    let at = &input.candidates_at;
    let mut checked = check_clauses(input)?;
    checked.sort_by(|a, b| {
        let (x, y) = (a.clause.span(), b.clause.span());
        (x.start(), x.doc().as_str().as_bytes(), x.end()).cmp(&(
            y.start(),
            y.doc().as_str().as_bytes(),
            y.end(),
        ))
    });
    let (Some(declarations), Some(vocabulary)) =
        (declarations(input.rulebook), vocabulary(input.option))
    else {
        return Err(cannot_build(at));
    };
    let revise = match input.revise {
        None => Value::null(),
        Some(context) => context.value().ok_or_else(|| cannot_build(at))?,
    };
    let shared = Shared {
        config: input.config.clone(),
        declarations,
        vocabulary,
        revise,
    };
    // Size of the object with no clauses; each clause adds its own bytes and
    // one comma between neighbours.
    let empty = shared
        .request(Vec::new())
        .canonical_bytes()
        .ok_or_else(|| cannot_build(at))?
        .len();
    if empty > MAX_REQUEST_BYTES {
        return Err(vec![invalid(
            at,
            format!("the request has {empty} bytes before any clause, above {MAX_REQUEST_BYTES}"),
        )]);
    }
    let max_clauses = usize::try_from(input.config.max_clauses_per_request).unwrap_or(usize::MAX);
    let mut chunks: Vec<Vec<(Checked<'_>, Value<()>)>> = Vec::new();
    let mut current: Vec<(Checked<'_>, Value<()>)> = Vec::new();
    let mut size = empty;
    for item in checked {
        let value = clause_value(&item).ok_or_else(|| cannot_build(at))?;
        let own = write_compact(&value).len();
        let comma = usize::from(!current.is_empty());
        let grown = size + own + comma;
        if !current.is_empty() && (current.len() >= max_clauses || grown > MAX_REQUEST_BYTES) {
            chunks.push(std::mem::take(&mut current));
            size = empty;
        }
        let comma = usize::from(!current.is_empty());
        size += own + comma;
        if size > MAX_REQUEST_BYTES {
            return Err(vec![invalid(
                at,
                format!(
                    "{} cannot fit a request of {MAX_REQUEST_BYTES} bytes",
                    describe(item.clause)
                ),
            )]);
        }
        current.push((item, value));
    }
    chunks.push(current);
    let requests = chunks
        .iter()
        .map(|c| shared.finish(c))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| cannot_build(at))?;
    Ok(RunPlan { requests })
}
