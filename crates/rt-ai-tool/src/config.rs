//! The AI_Configuration and its checks in the order of Req 28.17.

use crate::consent::Consent;
use crate::credential::{read_credential, Credential, EnvReader};
use crate::endpoint;
use crate::locate::Locator;
use crate::profile::Profile;
use crate::raw::{parse_tri, RawConfig};
use crate::reader::{Found, Reader};
use crate::scan;
use rt_ai_adapter::{
    negotiate, Capabilities, RequestLimits, Required, RetryPolicy, RetryPolicyError, Tri,
};
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

    /// The provider identifier that a configuration of schema version 1
    /// maps to, in memory only (Req 35.9).
    #[must_use]
    pub fn provider_id(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai",
            Self::MockReplay => "mock-replay",
        }
    }

    /// The protocol that a configuration of schema version 1 maps to.
    #[must_use]
    pub fn protocol(self) -> &'static str {
        self.name()
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
    /// Built-in provider kind, which follows from the protocol.
    pub provider: ProviderKind,
    /// The documented profile of the provider identifier.
    pub profile: &'static Profile,
    /// Reasoning effort, when configured. Never defaulted.
    pub reasoning_effort: Option<String>,
    /// Whether the model catalogue is queried before the first request.
    pub verify_model: bool,
    /// Whether a model identifier with `contributor` is allowed.
    pub allow_provider_training: bool,
    /// The profile capabilities after the configuration's lowering.
    pub capabilities: Capabilities,
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
    /// Location of the provider identifier field.
    pub provider_at: Location,
    /// Location of the model field.
    pub model_at: Location,
    credential_at: Option<Location>,
    effort_at: Option<Location>,
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

// The field name of an Input_Location, for messages.
fn field_path(at: &Location) -> String {
    match at {
        Location::Input(i) => i.field.clone().unwrap_or_default(),
        _ => String::new(),
    }
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
            format!("field `{}` is not an absolute URL", field_path(&found.at)),
        );
        return Err(vec![d]);
    };
    if !endpoint::transport_permitted(&parsed) {
        let d = Diagnostic::new(
            Code::AiConfigEndpointInsecure,
            format!(
                "field `{}` must use https unless the host is a loopback address",
                field_path(&found.at)
            ),
            found.at.clone(),
        );
        return Err(vec![d]);
    }
    if let Some(official) = raw.profile().and_then(|p| p.official_host) {
        if !endpoint::host_permitted(&parsed, official) {
            let d = Diagnostic::new(
                Code::AiConfigEndpointHostNotPermitted,
                format!(
                    "field `{}` must name the official host of the provider or a loopback address",
                    field_path(&found.at)
                ),
                found.at.clone(),
            );
            return Err(vec![d]);
        }
    }
    Ok(Some(Endpoint {
        url: found.value.clone(),
        host: parsed.host,
    }))
}

/// Loads a configuration without authorizing a send: every check of
/// [`load`] up to and including the model guard, and neither the capability
/// negotiation (see [`AiConfig::check_capabilities`]) nor the consent nor
/// the credential. `propose --dry-run` uses it, because nothing is sent
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
        prefix: "",
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
/// The checks run in the order of Req 28.17 as amended by Req 35 and the
/// result carries only the Diagnostics of the earliest failing check: TOML
/// syntax, inline secret, field checks, limits, endpoint scheme, endpoint
/// host, enabled state, provider identifier and protocol, model guard,
/// capability negotiation, consent, credential. The environment is read
/// last, and only for a provider kind that needs a credential, and only the
/// variable the configuration names.
///
/// `run_documents` is the set of documents whose text the run sends.
///
pub fn load(
    input: &InputId,
    text: &str,
    consent: &Consent,
    run_documents: &BTreeSet<DocumentId>,
    env: &dyn EnvReader,
) -> Result<Authorized, Vec<Diagnostic>> {
    let config = load_config(input, text)?;
    config.check_capabilities()?;
    if let Some(d) = consent.check(config.allow_document_text, run_documents) {
        return Err(vec![d]);
    }
    let credential = match config.provider {
        ProviderKind::MockReplay => None,
        ProviderKind::OpenAiResponses => Some(read_credential(&config, env).map_err(|d| vec![d])?),
    };
    Ok(Authorized { config, credential })
}

/// Reads the credential for `ruletrace-ai models` after the configuration
/// checks of Req 28.17 and the provider link check.
///
/// Capability negotiation and consent are skipped because they concern a
/// Provider_Request (Req 35.18). The environment is read only here.
///
/// # Errors
/// `RT-AI-0007` when a hosted provider's credential is unset or empty.
pub(crate) fn authorize_for_catalogue(
    config: AiConfig,
    env: &dyn EnvReader,
) -> Result<Authorized, Vec<Diagnostic>> {
    let credential = match config.provider {
        ProviderKind::MockReplay => None,
        ProviderKind::OpenAiResponses => Some(read_credential(&config, env).map_err(|d| vec![d])?),
    };
    Ok(Authorized { config, credential })
}

fn unknown_provider(at: Location, what: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::new(
        Code::AiProviderUnknown,
        format!("no built-in provider implements the {what}"),
        at,
    )]
}

// Req 28.12, 35.10: the identifier is one of the profiles and the protocol
// is the one that profile names.
fn check_pair(raw: &RawConfig) -> Result<(&'static Profile, ProviderKind), Vec<Diagnostic>> {
    let (Some(id), Some(protocol)) = (&raw.provider_id, &raw.protocol) else {
        return Err(missing_field());
    };
    let Some(profile) = raw.profile() else {
        return Err(unknown_provider(id.at.clone(), "provider identifier"));
    };
    let kind = ProviderKind::from_name(&protocol.value)
        .filter(|k| k.protocol() == profile.protocol.as_str());
    match kind {
        Some(kind) => Ok((profile, kind)),
        None => Err(unknown_provider(
            protocol.at.clone(),
            "protocol of the provider identifier",
        )),
    }
}

fn missing_field() -> Vec<Diagnostic> {
    vec![Diagnostic::new(
        Code::InternalError,
        "a required field is missing after the field checks".to_owned(),
        Location::None(Code::InternalError),
    )]
}

// Req 35.8: a model identifier that contains `contributor` in any letter
// case is refused unless training is allowed explicitly.
fn check_model_guard(model: &Found<String>, allowed: bool) -> Result<(), Vec<Diagnostic>> {
    if allowed || !model.value.to_lowercase().contains("contributor") {
        return Ok(());
    }
    Err(vec![Diagnostic::new(
        Code::AiModelTrainingModeRefused,
        format!(
            "field `{}` names a contributor model and `allow_provider_training` is not true",
            field_path(&model.at)
        ),
        model.at.clone(),
    )])
}

// The profile capabilities with each declaration applied. A declaration can
// only restate or lower a value, which the field checks already enforced.
fn effective_capabilities(raw: &RawConfig, profile: &Profile) -> Capabilities {
    let mut caps = profile.capabilities;
    for (name, found) in &raw.capabilities {
        let Some(value) = parse_tri(&found.value) else {
            continue;
        };
        match *name {
            "reasoning_effort" => caps.reasoning_effort = value,
            "structured_output" => caps.structured_output = value,
            _ => caps.usage_reporting = value,
        }
    }
    caps
}

fn build(
    raw: &RawConfig,
    limits: Limits,
    endpoint: Option<Endpoint>,
) -> Result<AiConfig, Vec<Diagnostic>> {
    let (Some(model), Some(allow), Some(identities), Some(log)) = (
        &raw.model,
        &raw.allow_document_text,
        &raw.ai_identities,
        &raw.transmission_log,
    ) else {
        return Err(missing_field());
    };
    let (profile, kind) = check_pair(raw)?;
    let training = raw
        .allow_provider_training
        .as_ref()
        .is_some_and(|f| f.value);
    check_model_guard(model, training)?;
    let provider_at = raw
        .provider_id
        .as_ref()
        .map_or_else(|| Location::None(Code::InternalError), |f| f.at.clone());
    Ok(AiConfig {
        provider: kind,
        profile,
        reasoning_effort: raw.reasoning_effort.as_ref().map(|f| f.value.clone()),
        verify_model: raw.verify_model.as_ref().is_some_and(|f| f.value),
        allow_provider_training: training,
        capabilities: effective_capabilities(raw, profile),
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
        provider_at,
        model_at: model.at.clone(),
        credential_at: raw.credential_env_var.as_ref().map(|f| f.at.clone()),
        effort_at: raw.reasoning_effort.as_ref().map(|f| f.at.clone()),
    })
}

fn tri_word(value: Tri) -> &'static str {
    match value {
        Tri::Yes => "yes",
        Tri::No => "no",
        Tri::Unknown => "unknown",
    }
}

impl AiConfig {
    pub(crate) fn credential_location(&self) -> Option<&Location> {
        self.credential_at.as_ref()
    }

    /// The capability check of Req 35.3 for a propose run: structured output
    /// is always required and a reasoning effort control only when an effort
    /// is configured. An unknown value fails closed. It reads no environment
    /// variable and makes no call.
    ///
    /// # Errors
    /// One `RT-AI-0011` per unmet capability, in ascending capability-name
    /// order, at the field that caused the requirement.
    pub fn check_capabilities(&self) -> Result<(), Vec<Diagnostic>> {
        let required = Required {
            structured_output: true,
            reasoning_effort: self.reasoning_effort.is_some(),
        };
        let Err(unmet) = negotiate(&required, &self.capabilities) else {
            return Ok(());
        };
        let found = unmet
            .iter()
            .map(|u| {
                let at = match (u.capability, &self.effort_at) {
                    ("reasoning_effort", Some(at)) => at.clone(),
                    _ => self.provider_at.clone(),
                };
                Diagnostic::new(
                    Code::AiCapabilityUnsupported,
                    format!(
                        "provider '{}' has capability {} = {}, which this run requires",
                        self.profile.id,
                        u.capability,
                        tri_word(u.value)
                    ),
                    at,
                )
            })
            .collect();
        Err(found)
    }
}
