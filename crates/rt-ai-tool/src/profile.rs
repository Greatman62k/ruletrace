//! Provider_Profiles (UD-37, Req 35.5): documented constants and data.
//!
//! A profile holds no network, environment or credential code and selects
//! no model. Each fact names its documentation URL and the date it was
//! read; `docs/provider-profiles.md` repeats the table for reviewers.

use rt_ai_adapter::{Capabilities, Protocol, Tri};

/// Request path of the Responses protocol, appended to the endpoint.
pub const RESPONSES_PATH: &str = "/v1/responses";

/// Request path of the model catalogue, appended to the endpoint.
pub const MODELS_PATH: &str = "/v1/models";

/// The provisional `max_output_tokens` of a Responses request, passed to
/// `rt-ai-responses` through its settings. Reasoning tokens count toward it.
/// The value is not measured: task 10.8 revises it after the live run. A
/// truncated answer is reported as incomplete, never as a result. The
/// documented minimum is 16.
pub const PROVISIONAL_MAX_OUTPUT_TOKENS: u32 = 8192;

/// The structured-output mode that a provider supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaMode {
    /// JSON Schema with `strict: true`.
    JsonSchemaStrict,
    /// No structured output is declared.
    None,
}

/// A numeric provider limit that is either documented or not known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    /// No source read for this version states the value.
    Unknown,
    /// The documented value.
    Documented(u32),
}

/// One documented fact with its source and the date it was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocFact {
    /// What the page states, in one line.
    pub fact: &'static str,
    /// The public page.
    pub url: &'static str,
    /// The day the page was read (ISO 8601).
    pub read_on: &'static str,
}

/// The documented constants of one provider identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    /// The provider identifier of the configuration.
    pub id: &'static str,
    /// The wire protocol.
    pub protocol: Protocol,
    /// The only non-loopback host that may receive the credential. `None`
    /// for the offline replay, which has no endpoint.
    pub official_host: Option<&'static str>,
    /// The default capabilities. A configuration may restate or lower them.
    pub capabilities: Capabilities,
    /// The structured-output mode.
    pub schema_mode: SchemaMode,
    /// Whether the provider accepts a recursive schema.
    pub recursive_schema: Tri,
    /// Maximum schema nesting depth, when documented.
    pub max_schema_depth: Option<u32>,
    /// Maximum properties in a schema, when documented.
    pub max_schema_properties: Option<u32>,
    /// Whether a request parameter disables provider-side retention.
    pub store_control: Tri,
    /// Whether the provider lists its models.
    pub model_discovery: Tri,
    /// Context window of the configured model.
    pub context_window: Limit,
    /// Output token limit of the configured model.
    pub output_limit: Limit,
    /// The reasoning effort values the provider documents. Empty when the
    /// capability is not established, so no value is accepted.
    pub reasoning_efforts: &'static [&'static str],
    /// The models for which the effort `max` is documented. `None` puts no
    /// per-model limit on any effort; a list refuses `max` for every model
    /// that it does not name.
    pub max_effort_models: Option<&'static [&'static str]>,
    /// The `max_output_tokens` constant (at least 16).
    pub max_output_tokens: u32,
    /// The sources of the facts above.
    pub documentation: &'static [DocFact],
}

const META_DOCS: &[DocFact] = &[
    DocFact {
        fact: "structured-output limits: depth 10, 5,000 properties, no recursion; strict mode",
        url: "https://dev.meta.ai/docs/structured-output",
        read_on: "2026-11-16",
    },
    DocFact {
        fact:
            "reasoning_effort values none, minimal, low, medium, high, xhigh, max; none is refused",
        url: "https://dev.meta.ai/docs/reasoning",
        read_on: "2026-11-16",
    },
    DocFact {
        fact: "Responses: usage has input_tokens, output_tokens, total_tokens; store defaults to true and store false keeps no server copy; max effort on three models; a non-streaming request that runs too long returns HTTP 504",
        url: "https://dev.meta.ai/docs/protocols/responses",
        read_on: "2026-11-16",
    },
    DocFact {
        fact:
            "OpenAI-compatible clients use base https://api.meta.ai/v1 with the Responses protocol",
        url: "https://dev.meta.ai/docs/authentication",
        read_on: "2026-11 (UD-37, search snippet)",
    },
];

const OPENAI_DOCS: &[DocFact] = &[DocFact {
    fact: "structured outputs: up to 5000 object properties and 10 levels of nesting; recursive schemas are supported",
    url: "https://developers.openai.com/api/docs/guides/structured-outputs",
    read_on: "2026-11-16",
}];

const META_EFFORTS: &[&str] = &["minimal", "low", "medium", "high", "xhigh", "max"];

// The reasoning page and the Responses page name these three models for
// `max`. It is not available with `muse-spark-1.3-contributor`.
const META_MAX_EFFORT_MODELS: &[&str] = &["muse-spark-1.3", "muse-ember-1.0", "muse-wonder-1.0"];

/// The Meta Model API over the Responses protocol.
pub const META: Profile = Profile {
    id: "meta",
    protocol: Protocol::OpenAiResponses,
    official_host: Some("api.meta.ai"),
    capabilities: Capabilities {
        structured_output: Tri::Yes,
        reasoning_effort: Tri::Yes,
        usage_reporting: Tri::Unknown,
    },
    schema_mode: SchemaMode::JsonSchemaStrict,
    recursive_schema: Tri::No,
    max_schema_depth: Some(10),
    max_schema_properties: Some(5000),
    store_control: Tri::Yes,
    model_discovery: Tri::Yes,
    context_window: Limit::Unknown,
    output_limit: Limit::Unknown,
    reasoning_efforts: META_EFFORTS,
    max_effort_models: Some(META_MAX_EFFORT_MODELS),
    max_output_tokens: PROVISIONAL_MAX_OUTPUT_TOKENS,
    documentation: META_DOCS,
};

/// The OpenAI Responses API. Reasoning effort and usage reporting are not
/// established for this version, so a run that needs them fails closed.
pub const OPENAI: Profile = Profile {
    id: "openai",
    protocol: Protocol::OpenAiResponses,
    official_host: Some("api.openai.com"),
    capabilities: Capabilities {
        structured_output: Tri::Yes,
        reasoning_effort: Tri::Unknown,
        usage_reporting: Tri::Unknown,
    },
    schema_mode: SchemaMode::JsonSchemaStrict,
    recursive_schema: Tri::Yes,
    max_schema_depth: Some(10),
    max_schema_properties: Some(5000),
    store_control: Tri::Unknown,
    model_discovery: Tri::Unknown,
    context_window: Limit::Unknown,
    output_limit: Limit::Unknown,
    reasoning_efforts: &[],
    max_effort_models: None,
    max_output_tokens: PROVISIONAL_MAX_OUTPUT_TOKENS,
    documentation: OPENAI_DOCS,
};

/// The offline replay of a recorded script. It sends nothing and applies no
/// reasoning effort, matching the capabilities of the replay provider.
pub const MOCK_REPLAY: Profile = Profile {
    id: "mock-replay",
    protocol: Protocol::MockReplay,
    official_host: None,
    capabilities: Capabilities {
        structured_output: Tri::Yes,
        reasoning_effort: Tri::No,
        usage_reporting: Tri::No,
    },
    schema_mode: SchemaMode::JsonSchemaStrict,
    recursive_schema: Tri::No,
    max_schema_depth: None,
    max_schema_properties: None,
    store_control: Tri::No,
    model_discovery: Tri::No,
    context_window: Limit::Unknown,
    output_limit: Limit::Unknown,
    reasoning_efforts: &[],
    max_effort_models: None,
    max_output_tokens: PROVISIONAL_MAX_OUTPUT_TOKENS,
    documentation: &[],
};

/// The three provider identifiers of this version.
pub const PROFILES: [&Profile; 3] = [&META, &OPENAI, &MOCK_REPLAY];

/// The profile for a provider identifier, compared exactly.
#[must_use]
pub fn profile_for(id: &str) -> Option<&'static Profile> {
    PROFILES.into_iter().find(|p| p.id == id)
}
