//! `ruletrace ingest`: stage 1, and stage 2 only with `--candidates` (Req 17.1).

use crate::args::IngestArgs;
use crate::io::{commit_files, input_location, Fs};
use rt_foundation::{Code, Diagnostic, DiagnosticState, MAX_SOURCE_DOCUMENT_BYTES};
use rt_source::{extract, ingest, ExtractionMode};
use rt_srcmodel::print_candidates;
use std::io::ErrorKind;
use std::path::Path;

/// Runs the command.
///
/// # Errors
/// The Diagnostics of the earliest failing step. Nothing is written in that case.
pub(crate) fn run(args: &IngestArgs, fs: &dyn Fs) -> Result<DiagnosticState, DiagnosticState> {
    let limit = args.max_bytes.unwrap_or(MAX_SOURCE_DOCUMENT_BYTES);
    // Step 1: read the input. One byte more than the limit is enough to see that it is larger.
    let bytes = read_document(fs, &args.document, limit).map_err(|d| single(*d))?;
    // Step 2: stage 1, ingestion.
    let model = ingest(&bytes, &args.document_id, args.format, limit).map_err(|d| single(*d))?;
    let mut warnings = DiagnosticState::empty();
    let mut outputs: Vec<(&Path, Vec<u8>)> = vec![(&args.out, model.print().into_bytes())];
    // Step 3: stage 2, extraction, only when candidates are requested.
    if let Some(path) = &args.candidates {
        let staged = extract(&model, ExtractionMode::first_version())
            .map_err(rt_foundation::Rejected::into_diagnostics)?;
        let (clauses, extraction_warnings) = staged.into_parts();
        warnings.extend(extraction_warnings);
        let text = print_candidates(model.document(), &clauses);
        outputs.push((path, text.into_bytes()));
    }
    // Step 4: write all outputs, or none.
    let files: Vec<(&Path, &[u8])> = outputs.iter().map(|(p, b)| (*p, b.as_slice())).collect();
    commit_files(fs, &files).map_err(|d| single(*d))?;
    Ok(warnings)
}

fn single(d: Diagnostic) -> DiagnosticState {
    let mut state = DiagnosticState::empty();
    state.push(d);
    state
}

fn read_document(fs: &dyn Fs, path: &Path, limit: u64) -> Result<Vec<u8>, Box<Diagnostic>> {
    let ask = limit.min(MAX_SOURCE_DOCUMENT_BYTES).saturating_add(1);
    fs.read(path, ask).map_err(|e| {
        let what = if e.kind() == ErrorKind::NotFound {
            "does not exist".to_owned()
        } else {
            format!("cannot be read: {e}")
        };
        Box::new(Diagnostic::new(
            Code::CliInputNotFound,
            format!(
                "input file '{}' {what}",
                rt_foundation::escape_controls(&path.display().to_string())
            ),
            input_location(path),
        ))
    })
}
