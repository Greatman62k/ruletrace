//! The Responses adapter: request, attempt and response mapping.

use crate::body::{self, BodyParts, MIN_MAX_OUTPUT_TOKENS};
use crate::clock::SystemClock;
use crate::credential::Credential;
use crate::endpoint::{Endpoint, EndpointError};
use crate::envelope::{self, Verdict};
use crate::transport::{self, Call};
use rt_ai_adapter::{
    AdapterInfo, AiAdapter, AttemptOutcome, AttemptReport, Capabilities, Completion, Identity,
    MonotonicClock, Protocol, ProviderFailure, ProviderId, ProviderMeta, ProviderRequest,
    ProviderResponse, StructuredOutput, Tri, Usage,
};
use rt_foundation::InputId;
use rt_json::{parse, write_compact, ParseLimits};
use std::time::{Duration, Instant};

/// Request path of the Responses protocol, appended to the endpoint.
const RESPONSES_PATH: &str = "/v1/responses";

/// Largest schema that the adapter accepts, in bytes.
const MAX_SCHEMA_BYTES: u64 = 1 << 20;

/// Deepest schema nesting that the adapter parses.
const MAX_SCHEMA_DEPTH: u32 = 64;

/// The adapter version that this crate reports.
const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Everything the adapter needs besides the credential.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The provider identifier of the configuration, reported in the
    /// identity.
    pub provider: ProviderId,
    /// The endpoint text of the configuration.
    pub endpoint: String,
    /// The schema name and the canonical schema bytes of the caller.
    pub structured_output: StructuredOutput,
    /// The `max_output_tokens` constant of the Provider_Profile, at least
    /// [`MIN_MAX_OUTPUT_TOKENS`]. Reasoning tokens count toward it.
    pub max_output_tokens: u32,
}

/// Why [`ResponsesAdapter::new`] refused its settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsError {
    /// The endpoint is not acceptable.
    Endpoint(EndpointError),
    /// The schema name is not 1 to 64 of letters, digits, `_` and `-`.
    SchemaName,
    /// The schema bytes are not one JSON object within the limits.
    Schema,
    /// `max_output_tokens` is below [`MIN_MAX_OUTPUT_TOKENS`].
    MaxOutputTokens,
}

/// One provider that speaks the Responses protocol.
pub struct ResponsesAdapter {
    identity: Identity,
    endpoint: Endpoint,
    schema_name: String,
    schema_compact: String,
    max_output_tokens: u32,
    credential: Credential,
    clock: Box<dyn MonotonicClock>,
}

impl ResponsesAdapter {
    /// An adapter for `settings`, holding `credential`.
    ///
    /// # Errors
    /// The first setting that is not acceptable.
    pub fn new(settings: Settings, credential: Credential) -> Result<Self, SettingsError> {
        let endpoint = Endpoint::parse(&settings.endpoint).map_err(SettingsError::Endpoint)?;
        let name = &settings.structured_output.name;
        let name_ok = (1..=64).contains(&name.len())
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if !name_ok {
            return Err(SettingsError::SchemaName);
        }
        if settings.max_output_tokens < MIN_MAX_OUTPUT_TOKENS {
            return Err(SettingsError::MaxOutputTokens);
        }
        let schema_compact = compact_schema(&settings.structured_output.schema)?;
        Ok(Self {
            identity: Identity::new(
                settings.provider,
                Protocol::OpenAiResponses,
                ADAPTER_VERSION,
            ),
            endpoint,
            schema_name: settings.structured_output.name,
            schema_compact,
            max_output_tokens: settings.max_output_tokens,
            credential,
            clock: Box::new(SystemClock::new()),
        })
    }

    /// Replaces the monotonic clock, so that a test drives the retry
    /// schedule with simulated time and sleeps for no real duration.
    #[must_use]
    pub fn with_clock(mut self, clock: Box<dyn MonotonicClock>) -> Self {
        self.clock = clock;
        self
    }

    fn meta(&self, request: &ProviderRequest) -> ProviderMeta {
        ProviderMeta::new(self.identity.clone(), &request.model)
    }

    fn run(
        &self,
        request: &ProviderRequest,
        timeout: Duration,
    ) -> (AttemptOutcome, Option<String>) {
        let fail = |failure: ProviderFailure, why: &str| {
            (AttemptOutcome::Failure(failure), Some(why.to_owned()))
        };
        let other = ProviderFailure::Other { http_status: None };
        let Some(canonical) = request.canonical_bytes() else {
            return fail(other, "the request has no canonical bytes");
        };
        let Ok(canonical) = String::from_utf8(canonical) else {
            return fail(other, "the canonical request is not UTF-8");
        };
        let body = body::build(&BodyParts {
            model: &request.model,
            canonical_request: &canonical,
            schema_name: &self.schema_name,
            schema_compact: &self.schema_compact,
            max_output_tokens: self.max_output_tokens,
            reasoning_effort: request.reasoning_effort.as_deref(),
        });
        let limit = u64::from(request.limits.response_bytes);
        let authorization = format!("Bearer {}", self.credential.expose());
        let url = self.endpoint.url(RESPONSES_PATH);
        let reply = transport::post(&Call {
            url: &url,
            clear_text: self.endpoint.is_clear_text(),
            authorization: &authorization,
            body: &body,
            timeout,
            read_limit: limit.saturating_add(1),
        });
        match reply {
            Err(failure) => (AttemptOutcome::Failure(failure), None),
            Ok(reply) if u64::try_from(reply.body.len()).unwrap_or(u64::MAX) > limit => (
                // The caller measures the payload against the limit and
                // reports RT-AI-0203.
                AttemptOutcome::Response(ProviderResponse {
                    completion: Completion::Complete,
                    payload: reply.body,
                    meta: self.meta(request),
                }),
                None,
            ),
            Ok(reply) => self.map_envelope(request, &reply.body),
        }
    }

    fn map_envelope(
        &self,
        request: &ProviderRequest,
        body: &[u8],
    ) -> (AttemptOutcome, Option<String>) {
        let mapped = envelope::map(body);
        let mut meta = self.meta(request);
        if let Some(model) = &mapped.reported_model {
            meta = meta.with_reported_model(model);
        }
        if let Some(id) = &mapped.response_id {
            meta = meta.with_response_id(id);
        }
        if let Some(counts) = mapped.usage {
            meta = meta.with_usage(Usage {
                input_tokens: counts.input,
                output_tokens: counts.output,
                total_tokens: counts.total,
            });
        }
        let (completion, payload, detail) = match mapped.verdict {
            Verdict::Text(text) => (Completion::Complete, text.into_bytes(), None),
            Verdict::Refused => (Completion::Refused, Vec::new(), None),
            Verdict::Incomplete => (Completion::Incomplete, Vec::new(), None),
            // No typed failure exists for a malformed envelope. An empty
            // payload is not JSON, so the caller reports RT-AI-0201.
            Verdict::Malformed(why) => (Completion::Complete, Vec::new(), Some(why.to_owned())),
        };
        (
            AttemptOutcome::Response(ProviderResponse {
                completion,
                payload,
                meta,
            }),
            detail,
        )
    }
}

impl std::fmt::Debug for ResponsesAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResponsesAdapter")
            .field("identity", &self.identity)
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

fn compact_schema(bytes: &[u8]) -> Result<String, SettingsError> {
    let id = InputId::new("schema").map_err(|_| SettingsError::Schema)?;
    let limits = ParseLimits::new(MAX_SCHEMA_BYTES, MAX_SCHEMA_DEPTH);
    let value = parse(&id, bytes, limits).map_err(|_| SettingsError::Schema)?;
    value.as_object().ok_or(SettingsError::Schema)?;
    Ok(write_compact(&value))
}

impl AdapterInfo for ResponsesAdapter {
    fn identity(&self) -> Identity {
        self.identity.clone()
    }

    fn capabilities(&self) -> Capabilities {
        // The protocol carries a strict schema and a reasoning effort. Which
        // provider honours them is the profile's statement, and usage
        // reporting is not established for any provider yet.
        Capabilities {
            structured_output: Tri::Yes,
            reasoning_effort: Tri::Yes,
            usage_reporting: Tri::Unknown,
        }
    }
}

impl AiAdapter for ResponsesAdapter {
    fn attempt(&mut self, request: &ProviderRequest, timeout: Duration) -> AttemptReport {
        let started = Instant::now();
        let (outcome, detail) = self.run(request, timeout);
        AttemptReport {
            outcome,
            elapsed: started.elapsed(),
            detail,
        }
    }

    fn clock(&self) -> &dyn MonotonicClock {
        self.clock.as_ref()
    }
}
