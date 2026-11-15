//! The check of the existing Proposal_Store before a run (design 3.7.5;
//! Req 23.33, step 4 of Req 16.17).
//!
//! The files of `batches/` that end in `.json` are the batches. Every one
//! is read and parsed before the first request, so a store that a reviewer
//! could not load stops the run before anything is sent. Files in `.tmp/`
//! and names with another ending are not batches and are never read. A
//! batch whose file stem is not its own `request_id` is malformed too.

use crate::fs::StoreFs;
use rt_foundation::{
    escape_controls, Code, Diagnostic, InputId, InputLocation, Location, MAX_PROPOSAL_BATCH_BYTES,
};
use rt_proposals::parse_batch;
use std::io::{self, ErrorKind};
use std::path::Path;

fn input_for(path: &Path) -> Option<InputId> {
    InputId::new(escape_controls(&path.to_string_lossy())).ok()
}

fn at_file(path: &Path) -> Location {
    input_for(path).map_or(Location::None(Code::AiStoreInvalid), |input| {
        Location::Input(InputLocation {
            input,
            line: None,
            column: None,
            field: None,
        })
    })
}

fn unreadable(path: &Path, what: &str, error: &io::Error) -> Diagnostic {
    let at = at_file(path);
    Diagnostic::new(
        Code::AiStoreInvalid,
        format!(
            "the {what} of the Proposal_Store cannot be read ({:?})",
            error.kind()
        ),
        at,
    )
}

// Req 19.3, applied to a Proposal_Store file by Req 19.8: the file is
// rejected before its content is read, naming the limit and the true size.
fn oversize(input: &InputId, path: &Path, size: u64) -> Diagnostic {
    Diagnostic::new(
        Code::LimitFileTooLarge,
        format!(
            "input file '{input}' is {size} bytes, larger than the effective limit of \
             {MAX_PROPOSAL_BATCH_BYTES} bytes"
        ),
        at_file(path),
    )
}

/// Parses every batch file of the Proposal_Store under `root`.
///
/// A missing `batches/` directory is an empty store. Files are checked in
/// ascending path order and the check stops at the first malformed one.
///
/// # Errors
/// The Diagnostics of the first batch file that cannot be read or parsed:
/// `RT-LIM-0001` for a file above `MAX_PROPOSAL_BATCH_BYTES`, `RT-AI-0302`,
/// or the lifecycle flag code that the parser reports.
pub fn check_store(fs: &dyn StoreFs, root: &Path) -> Result<(), Vec<Diagnostic>> {
    let dir = root.join("batches");
    let entries = match fs.list_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(vec![unreadable(&dir, "batch directory", &e)]),
    };
    let mut batches: Vec<_> = entries
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    batches.sort();
    for path in batches {
        let Some(input) = input_for(&path) else {
            return Err(vec![Diagnostic::new(
                Code::AiStoreInvalid,
                "a batch file has a name that cannot be shown".to_owned(),
                Location::None(Code::AiStoreInvalid),
            )]);
        };
        let size = fs
            .len(&path)
            .map_err(|e| vec![unreadable(&path, "batch file", &e)])?;
        if size > MAX_PROPOSAL_BATCH_BYTES {
            return Err(vec![oversize(&input, &path, size)]);
        }
        let bytes = fs
            .read(&path, MAX_PROPOSAL_BATCH_BYTES.saturating_add(1))
            .map_err(|e| vec![unreadable(&path, "batch file", &e)])?;
        let batch = parse_batch(&input, &bytes)?;
        // A batch is found by `batches/<request_id>.json`, so a file under
        // another name would be missed by the idempotence check.
        let stem = path.file_stem().and_then(|s| s.to_str());
        if stem != Some(batch.request_id.as_str()) {
            return Err(vec![Diagnostic::new(
                Code::AiStoreInvalid,
                format!(
                    "the batch file is not named after its request_id {}",
                    batch.request_id
                ),
                at_file(&path),
            )]);
        }
    }
    Ok(())
}
