//! Lowering of a flat rule draft (Proposal_Schema version "2") to the
//! nested rule text of version "1" (UD-37, Req 23.39, design 3.7.3).
//!
//! A rule draft carries a `nodes` table. Every member that holds a
//! sub-expression in the nested form holds a node index here. The table
//! rules are checked first: every index is in range, a child index is
//! greater than the index of the node that names it (so there is no
//! cycle), and every node is named exactly once (so the table is a tree
//! and the lowered text equals the nested form). Lowering then writes
//! the text with an explicit work list, so the depth of the table cannot
//! exhaust the stack.

use super::child;
use super::closed::{breach, operand_keys, Breach};
use rt_json::{write_compact, Member, Position, Value};

/// A node index as written, with where it was written.
struct Ref {
    index: usize,
    pos: Position,
    path: String,
}

enum Tok<'v> {
    Lit(String),
    Val(&'v Value<Position>),
    Node(usize),
}

/// Lowers `rule`, a version "2" rule draft, to the compact text of the
/// nested draft.
///
/// # Errors
/// The first breach of the table rules or of the shape of the draft, with
/// its position and field path.
pub(super) fn lower(rule: &Value<Position>, path: &str) -> Result<String, Breach> {
    let Some(members) = rule.as_object() else {
        return Err(breach(
            *rule.pos(),
            path.to_owned(),
            format!("{path} must be an object"),
        ));
    };
    let nodes = table(members, path)?;
    let roots = rule_refs(rule, members, path)?;
    check_table(nodes, &roots, path)?;
    Ok(write(rule, members, nodes))
}

fn table<'v>(members: &'v [Member<Position>], path: &str) -> Result<&'v [Value<Position>], Breach> {
    let Some(m) = members.iter().find(|m| m.key == "nodes") else {
        return Ok(&[]);
    };
    m.value.as_array().ok_or_else(|| {
        breach(
            *m.value.pos(),
            child(path, "nodes"),
            "nodes must be an array".to_owned(),
        )
    })
}

fn index_of(value: &Value<Position>, at: String) -> Result<Ref, Breach> {
    value
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .map(|index| Ref {
            index,
            pos: *value.pos(),
            path: at.clone(),
        })
        .ok_or_else(|| {
            breach(
                *value.pos(),
                at,
                "a node index must be a non-negative integer".to_owned(),
            )
        })
}

fn required<'v>(
    owner: &Value<Position>,
    members: &'v [Member<Position>],
    path: &str,
    key: &str,
) -> Result<&'v Member<Position>, Breach> {
    members.iter().find(|m| m.key == key).ok_or_else(|| {
        breach(
            *owner.pos(),
            path.to_owned(),
            format!("missing required field '{key}'"),
        )
    })
}

fn single(
    owner: &Value<Position>,
    members: &[Member<Position>],
    path: &str,
    key: &str,
) -> Result<Ref, Breach> {
    let m = required(owner, members, path, key)?;
    index_of(&m.value, child(path, key))
}

fn each_item(
    owner: &Value<Position>,
    members: &[Member<Position>],
    path: &str,
    key: &str,
    keys: &[&str],
) -> Result<Vec<Ref>, Breach> {
    let m = required(owner, members, path, key)?;
    let here = child(path, key);
    let Some(items) = m.value.as_array() else {
        return Err(breach(
            *m.value.pos(),
            here,
            format!("{key} must be an array"),
        ));
    };
    let mut refs = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let at = format!("{here}[{i}]");
        let Some(fields) = item.as_object() else {
            return Err(breach(
                *item.pos(),
                at,
                format!("{key} items must be objects"),
            ));
        };
        for k in keys {
            refs.push(single(item, fields, &at, k)?);
        }
    }
    Ok(refs)
}

/// The indexes that the rule itself holds, in the order of its kind.
fn rule_refs(
    rule: &Value<Position>,
    members: &[Member<Position>],
    path: &str,
) -> Result<Vec<Ref>, Breach> {
    let kind = members
        .iter()
        .find(|m| m.key == "kind")
        .and_then(|m| m.value.as_str());
    let mut refs = Vec::new();
    match kind {
        Some("eligibility" | "derived") => refs.push(single(rule, members, path, "expr")?),
        Some("default") => refs.push(single(rule, members, path, "value")?),
        Some("ranking") => {
            refs.push(single(rule, members, path, "key")?);
            refs.extend(each_item(rule, members, path, "tie_breakers", &["expr"])?);
        }
        Some("weighted_score") => {
            refs.extend(each_item(
                rule,
                members,
                path,
                "terms",
                &["weight", "value"],
            )?);
        }
        _ => {}
    }
    Ok(refs)
}

fn node_name(node: &Value<Position>) -> Option<&str> {
    node.as_object()?
        .iter()
        .find(|m| m.key == "node")?
        .value
        .as_str()
}

/// The indexes that `node` names, in member order.
fn node_refs(node: &Value<Position>, at: &str) -> Result<Vec<Ref>, Breach> {
    let (Some(name), Some(members)) = (node_name(node), node.as_object()) else {
        return Ok(Vec::new());
    };
    let keys = operand_keys(name);
    let mut refs = Vec::new();
    for m in members.iter().filter(|m| keys.contains(&m.key.as_str())) {
        let here = child(at, &m.key);
        match m.value.as_array() {
            Some(items) => {
                for (i, item) in items.iter().enumerate() {
                    refs.push(index_of(item, format!("{here}[{i}]"))?);
                }
            }
            None => refs.push(index_of(&m.value, here)?),
        }
    }
    Ok(refs)
}

fn check_table(nodes: &[Value<Position>], roots: &[Ref], path: &str) -> Result<(), Breach> {
    let count = nodes.len();
    let mut seen = vec![false; count];
    let mut mark = |r: &Ref, parent: Option<usize>| -> Result<(), Breach> {
        let fail = |message: String| Err(breach(r.pos, r.path.clone(), message));
        if r.index >= count {
            return fail(format!(
                "node index {} is out of range, the table has {count} nodes",
                r.index
            ));
        }
        if parent.is_some_and(|p| r.index <= p) {
            return fail(format!(
                "node {} must have a greater index than the node {} that names it",
                r.index,
                parent.unwrap_or_default()
            ));
        }
        match seen.get_mut(r.index) {
            Some(slot) if !*slot => {
                *slot = true;
                Ok(())
            }
            _ => fail(format!("node {} is named more than once", r.index)),
        }
    };
    for r in roots {
        mark(r, None)?;
    }
    let table = child(path, "nodes");
    for (i, node) in nodes.iter().enumerate() {
        for r in node_refs(node, &format!("{table}[{i}]"))? {
            mark(&r, Some(i))?;
        }
    }
    if let Some((i, node)) = nodes
        .iter()
        .enumerate()
        .find(|(i, _)| seen.get(*i) == Some(&false))
    {
        return Err(breach(
            *node.pos(),
            format!("{table}[{i}]"),
            format!("node {i} is not named by the rule or by another node"),
        ));
    }
    Ok(())
}

fn key_text(key: &str) -> String {
    write_compact(&Value::<()>::string(key))
}

/// Tokens of a member value that holds one index or a list of indexes.
fn ref_tokens(value: &Value<Position>) -> Vec<Tok<'_>> {
    let index = |v: &Value<Position>| v.as_u64().and_then(|n| usize::try_from(n).ok());
    match value.as_array() {
        Some(items) => {
            let mut out = vec![Tok::Lit("[".to_owned())];
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(Tok::Lit(",".to_owned()));
                }
                out.extend(index(item).map(Tok::Node));
            }
            out.push(Tok::Lit("]".to_owned()));
            out
        }
        None => index(value).map(Tok::Node).into_iter().collect(),
    }
}

/// Tokens of an object whose members named in `refs` hold indexes. A null
/// `t` or `type` is left out, as an absent optional member.
fn object_tokens<'v>(members: &'v [Member<Position>], refs: &[&str]) -> Vec<Tok<'v>> {
    let mut out = vec![Tok::Lit("{".to_owned())];
    let mut first = true;
    for m in members {
        let optional = m.key == "t" || m.key == "type";
        if m.key == "nodes" || (optional && matches!(m.value.kind(), rt_json::Kind::Null)) {
            continue;
        }
        if !first {
            out.push(Tok::Lit(",".to_owned()));
        }
        first = false;
        out.push(Tok::Lit(format!("{}:", key_text(&m.key))));
        if refs.contains(&m.key.as_str()) {
            out.extend(ref_tokens(&m.value));
        } else {
            out.push(Tok::Val(&m.value));
        }
    }
    out.push(Tok::Lit("}".to_owned()));
    out
}

fn list_tokens<'v>(value: &'v Value<Position>, refs: &[&str]) -> Vec<Tok<'v>> {
    let mut out = vec![Tok::Lit("[".to_owned())];
    for (i, item) in value.as_array().unwrap_or_default().iter().enumerate() {
        if i > 0 {
            out.push(Tok::Lit(",".to_owned()));
        }
        match item.as_object() {
            Some(fields) => out.extend(object_tokens(fields, refs)),
            None => out.push(Tok::Val(item)),
        }
    }
    out.push(Tok::Lit("]".to_owned()));
    out
}

fn rule_tokens<'v>(members: &'v [Member<Position>], kind: &str) -> Vec<Tok<'v>> {
    let mut out = vec![Tok::Lit("{".to_owned())];
    let mut first = true;
    for m in members.iter().filter(|m| m.key != "nodes") {
        if m.key == "type" && matches!(m.value.kind(), rt_json::Kind::Null) {
            continue;
        }
        if !first {
            out.push(Tok::Lit(",".to_owned()));
        }
        first = false;
        out.push(Tok::Lit(format!("{}:", key_text(&m.key))));
        let key = m.key.as_str();
        match (kind, key) {
            ("eligibility" | "derived", "expr") | ("default", "value") | ("ranking", "key") => {
                out.extend(ref_tokens(&m.value))
            }
            ("weighted_score", "terms") => out.extend(list_tokens(&m.value, &["weight", "value"])),
            ("ranking", "tie_breakers") => out.extend(list_tokens(&m.value, &["expr"])),
            _ => out.push(Tok::Val(&m.value)),
        }
    }
    out.push(Tok::Lit("}".to_owned()));
    out
}

fn write(
    rule: &Value<Position>,
    members: &[Member<Position>],
    nodes: &[Value<Position>],
) -> String {
    let kind = rule
        .get("kind")
        .and_then(|m| m.value.as_str())
        .unwrap_or_default();
    let mut out = String::new();
    let mut work: Vec<Tok<'_>> = rule_tokens(members, kind).into_iter().rev().collect();
    while let Some(tok) = work.pop() {
        match tok {
            Tok::Lit(text) => out.push_str(&text),
            Tok::Val(value) => out.push_str(&write_compact(value)),
            Tok::Node(i) => {
                let fields = nodes.get(i).and_then(Value::as_object);
                let name = nodes.get(i).and_then(node_name).unwrap_or_default();
                if let Some(fields) = fields {
                    work.extend(object_tokens(fields, operand_keys(name)).into_iter().rev());
                }
            }
        }
    }
    out
}
