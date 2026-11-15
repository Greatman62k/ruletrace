//! The `declarations` member of a Provider_Request (design 3.7.2).

use rt_ir::Rulebook;
use rt_json::Value;
use std::collections::BTreeMap;

fn named(name: &str, ty: &str, key: &str) -> Option<Value<()>> {
    Value::object(vec![
        ("name".to_owned(), Value::string(name)),
        (key.to_owned(), Value::string(ty)),
    ])
    .ok()
}

/// The declarations object of `rulebook`: type declarations in declaration
/// order and the highest revision of every rule, ordered by rule identifier.
pub(crate) fn declarations(rulebook: &Rulebook) -> Option<Value<()>> {
    let types = &rulebook.types;
    let inputs = types
        .inputs
        .iter()
        .map(|d| named(d.name.as_str(), &d.ty.text(), "type"))
        .collect::<Option<Vec<_>>>()?;
    let enums = types
        .enums
        .iter()
        .map(|d| {
            let members = d
                .members
                .iter()
                .map(|m| Value::string(m.as_str()))
                .collect();
            Value::object(vec![
                ("name".to_owned(), Value::string(d.name.as_str())),
                ("members".to_owned(), Value::array(members)),
            ])
            .ok()
        })
        .collect::<Option<Vec<_>>>()?;
    let sets = types
        .sets
        .iter()
        .map(|d| named(d.name.as_str(), &d.element_type.text(), "element_type"))
        .collect::<Option<Vec<_>>>()?;
    let collections = types
        .collections
        .iter()
        .map(|d| {
            let fields = d
                .fields
                .iter()
                .map(|f| named(f.name.as_str(), &f.ty.text(), "type"))
                .collect::<Option<Vec<_>>>()?;
            Value::object(vec![
                ("name".to_owned(), Value::string(d.name.as_str())),
                ("fields".to_owned(), Value::array(fields)),
            ])
            .ok()
        })
        .collect::<Option<Vec<_>>>()?;
    let mut highest: BTreeMap<&str, u32> = BTreeMap::new();
    for revision in &rulebook.revisions {
        let entry = highest.entry(revision.key.id.as_str()).or_insert(0);
        *entry = (*entry).max(revision.key.revision.get());
    }
    let highest = highest
        .into_iter()
        .map(|(id, n)| (id.to_owned(), Value::integer(n)))
        .collect();
    Value::object(vec![
        (
            "ir_version".to_owned(),
            Value::string(rulebook.ir_version.as_str()),
        ),
        ("inputs".to_owned(), Value::array(inputs)),
        ("enums".to_owned(), Value::array(enums)),
        ("sets".to_owned(), Value::array(sets)),
        ("collections".to_owned(), Value::array(collections)),
        ("highest_revision".to_owned(), Value::object(highest).ok()?),
    ])
    .ok()
}
