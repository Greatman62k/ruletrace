//! Inline-secret detection (Req 28.4).

const KEY_NAME_PARTS: [&str; 5] = ["key", "token", "secret", "password", "authorization"];
const VALUE_PREFIXES: [&str; 5] = ["sk-", "sk_", "Bearer", "ghp_", "AKIA"];

/// True when a field name suggests that it carries a credential.
pub(crate) fn name_looks_secret(name: &str) -> bool {
    let lower = name.to_lowercase();
    KEY_NAME_PARTS.iter().any(|part| lower.contains(part))
}

/// True when a value begins like a well-known credential.
pub(crate) fn value_looks_secret(value: &str) -> bool {
    VALUE_PREFIXES
        .iter()
        .any(|prefix| value.starts_with(prefix))
}

/// True for `[A-Z_][A-Z0-9_]*`.
pub(crate) fn is_env_var_name(name: &str) -> bool {
    let mut chars = name.chars();
    let first_ok = matches!(chars.next(), Some('A'..='Z' | '_'));
    first_ok && chars.all(|c| matches!(c, 'A'..='Z' | '0'..='9' | '_'))
}

/// True when a field name is a credential in disguise: it contains a
/// key-like word or it begins like a well-known credential.
pub(crate) fn field_name_looks_secret(name: &str) -> bool {
    name_looks_secret(name) || value_looks_secret(name)
}
