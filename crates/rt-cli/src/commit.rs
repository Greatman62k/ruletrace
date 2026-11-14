//! The journaled two-file commit and its recovery (design §3.8.3 steps 5 to 8;
//! Req 26.19, 26.30, 26.31, 25.1, 23.13).
//!
//! An adoption changes the Rulebook and the ledger together. The new bytes of
//! both are written to `.new` files, the current files are kept as `.old`
//! backups, and a journal naming all of it is published with one rename. From
//! that rename on the decision is durable. The ledger is renamed first and a
//! rollback restores the Rulebook first, so the only intermediate pair, in
//! either direction, is a new ledger with an old Rulebook. That pair is
//! physically inconsistent. It is never consumed: while the journal exists,
//! every reader refuses (`RT-APR-0214`) and only `review approve` or
//! `review reject` read the files, after recovery has made them one pair.
//!
//! The journal is untrusted input like any other file: recovery accepts it
//! only when every path in it is the one the command line names, and every
//! temp and backup name is the derived one.

use crate::io::{input_location, sibling, Fs};
use crate::validate::input_id;
use rt_foundation::{escape_controls, Code, Diagnostic, MAX_LEDGER_BYTES, MAX_RULEBOOK_BYTES};
use rt_hash::{sha256, Sha256Hex};
use rt_json::{parse, write_compact, ParseLimits, Value};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

const JOURNAL_FORMAT: &str = "ruletrace-commit-journal";
const JOURNAL_VERSION: u64 = 1;
const MAX_JOURNAL_BYTES: u64 = 1 << 16;
const JOURNAL_DEPTH: u32 = 8;

/// How a finished commit left the journal.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Finish {
    /// Both files carry the new state and no commit file remains.
    Clean,
    /// Both files carry the new state, but a cleanup step failed and the
    /// journal remains. The next review command finishes the cleanup.
    JournalRetained,
}

/// What recovery found and did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Recovered {
    /// No journal exists.
    NoJournal,
    /// Both files were new: only the leftover files were removed.
    CleanedUp,
    /// The Rulebook rename was completed.
    RolledForward,
    /// Both files were old: the unfinished commit was dropped.
    RolledBack,
}

/// The path of the journal of a ledger.
pub(crate) fn journal_path(ledger: &Path) -> PathBuf {
    sibling(ledger, ".journal")
}

struct Entry {
    path: PathBuf,
    old: Option<Sha256Hex>,
    new: Sha256Hex,
}

impl Entry {
    fn temp(&self) -> PathBuf {
        sibling(&self.path, ".new")
    }

    fn backup(&self) -> PathBuf {
        sibling(&self.path, ".old")
    }
}

struct Journal {
    ledger: Entry,
    rulebook: Entry,
}

impl Journal {
    fn entries(&self) -> [&Entry; 2] {
        [&self.ledger, &self.rulebook]
    }
}

fn failure(path: &Path, message: String) -> Box<Diagnostic> {
    Box::new(Diagnostic::new(
        Code::ReviewWriteFailed,
        message,
        input_location(path),
    ))
}

fn shown(path: &Path) -> String {
    escape_controls(&path.display().to_string())
}

fn dir_of(path: &Path) -> PathBuf {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

fn hash_of(fs: &dyn Fs, path: &Path) -> io::Result<Option<Sha256Hex>> {
    match fs.read(path, u64::MAX) {
        Ok(bytes) => Ok(Some(sha256(&bytes))),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn remove_if_present(fs: &dyn Fs, path: &Path) -> io::Result<()> {
    match fs.remove(path) {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn sync_dirs(fs: &dyn Fs, paths: &[&Path]) -> io::Result<()> {
    let mut done: Vec<PathBuf> = Vec::new();
    for path in paths {
        let dir = dir_of(path);
        if !done.contains(&dir) {
            fs.sync_dir(&dir)?;
            done.push(dir);
        }
    }
    Ok(())
}

fn encode(journal: &Journal) -> Option<Vec<u8>> {
    let mut files = Vec::new();
    for (role, entry) in ["ledger", "rulebook"].into_iter().zip(journal.entries()) {
        let optional = |text: Option<String>| text.map_or_else(Value::null, Value::string);
        let object = Value::object(vec![
            ("role".to_owned(), Value::string(role)),
            (
                "path".to_owned(),
                Value::string(entry.path.to_str()?.to_owned()),
            ),
            (
                "old_sha256".to_owned(),
                optional(entry.old.as_ref().map(|h| h.as_str().to_owned())),
            ),
            (
                "new_sha256".to_owned(),
                Value::string(entry.new.as_str().to_owned()),
            ),
            (
                "temp".to_owned(),
                Value::string(entry.temp().to_str()?.to_owned()),
            ),
            (
                "backup".to_owned(),
                optional(
                    entry
                        .old
                        .as_ref()
                        .and_then(|_| entry.backup().to_str().map(str::to_owned)),
                ),
            ),
        ])
        .ok()?;
        files.push(object);
    }
    let root = Value::object(vec![
        ("format".to_owned(), Value::string(JOURNAL_FORMAT)),
        ("version".to_owned(), Value::integer(1)),
        ("files".to_owned(), Value::array(files)),
    ])
    .ok()?;
    let mut text = write_compact(&root);
    text.push('\n');
    Some(text.into_bytes())
}

/// Decodes a journal, requiring that it names exactly the given files.
fn decode(bytes: &[u8], journal_file: &Path, ledger: &Path, rulebook: &Path) -> Option<Journal> {
    let input = input_id(journal_file).ok()?;
    let root = parse(
        &input,
        bytes,
        ParseLimits::new(MAX_JOURNAL_BYTES, JOURNAL_DEPTH),
    )
    .ok()?;
    if root.as_object()?.len() != 3
        || root.get("format")?.value.as_str()? != JOURNAL_FORMAT
        || root.get("version")?.value.as_u64()? != JOURNAL_VERSION
    {
        return None;
    }
    let files = root.get("files")?.value.as_array()?;
    let [first, second] = files else {
        return None;
    };
    let entry = |value: &Value<rt_json::Position>, role: &str, expected: &Path| -> Option<Entry> {
        if value.as_object()?.len() != 6 || value.get("role")?.value.as_str()? != role {
            return None;
        }
        if value.get("path")?.value.as_str()? != expected.to_str()? {
            return None;
        }
        let old = match value.get("old_sha256")?.value.as_str() {
            Some(text) => Some(Sha256Hex::parse(text)?),
            None => match value.get("old_sha256")?.value.kind() {
                rt_json::Kind::Null => None,
                _ => return None,
            },
        };
        let new = Sha256Hex::parse(value.get("new_sha256")?.value.as_str()?)?;
        let entry = Entry {
            path: expected.to_path_buf(),
            old,
            new,
        };
        if value.get("temp")?.value.as_str()? != entry.temp().to_str()? {
            return None;
        }
        let backup = value.get("backup")?;
        let backup_ok = match (&entry.old, backup.value.as_str()) {
            (Some(_), Some(text)) => text == entry.backup().to_str()?,
            (None, None) => matches!(backup.value.kind(), rt_json::Kind::Null),
            _ => false,
        };
        backup_ok.then_some(entry)
    };
    Some(Journal {
        ledger: entry(first, "ledger", ledger)?,
        rulebook: entry(second, "rulebook", rulebook)?,
    })
}

/// Removes the temp and backup files, then the journal. The journal goes last
/// so that an interrupted cleanup can be repeated.
fn cleanup(fs: &dyn Fs, journal: &Journal, journal_file: &Path) -> io::Result<()> {
    for entry in journal.entries() {
        remove_if_present(fs, &entry.temp())?;
        remove_if_present(fs, &entry.backup())?;
    }
    remove_if_present(fs, &sibling(journal_file, ".tmp"))?;
    sync_dirs(fs, &[&journal.ledger.path, &journal.rulebook.path])?;
    remove_if_present(fs, journal_file)?;
    sync_dirs(fs, &[&journal.ledger.path, &journal.rulebook.path])
}

/// Puts every replaced target back from its backup, then cleans up. The
/// decision is read from the actual file contents, so it can be repeated after
/// a partial rollback.
///
/// The Rulebook is restored first, the reverse of the forward order. A Rulebook
/// that holds the adopted revision therefore never meets a ledger without its
/// record (Req 26.30). An interruption between the two restores leaves the new
/// ledger with the old Rulebook, which `recover` finishes.
fn rollback(fs: &dyn Fs, journal: &Journal, journal_file: &Path) -> io::Result<()> {
    for entry in [&journal.rulebook, &journal.ledger] {
        let current = hash_of(fs, &entry.path)?;
        if current.as_ref() == Some(&entry.new) {
            match &entry.old {
                Some(_) => fs.rename(&entry.backup(), &entry.path)?,
                None => fs.remove(&entry.path)?,
            }
        } else if current != entry.old {
            return Err(io::Error::other(format!(
                "'{}' is in an unexpected state",
                shown(&entry.path)
            )));
        }
    }
    cleanup(fs, journal, journal_file)
}

/// Refuses when a name the commit uses already exists and no earlier attempt
/// of this commit accounts for it, so a file of the user is never deleted.
///
/// An earlier attempt is recognised by its marker, the complete journal text
/// that `prepare` writes to `<ledger>.journal.tmp` before any other file.
fn claim(
    fs: &dyn Fs,
    journal: &Journal,
    journal_file: &Path,
    ledger: &Path,
    rulebook: &Path,
) -> Result<(), Box<Diagnostic>> {
    let marker = sibling(journal_file, ".tmp");
    let mut names: Vec<PathBuf> = Vec::new();
    for entry in journal.entries() {
        names.push(entry.temp());
        names.push(entry.backup());
    }
    names.push(marker.clone());
    let mut existing: Vec<PathBuf> = Vec::new();
    for name in names {
        // A directory entry counts even when it is a dangling link, so that a
        // file of the user is never deleted.
        match fs.exists(&name) {
            Ok(true) => existing.push(name),
            Ok(false) => {}
            Err(e) => {
                return Err(failure(
                    &name,
                    format!("cannot inspect '{}': {e}; no file changed", shown(&name)),
                ))
            }
        }
    }
    let Some(first) = existing.first() else {
        return Ok(());
    };
    let ours = fs
        .read(&marker, MAX_JOURNAL_BYTES + 1)
        .ok()
        .and_then(|bytes| decode(&bytes, journal_file, ledger, rulebook))
        .is_some();
    if !ours {
        return Err(failure(
            first,
            format!(
                "'{}' already exists and no commit journal accounts for it; move or remove it and retry; no file changed",
                shown(first)
            ),
        ));
    }
    for name in &existing {
        remove_if_present(fs, name).map_err(|e| {
            failure(
                name,
                format!(
                    "cannot remove the leftover '{}' of an earlier attempt: {e}",
                    shown(name)
                ),
            )
        })?;
    }
    Ok(())
}

fn prepare(
    fs: &dyn Fs,
    journal: &Journal,
    files: [&[u8]; 2],
    journal_bytes: &[u8],
    journal_file: &Path,
) -> io::Result<()> {
    // The marker comes first: if this attempt dies, its leftovers stay
    // recognisable as its own.
    fs.create_new(&sibling(journal_file, ".tmp"), journal_bytes)?;
    for (entry, bytes) in journal.entries().into_iter().zip(files) {
        fs.create_new(&entry.temp(), bytes)?;
    }
    for entry in journal.entries() {
        if entry.old.is_some() && fs.hard_link(&entry.path, &entry.backup()).is_err() {
            // Some file systems have no hard links; a copy is as good.
            remove_if_present(fs, &entry.backup())?;
            let old = fs.read(&entry.path, u64::MAX)?;
            fs.create_new(&entry.backup(), &old)?;
        }
    }
    sync_dirs(fs, &[&journal.ledger.path, &journal.rulebook.path])
}

/// Publishes the marker as the journal; from here the decision is durable.
fn publish(fs: &dyn Fs, journal: &Journal, journal_file: &Path) -> io::Result<()> {
    fs.rename(&sibling(journal_file, ".tmp"), journal_file)?;
    sync_dirs(fs, &[&journal.ledger.path, &journal.rulebook.path])
}

fn replace(fs: &dyn Fs, journal: &Journal) -> io::Result<()> {
    for entry in journal.entries() {
        fs.rename(&entry.temp(), &entry.path)?;
        sync_dirs(fs, &[&entry.path])?;
    }
    Ok(())
}

/// What the two files hold after a rollback that failed.
enum Rolled {
    /// Both files carry their earlier bytes; only the cleanup failed.
    Restored,
    /// Both files carry the new bytes: the decision is committed.
    Committed,
    /// Any other pair, or one that cannot be inspected.
    Intermediate,
}

fn after_failed_rollback(fs: &dyn Fs, journal: &Journal) -> Rolled {
    let (Ok(ledger), Ok(rulebook)) = (
        hash_of(fs, &journal.ledger.path),
        hash_of(fs, &journal.rulebook.path),
    ) else {
        return Rolled::Intermediate;
    };
    if ledger == journal.ledger.old && rulebook == journal.rulebook.old {
        Rolled::Restored
    } else if ledger.as_ref() == Some(&journal.ledger.new)
        && rulebook.as_ref() == Some(&journal.rulebook.new)
    {
        Rolled::Committed
    } else {
        Rolled::Intermediate
    }
}

/// The message of a commit that failed and whose rollback failed too. It
/// states which of the three cases of Req 26.19 holds and never calls an
/// intermediate pair consistent.
fn rollback_failure_message(
    fs: &dyn Fs,
    journal: &Journal,
    journal_file: &Path,
    cause: &io::Error,
    second: &io::Error,
) -> String {
    let retained = fs.exists(journal_file).unwrap_or(true);
    let named = shown(journal_file);
    let refusal = "every other command refuses to read the ledger and the Rulebook until the next 'review approve' or 'review reject'";
    match (after_failed_rollback(fs, journal), retained) {
        (Rolled::Restored, false) => format!(
            "the commit failed: {cause}; the ledger and the Rulebook are unchanged; the cleanup of leftover files failed: {second}; the next review commit removes them"
        ),
        (Rolled::Restored, true) => format!(
            "the commit failed: {cause}; the rollback restored the ledger and the Rulebook to their earlier bytes, but its cleanup failed: {second}; the journal '{named}' is retained and {refusal} removes it"
        ),
        (Rolled::Committed, _) => format!(
            "the commit failed: {cause}; the rollback failed: {second}; the decision is committed: both files carry the new state; the journal '{named}' is retained and {refusal} completes the cleanup"
        ),
        (Rolled::Intermediate, _) => format!(
            "the commit failed: {cause}; the rollback failed: {second}; the files are in an intermediate state and must not be used; the journal '{named}' is retained and {refusal} either completes the commit or undoes it"
        ),
    }
}

/// Replaces the ledger and the Rulebook together (design §3.8.3 steps 5 to 8).
/// The caller holds the lock and has recovered any earlier journal.
///
/// # Errors
/// `RT-APR-0208` when any step fails. The message states which case holds
/// (Req 26.19): the rollback succeeded and both files are byte-identical to
/// their earlier state; the rollback failed with both new files in place, so
/// the decision is committed; or the rollback failed in an intermediate state.
/// In the last two cases, and when only the rollback cleanup failed, the
/// journal is retained and every reader refuses until a review command
/// recovers.
pub(crate) fn commit_pair(
    fs: &dyn Fs,
    ledger: (&Path, &[u8]),
    rulebook: (&Path, &[u8]),
) -> Result<Finish, Box<Diagnostic>> {
    let journal_file = journal_path(ledger.0);
    let inspect = |path: &Path| {
        hash_of(fs, path).map_err(|e| {
            failure(
                path,
                format!("cannot inspect '{}': {e}; no file changed", shown(path)),
            )
        })
    };
    let ledger_old = inspect(ledger.0)?;
    let Some(rulebook_old) = inspect(rulebook.0)? else {
        return Err(failure(
            rulebook.0,
            format!("the Rulebook '{}' does not exist", shown(rulebook.0)),
        ));
    };
    let journal = Journal {
        ledger: Entry {
            path: ledger.0.to_path_buf(),
            old: ledger_old,
            new: sha256(ledger.1),
        },
        rulebook: Entry {
            path: rulebook.0.to_path_buf(),
            old: Some(rulebook_old),
            new: sha256(rulebook.1),
        },
    };
    let Some(journal_bytes) = encode(&journal) else {
        return Err(failure(
            ledger.0,
            "a file path is not valid UTF-8, so no commit journal can be written; no file changed"
                .to_owned(),
        ));
    };
    claim(fs, &journal, &journal_file, ledger.0, rulebook.0)?;
    if let Err(e) = prepare(
        fs,
        &journal,
        [ledger.1, rulebook.1],
        &journal_bytes,
        &journal_file,
    ) {
        // Nothing changed yet: only the new files and backups exist.
        let _ = cleanup(fs, &journal, &journal_file);
        return Err(failure(
            ledger.0,
            format!("cannot prepare the commit: {e}; no file changed"),
        ));
    }
    let outcome = publish(fs, &journal, &journal_file).and_then(|()| replace(fs, &journal));
    if let Err(cause) = outcome {
        return match rollback(fs, &journal, &journal_file) {
            Ok(()) => Err(failure(
                ledger.0,
                format!("the commit failed: {cause}; the rollback succeeded and the ledger and the Rulebook are unchanged"),
            )),
            Err(second) => Err(failure(
                ledger.0,
                rollback_failure_message(fs, &journal, &journal_file, &cause, &second),
            )),
        };
    }
    Ok(match cleanup(fs, &journal, &journal_file) {
        Ok(()) => Finish::Clean,
        Err(_) => Finish::JournalRetained,
    })
}

/// Finishes or drops an interrupted commit (design §3.8.3 crash recovery). The
/// caller holds the lock.
///
/// # Errors
/// `RT-APR-0208` when the journal is unreadable, does not name the given
/// files, or the files are in a state the protocol cannot produce. No file is
/// changed in the last two cases.
pub(crate) fn recover(
    fs: &dyn Fs,
    ledger: &Path,
    rulebook: &Path,
) -> Result<Recovered, Box<Diagnostic>> {
    let journal_file = journal_path(ledger);
    let bytes = match fs.read(&journal_file, MAX_JOURNAL_BYTES + 1) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Recovered::NoJournal),
        Err(e) => {
            return Err(failure(
                &journal_file,
                format!(
                    "the commit journal '{}' cannot be read: {e}; no file changed",
                    shown(&journal_file)
                ),
            ))
        }
    };
    let refuse = |why: String| failure(&journal_file, format!("{why}; no file changed"));
    let Some(journal) = decode(&bytes, &journal_file, ledger, rulebook) else {
        return Err(refuse(format!(
            "the commit journal '{}' is malformed or names other files",
            shown(&journal_file)
        )));
    };
    let current = |entry: &Entry| {
        hash_of(fs, &entry.path)
            .map_err(|e| refuse(format!("cannot inspect '{}': {e}", shown(&entry.path))))
    };
    let ledger_now = current(&journal.ledger)?;
    let rulebook_now = current(&journal.rulebook)?;
    let ledger_new = ledger_now.as_ref() == Some(&journal.ledger.new);
    let rulebook_new = rulebook_now.as_ref() == Some(&journal.rulebook.new);
    let ledger_old = ledger_now == journal.ledger.old;
    let rulebook_old = rulebook_now == journal.rulebook.old;
    let cleaned = |result: io::Result<()>, done: Recovered| {
        result.map(|()| done).map_err(|e| {
            failure(
                &journal_file,
                format!(
                    "the recovery cleanup failed: {e}; the journal '{}' is retained",
                    shown(&journal_file)
                ),
            )
        })
    };
    match (ledger_new, rulebook_new, ledger_old, rulebook_old) {
        (true, true, _, _) => cleaned(cleanup(fs, &journal, &journal_file), Recovered::CleanedUp),
        (true, false, _, true) => match hash_of(fs, &journal.rulebook.temp()) {
            // The new Rulebook file is still there: the decision is durable.
            Ok(Some(found)) if found == journal.rulebook.new => {
                let forward = fs
                    .rename(&journal.rulebook.temp(), &journal.rulebook.path)
                    .and_then(|()| sync_dirs(fs, &[&journal.rulebook.path]))
                    .and_then(|()| cleanup(fs, &journal, &journal_file));
                cleaned(forward, Recovered::RolledForward)
            }
            // The Rulebook was restored by an interrupted rollback, which
            // consumed the new file; the ledger is next.
            Ok(None) => {
                let backup = journal.ledger.backup();
                let intact = match &journal.ledger.old {
                    Some(expected) => {
                        matches!(hash_of(fs, &backup), Ok(Some(found)) if found == *expected)
                    }
                    None => true,
                };
                if !intact {
                    return Err(refuse(format!(
                        "the ledger backup '{}' is missing or does not match the journal",
                        shown(&backup)
                    )));
                }
                let back = match &journal.ledger.old {
                    Some(_) => fs.rename(&backup, &journal.ledger.path),
                    None => fs.remove(&journal.ledger.path),
                }
                .and_then(|()| sync_dirs(fs, &[&journal.ledger.path]))
                .and_then(|()| cleanup(fs, &journal, &journal_file));
                cleaned(back, Recovered::RolledBack)
            }
            _ => Err(refuse(format!(
                "the new Rulebook file '{}' cannot be read or does not match the journal",
                shown(&journal.rulebook.temp())
            ))),
        },
        (_, _, true, true) => cleaned(cleanup(fs, &journal, &journal_file), Recovered::RolledBack),
        _ => Err(refuse(
            "the ledger and the Rulebook are in a state the commit protocol cannot produce"
                .to_owned(),
        )),
    }
}

/// The `RT-APR-0214` Error for a command that reads the ledger and the
/// Rulebook but is not a recovering review command, or `None` when no journal
/// exists. A journal that cannot be inspected also blocks.
///
/// A reader that finds the journal reads nothing else and changes nothing: the
/// files may be an intermediate pair that must not be consumed (Req 26.30,
/// 26.32). This check alone cannot stop a reader that straddles a whole
/// commit; [`read_pair`] closes that window.
pub(crate) fn journal_blocks(fs: &dyn Fs, ledger: &Path) -> Option<Diagnostic> {
    let journal_file = journal_path(ledger);
    let why = match fs.exists(&journal_file) {
        Ok(false) => return None,
        Ok(true) => "an interrupted or unfinished review commit exists".to_owned(),
        Err(e) => format!("the commit journal cannot be inspected ({e})"),
    };
    Some(Diagnostic::new(
        Code::ReviewCommitJournalBlocksRead,
        format!(
            "{why} ('{}'); the ledger and the Rulebook are not read and nothing was changed; run 'review approve' or 'review reject' to recover it first",
            shown(&journal_file)
        ),
        input_location(&journal_file),
    ))
}

/// What one read of a file returned. Only the error kind of a failure is
/// compared, because that is all the loaders look at.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Seen {
    Bytes(Vec<u8>),
    Failed(ErrorKind),
}

fn seen(result: io::Result<Vec<u8>>) -> Seen {
    match result {
        Ok(bytes) => Seen::Bytes(bytes),
        Err(e) => Seen::Failed(e.kind()),
    }
}

/// The Rulebook and the ledger as one reader saw them, served to the loaders
/// in place of a further read. Every other path goes to the file system.
pub(crate) struct PairFs<'a> {
    inner: &'a dyn Fs,
    ledger: (PathBuf, Seen),
    rulebook: (PathBuf, Seen),
}

impl Fs for PairFs<'_> {
    fn read(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        let held = [&self.rulebook, &self.ledger]
            .into_iter()
            .find(|(held, _)| held == path);
        match held {
            None => self.inner.read(path, max_bytes),
            Some((_, Seen::Failed(kind))) => Err((*kind).into()),
            Some((_, Seen::Bytes(bytes))) => Ok(bytes
                .iter()
                .take(usize::try_from(max_bytes).unwrap_or(usize::MAX))
                .copied()
                .collect()),
        }
    }

    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.inner.create_new(path, bytes)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        self.inner.remove(path)
    }
}

fn read_both(fs: &dyn Fs, ledger: &Path, rulebook: &Path) -> (Seen, Seen) {
    // The Rulebook first: a new Rulebook implies that the ledger was already
    // renamed in the forward direction.
    let rulebook_seen = seen(fs.read(rulebook, MAX_RULEBOOK_BYTES.saturating_add(1)));
    let ledger_seen = seen(fs.read(ledger, MAX_LEDGER_BYTES.saturating_add(1)));
    (rulebook_seen, ledger_seen)
}

fn changed_during_read(at: &Path, why: &str) -> Box<Diagnostic> {
    Box::new(Diagnostic::new(
        Code::ReviewCommitJournalBlocksRead,
        format!(
            "{why}; the ledger and the Rulebook are not used and nothing was changed; run the command again when no review command is running"
        ),
        input_location(at),
    ))
}

/// The `RT-APR-0215` Error for a command that needs an advisory lock on `at`
/// and cannot have one. `needs` says what the lock protects.
pub(crate) fn locking_unsupported(at: &Path, needs: &str) -> Diagnostic {
    Diagnostic::new(
        Code::ReviewLockingUnsupported,
        format!(
            "the file system holding '{}' does not support advisory file locks; the operation refused because advisory locks are required {needs}, and nothing was read or changed",
            shown(at)
        ),
        input_location(at),
    )
}

const READ_NEEDS: &str = "to read the Rulebook and the ledger as one consistent pair";

/// With no lock file yet, no review command has taken the lock, so there is
/// nothing to lock. Support is probed on the Rulebook instead, so that a file
/// system without advisory locks refuses here as well. The probe lock is
/// released at once. A missing Rulebook is left to the missing-input path;
/// every other failure, including a held lock, refuses with `RT-APR-0214`.
fn probe_locking(fs: &dyn Fs, rulebook: &Path) -> Result<(), Box<Diagnostic>> {
    match fs.try_lock_shared(rulebook) {
        Err(e) if e.kind() == ErrorKind::Unsupported => {
            Err(Box::new(locking_unsupported(rulebook, READ_NEEDS)))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(changed_during_read(
            rulebook,
            &format!(
                "the lock probe on '{}' failed ({e}), so no lock could be taken; a review commit may be running",
                shown(rulebook)
            ),
        )),
        Ok(_held) => Ok(()),
    }
}

/// Reads the Rulebook and the ledger of a reader command as one pair, or
/// refuses (Req 26.30, 26.32). Nothing is changed.
///
/// The reader takes the shared lock first, so that no writer can run while it
/// reads. A file system or platform without advisory locks refuses with
/// `RT-APR-0215` before any file is read: there is no lock-file fallback.
/// With the lock held, the reader checks the journal, reads both files, checks
/// the journal again, reads both again and refuses when a journal exists at
/// either check or any byte differs between the two reads. The loaders take
/// both files from the returned value and never read them again.
///
/// # Errors
/// `RT-APR-0215` when locking is unsupported. `RT-APR-0214` for a journal, an
/// uninspectable journal, a lock held by a writer, or files that changed
/// during the read.
pub(crate) fn read_pair<'a>(
    fs: &'a dyn Fs,
    ledger: &Path,
    rulebook: &Path,
) -> Result<PairFs<'a>, Box<Diagnostic>> {
    let lock_file = sibling(ledger, ".lock");
    let (_shared, lock_absent) = match fs.try_lock_shared(&lock_file) {
        Ok(held) => (Some(held), false),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            probe_locking(fs, rulebook)?;
            (None, true)
        }
        Err(e) if e.kind() == ErrorKind::Unsupported => {
            return Err(Box::new(locking_unsupported(&lock_file, READ_NEEDS)))
        }
        Err(e) => {
            return Err(changed_during_read(
                &lock_file,
                &format!(
                    "the review lock '{}' cannot be taken for reading ({e}); a review commit may be running",
                    shown(&lock_file)
                ),
            ))
        }
    };
    if let Some(blocked) = journal_blocks(fs, ledger) {
        return Err(Box::new(blocked));
    }
    let first = read_both(fs, ledger, rulebook);
    if let Some(blocked) = journal_blocks(fs, ledger) {
        return Err(Box::new(blocked));
    }
    let second = read_both(fs, ledger, rulebook);
    let lock_appeared = lock_absent && !matches!(fs.exists(&lock_file), Ok(false));
    if lock_appeared {
        return Err(changed_during_read(
            &lock_file,
            "a review command created its lock while the files were being read",
        ));
    }
    if first != second {
        return Err(changed_during_read(
            ledger,
            "the ledger or the Rulebook changed while it was being read; a review commit is running",
        ));
    }
    Ok(PairFs {
        inner: fs,
        ledger: (ledger.to_path_buf(), second.1),
        rulebook: (rulebook.to_path_buf(), second.0),
    })
}

/// The `RT-APR-0211` Warning for a commit that completed but whose cleanup
/// failed.
pub(crate) fn retained_warning(ledger: &Path) -> Diagnostic {
    let journal_file = journal_path(ledger);
    Diagnostic::new(
        Code::ReviewCommitJournalPresent,
        format!(
            "the commit completed and both files carry the decision, but its cleanup failed and leftover files remain ('{}'); every other command refuses to read the ledger and the Rulebook until the next 'review approve' or 'review reject' removes them",
            shown(&journal_file)
        ),
        input_location(&journal_file),
    )
}

/// The Warning that reports what recovery did.
pub(crate) fn recovery_notice(ledger: &Path, how: &Recovered) -> Option<Diagnostic> {
    let what = match how {
        Recovered::NoJournal => return None,
        Recovered::CleanedUp => "the commit had completed; leftover files were removed",
        Recovered::RolledForward => "the commit was completed",
        Recovered::RolledBack => "the commit was undone and both files are as before it",
    };
    let journal_file = journal_path(ledger);
    Some(Diagnostic::new(
        Code::ReviewCommitJournalPresent,
        format!(
            "an interrupted review commit was found and recovered: {what} ('{}')",
            shown(&journal_file)
        ),
        input_location(&journal_file),
    ))
}
