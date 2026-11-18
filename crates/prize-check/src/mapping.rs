//! Prepare, part 1: what the mappers of `rt-data::tabular` need, built from
//! the Compiled_Rulebook. The mapping of the three input files is
//! `rt_data::tabular::map_files`.
//!
//! The schema and Target kinds come from `rt_data::tabular`
//! functions; the mapping errors of Req 31.9 pass through unchanged.

pub use rt_data::tabular::{FileInputs, NamedFile};
use rt_foundation::{Code, Diagnostic, Location};

pub(crate) fn internal(message: String) -> Diagnostic {
    Diagnostic::new(
        Code::InternalError,
        message,
        Location::None(Code::InternalError),
    )
}
