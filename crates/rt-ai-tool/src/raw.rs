//! Field-level checks of Req 28.3: unknown fields, wrong types and missing
//! required fields. Values are kept as read; ranges are checked later.

use crate::reader::{Found, Reader};
use crate::scan::finding;
use crate::secret;
use crate::ProviderKind;
use rt_foundation::Diagnostic;

const KNOWN_FIELDS: [&str; 15] = [
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

/// The supported schema version.
pub(crate) const SCHEMA_VERSION: i64 = 1;

/// Fields as written in the file.
#[derive(Debug, Default)]
pub(crate) struct RawConfig {
    pub(crate) schema_version: Option<Found<i64>>,
    pub(crate) enabled: Option<Found<bool>>,
    pub(crate) provider: Option<Found<String>>,
    pub(crate) model: Option<Found<String>>,
    pub(crate) endpoint: Option<Found<String>>,
    pub(crate) credential_env_var: Option<Found<String>>,
    pub(crate) script: Option<Found<String>>,
    pub(crate) allow_document_text: Option<Found<bool>>,
    pub(crate) ai_identities: Option<Found<Vec<String>>>,
    pub(crate) transmission_log: Option<Found<String>>,
    pub(crate) max_clauses: Option<Found<i64>>,
    pub(crate) total_deadline: Option<Found<i64>>,
    pub(crate) attempt_timeout: Option<Found<i64>>,
    pub(crate) max_retries: Option<Found<i64>>,
    pub(crate) max_response_bytes: Option<Found<i64>>,
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

impl RawConfig {
    /// Reads every field and returns every field-level finding.
    pub(crate) fn read(r: &Reader<'_>) -> Result<Self, Vec<Diagnostic>> {
        let mut out = Vec::new();
        let mut raw = Self::default();
        for (name, _) in r.table.iter() {
            if !KNOWN_FIELDS.contains(&name) {
                if let Some((key, _)) = r.table.get_key_value(name) {
                    if secret::field_name_looks_secret(name) {
                        let at = r.locator.at(key.span(), None);
                        out.push(finding(
                            at,
                            "a field name looks like it carries a credential",
                        ));
                        continue;
                    }
                    let at = r.key_location(key, name);
                    out.push(r.invalid(at, format!("unknown field `{name}`")));
                }
            }
        }
        take(
            &mut raw.schema_version,
            r.integer("schema_version"),
            &mut out,
        );
        take(&mut raw.enabled, r.boolean("enabled"), &mut out);
        take(&mut raw.provider, r.string("provider"), &mut out);
        take(&mut raw.model, r.string("model"), &mut out);
        take(&mut raw.endpoint, r.string("endpoint"), &mut out);
        take(
            &mut raw.credential_env_var,
            r.string("credential_env_var"),
            &mut out,
        );
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
        take(
            &mut raw.total_deadline,
            r.integer("total_deadline_secs"),
            &mut out,
        );
        take(
            &mut raw.attempt_timeout,
            r.integer("attempt_timeout_secs"),
            &mut out,
        );
        take(&mut raw.max_retries, r.integer("max_retries"), &mut out);
        take(
            &mut raw.max_response_bytes,
            r.integer("max_response_bytes"),
            &mut out,
        );
        raw.check_values(r, &mut out);
        if raw.is_enabled() {
            raw.check_required(r, &mut out);
        }
        if out.is_empty() {
            Ok(raw)
        } else {
            Err(out)
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled.as_ref().is_some_and(|e| e.value)
    }

    fn check_values(&self, r: &Reader<'_>, out: &mut Vec<Diagnostic>) {
        if let Some(v) = &self.schema_version {
            if v.value != SCHEMA_VERSION {
                out.push(r.invalid(
                    v.at.clone(),
                    format!("`schema_version` must be {SCHEMA_VERSION}"),
                ));
            }
        }
        let non_empty = [
            ("model", self.model.as_ref()),
            ("provider", self.provider.as_ref()),
            ("transmission_log", self.transmission_log.as_ref()),
            ("script", self.script.as_ref()),
        ];
        for (name, field) in non_empty {
            if let Some(f) = field.filter(|f| f.value.is_empty()) {
                out.push(r.invalid(f.at.clone(), format!("field `{name}` must not be empty")));
            }
        }
    }

    fn check_required(&self, r: &Reader<'_>, out: &mut Vec<Diagnostic>) {
        let always = [
            ("schema_version", self.schema_version.is_some()),
            ("provider", self.provider.is_some()),
            ("model", self.model.is_some()),
            ("allow_document_text", self.allow_document_text.is_some()),
            ("ai_identities", self.ai_identities.is_some()),
            ("transmission_log", self.transmission_log.is_some()),
        ];
        for (name, present) in always {
            r.require(name, present, out);
        }
        let kind = self
            .provider
            .as_ref()
            .and_then(|p| ProviderKind::from_name(&p.value));
        match kind {
            Some(ProviderKind::MockReplay) => self.check_mock_fields(r, out),
            Some(ProviderKind::OpenAiResponses) => self.check_real_fields(r, out),
            None => {}
        }
    }

    fn check_real_fields(&self, r: &Reader<'_>, out: &mut Vec<Diagnostic>) {
        r.require("endpoint", self.endpoint.is_some(), out);
        r.require("credential_env_var", self.credential_env_var.is_some(), out);
        if let Some(s) = &self.script {
            out.push(r.invalid(
                s.at.clone(),
                "field `script` is only for provider `mock-replay`".to_owned(),
            ));
        }
    }

    fn check_mock_fields(&self, r: &Reader<'_>, out: &mut Vec<Diagnostic>) {
        r.require("script", self.script.is_some(), out);
        let forbidden = [
            ("endpoint", self.endpoint.as_ref()),
            ("credential_env_var", self.credential_env_var.as_ref()),
        ];
        for (name, field) in forbidden {
            if let Some(f) = field {
                out.push(r.invalid(
                    f.at.clone(),
                    format!("field `{name}` must be absent for provider `mock-replay`"),
                ));
            }
        }
    }
}
