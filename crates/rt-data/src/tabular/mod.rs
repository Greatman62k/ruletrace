//! `rt-data::tabular`: the CSV parser and the schema-driven mapping of the
//! Application_Input_Format to Competition_Data and Published_Result
//! (design §8.3, §8.4).
//!
//! The crate does no file access and calls no engine. An application builds
//! an [`InputSchema`] and [`TargetKinds`] from its Compiled_Rulebook and passes
//! them in. Every function returns the diagnostics of one file; the
//! application merges the files and sorts the result.

mod cells;
mod collections;
mod csv;
mod header;
mod participants;
mod published;
mod report;
mod reserved;
mod schema;

#[cfg(test)]
mod tests_csv;
#[cfg(test)]
mod tests_map;
#[cfg(test)]
mod tests_property;

pub use collections::map_collections;
pub use csv::{parse_csv, ByteLimit, Cell, Table, MAX_DATA_ROWS};
pub use participants::map_participants;
pub use published::map_published;
pub use schema::{
    FieldType, InputSchema, MappedPublished, MappingOptions, TargetKind, TargetKinds,
    UnusedPublished,
};

use crate::{
    serialize_data, serialize_published, serialize_questions, CompetitionData, PublishedResult,
    Question,
};

/// The canonical RT-JSON bytes of Competition_Data, from the writer
/// `ruletrace` uses.
#[must_use]
pub fn data_bytes(data: &CompetitionData) -> Vec<u8> {
    serialize_data(data)
}

/// The canonical RT-JSON bytes of a Question list, from the writer
/// `ruletrace` uses.
#[must_use]
pub fn questions_bytes(questions: &[Question]) -> Vec<u8> {
    serialize_questions(questions)
}

/// The canonical RT-JSON bytes of a Published_Result, from the writer
/// `ruletrace` uses.
#[must_use]
pub fn published_bytes(published: &PublishedResult) -> Vec<u8> {
    serialize_published(published)
}
