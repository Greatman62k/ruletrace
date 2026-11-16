//! Field-level checks of Req 28.3 and 35.9: unknown fields, wrong types and
//! missing required fields, for schema version 1 (flat) and version 2
//! (`[provider]`, `[limits]`, `[capabilities]`). Values are kept as read;
//! ranges and provider rules are checked later.
//!
//! Both versions fill the same [`RawConfig`]. A version 1 `provider` kind is
//! mapped to an identifier and a protocol here, in memory only.

use crate::profile::{profile_for, Profile};
use crate::reader::{Found, Reader};
use crate::scan::finding;
use crate::secret;
use crate::ProviderKind;
use rt_ai_adapter::Tri;
use rt_foundation::Diagnostic;

const V1_FIELDS: [&str; 15] = [
    "schema_version",
    "enabled",
    "provider",
    "model",
    "endpoint",
    "credential_env_var",
    "script",
    "allow_document_text",
    "ai_identities",
    "transmission_log",
    "max_clauses_per_request",
    "total_deadline_secs",
    "attempt_timeout_secs",
    "max_retries",
    "max_response_bytes",
];

const V2_FIELDS: [&str; 10] = [
    "schema_version",
    "enabled",
    "provider",
    "limits",
    "capabilities",
    "script",
    "allow_document_text",
    "ai_identities",
    "transmission_log",
    "max_clauses_per_request",
];

const PROVIDER_FIELDS: [&str; 8] = [
    "id",
    "protocol",
    "model",
    "endpoint",
    "credential_env_var",
    "reasoning_effort",
    "verify_model",
    "allow_provider_training",
];

const LIMIT_FIELDS: [&str; 4] = [
    "total_deadline_secs",
    "attempt_timeout_secs",
    "max_retries",
    "max_response_bytes",
];

/// The capability names that a configuration may declare.
pub(crate) const CAPABILITY_NAMES: [&str; 3] =
    ["reasoning_effort", "structured_output", "usage_reporting"];

/// The accepted schema versions.
const SCHEMA_VERSIONS: [i64; 2] = [1, 2];

/// The longest model identifier, in Unicode scalar values (Req 35.10).
const MODEL_MAX_SCALARS: usize = 200;

const PROVIDER_PREFIX: &str = "provider.";
const LIMITS_PREFIX: &str = "limits.";
const CAPABILITIES_PREFIX: &str = "capabilities.";

/// Fields as written in the file.
#[derive(Debug, Default)]
pub(crate) struct RawConfig {
    pub(crate) schema_version: Option<Found<i64>>,
    pub(crate) enabled: Option<Found<bool>>,
    /// The provider identifier. For version 1 it is mapped from the kind.
    pub(crate) provider_id: Option<Found<String>>,
    /// The protocol. For version 1 it is mapped from the kind.
    pub(crate) protocol: Option<Found<String>>,
    pub(crate) model: Option<Found<String>>,
    pub(crate) endpoint: Option<Found<String>>,
    pub(crate) credential_env_var: Option<Found<String>>,
    pub(crate) reasoning_effort: Option<Found<String>>,
    pub(crate) verify_model: Option<Found<bool>>,
    pub(crate) allow_provider_training: Option<Found<bool>>,
    /// Capability declarations as written: name and value text.
    pub(crate) capabilities: Vec<(&'static str, Found<String>)>,
    pub(crate) script: Option<Found<String>>,
    pub(crate) allow_document_text: Option<Found<bool>>,
    pub(crate) ai_identities: Option<Found<Vec<String>>>,
    pub(crate) transmission_log: Option<Found<String>>,
    pub(crate) max_clauses: Option<Found<i64>>,
    pub(crate) total_deadline: Option<Found<i64>>,
    pub(crate) attempt_timeout: Option<Found<i64>>,
    pub(crate) max_retries: Option<Found<i64>>,
    pub(crate) max_response_bytes: Option<Found<i64>>,
    v2: bool,
    // True for a version 1 kind that no built-in provider implements.
    unmapped_kind: bool,
    has_provider_table: bool,
}

fn take<T>(
    slot: &mut Option<Found<T>>,
    read: Result<Option<Found<T>>, Diagnostic>,
    out: &mut Vec<Diagnostic>,
) {
    match read {
        Ok(found) => *slot = found,
        Err(d) => out.push(d),
    }
}

// Reports every key of `r` that is not in `known`. A credential-like name
// is an inline secret and is never echoed.
fn unknown_fields(r: &Reader<'_>, known: &[&str], out: &mut Vec<Diagnostic>) {
    for (name, _) in r.table.iter() {
        if known.contains(&name) {
            continue;
        }
        let Some((key, _)) = r.table.get_key_value(name) else {
            continue;
        };
        if secret::field_name_looks_secret(name) {
            let at = r.locator.at(key.span(), None);
            out.push(finding(
                at,
                "a field name looks like it carries a credential",
            ));
            continue;
        }
        let at = r.key_location(key, name);
        out.push(r.invalid(at, format!("unknown field `{}`", r.path(name))));
    }
}

fn subreader<'a>(
    r: &Reader<'a>,
    name: &str,
    prefix: &'a str,
    out: &mut Vec<Diagnostic>,
) -> Option<Reader<'a>> {
    match r.subtable(name, prefix) {
        Ok(found) => found.map(|(_, reader)| reader),
        Err(d) => {
            out.push(d);
            None
        }
    }
}

impl RawConfig {
    /// Reads every field and returns every field-level finding.
    pub(crate) fn read(r: &Reader<'_>) -> Result<Self, Vec<Diagnostic>> {
        let mut out = Vec::new();
        let mut raw = Self::default();
        take(
            &mut raw.schema_version,
            r.integer("schema_version"),
            &mut out,
        );
        raw.v2 = raw.schema_version.as_ref().is_some_and(|v| v.value == 2);
        let known: &[&str] = if raw.v2 { &V2_FIELDS } else { &V1_FIELDS };
        unknown_fields(r, known, &mut out);
        take(&mut raw.enabled, r.boolean("enabled"), &mut out);
        take(&mut raw.script, r.string("script"), &mut out);
        take(
            &mut raw.allow_document_text,
            r.boolean("allow_document_text"),
            &mut out,
        );
        take(
            &mut raw.ai_identities,
            r.string_list("ai_identities"),
            &mut out,
        );
        take(
            &mut raw.transmission_log,
            r.string("transmission_log"),
            &mut out,
        );
        take(
            &mut raw.max_clauses,
            r.integer("max_clauses_per_request"),
            &mut out,
        );
        let provider_reader = if raw.v2 {
            raw.read_v2_tables(r, &mut out)
        } else {
            raw.read_v1_fields(r, &mut out);
            None
        };
        raw.check_values(r, &mut out);
        if raw.is_enabled() {
            raw.check_required(r, provider_reader.as_ref(), &mut out);
        }
        if out.is_empty() {
            Ok(raw)
        } else {
            Err(out)
        }
    }

    fn read_limits(&mut self, r: &Reader<'_>, out: &mut Vec<Diagnostic>) {
        take(
            &mut self.total_deadline,
            r.integer("total_deadline_secs"),
            out,
        );
        take(
            &mut self.attempt_timeout,
            r.integer("attempt_timeout_secs"),
            out,
        );
        take(&mut self.max_retries, r.integer("max_retries"), out);
        take(
            &mut self.max_response_bytes,
            r.integer("max_response_bytes"),
            out,
        );
    }

    fn read_v1_fields<'a>(&mut self, r: &Reader<'a>, out: &mut Vec<Diagnostic>) {
        let mut kind = None;
        take(&mut kind, r.string("provider"), out);
        take(&mut self.model, r.string("model"), out);
        take(&mut self.endpoint, r.string("endpoint"), out);
        take(
            &mut self.credential_env_var,
            r.string("credential_env_var"),
            out,
        );
        self.read_limits(r, out);
        if let Some(found) = kind {
            let (id, protocol) = match ProviderKind::from_name(&found.value) {
                Some(k) => (k.provider_id().to_owned(), k.protocol().to_owned()),
                None => {
                    self.unmapped_kind = true;
                    (found.value.clone(), found.value.clone())
                }
            };
            self.provider_id = Some(Found {
                value: id,
                at: found.at.clone(),
            });
            self.protocol = Some(Found {
                value: protocol,
                at: found.at,
            });
        }
    }

    // Returns the reader of the `[provider]` table when it exists.
    fn read_v2_tables<'a>(
        &mut self,
        r: &Reader<'a>,
        out: &mut Vec<Diagnostic>,
    ) -> Option<Reader<'a>> {
        let provider = subreader(r, "provider", PROVIDER_PREFIX, out);
        self.has_provider_table = provider.is_some();
        if let Some(p) = &provider {
            unknown_fields(p, &PROVIDER_FIELDS, out);
            take(&mut self.provider_id, p.string("id"), out);
            take(&mut self.protocol, p.string("protocol"), out);
            take(&mut self.model, p.string("model"), out);
            take(&mut self.endpoint, p.string("endpoint"), out);
            take(
                &mut self.credential_env_var,
                p.string("credential_env_var"),
                out,
            );
            take(
                &mut self.reasoning_effort,
                p.string("reasoning_effort"),
                out,
            );
            take(&mut self.verify_model, p.boolean("verify_model"), out);
            take(
                &mut self.allow_provider_training,
                p.boolean("allow_provider_training"),
                out,
            );
        }
        if let Some(l) = subreader(r, "limits", LIMITS_PREFIX, out) {
            unknown_fields(&l, &LIMIT_FIELDS, out);
            self.read_limits(&l, out);
        }
        if let Some(c) = subreader(r, "capabilities", CAPABILITIES_PREFIX, out) {
            unknown_fields(&c, &CAPABILITY_NAMES, out);
            for name in CAPABILITY_NAMES {
                let mut value = None;
                take(&mut value, c.string(name), out);
                if let Some(found) = value {
                    self.capabilities.push((name, found));
                }
            }
        }
        provider
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled.as_ref().is_some_and(|e| e.value)
    }

    /// The profile of the configured provider identifier, when it names one.
    pub(crate) fn profile(&self) -> Option<&'static Profile> {
        self.provider_id
            .as_ref()
            .filter(|_| !self.unmapped_kind)
            .and_then(|id| profile_for(&id.value))
    }

    fn id_name(&self) -> &'static str {
        if self.v2 {
            "provider.id"
        } else {
            "provider"
        }
    }

    fn check_values(&self, r: &Reader<'_>, out: &mut Vec<Diagnostic>) {
        if let Some(v) = &self.schema_version {
            if !SCHEMA_VERSIONS.contains(&v.value) {
                out.push(r.invalid(v.at.clone(), "`schema_version` must be 1 or 2".to_owned()));
            }
        }
        let non_empty = [
            (self.id_name(), self.provider_id.as_ref()),
            (
                "provider.protocol",
                self.protocol.as_ref().filter(|_| self.v2),
            ),
            ("transmission_log", self.transmission_log.as_ref()),
            ("script", self.script.as_ref()),
            ("provider.reasoning_effort", self.reasoning_effort.as_ref()),
        ];
        for (name, field) in non_empty {
            if let Some(f) = field.filter(|f| f.value.is_empty()) {
                out.push(r.invalid(f.at.clone(), format!("field `{name}` must not be empty")));
            }
        }
        self.check_model(r, out);
        if let Some(profile) = self.profile() {
            self.check_effort(r, profile, out);
            self.check_declarations(r, profile, out);
        }
    }

    // Req 35.10: 1 to 200 Unicode scalar values, no control character.
    fn check_model(&self, r: &Reader<'_>, out: &mut Vec<Diagnostic>) {
        let name = if self.v2 { "provider.model" } else { "model" };
        let Some(model) = &self.model else {
            return;
        };
        let scalars = model.value.chars().count();
        let bad = scalars == 0
            || scalars > MODEL_MAX_SCALARS
            || model.value.chars().any(char::is_control);
        if bad {
            out.push(r.invalid(
                model.at.clone(),
                format!("field `{name}` must be 1 to 200 characters without control characters"),
            ));
        }
    }

    // Req 35.11: a provider that documents effort values accepts only those.
    // A provider with no documented values reaches capability negotiation.
    fn check_effort(&self, r: &Reader<'_>, profile: &Profile, out: &mut Vec<Diagnostic>) {
        let Some(effort) = &self.reasoning_effort else {
            return;
        };
        if effort.value.is_empty() || profile.reasoning_efforts.is_empty() {
            return;
        }
        // A missing model is reported as a missing field, not twice.
        let max_elsewhere = effort.value == "max"
            && matches!(
                (profile.max_effort_models, &self.model),
                (Some(listed), Some(m)) if !listed.contains(&m.value.as_str())
            );
        if !profile.reasoning_efforts.contains(&effort.value.as_str()) || max_elsewhere {
            out.push(
                r.invalid(
                    effort.at.clone(),
                    "field `provider.reasoning_effort` is not an accepted value for the provider and model"
                        .to_owned(),
                ),
            );
        }
    }

    // Req 35.4: a declaration restates the profile value or lowers yes to no.
    fn check_declarations(&self, r: &Reader<'_>, profile: &Profile, out: &mut Vec<Diagnostic>) {
        for (name, found) in &self.capabilities {
            let path = format!("capabilities.{name}");
            let Some(declared) = parse_tri(&found.value) else {
                out.push(r.invalid(
                    found.at.clone(),
                    format!("field `{path}` must be \"yes\", \"no\" or \"unknown\""),
                ));
                continue;
            };
            let available = capability_value(profile, name);
            let lowers = available == Tri::Yes && declared == Tri::No;
            if declared != available && !lowers {
                out.push(r.invalid(
                    found.at.clone(),
                    format!(
                        "field `{path}` may only restate the provider profile or lower yes to no"
                    ),
                ));
            }
        }
    }

    fn check_required(
        &self,
        r: &Reader<'_>,
        provider: Option<&Reader<'_>>,
        out: &mut Vec<Diagnostic>,
    ) {
        let always = [
            ("schema_version", self.schema_version.is_some()),
            ("allow_document_text", self.allow_document_text.is_some()),
            ("ai_identities", self.ai_identities.is_some()),
            ("transmission_log", self.transmission_log.is_some()),
        ];
        for (name, present) in always {
            r.require(name, present, out);
        }
        // Version 1 holds the provider fields at the top level. Version 2
        // reports a missing table once and checks its fields when it exists.
        let fields = if self.v2 { provider } else { Some(r) };
        if self.v2 {
            r.require("provider", self.has_provider_table, out);
        }
        let Some(fields) = fields else {
            return;
        };
        let (id, protocol) = if self.v2 {
            ("id", "protocol")
        } else {
            ("provider", "provider")
        };
        fields.require(id, self.provider_id.is_some(), out);
        if self.v2 {
            fields.require(protocol, self.protocol.is_some(), out);
        }
        fields.require("model", self.model.is_some(), out);
        match self.profile().map(|p| p.official_host.is_some()) {
            Some(false) => self.check_mock_fields(r, fields, out),
            Some(true) => self.check_real_fields(r, fields, out),
            None => {}
        }
    }

    fn check_real_fields(&self, r: &Reader<'_>, fields: &Reader<'_>, out: &mut Vec<Diagnostic>) {
        fields.require("endpoint", self.endpoint.is_some(), out);
        fields.require("credential_env_var", self.credential_env_var.is_some(), out);
        if let Some(s) = &self.script {
            out.push(r.invalid(
                s.at.clone(),
                "field `script` is only for provider `mock-replay`".to_owned(),
            ));
        }
    }

    fn check_mock_fields(&self, r: &Reader<'_>, fields: &Reader<'_>, out: &mut Vec<Diagnostic>) {
        r.require("script", self.script.is_some(), out);
        let forbidden = [
            ("endpoint", self.endpoint.as_ref()),
            ("credential_env_var", self.credential_env_var.as_ref()),
        ];
        for (name, field) in forbidden {
            if let Some(f) = field {
                out.push(r.invalid(
                    f.at.clone(),
                    format!(
                        "field `{}` must be absent for provider `mock-replay`",
                        fields.path(name)
                    ),
                ));
            }
        }
    }
}

/// The declared value of a capability text, or `None` when it is not one of
/// the three words.
pub(crate) fn parse_tri(text: &str) -> Option<Tri> {
    match text {
        "yes" => Some(Tri::Yes),
        "no" => Some(Tri::No),
        "unknown" => Some(Tri::Unknown),
        _ => None,
    }
}

/// The profile value of a capability name from [`CAPABILITY_NAMES`].
pub(crate) fn capability_value(profile: &Profile, name: &str) -> Tri {
    match name {
        "reasoning_effort" => profile.capabilities.reasoning_effort,
        "structured_output" => profile.capabilities.structured_output,
        _ => profile.capabilities.usage_reporting,
    }
}
