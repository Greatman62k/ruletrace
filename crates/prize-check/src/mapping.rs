//! Prepare, part 1: what the mappers of `rt-data::tabular` need, built from
//! the Compiled_Rulebook. The mapping of the three input files is
//! `rt_data::tabular::map_files`.
//!
//! No competition field name appears here. Names, types and Target kinds come
//! from the Compiled_Rulebook, and the mapping errors of Req 31.9 come from
//! `rt-data::tabular` and pass through unchanged.

use rt_compiled::{CType, CompiledBody, CompiledRulebook};
use rt_data::tabular::{FieldType, InputSchema, TargetKind, TargetKinds};
pub use rt_data::tabular::{FileInputs, NamedFile};
use rt_data::TargetName;
use rt_foundation::{escape_controls, Code, Diagnostic, Location};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn internal(message: String) -> Diagnostic {
    Diagnostic::new(
        Code::InternalError,
        message,
        Location::None(Code::InternalError),
    )
}

fn field_type(compiled: &CompiledRulebook, ty: CType) -> Option<FieldType> {
    match ty {
        CType::Number => Some(FieldType::Number),
        CType::Boolean => Some(FieldType::Boolean),
        CType::Text => Some(FieldType::Text),
        CType::Date => Some(FieldType::Date),
        CType::Enum(idx) => {
            let declared = compiled.enums().get(idx.at())?;
            let members: BTreeSet<String> = declared
                .members
                .iter()
                .map(|m| m.as_str().to_owned())
                .collect();
            Some(FieldType::Enum(members))
        }
        CType::Ranking => None,
    }
}

/// The declared inputs and collections of the Compiled_Rulebook.
pub(crate) fn input_schema(compiled: &CompiledRulebook) -> Result<InputSchema, Box<Diagnostic>> {
    let unmappable = |what: &str, name: &str| {
        Box::new(internal(format!(
            "the {what} '{}' of the Compiled_Rulebook has a type that no input file can carry",
            escape_controls(name)
        )))
    };
    let mut inputs = Vec::new();
    for input in compiled.inputs() {
        let ty = field_type(compiled, input.ty)
            .ok_or_else(|| unmappable("input", input.name.as_str()))?;
        inputs.push((input.name.as_str().to_owned(), ty));
    }
    let mut collections = Vec::new();
    for collection in compiled.collections() {
        let mut fields = Vec::new();
        for field in &collection.fields {
            let ty = field_type(compiled, field.ty)
                .ok_or_else(|| unmappable("collection field", field.name.as_str()))?;
            fields.push((field.name.as_str().to_owned(), ty));
        }
        collections.push((collection.name.as_str().to_owned(), fields));
    }
    Ok(InputSchema::new(inputs, collections))
}

fn kind_of(body: &CompiledBody) -> TargetKind {
    match body {
        CompiledBody::Eligibility { .. } => TargetKind::Eligibility,
        CompiledBody::Derived { .. } => TargetKind::Derived,
        CompiledBody::WeightedScore { .. } => TargetKind::Score,
        CompiledBody::Ranking { .. } => TargetKind::Ranking,
        CompiledBody::Default { .. } => TargetKind::Other,
    }
}

/// The kind of every declared Target, by name. A Target whose rules differ in
/// kind, or that only a default defines, is `Other`: an application cannot
/// report it.
pub(crate) fn target_kinds_by_name(compiled: &CompiledRulebook) -> BTreeMap<String, TargetKind> {
    let mut kinds: BTreeMap<String, TargetKind> = BTreeMap::new();
    for rule in compiled.rules() {
        let kind = kind_of(&rule.body);
        let name = rule.target.as_str().to_owned();
        match kinds.get(&name) {
            Some(known) if *known == kind => {}
            Some(_) => {
                kinds.insert(name, TargetKind::Other);
            }
            None => {
                kinds.insert(name, kind);
            }
        }
    }
    kinds
}

/// The kinds as `rt-data` wants them.
pub(crate) fn target_kinds(compiled: &CompiledRulebook) -> Result<TargetKinds, Box<Diagnostic>> {
    let mut named = Vec::new();
    for (name, kind) in target_kinds_by_name(compiled) {
        let target = TargetName::new(name.as_str()).map_err(|e| {
            Box::new(internal(format!(
                "the Target '{}' is not a valid Target name: {e}",
                escape_controls(&name)
            )))
        })?;
        named.push((target, kind));
    }
    Ok(TargetKinds::new(named))
}
