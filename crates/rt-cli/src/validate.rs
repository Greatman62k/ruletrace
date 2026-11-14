//! `ruletrace validate`: stages 3 and 4 (Req 16.2, 17.1).
//!
//! The command reads the Rulebook, the ledger, the Source_Models and the optional candidates
//! (all invalid input is exit 2), runs the validator, and writes the Validation_Result artifact.
//! The artifact rule is the RN-29 record, option (a): the artifact is also written when only
//! blocking Diagnostics of the Rulebook prevent success (exit 3). Failures that are invalid
//! input (exit 2, for example `RT-VAL-0017` and `RT-VAL-0018`) and failed writes (exit 70)
//! create no file.
//!
//! The artifact is RT-JSON, compact, with the keys in this order: `format`, `version`,
//! `executable`, `rulebook`, `ledger`, `diagnostics`, `checked_conflict_classes`,
//! `rule_metadata`. `executable` is always `false`: a Validation_Result is never a
//! Compiled_Rulebook, and `compile` re-derives its verdict from the Rulebook, the ledger and the
//! Diagnostics instead of reading a stored flag, so no validity flag or status is stored
//! (Req 22). `ledger` holds the canonical ledger text as one string.

use crate::args::ValidateArgs;
use crate::io::{commit_files, input_location, Fs};
use rt_foundation::{
    Code, Diagnostic, DiagnosticState, DocumentId, ExitCode, InputId, InputLocation, Location,
    MAX_CONTAINER_DEPTH, MAX_LEDGER_BYTES, MAX_RULEBOOK_BYTES,
};
use rt_json::{write_compact, ParseLimits, Value};
use rt_srcmodel::{
    default_load_limits, parse_candidates, CandidateClause, CandidatesError, SourceModel,
    SourceModels,
};
use rt_validate::ValidationResult;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// What the command produced: every Diagnostic and the exit code.
pub(crate) struct Done {
    pub(crate) state: DiagnosticState,
    pub(crate) code: ExitCode,
    /// Diagnostics of a stored artifact, already rendered, that follow `state` on standard
    /// error. Only `compile` has any: it passes on the Diagnostics of a refused artifact.
    pub(crate) stored: Vec<String>,
}

/// The exit code of a Diagnostic list: success without a blocking Diagnostic, else the lowest
/// catalogue exit code among the blocking ones. A lower code belongs to an earlier stage of the
/// order of Req 16.17 (invalid input before blocking results before output failures).
pub(crate) fn exit_code_of(state: &DiagnosticState) -> ExitCode {
    state
        .as_slice()
        .iter()
        .filter(|d| d.severity().is_blocking())
        .map(|d| d.code().exit_code())
        .min()
        .unwrap_or(ExitCode::Success)
}

/// The exit code after a failure of a later stage (output write): the code of an earlier failing
/// stage stands, exit 70 applies only when no earlier stage failed (Req 16.14, 16.17).
pub(crate) fn later_failure(earlier: ExitCode) -> ExitCode {
    match earlier {
        ExitCode::Success => ExitCode::Internal,
        earlier => earlier,
    }
}

pub(crate) struct Inputs {
    pub(crate) rulebook: rt_ir::Rulebook,
    /// Warnings of the Rulebook parser. They never block.
    pub(crate) parse_warnings: DiagnosticState,
    pub(crate) ledger: rt_ledger::ApprovalLedger,
    pub(crate) sources: SourceModels,
    pub(crate) candidates: Option<Vec<CandidateClause>>,
}

/// Runs the command.
pub(crate) fn run(args: &ValidateArgs, fs: &dyn Fs) -> Done {
    let inputs = match load(args, fs) {
        Ok(inputs) => inputs,
        Err(state) => {
            let state = state.sorted();
            let code = match exit_code_of(&state) {
                ExitCode::Success => ExitCode::Internal,
                code => code,
            };
            return Done {
                state,
                code,
                stored: Vec::new(),
            };
        }
    };
    let result = rt_validate::validate(
        &inputs.rulebook,
        &inputs.ledger,
        &inputs.sources,
        inputs.candidates.as_deref(),
    );
    let mut state = result.state().clone();
    state.extend(inputs.parse_warnings.clone());
    let mut state = state.sorted();
    let code = exit_code_of(&state);
    // Success, or blocking Diagnostics of the Rulebook only (exit 3, RN-29): write the artifact.
    if matches!(code, ExitCode::Success | ExitCode::Blocked) {
        match artifact(&result) {
            Ok(text) => {
                if let Err(d) = commit_files(fs, &[(args.out.as_path(), text.as_bytes())]) {
                    state.push(*d);
                    // An earlier failing stage keeps its code (Req 16.17).
                    return Done {
                        state: state.sorted(),
                        code: later_failure(code),
                        stored: Vec::new(),
                    };
                }
            }
            Err(d) => {
                state.push(*d);
                return Done {
                    state: state.sorted(),
                    code: later_failure(code),
                    stored: Vec::new(),
                };
            }
        }
    }
    Done {
        state,
        code,
        stored: Vec::new(),
    }
}

pub(crate) fn load(args: &ValidateArgs, fs: &dyn Fs) -> Result<Inputs, DiagnosticState> {
    let mut problems = DiagnosticState::empty();
    let rulebook = load_rulebook(args, fs, &mut problems);
    let ledger = load_ledger(args, fs, &mut problems);
    let sources = load_sources(&args.source_models, fs, &mut problems);
    let candidates = match (&args.candidates, &sources) {
        (Some(path), Some(sources)) => load_candidates(path, sources, fs, &mut problems).map(Some),
        (None, _) => Some(None),
        (Some(_), None) => None,
    };
    match (rulebook, ledger, sources, candidates) {
        (Some((rulebook, parse_warnings)), Some(ledger), Some(sources), Some(candidates))
            if problems.as_slice().is_empty() =>
        {
            Ok(Inputs {
                rulebook,
                parse_warnings,
                ledger,
                sources,
                candidates,
            })
        }
        _ => Err(problems),
    }
}

pub(crate) fn input_id(path: &Path) -> Result<InputId, Box<Diagnostic>> {
    InputId::new(rt_foundation::escape_controls(&path.display().to_string())).map_err(|_| {
        Box::new(Diagnostic::new(
            Code::CliInputNotFound,
            "an input path is empty".to_owned(),
            input_location(path),
        ))
    })
}

/// Reads one input. One byte more than `limit` is read, so the parser can see that the file is
/// larger than the limit.
pub(crate) fn read_input(fs: &dyn Fs, path: &Path, limit: u64) -> Result<Vec<u8>, Box<Diagnostic>> {
    fs.read(path, limit.saturating_add(1)).map_err(|e| {
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

fn limits(args: &ValidateArgs, cap_bytes: u64) -> ParseLimits {
    ParseLimits::new(
        args.max_bytes.unwrap_or(cap_bytes).min(cap_bytes),
        args.max_depth
            .unwrap_or(MAX_CONTAINER_DEPTH)
            .min(MAX_CONTAINER_DEPTH),
    )
}

fn load_rulebook(
    args: &ValidateArgs,
    fs: &dyn Fs,
    problems: &mut DiagnosticState,
) -> Option<(rt_ir::Rulebook, DiagnosticState)> {
    let limit = limits(args, MAX_RULEBOOK_BYTES);
    let step = || -> Result<(rt_ir::Rulebook, DiagnosticState), Vec<Diagnostic>> {
        let input = input_id(&args.rulebook).map_err(|d| vec![*d])?;
        let bytes = read_input(fs, &args.rulebook, limit.max_bytes()).map_err(|d| vec![*d])?;
        let staged = rt_ir::parse_rulebook(&bytes, &input, limit.max_bytes(), limit.max_depth())
            .map_err(|r| r.into_diagnostics().as_slice().to_vec())?;
        Ok(staged.into_parts())
    };
    collect(step(), problems)
}

fn load_ledger(
    args: &ValidateArgs,
    fs: &dyn Fs,
    problems: &mut DiagnosticState,
) -> Option<rt_ledger::ApprovalLedger> {
    let limit = limits(args, MAX_LEDGER_BYTES);
    let step = || -> Result<rt_ledger::ApprovalLedger, Vec<Diagnostic>> {
        let input = input_id(&args.ledger).map_err(|d| vec![*d])?;
        let bytes = read_input(fs, &args.ledger, limit.max_bytes()).map_err(|d| vec![*d])?;
        rt_ledger::parse_ledger(&input, &bytes, limit)
    };
    collect(step(), problems)
}

pub(crate) fn load_sources(
    paths: &[PathBuf],
    fs: &dyn Fs,
    problems: &mut DiagnosticState,
) -> Option<SourceModels> {
    let limit = default_load_limits();
    let mut models = SourceModels::new();
    let mut ok = true;
    for path in paths {
        let step = || -> Result<SourceModel, Box<Diagnostic>> {
            let input = input_id(path)?;
            let bytes = read_input(fs, path, limit.max_bytes())?;
            SourceModel::load(&input, &bytes, limit)
        };
        match step() {
            Ok(model) => {
                if let Err(duplicate) = models.insert(model) {
                    ok = false;
                    problems.push(Diagnostic::new(
                        Code::CliArgumentInvalid,
                        duplicate.to_string(),
                        input_location(path),
                    ));
                }
            }
            Err(d) => {
                ok = false;
                problems.push(*d);
            }
        }
    }
    ok.then_some(models)
}

fn load_candidates(
    path: &Path,
    sources: &SourceModels,
    fs: &dyn Fs,
    problems: &mut DiagnosticState,
) -> Option<Vec<CandidateClause>> {
    let limit = default_load_limits();
    let step = || -> Result<Vec<CandidateClause>, Box<Diagnostic>> {
        let input = input_id(path)?;
        let bytes = read_input(fs, path, limit.max_bytes())?;
        let document = candidates_document(&input, &bytes, limit).map_err(|message| {
            Box::new(Diagnostic::new(
                Code::CliArgumentInvalid,
                message,
                input_location(path),
            ))
        })?;
        let model = sources.get(&document).ok_or_else(|| {
            Box::new(Diagnostic::new(
                Code::CliArgumentInvalid,
                format!(
                    "the candidates file names document '{document}', which has no --source-model"
                ),
                input_location(path),
            ))
        })?;
        parse_candidates(&input, &bytes, limit, model).map_err(|e| match e {
            CandidatesError::Limit(d) => d,
            CandidatesError::Invalid { message, at, field } => Box::new(Diagnostic::new(
                Code::CliArgumentInvalid,
                format!("candidates file is invalid: {message}"),
                Location::Input(InputLocation {
                    input: input.clone(),
                    line: at.map(|(line, _)| line),
                    column: at.map(|(_, column)| column),
                    field,
                }),
            )),
        })
    };
    match step() {
        Ok(clauses) => Some(clauses),
        Err(d) => {
            problems.push(*d);
            None
        }
    }
}

/// The `document` member of a candidates file, read before the file is checked against a model.
fn candidates_document(
    input: &InputId,
    bytes: &[u8],
    limit: ParseLimits,
) -> Result<DocumentId, String> {
    let root = rt_json::parse(input, bytes, limit).map_err(|d| d.message().to_owned())?;
    let name = root
        .get("document")
        .and_then(|m| m.value.as_str())
        .ok_or_else(|| "the candidates file has no string member 'document'".to_owned())?;
    DocumentId::new(name).map_err(|e| {
        format!(
            "'{}' is not a document identifier: {e}",
            rt_foundation::escape_controls(name)
        )
    })
}

pub(crate) fn collect<T>(
    step: Result<T, Vec<Diagnostic>>,
    problems: &mut DiagnosticState,
) -> Option<T> {
    match step {
        Ok(value) => Some(value),
        Err(found) => {
            for d in found {
                problems.push(d);
            }
            None
        }
    }
}

pub(crate) fn object(members: Vec<(&str, Value<()>)>) -> Value<()> {
    // The keys are distinct constants at every call; the fallback is unreachable.
    Value::object(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
    .unwrap_or_else(|_| Value::null())
}

/// The canonical Validation_Result artifact (see the module documentation).
fn artifact(result: &ValidationResult) -> Result<String, Box<Diagnostic>> {
    let internal = |message: String| {
        Diagnostic::new(
            Code::InternalError,
            message,
            Location::None(Code::InternalError),
        )
    };
    let rulebook = rt_ir::print_compact(result.rulebook()).map_err(|found| {
        internal(format!(
            "the validated Rulebook cannot be printed: {}",
            found.first().map_or("no reason given", |d| d.message())
        ))
    })?;
    let diagnostics = result
        .state()
        .as_slice()
        .iter()
        .map(crate::render::diagnostic_value)
        .collect();
    let classes = result
        .checked_conflict_classes()
        .iter()
        .map(|c| Value::string(c.name()))
        .collect();
    let metadata = result
        .rule_metadata()
        .iter()
        .map(|(id, meta)| {
            let names = |list: &[rt_ir::Name]| {
                Value::array(list.iter().map(|n| Value::string(n.as_str())).collect())
            };
            object(vec![
                ("rule", Value::string(id.as_str())),
                ("input_fields", names(meta.input_fields())),
                ("collections", names(meta.collections())),
            ])
        })
        .collect();
    let ledger = rt_ledger::print_ledger(result.ledger());
    // The Rulebook is already canonical compact RT-JSON, so it is spliced as it is.
    let mut out = String::from(
        r#"{"format":"ruletrace-validation","version":1,"executable":false,"rulebook":"#,
    );
    out.push_str(&rulebook);
    out.push_str(r#","ledger":"#);
    out.push_str(&write_compact(&Value::string(ledger)));
    out.push_str(r#","diagnostics":"#);
    out.push_str(&write_compact(&Value::array(diagnostics)));
    out.push_str(r#","checked_conflict_classes":"#);
    out.push_str(&write_compact(&Value::array(classes)));
    out.push_str(r#","rule_metadata":"#);
    out.push_str(&write_compact(&Value::array(metadata)));
    out.push_str("}\n");
    Ok(out)
}
