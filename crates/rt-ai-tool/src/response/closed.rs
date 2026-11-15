//! Closed vocabulary of a rule draft on the AI path (design 3.7.3).
//!
//! The Rule_IR decoder carries any well-formed unknown kind or node name as
//! an opaque construct, which suits authored files (Req 4.9). A provider
//! response may only use the vocabulary it was sent, or the one explicit
//! `unrecognized` variant with exactly its three fields (Req 23.4).

use super::{child, shown};
use crate::vocab::RULE_KINDS;
use rt_ir::OpaqueName;
use rt_json::{Kind, Member, Position, Value};

const UNRECOGNIZED: &str = "unrecognized";

const NODES: [&str; 16] = [
    "num",
    "bool",
    "text",
    "date",
    "enum",
    "field",
    "ref",
    "binary",
    "cmp",
    "and",
    "or",
    "not",
    "in",
    "agg",
    "round",
    "rank_reference",
];

// Fields a rule of the unrecognized kind keeps from every other rule.
const ORDINARY_RULE_FIELDS: [&str; 5] = ["id", "target", "type", "markers", "spans"];

/// The first place where a rule draft leaves the closed vocabulary.
pub(super) struct Breach {
    pub(super) pos: Position,
    pub(super) path: String,
    pub(super) message: String,
}

fn breach(pos: Position, path: String, message: String) -> Breach {
    Breach { pos, path, message }
}

/// Checks the rule kind and every expression node of `rule`, in document
/// order. Shapes that are not objects or strings are left to the decoder.
pub(super) fn check(rule: &Value<Position>, path: &str) -> Result<(), Breach> {
    let Some(members) = rule.as_object() else {
        return Ok(());
    };
    if let Some(kind) = members.iter().find(|m| m.key == "kind") {
        if let Some(name) = kind.value.as_str() {
            let here = child(path, "kind");
            if name == UNRECOGNIZED {
                variant(rule, members, path, "kind", &ORDINARY_RULE_FIELDS)?;
            } else if !RULE_KINDS.contains(&name) {
                return Err(breach(
                    *kind.value.pos(),
                    here,
                    format!(
                        "unknown rule kind '{}'; a capability outside the vocabulary \
                         is stated as kind \"{UNRECOGNIZED}\"",
                        shown(name)
                    ),
                ));
            }
        }
    }
    for m in members {
        walk(&m.value, &child(path, &m.key))?;
    }
    Ok(())
}

fn walk(value: &Value<Position>, path: &str) -> Result<(), Breach> {
    match value.kind() {
        Kind::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk(item, &format!("{path}[{i}]"))?;
            }
        }
        Kind::Object(members) => {
            if let Some(node) = members.iter().find(|m| m.key == "node") {
                node_name(value, members, node, path)?;
            }
            for m in members {
                walk(&m.value, &child(path, &m.key))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn node_name(
    value: &Value<Position>,
    members: &[Member<Position>],
    node: &Member<Position>,
    path: &str,
) -> Result<(), Breach> {
    let Some(name) = node.value.as_str() else {
        return Ok(());
    };
    if name == UNRECOGNIZED {
        return variant(value, members, path, "node", &[]);
    }
    if NODES.contains(&name) {
        return Ok(());
    }
    Err(breach(
        *node.value.pos(),
        child(path, "node"),
        format!(
            "unknown expression node '{}'; a capability outside the vocabulary \
             is stated as node \"{UNRECOGNIZED}\"",
            shown(name)
        ),
    ))
}

/// The unrecognized variant: its tag, `name` and `detail`, plus `extra`.
fn variant(
    owner: &Value<Position>,
    members: &[Member<Position>],
    path: &str,
    tag: &str,
    extra: &[&str],
) -> Result<(), Breach> {
    for m in members {
        let key = m.key.as_str();
        let allowed = key == tag || key == "name" || key == "detail" || extra.contains(&key);
        if !allowed {
            return Err(breach(
                m.key_pos,
                child(path, key),
                format!(
                    "unknown field '{}' in an unrecognized construct",
                    shown(key)
                ),
            ));
        }
    }
    let text = |key: &str| -> Result<&Member<Position>, Breach> {
        let m = members.iter().find(|m| m.key == key).ok_or_else(|| {
            breach(
                *owner.pos(),
                path.to_owned(),
                format!("missing required field '{key}'"),
            )
        })?;
        if m.value.as_str().is_none() {
            return Err(breach(
                *m.value.pos(),
                child(path, key),
                format!("field '{key}' must be a string"),
            ));
        }
        Ok(m)
    };
    let name = text("name")?;
    let well_formed = name
        .value
        .as_str()
        .is_some_and(|s| OpaqueName::new(s).is_ok());
    if !well_formed {
        return Err(breach(
            *name.value.pos(),
            child(path, "name"),
            "name must match [a-z][a-z0-9_]*".to_owned(),
        ));
    }
    text("detail")?;
    Ok(())
}
