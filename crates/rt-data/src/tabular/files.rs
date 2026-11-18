//! Mapping of the three input files of a Workflow_Module run: the
//! participants file, the optional collections file and the optional
//! published-result file.

use super::{
    map_collections, map_participants, map_published, parse_csv, ByteLimit, InputSchema,
    MappedPublished, MappingOptions, Table, TargetKinds,
};
use crate::{CompetitionData, TargetName};
use rt_foundation::{DiagnosticState, InputId};
use std::collections::BTreeSet;

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
#[derive(Clone, Debug)]
pub struct MappedFiles {
    /// The Competition_Data of the participants and collections files.
    pub data: CompetitionData,
    /// The mapped published-result file, when one was given.
    pub published: Option<MappedPublished>,
}

fn parse(file: NamedFile<'_>) -> Result<Table, DiagnosticState> {
    parse_csv(file.bytes, file.name, ByteLimit::DEFAULT)
}

/// Maps the files. Every fault of every file that can be checked is
/// collected into `problems`.
///
/// The collections and the published file name participants of the
/// participants file, so they are mapped only when that file mapped. Their
/// CSV syntax is checked either way.
///
/// Returns `None` when any file had a fault.
pub fn map_files(
    schema: &InputSchema,
    kinds: &TargetKinds,
    requested: &BTreeSet<TargetName>,
    files: &FileInputs<'_>,
    problems: &mut DiagnosticState,
) -> Option<MappedFiles> {
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
    ok.then_some(MappedFiles { data, published })
}
