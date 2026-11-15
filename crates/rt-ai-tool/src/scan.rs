//! The inline-secret check (Req 28.4, 28.18). It runs on the raw TOML, before
//! any field is interpreted, and never echoes a rejected value or a rejected
//! field name.

use crate::endpoint;
use crate::reader::Reader;
use crate::secret;
use rt_foundation::{Code, Diagnostic, Location};
use toml_edit::{Item, Table, Value};

pub(crate) fn finding(at: Location, what: &str) -> Diagnostic {
    Diagnostic::new(
        Code::AiConfigInlineSecret,
        format!("{what}; the configuration names a credential only by environment variable"),
        at,
    )
}

fn value_text(value: &Value) -> Vec<&str> {
    match value {
        Value::String(s) => vec![s.value().as_str()],
        Value::Array(a) => a.iter().flat_map(value_text).collect(),
        Value::InlineTable(t) => t.iter().flat_map(|(_, v)| value_text(v)).collect(),
        _ => Vec::new(),
    }
}

fn item_text(item: &Item) -> Vec<&str> {
    match item {
        Item::Value(v) => value_text(v),
        Item::Table(t) => t.iter().flat_map(|(_, i)| item_text(i)).collect(),
        Item::ArrayOfTables(a) => a
            .iter()
            .flat_map(|t| t.iter().flat_map(|(_, i)| item_text(i)))
            .collect(),
        Item::None => Vec::new(),
    }
}

fn value_name_hit(value: &Value) -> bool {
    match value {
        Value::Array(a) => a.iter().any(value_name_hit),
        Value::InlineTable(t) => t
            .iter()
            .any(|(k, v)| secret::field_name_looks_secret(k) || value_name_hit(v)),
        _ => false,
    }
}

fn table_name_hit(table: &Table) -> bool {
    table
        .iter()
        .any(|(k, i)| secret::field_name_looks_secret(k) || nested_name_hit(i))
}

fn nested_name_hit(item: &Item) -> bool {
    match item {
        Item::Table(t) => table_name_hit(t),
        Item::Value(v) => value_name_hit(v),
        Item::ArrayOfTables(a) => a.iter().any(table_name_hit),
        Item::None => false,
    }
}

/// All inline-secret findings, in table order.
pub(crate) fn scan(reader: &Reader<'_>) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    for (name, item) in reader.table.iter() {
        let Some((key, _)) = reader.table.get_key_value(name) else {
            continue;
        };
        if secret::field_name_looks_secret(name) || nested_name_hit(item) {
            let at = reader.locator.at(key.span(), None);
            found.push(finding(
                at,
                "a field name looks like it carries a credential",
            ));
            continue;
        }
        let at = reader.key_location(key, name);
        let texts = item_text(item);
        if texts.iter().any(|t| secret::value_looks_secret(t)) {
            found.push(finding(
                at,
                &format!("field `{name}` holds a credential-like value"),
            ));
            continue;
        }
        if name == "credential_env_var" && texts.iter().any(|t| !secret::is_env_var_name(t)) {
            found.push(finding(
                at,
                "field `credential_env_var` is not an environment variable name",
            ));
            continue;
        }
        if name == "endpoint" && texts.iter().any(|t| endpoint::has_userinfo_or_query(t)) {
            found.push(finding(
                at,
                "field `endpoint` has a user name, password or query",
            ));
        }
    }
    found
}
