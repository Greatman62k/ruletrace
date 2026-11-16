//! `rt-xtask export-script` (task 10.6, design §8.11.7, UD-36).
//!
//! Reads every batch of a Proposal_Store, exports them with
//! `rt_proposals::export_script` and writes the Recorded_Provider_Script to
//! a new file. It reads no environment value, writes nothing else, never
//! overwrites a file, and never touches a Rulebook or an Approval_Ledger.

use rt_foundation::InputId;
use rt_proposals::{export_script, parse_batch, NamedBatch, ScriptHeader};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The parsed arguments of `export-script`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportArgs {
    /// The Proposal_Store directory.
    pub store: PathBuf,
    /// The header of the script.
    pub header: ScriptHeader,
    /// The script file to create.
    pub out: PathBuf,
}

/// The usage line of the command.
pub const USAGE: &str = "usage: rt-xtask [--root <dir>] export-script [--root <dir>] --store <dir> --provider <kind> [--provider-id <id>] --model <id> --captured \"<text>\" --out <file>";

/// Parses the arguments that follow `export-script`.
///
/// # Errors
/// A message for an unknown, repeated or missing option or a missing value.
pub fn parse_args(args: &[String]) -> Result<ExportArgs, String> {
    let names = [
        "--root",
        "--store",
        "--provider",
        "--provider-id",
        "--model",
        "--captured",
        "--out",
    ];
    let mut given: std::collections::BTreeMap<&str, String> = std::collections::BTreeMap::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let Some(name) = names.iter().find(|n| **n == arg.as_str()) else {
            return Err(format!("unknown argument `{arg}`\n{USAGE}"));
        };
        let value = iter
            .next()
            .ok_or_else(|| format!("{name} needs a value\n{USAGE}"))?;
        // `--root` is part of the common usage line. Every path of this
        // command is explicit, so the value is not used.
        if given.insert(name, value.clone()).is_some() {
            return Err(format!("{name} is given twice\n{USAGE}"));
        }
    }
    let mut take = |name: &str| {
        given
            .remove(name)
            .ok_or_else(|| format!("{name} is required\n{USAGE}"))
    };
    let store = PathBuf::from(take("--store")?);
    let kind = take("--provider")?;
    let model = take("--model")?;
    let captured = take("--captured")?;
    let out = PathBuf::from(take("--out")?);
    let provider_id = given.remove("--provider-id");
    for (name, value) in [
        ("--provider", &kind),
        ("--model", &model),
        ("--captured", &captured),
    ] {
        if value.trim().is_empty() {
            return Err(format!("{name} must not be empty"));
        }
    }
    Ok(ExportArgs {
        store,
        header: ScriptHeader {
            kind,
            provider_id,
            model,
            captured,
        },
        out,
    })
}

fn is_batch_name(name: &str) -> bool {
    name.strip_suffix(".json").is_some_and(|stem| {
        stem.len() == 64
            && stem
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Reads every batch file of `<store>/batches`, in file name order.
///
/// # Errors
/// A message naming the file (and the Request_Identifier its name gives)
/// for a file that is not a batch name, cannot be read, or does not parse.
pub fn read_store(store: &Path) -> Result<Vec<NamedBatch>, String> {
    let dir = store.join("batches");
    let listing =
        std::fs::read_dir(&dir).map_err(|e| format!("{}: cannot be read: {e}", dir.display()))?;
    let mut paths = Vec::new();
    for item in listing {
        let item = item.map_err(|e| format!("{}: cannot be read: {e}", dir.display()))?;
        paths.push(item.path());
    }
    paths.sort();
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let shown = format!("batches/{name}");
        if !is_batch_name(&name) {
            return Err(format!(
                "{shown}: not a batch file name (<request_id>.json)"
            ));
        }
        let request_id = name.trim_end_matches(".json");
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("{shown}: request {request_id}: cannot be read: {e}"))?;
        let input = InputId::new(shown.as_str())
            .map_err(|_| format!("{shown}: not a usable input name"))?;
        let batch = parse_batch(&input, &bytes).map_err(|found| {
            let messages: Vec<String> = found
                .iter()
                .map(|d| format!("{} {}", d.code().id(), d.message()))
                .collect();
            format!(
                "{shown}: request {request_id}: not a valid batch: {}",
                messages.join("; ")
            )
        })?;
        if batch.request_id.as_str() != request_id {
            return Err(format!(
                "{shown}: request {request_id}: the file holds request {}",
                batch.request_id
            ));
        }
        out.push(NamedBatch { file: shown, batch });
    }
    Ok(out)
}

/// Runs the export and writes the script to a file that must not exist.
///
/// # Errors
/// A message for a store that cannot be read, an export refusal, an output
/// file that exists already, or a write failure.
pub fn run(args: &ExportArgs) -> Result<String, String> {
    let batches = read_store(&args.store)?;
    let bytes = export_script(&args.header, &batches).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.out)
        .map_err(|e| format!("{}: cannot be created: {e}", args.out.display()))?;
    // A partly written script must not stay behind and look complete.
    if let Err(e) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(&args.out);
        return Err(format!("{}: cannot be written: {e}", args.out.display()));
    }
    Ok(format!(
        "export-script: ok ({} entries, {} bytes) -> {}",
        batches.len(),
        bytes.len(),
        args.out.display()
    ))
}
