//! Reading the input files named on the command line (design 3.9.2).
//!
//! The tool reads only the files its arguments and its configuration name.
//! It never opens a ledger file: no function here takes a ledger path.

use crate::cli::{at_argument, ProposeArgs, ReviseArgs};
use crate::revise::{ReviseContext, Unknown, MAX_FEEDBACK_BYTES};
use rt_ai_mock::RecordedScript;
use rt_foundation::{
    escape_controls, Code, Diagnostic, DocumentId, InputId, InputLocation, Location,
    MAX_CONTAINER_DEPTH, MAX_RULEBOOK_BYTES,
};
use rt_ir::Rulebook;
use rt_json::ParseLimits;
use rt_srcmodel::{
    default_load_limits, parse_candidates, CandidateClause, CandidatesError, SourceModel,
    SourceModels,
};
use std::io::Read;
use std::path::Path;

/// Largest AI_Configuration file, in bytes.
pub(crate) const MAX_CONFIG_BYTES: u64 = 1 << 20;

/// What the stage "input load" reads, without the recorded script.
pub(crate) struct Inputs {
    pub(crate) rulebook: Rulebook,
    pub(crate) models: SourceModels,
    pub(crate) candidates: Vec<CandidateClause>,
    /// The revision request, when the command line carries one.
    pub(crate) revise: Option<ReviseContext>,
    /// Warnings of the Rulebook parser. They never block.
    pub(crate) warnings: Vec<Diagnostic>,
}

pub(crate) fn input_location(path: &Path) -> Location {
    match InputId::new(escape_controls(&path.display().to_string())) {
        Ok(input) => Location::Input(InputLocation {
            input,
            line: None,
            column: None,
            field: None,
        }),
        Err(_) => Location::None(Code::CliInputNotFound),
    }
}

pub(crate) fn input_id(path: &Path) -> Result<InputId, Diagnostic> {
    InputId::new(escape_controls(&path.display().to_string())).map_err(|_| {
        Diagnostic::new(
            Code::CliInputNotFound,
            "an input path is empty".to_owned(),
            input_location(path),
        )
    })
}

/// Reads one file. One byte more than `limit` is read, so that a parser can
/// see that the file is larger than the limit.
pub(crate) fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, Diagnostic> {
    let read = || -> std::io::Result<Vec<u8>> {
        let file = std::fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    read().map_err(|e| {
        let what = if e.kind() == std::io::ErrorKind::NotFound {
            "does not exist".to_owned()
        } else {
            format!("cannot be read: {:?}", e.kind())
        };
        Diagnostic::new(
            Code::CliInputNotFound,
            format!(
                "input file '{}' {what}",
                escape_controls(&path.display().to_string())
            ),
            input_location(path),
        )
    })
}

/// Loads the Rulebook, the Source_Models and the candidates. Every problem
/// of every file is returned, so a user sees them at once.
pub(crate) fn load_inputs(args: &ProposeArgs) -> Result<Inputs, Vec<Diagnostic>> {
    let mut problems = Vec::new();
    let rulebook = load_rulebook(&args.rulebook, &mut problems);
    let models = load_models(args, &mut problems);
    let candidates = models
        .as_ref()
        .and_then(|m| load_candidates(&args.candidates, m, &mut problems));
    let revise = args.revise.as_ref().and_then(|wanted| match &rulebook {
        Some((rulebook, _)) => load_revise(wanted, rulebook, &mut problems),
        None => {
            let _ = load_feedback(wanted, &mut problems);
            None
        }
    });
    match (rulebook, models, candidates, revise) {
        (Some((rulebook, warnings)), Some(models), Some(candidates), revise)
            if problems.is_empty() =>
        {
            Ok(Inputs {
                rulebook,
                models,
                candidates,
                revise,
                warnings,
            })
        }
        _ => {
            if problems.is_empty() {
                problems.push(Diagnostic::new(
                    Code::InternalError,
                    "an input was not loaded and no problem was recorded".to_owned(),
                    Location::None(Code::InternalError),
                ));
            }
            Err(problems)
        }
    }
}

fn load_rulebook(
    path: &Path,
    problems: &mut Vec<Diagnostic>,
) -> Option<(Rulebook, Vec<Diagnostic>)> {
    let step = || -> Result<(Rulebook, Vec<Diagnostic>), Vec<Diagnostic>> {
        let input = input_id(path).map_err(|d| vec![d])?;
        let bytes = read_bounded(path, MAX_RULEBOOK_BYTES).map_err(|d| vec![d])?;
        let staged = rt_ir::parse_rulebook(&bytes, &input, MAX_RULEBOOK_BYTES, MAX_CONTAINER_DEPTH)
            .map_err(|r| r.into_diagnostics().as_slice().to_vec())?;
        let (rulebook, warnings) = staged.into_parts();
        Ok((rulebook, warnings.as_slice().to_vec()))
    };
    match step() {
        Ok(found) => Some(found),
        Err(found) => {
            problems.extend(found);
            None
        }
    }
}

fn refused(wanted_at: usize, option: &str, message: String) -> Diagnostic {
    Diagnostic::new(
        Code::AiRequestInvalid,
        message,
        at_argument(wanted_at, Some(option)),
    )
}

// Req 19.3: an oversize file is rejected before its content is used, and the
// size is the one the file system reports, not the size of a bounded read.
fn oversize_feedback(path: &Path, read_len: usize) -> Diagnostic {
    let size = std::fs::metadata(path)
        .map_or(read_len as u64, |m| m.len())
        .max(read_len as u64);
    let input = escape_controls(&path.display().to_string());
    Diagnostic::new(
        Code::LimitFileTooLarge,
        format!(
            "input file '{input}' is {size} bytes, larger than the effective limit of \
             {MAX_FEEDBACK_BYTES} bytes"
        ),
        input_location(path),
    )
}

// The feedback is untrusted text: it must be UTF-8, not empty and at most
// MAX_FEEDBACK_BYTES, and it is kept byte for byte.
fn load_feedback(wanted: &ReviseArgs, problems: &mut Vec<Diagnostic>) -> Option<String> {
    let at = input_location(&wanted.feedback);
    let bytes = match read_bounded(&wanted.feedback, MAX_FEEDBACK_BYTES as u64) {
        Ok(bytes) => bytes,
        Err(d) => {
            problems.push(d);
            return None;
        }
    };
    if bytes.len() > MAX_FEEDBACK_BYTES {
        problems.push(oversize_feedback(&wanted.feedback, bytes.len()));
        return None;
    }
    if bytes.is_empty() {
        problems.push(invalid_request(
            "the feedback file given by --feedback-file is empty".to_owned(),
            at,
        ));
        return None;
    }
    match String::from_utf8(bytes) {
        Ok(text) => Some(text),
        Err(_) => {
            problems.push(invalid_request(
                "the feedback file given by --feedback-file is not valid UTF-8".to_owned(),
                at,
            ));
            None
        }
    }
}

fn load_revise(
    wanted: &ReviseArgs,
    rulebook: &Rulebook,
    problems: &mut Vec<Diagnostic>,
) -> Option<ReviseContext> {
    let feedback = load_feedback(wanted, problems);
    let read = feedback.is_some();
    let resolved = ReviseContext::resolve(
        rulebook,
        &wanted.rule,
        wanted.revision,
        feedback.unwrap_or_default(),
    );
    match resolved {
        Ok(context) => read.then_some(context),
        Err(Unknown::Rule) => {
            problems.push(refused(
                wanted.rule_at,
                "--revise-rule",
                format!(
                    "--revise-rule names rule '{}', which the Rulebook does not contain",
                    escape_controls(wanted.rule.as_str())
                ),
            ));
            None
        }
        Err(Unknown::Revision) => {
            problems.push(refused(
                wanted.revision_at,
                "--revise-revision",
                format!(
                    "--revise-revision names revision {} of rule '{}', which the Rulebook does not contain",
                    wanted.revision.get(),
                    escape_controls(wanted.rule.as_str())
                ),
            ));
            None
        }
    }
}

fn load_models(args: &ProposeArgs, problems: &mut Vec<Diagnostic>) -> Option<SourceModels> {
    let limit = default_load_limits();
    let mut models = SourceModels::new();
    let mut ok = true;
    for path in &args.source_models {
        let step = || -> Result<SourceModel, Diagnostic> {
            let input = input_id(path)?;
            let bytes = read_bounded(path, limit.max_bytes())?;
            SourceModel::load(&input, &bytes, limit).map_err(|d| *d)
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
                problems.push(d);
            }
        }
    }
    ok.then_some(models)
}

fn invalid_request(message: String, at: Location) -> Diagnostic {
    Diagnostic::new(Code::AiRequestInvalid, message, at)
}

fn load_candidates(
    path: &Path,
    models: &SourceModels,
    problems: &mut Vec<Diagnostic>,
) -> Option<Vec<CandidateClause>> {
    let limit = default_load_limits();
    let step = || -> Result<Vec<CandidateClause>, Diagnostic> {
        let input = input_id(path)?;
        let bytes = read_bounded(path, limit.max_bytes())?;
        let document = candidates_document(&input, &bytes, limit)
            .map_err(|m| invalid_request(m, input_location(path)))?;
        let model = models.get(&document).ok_or_else(|| {
            invalid_request(
                format!(
                    "the candidates file names document '{}', which has no --source-model",
                    escape_controls(document.as_str())
                ),
                input_location(path),
            )
        })?;
        parse_candidates(&input, &bytes, limit, model).map_err(|e| match e {
            CandidatesError::Limit(d) => *d,
            CandidatesError::Invalid { message, at, field } => invalid_request(
                format!("candidates file is invalid: {message}"),
                Location::Input(InputLocation {
                    input: input.clone(),
                    line: at.map(|(line, _)| line),
                    column: at.map(|(_, column)| column),
                    field,
                }),
            ),
        })
    };
    match step() {
        Ok(clauses) => Some(clauses),
        Err(d) => {
            problems.push(d);
            None
        }
    }
}

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
            escape_controls(name)
        )
    })
}

/// Loads the recorded script that a `mock-replay` configuration names.
pub(crate) fn load_script(path: &Path) -> Result<RecordedScript, Diagnostic> {
    let input = input_id(path)?;
    let bytes = read_bounded(path, 64 * 1024 * 1024)?;
    RecordedScript::parse(&input, &bytes).map_err(|e| {
        Diagnostic::new(
            Code::CliArgumentInvalid,
            format!("the recorded script is invalid: {e}"),
            input_location(path),
        )
    })
}
