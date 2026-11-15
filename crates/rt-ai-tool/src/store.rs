//! The Proposal_Store writer (design 3.7.5; Req 29.10, 29.11, 29.19).
//!
//! A batch file appears at its final name only through a hard link to a
//! temporary file that was written and synced in full, so the final name
//! never holds a partial batch. The writer fails closed: it has no
//! fallback for a file system that cannot link.

use crate::accept::AcceptedBatch;
use crate::fs::StoreFs;
use rt_foundation::{escape_controls, Code, Diagnostic, InputId, InputLocation, Location};
use rt_hash::Sha256Hex;
use rt_ir::Identity;
use rt_proposals::{print_batch, Batch, StoredResponse};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

/// Temporary names tried for one batch: `<id>.tmp`, then `<id>.<n>.tmp`.
/// A file left by an interrupted run is never reused or removed, because
/// a live run may own it.
const TEMP_SLOTS: u32 = 8;

/// What a batch file records besides the accepted response. Every value is
/// supplied by the caller; the writer reads no clock and no environment.
#[derive(Clone, Debug)]
pub struct StoreMeta {
    /// Provider kind from the configuration.
    pub provider: String,
    /// Model name from the configuration.
    pub model: String,
    /// Version of the adapter that made the request.
    pub adapter_version: String,
    /// The AI identities of the configuration.
    pub ai_identities: Vec<Identity>,
    /// Caller-supplied creation time, stored verbatim.
    pub created_at: String,
}

/// How a call of [`store_batch`] ended without a failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreOutcome {
    /// This call created the batch file.
    Stored,
    /// A batch with this Request_Identifier already existed, or a
    /// concurrent run created it first. Nothing was changed. The value is
    /// the Warning `RT-AI-0301`.
    AlreadyStored(Diagnostic),
}

/// The result of a call of [`store_batch`] that did not fail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreReport {
    /// How the call ended.
    pub outcome: StoreOutcome,
    /// Warnings `RT-AI-0307` for temporary files that could not be
    /// removed.
    pub warnings: Vec<Diagnostic>,
}

/// A refused write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreFailure {
    /// The Error `RT-AI-0303`.
    pub error: Diagnostic,
    /// Warnings `RT-AI-0307` that arose while the write was undone.
    pub warnings: Vec<Diagnostic>,
    /// True only when the complete batch file stays at its final name
    /// because the directory sync after the link failed. The batch is
    /// then stored, and its durability is not confirmed.
    pub kept: bool,
}

impl StoreFailure {
    fn new(error: Diagnostic) -> Self {
        Self {
            error,
            warnings: Vec::new(),
            kept: false,
        }
    }
}

/// The location of the batch file of `request_id` under `root`.
#[must_use]
pub fn batch_path(root: &Path, request_id: &Sha256Hex) -> PathBuf {
    root.join("batches")
        .join(format!("{}.json", request_id.as_str()))
}

fn at(path: &Path) -> Location {
    let text = path.to_string_lossy();
    match InputId::new(escape_controls(&text)) {
        Ok(input) => Location::Input(InputLocation {
            input,
            line: None,
            column: None,
            field: None,
        }),
        Err(_) => Location::None(Code::AiStoreWriteFailed),
    }
}

fn failed(path: &Path, step: &str, error: &io::Error) -> Diagnostic {
    Diagnostic::new(
        Code::AiStoreWriteFailed,
        format!(
            "the batch was not stored: {step} failed ({:?})",
            error.kind()
        ),
        at(path),
    )
}

fn already_stored(path: &Path) -> Diagnostic {
    Diagnostic::new(
        Code::AiBatchAlreadyStored,
        "a batch with this request identifier is already stored".to_owned(),
        at(path),
    )
}

/// Whether the batch of `request_id` is stored. The call loop asks before
/// it makes a provider call, so a stored batch costs no request.
///
/// # Errors
/// `RT-AI-0303` when the store cannot be inspected.
pub fn batch_exists(
    fs: &dyn StoreFs,
    root: &Path,
    request_id: &Sha256Hex,
) -> Result<bool, Diagnostic> {
    let path = batch_path(root, request_id);
    fs.exists(&path)
        .map_err(|e| failed(&path, "the existence check", &e))
}

/// The Warning `RT-AI-0301` for a batch that is already stored.
#[must_use]
pub fn already_stored_warning(root: &Path, request_id: &Sha256Hex) -> Diagnostic {
    already_stored(&batch_path(root, request_id))
}

fn batch_of(accepted: &AcceptedBatch, meta: &StoreMeta) -> Batch {
    Batch {
        request_id: accepted.request_id().clone(),
        response_sha256: accepted.response_sha256().clone(),
        response: StoredResponse {
            completion: accepted.completion(),
            payload: accepted.payload().to_owned(),
        },
        provider: meta.provider.clone(),
        model: meta.model.clone(),
        adapter_version: meta.adapter_version.clone(),
        ai_identities: meta.ai_identities.clone(),
        created_at: meta.created_at.clone(),
        proposals: accepted.proposals().to_vec(),
    }
}

fn temp_path(root: &Path, request_id: &Sha256Hex, slot: u32) -> PathBuf {
    let name = if slot == 0 {
        format!("{}.tmp", request_id.as_str())
    } else {
        format!("{}.{slot}.tmp", request_id.as_str())
    };
    root.join(".tmp").join(name)
}

// Removes a temporary file this call created. A removal that fails is a
// Warning, because the temporary file is never read.
fn remove_temp(fs: &dyn StoreFs, temp: &Path) -> Option<Diagnostic> {
    match fs.remove_file(temp) {
        Ok(()) => None,
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => Some(Diagnostic::new(
            Code::AiStoreTempNotRemoved,
            format!(
                "the temporary file could not be removed ({:?}); \
                 it is never read and can be deleted",
                e.kind()
            ),
            at(temp),
        )),
    }
}

fn refuse(fs: &dyn StoreFs, temp: &Path, error: Diagnostic) -> StoreFailure {
    StoreFailure {
        error,
        warnings: remove_temp(fs, temp).into_iter().collect(),
        kept: false,
    }
}

// Creates the first free temporary file. `create_new` is atomic, so two
// runs never share one. When every name is taken the message lists them,
// so the operator can delete the stale ones.
fn open_temp(
    fs: &dyn StoreFs,
    root: &Path,
    request_id: &Sha256Hex,
    final_path: &Path,
) -> Result<(PathBuf, Box<dyn crate::fs::StoreFile>), StoreFailure> {
    let mut taken: Vec<String> = Vec::new();
    for slot in 0..TEMP_SLOTS {
        let path = temp_path(root, request_id, slot);
        match fs.create_new(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                taken.push(escape_controls(&path.to_string_lossy()));
            }
            Err(e) => {
                return Err(StoreFailure::new(failed(
                    final_path,
                    "creating the temporary file",
                    &e,
                )))
            }
        }
    }
    let d = failed(
        final_path,
        "finding a free temporary file name",
        &io::Error::from(ErrorKind::AlreadyExists),
    );
    Err(StoreFailure::new(Diagnostic::new(
        d.code(),
        format!(
            "{}; if no other run is active, delete these stale temporary \
             files: {}",
            d.message(),
            taken.join(", ")
        ),
        d.primary().clone(),
    )))
}

/// Stores one accepted batch.
///
/// The steps are the idempotency check, the temporary file written in full
/// and synced, the hard link to the final name, the directory sync and the
/// removal of the temporary file. Nothing else can create a batch file.
///
/// A hard link that is unsupported, or that fails for any reason other
/// than an existing target, refuses the write. The writer never copies,
/// renames or creates a marker file instead.
///
/// A temporary file that cannot be removed gives the Warning `RT-AI-0307`
/// and never changes the outcome.
///
/// # Errors
/// `RT-AI-0303` (exit 70) when any step fails. No partial file remains at
/// the final name, and the temporary file is removed. The one exception
/// is a failed directory sync after the link: the complete batch file
/// stays and [`StoreFailure::kept`] is true.
pub fn store_batch(
    fs: &dyn StoreFs,
    root: &Path,
    accepted: &AcceptedBatch,
    meta: &StoreMeta,
) -> Result<StoreReport, StoreFailure> {
    let request_id = accepted.request_id();
    let final_path = batch_path(root, request_id);
    let done = |outcome, warnings| Ok(StoreReport { outcome, warnings });
    if batch_exists(fs, root, request_id).map_err(StoreFailure::new)? {
        return done(
            StoreOutcome::AlreadyStored(already_stored(&final_path)),
            Vec::new(),
        );
    }
    let batches = root.join("batches");
    for dir in [&batches, &root.join(".tmp")] {
        fs.create_dir_all(dir).map_err(|e| {
            StoreFailure::new(failed(&final_path, "creating the store directories", &e))
        })?;
    }
    let bytes = print_batch(&batch_of(accepted, meta));
    let (temp, mut file) = open_temp(fs, root, request_id, &final_path)?;
    if let Err(e) = file
        .write_all(bytes.as_bytes())
        .and_then(|()| file.sync_all())
    {
        return Err(refuse(
            fs,
            &temp,
            failed(&final_path, "writing the temporary file", &e),
        ));
    }
    drop(file);
    match fs.hard_link(&temp, &final_path) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            return done(
                StoreOutcome::AlreadyStored(already_stored(&final_path)),
                remove_temp(fs, &temp).into_iter().collect(),
            );
        }
        Err(e) => {
            return Err(refuse(
                fs,
                &temp,
                failed(&final_path, "linking the batch file", &e),
            ));
        }
    }
    // The link made a whole file visible, and a concurrent run may already
    // have reported it as stored, so a later failure never removes it. The
    // message of that failure says that the store is unchanged only when
    // the file is absent, so it names the durability step instead.
    let synced = fs.sync_dir(&batches);
    let warnings: Vec<Diagnostic> = remove_temp(fs, &temp).into_iter().collect();
    match synced {
        Ok(()) => done(StoreOutcome::Stored, warnings),
        Err(e) => {
            let d = failed(&final_path, "making the batch durable", &e);
            Err(StoreFailure {
                error: Diagnostic::new(
                    d.code(),
                    format!("{}; the complete batch file stays in place", d.message()),
                    d.primary().clone(),
                ),
                warnings,
                kept: true,
            })
        }
    }
}
