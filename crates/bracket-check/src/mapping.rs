//! Prepare, part 1: what the mappers of `rt-data::tabular` need, built from
//! the Compiled_Rulebook, and the mapping of the three input files.
//!
//! No competition field name appears here. Names, types and Target kinds come
//! from the Compiled_Rulebook, and the mapping errors of Req 31.9 come from
//! `rt-data::tabular` and pass through unchanged.

use rt_compiled::{CType, CompiledBody, CompiledRulebook};
use rt_data::tabular::{
    map_collections, map_participants, map_published, parse_csv, ByteLimit, FieldType, InputSchema,
    MappedPublished, MappingOptions, TargetKind, TargetKinds,
};
use rt_data::{CompetitionData, TargetName};
use rt_foundation::{escape_controls, Code, Diagnostic, DiagnosticState, InputId, Location};
use std::collections::{BTreeMap, BTreeSet};

/// The bytes of one input file and the name its Diagnostics use. The name is
/// never a path the library opens.
#[derive(Clone, Copy, Debug)]
pub struct NamedFile<'a> {
    /// The file content.
    pub bytes: &'a [u8],
    /// The name given to the file in Input_Locations.
    pub name: &'a InputId,
}

/// The input files of a run, already read by the caller.
#[derive(Clone, Copy, Debug)]
pub struct FileInputs<'a> {
    /// The participants file (`participant` and one column per input).
    pub participants: NamedFile<'a>,
    /// The collections file, when given.
    pub collections: Option<NamedFile<'a>>,
    /// The published-result file, when given.
    pub published: Option<NamedFile<'a>>,
    /// The mapping options (`--ignore-column`).
    pub options: &'a MappingOptions,
}

/// The result of mapping every file that could be mapped.
pub(crate) struct Mapped {
    pub(crate) data: CompetitionData,
    pub(crate) published: Option<MappedPublished>,
}

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

fn parse(file: NamedFile<'_>) -> Result<rt_data::tabular::Table, DiagnosticState> {
    parse_csv(file.bytes, file.name, ByteLimit::DEFAULT)
}

/// Maps the files. Every fault of every file that can be checked is
/// collected into `problems`.
///
/// The collections and the published file name participants of the
/// participants file, so they are mapped only when that file mapped. Their
/// CSV syntax is checked either way.
pub(crate) fn map_files(
    schema: &InputSchema,
    kinds: &TargetKinds,
    requested: &BTreeSet<TargetName>,
    files: &FileInputs<'_>,
    problems: &mut DiagnosticState,
) -> Option<Mapped> {
    let participants =
        parse(files.participants).and_then(|table| map_participants(&table, schema, files.options));
    let base = match participants {
        Ok(data) => data,
        Err(state) => {
            problems.extend(state);
            for file in [files.collections, files.published].into_iter().flatten() {
                if let Err(state) = parse(file) {
                    problems.extend(state);
                }
            }
            return None;
        }
    };
    let mut ok = true;
    let mut data = base;
    if let Some(file) = files.collections {
        match parse(file).and_then(|table| map_collections(&table, schema, data.clone())) {
            Ok(mapped) => data = mapped,
            Err(state) => {
                problems.extend(state);
                ok = false;
            }
        }
    }
    let mut published = None;
    if let Some(file) = files.published {
        match parse(file).and_then(|table| map_published(&table, kinds, requested, &data)) {
            Ok(mapped) => published = Some(mapped),
            Err(state) => {
                problems.extend(state);
                ok = false;
            }
        }
    }
    ok.then_some(Mapped { data, published })
}
