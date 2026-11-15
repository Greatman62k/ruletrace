//! The AI_Configuration and its checks in the order of Req 28.17.

use crate::consent::Consent;
use crate::credential::{read_credential, Credential, EnvReader};
use crate::endpoint;
use crate::locate::Locator;
use crate::raw::RawConfig;
use crate::reader::{Found, Reader};
use crate::scan;
use rt_ai_adapter::{RequestLimits, RetryPolicy, RetryPolicyError};
use rt_foundation::{Code, Diagnostic, DocumentId, InputId, Location};
use std::collections::BTreeSet;
use toml_edit::Document;

const DEFAULT_MAX_CLAUSES: u32 = 20;
const DEFAULT_DEADLINE_S: u32 = 60;
const DEFAULT_ATTEMPT_S: u32 = 20;
const DEFAULT_RETRIES: u8 = 2;
const DEFAULT_RESPONSE_BYTES: u32 = 1_048_576;
const MAX_CLAUSES_RANGE: std::ops::RangeInclusive<i64> = 1..=100;
const RESPONSE_BYTES_RANGE: std::ops::RangeInclusive<i64> = 1_024..=16_777_216;

/// The provider kinds built into the tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    /// OpenAI Responses API (`openai-responses`).
    OpenAiResponses,
    /// Offline replay of a recorded script (`mock-replay`).
    MockReplay,
}

impl ProviderKind {
    /// The kind for a configuration value, or `None` when no built-in
    /// provider implements it.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "openai-responses" => Some(Self::OpenAiResponses),
            "mock-replay" => Some(Self::MockReplay),
            _ => None,
        }
    }

    /// The configuration value of the kind.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai-responses",
            Self::MockReplay => "mock-replay",
        }
    }
}

/// A validated endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// The URL as written.
    pub url: String,
    /// The lower-case host without brackets or port, for the log.
    pub host: String,
}

/// A configuration that passed every check up to and including the provider
/// kind. The enabled state is `true`, so a value of this type is never a
/// disabled configuration.
#[derive(Clone, Debug)]
pub struct AiConfig {
    /// Built-in provider kind.
    pub provider: ProviderKind,
    /// Model identifier.
    pub model: String,
    /// Endpoint of a real provider, absent for `mock-replay`.
    pub endpoint: Option<Endpoint>,
    /// Name of the environment variable that holds the credential.
    pub credential_env_var: Option<String>,
    /// Recorded script path of `mock-replay`.
    pub script: Option<String>,
    /// Whether Source_Document text may be transmitted.
    pub allow_document_text: bool,
    /// AI identities that review refuses as Reviewer.
    pub ai_identities: Vec<String>,
    /// Path of the Transmission_Log.
    pub transmission_log: String,
    /// Maximum Candidate_Clauses per Provider_Request.
    pub max_clauses_per_request: u32,
    /// Deadline, attempt, retry and response limits.
    pub limits: RequestLimits,
    /// The validated retry policy for `limits`.
    pub policy: RetryPolicy,
    /// Location of the `transmission_log` field.
    pub transmission_log_at: Location,
    credential_at: Option<Location>,
}

/// A configuration that passed every check, with the credential when the
/// provider needs one.
#[derive(Debug)]
pub struct Authorized {
    /// The validated configuration.
    pub config: AiConfig,
    /// Present exactly when the provider kind needs a credential.
    pub credential: Option<Credential>,
}

/// The Diagnostic for a run with no configuration argument (Req 28.1).
#[must_use]
pub fn disabled_without_configuration(at: Location) -> Diagnostic {
    Diagnostic::new(
        Code::AiDisabled,
        "no AI configuration file was named, so AI is disabled".to_owned(),
        at,
    )
}

fn sorted(mut found: Vec<Diagnostic>) -> Vec<Diagnostic> {
    found.sort();
    found
}

fn range_error(field: &str, at: Location, what: &str) -> Diagnostic {
    Diagnostic::new(
        Code::AiConfigLimitInvalid,
        format!("field `{field}` is {what}"),
        at,
    )
}

struct Limits {
    max_clauses: u32,
    request: RequestLimits,
    policy: RetryPolicy,
}

// A value that does not fit the target type is outside every accepted range.
fn narrow<T: TryFrom<i64>>(
    field: &str,
    value: Option<&Found<i64>>,
    default: T,
    out: &mut Vec<Diagnostic>,
) -> Option<T> {
    let Some(found) = value else {
        return Some(default);
    };
    let narrowed = T::try_from(found.value).ok();
    if narrowed.is_none() {
        out.push(range_error(
            field,
            found.at.clone(),
            "outside its accepted range",
        ));
    }
    narrowed
}

fn check_range(
    field: &str,
    value: Option<&Found<i64>>,
    range: &std::ops::RangeInclusive<i64>,
    what: &str,
    out: &mut Vec<Diagnostic>,
) {
    if let Some(found) = value.filter(|f| !range.contains(&f.value)) {
        out.push(range_error(field, found.at.clone(), what));
    }
}

fn policy_error(raw: &RawConfig, error: RetryPolicyError) -> Diagnostic {
    let (field, found) = match error {
        RetryPolicyError::Deadline => ("total_deadline_secs", raw.total_deadline.as_ref()),
        RetryPolicyError::AttemptTimeout => (
            "attempt_timeout_secs",
            raw.attempt_timeout.as_ref().or(raw.total_deadline.as_ref()),
        ),
        RetryPolicyError::Retries => ("max_retries", raw.max_retries.as_ref()),
    };
    let at = found.map_or_else(
        || Location::None(Code::AiConfigLimitInvalid),
        |f| f.at.clone(),
    );
    range_error(field, at, "outside its accepted range")
}

fn check_limits(raw: &RawConfig) -> Result<Limits, Vec<Diagnostic>> {
    let mut out = Vec::new();
    let deadline = narrow(
        "total_deadline_secs",
        raw.total_deadline.as_ref(),
        DEFAULT_DEADLINE_S,
        &mut out,
    );
    let attempt = narrow(
        "attempt_timeout_secs",
        raw.attempt_timeout.as_ref(),
        DEFAULT_ATTEMPT_S,
        &mut out,
    );
    let retries = narrow(
        "max_retries",
        raw.max_retries.as_ref(),
        DEFAULT_RETRIES,
        &mut out,
    );
    check_range(
        "max_clauses_per_request",
        raw.max_clauses.as_ref(),
        &MAX_CLAUSES_RANGE,
        "outside 1 to 100",
        &mut out,
    );
    check_range(
        "max_response_bytes",
        raw.max_response_bytes.as_ref(),
        &RESPONSE_BYTES_RANGE,
        "outside 1024 to 16777216",
        &mut out,
    );
    let (Some(deadline_s), Some(attempt_s), Some(retries)) = (deadline, attempt, retries) else {
        return Err(out);
    };
    let policy = match RetryPolicy::new(deadline_s, attempt_s, retries) {
        Ok(policy) => Some(policy),
        Err(e) => {
            out.push(policy_error(raw, e));
            None
        }
    };
    let (Some(policy), true) = (policy, out.is_empty()) else {
        return Err(out);
    };
    let from = |f: &Option<Found<i64>>, default: u32| {
        f.as_ref()
            .and_then(|f| u32::try_from(f.value).ok())
            .unwrap_or(default)
    };
    Ok(Limits {
        max_clauses: from(&raw.max_clauses, DEFAULT_MAX_CLAUSES),
        request: RequestLimits {
            deadline_s,
            attempt_s,
            retries: u32::from(retries),
            response_bytes: from(&raw.max_response_bytes, DEFAULT_RESPONSE_BYTES),
        },
        policy,
    })
}

fn check_endpoint(
    raw: &RawConfig,
    reader: &Reader<'_>,
) -> Result<Option<Endpoint>, Vec<Diagnostic>> {
    let Some(found) = &raw.endpoint else {
        return Ok(None);
    };
    let Some(parsed) = endpoint::parse(&found.value) else {
        let d = reader.invalid(
            found.at.clone(),
            "field `endpoint` is not an absolute URL".to_owned(),
        );
        return Err(vec![d]);
    };
    if !endpoint::transport_permitted(&parsed) {
        let d = Diagnostic::new(
            Code::AiConfigEndpointInsecure,
            "field `endpoint` must use https unless the host is a loopback address".to_owned(),
            found.at.clone(),
        );
        return Err(vec![d]);
    }
    Ok(Some(Endpoint {
        url: found.value.clone(),
        host: parsed.host,
    }))
}

/// Loads a configuration without authorizing a send: every check of
/// [`load`] up to and including the provider kind, and neither the consent
/// nor the credential. `propose --dry-run` uses it, because nothing is sent
/// and no credential may be read.
///
/// # Errors
/// The Diagnostics of the earliest failing check, in canonical order.
pub fn load_config(input: &InputId, text: &str) -> Result<AiConfig, Vec<Diagnostic>> {
    let locator = Locator::new(input, text);
    let document = Document::parse(text).map_err(|e| {
        let at = locator.at(e.span(), None);
        vec![Diagnostic::new(
            Code::AiConfigInvalid,
            format!("invalid TOML: {}", e.message()),
            at,
        )]
    })?;
    let reader = Reader {
        table: document.as_table(),
        locator: &locator,
    };
    let secrets = scan::scan(&reader);
    if !secrets.is_empty() {
        return Err(sorted(secrets));
    }
    let raw = RawConfig::read(&reader).map_err(sorted)?;
    let file = locator.at(None, None);
    let limits = check_limits(&raw).map_err(sorted)?;
    let endpoint = check_endpoint(&raw, &reader).map_err(sorted)?;
    if !raw.is_enabled() {
        let at = raw.enabled.as_ref().map_or(file, |e| e.at.clone());
        return Err(vec![Diagnostic::new(
            Code::AiDisabled,
            "AI is disabled: the field `enabled` is not true".to_owned(),
            at,
        )]);
    }
    build(&raw, limits, endpoint)
}

/// Loads and authorizes a configuration for one run.
///
/// The checks run in the order of Req 28.17 and the result carries only the
/// Diagnostics of the earliest failing check: TOML syntax, inline secret,
/// field checks, limits, endpoint, enabled state, provider kind, consent,
/// credential. The environment is read last, and only for a provider kind
/// that needs a credential, and only the variable the configuration names.
///
/// `run_documents` is the set of documents whose text the run sends.
///
/// # Errors
/// The Diagnostics of the earliest failing check, in canonical order.
pub fn load(
    input: &InputId,
    text: &str,
    consent: &Consent,
    run_documents: &BTreeSet<DocumentId>,
    env: &dyn EnvReader,
) -> Result<Authorized, Vec<Diagnostic>> {
    let config = load_config(input, text)?;
    if let Some(d) = consent.check(config.allow_document_text, run_documents) {
        return Err(vec![d]);
    }
    let credential = match config.provider {
        ProviderKind::MockReplay => None,
        ProviderKind::OpenAiResponses => Some(read_credential(&config, env).map_err(|d| vec![d])?),
    };
    Ok(Authorized { config, credential })
}

fn build(
    raw: &RawConfig,
    limits: Limits,
    endpoint: Option<Endpoint>,
) -> Result<AiConfig, Vec<Diagnostic>> {
    let (Some(provider), Some(model), Some(allow), Some(identities), Some(log)) = (
        &raw.provider,
        &raw.model,
        &raw.allow_document_text,
        &raw.ai_identities,
        &raw.transmission_log,
    ) else {
        return Err(vec![Diagnostic::new(
            Code::InternalError,
            "a required field is missing after the field checks".to_owned(),
            Location::None(Code::InternalError),
        )]);
    };
    let Some(kind) = ProviderKind::from_name(&provider.value) else {
        let d = Diagnostic::new(
            Code::AiProviderUnknown,
            "no built-in provider implements the provider kind".to_owned(),
            provider.at.clone(),
        );
        return Err(vec![d]);
    };
    Ok(AiConfig {
        provider: kind,
        model: model.value.clone(),
        endpoint,
        credential_env_var: raw.credential_env_var.as_ref().map(|f| f.value.clone()),
        script: raw.script.as_ref().map(|f| f.value.clone()),
        allow_document_text: allow.value,
        ai_identities: identities.value.clone(),
        transmission_log: log.value.clone(),
        max_clauses_per_request: limits.max_clauses,
        limits: limits.request,
        policy: limits.policy,
        transmission_log_at: log.at.clone(),
        credential_at: raw.credential_env_var.as_ref().map(|f| f.at.clone()),
    })
}

impl AiConfig {
    pub(crate) fn credential_location(&self) -> Option<&Location> {
        self.credential_at.as_ref()
    }
}
