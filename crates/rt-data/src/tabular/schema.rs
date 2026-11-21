//! The values an application builds from the Compiled_Rulebook and passes to
//! the mappers (design §8.4). This crate knows no competition field name.

use crate::{ParticipantId, PublishedResult, TargetName};
use std::collections::{BTreeMap, BTreeSet};

/// The declared type of one input or collection field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldType {
    /// An Exact_Number written as a decimal literal.
    Number,
    /// `true` or `false`.
    Boolean,
    /// A calendar date `YYYY-MM-DD`.
    Date,
    /// Any non-empty text.
    Text,
    /// A member of the listed names, compared case-sensitively.
    Enum(BTreeSet<String>),
}

/// The declared scalar inputs and collections of a Compiled_Rulebook.
///
/// Names are unique. A repeated name keeps its first declaration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputSchema {
    inputs: BTreeMap<String, FieldType>,
    collections: BTreeMap<String, BTreeMap<String, FieldType>>,
}

impl InputSchema {
    /// Builds a schema from scalar inputs and from collections with their
    /// element fields.
    #[must_use]
    pub fn new(
        inputs: impl IntoIterator<Item = (String, FieldType)>,
        collections: impl IntoIterator<Item = (String, Vec<(String, FieldType)>)>,
    ) -> Self {
        let mut out = Self::default();
        for (name, ty) in inputs {
            out.inputs.entry(name).or_insert(ty);
        }
        for (name, fields) in collections {
            let slot = out.collections.entry(name).or_default();
            for (field, ty) in fields {
                slot.entry(field).or_insert(ty);
            }
        }
        out
    }

    /// The scalar inputs by name.
    #[must_use]
    pub fn inputs(&self) -> &BTreeMap<String, FieldType> {
        &self.inputs
    }

    /// The collections by name, each with its element fields by name.
    #[must_use]
    pub fn collections(&self) -> &BTreeMap<String, BTreeMap<String, FieldType>> {
        &self.collections
    }
}

/// The Rule kind of a declared Target, as far as an application layer needs
/// it to read a published value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKind {
    /// A boolean eligibility Target.
    Eligibility,
    /// A derived numeric Target.
    Derived,
    /// A weighted score Target.
    Score,
    /// A ranking Target.
    Ranking,
    /// A declared Target whose kind an application cannot report.
    Other,
}

/// The declared Targets of a Compiled_Rulebook by name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TargetKinds(BTreeMap<TargetName, TargetKind>);

impl TargetKinds {
    /// Builds the map. A repeated name keeps its first kind.
    #[must_use]
    pub fn new(targets: impl IntoIterator<Item = (TargetName, TargetKind)>) -> Self {
        let mut map = BTreeMap::new();
        for (name, kind) in targets {
            map.entry(name).or_insert(kind);
        }
        Self(map)
    }

    /// The kind of a declared Target.
    #[must_use]
    pub fn get(&self, name: &TargetName) -> Option<TargetKind> {
        self.0.get(name).copied()
    }
}

/// Options of the participants mapping.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MappingOptions {
    /// Header names that are neither `participant` nor a declared input and
    /// are skipped instead of being an error (`--ignore-column`).
    pub ignore_columns: BTreeSet<String>,
}

/// Why a published row is listed as unused. Both reasons mean that the
/// Target is declared and was not requested; the row is not an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnusedReason {
    /// The Target is declared with a reportable kind and was not requested.
    NotRequested,
    /// The Target is declared with a kind an application cannot report
    /// ([`TargetKind::Other`]) and was not requested. Its value cell is not
    /// type-checked, because no value type is known for it.
    NotRequestedKindNotReportable,
}

/// A published row for a declared Target that was not requested. It is not an
/// error and is not part of the Published_Result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnusedPublished {
    /// The participant of the row.
    pub participant: ParticipantId,
    /// The Target of the row.
    pub target: TargetName,
    /// 1-based line of the row.
    pub line: u32,
    /// 1-based column of the first cell of the row.
    pub column: u32,
    /// Why the row is unused.
    pub reason: UnusedReason,
}

/// The result of mapping a published file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappedPublished {
    /// The rows for requested Targets.
    pub result: PublishedResult,
    /// The rows for other declared Targets, in file order.
    pub unused: Vec<UnusedPublished>,
}
