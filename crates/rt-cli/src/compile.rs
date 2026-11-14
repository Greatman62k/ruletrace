//! `ruletrace compile`: stage 5 (Req 9, 16.10, 16.21, 17.1, 17.9).
//!
//! The command reads the Validation_Result artifact that `validate` wrote (see the module
//! documentation of `validate.rs` for its format) and writes the Compiled_Rulebook only on
//! success.
//!
//! The verdict is derived from the stored Diagnostics, never from a stored flag (Req 22):
//!
//! * A Diagnostic of Severity Error, Ambiguity or Unsupported in the artifact makes the artifact
//!   not executable. `compile` writes those Diagnostics unchanged (code, Severity, message and
//!   Locations as stored) and exits with 3. No file is written (RN-29, Req 9.2, 17.9, 16.21).
//! * Otherwise the Rulebook, the ledger and the Diagnostics are read back into a
//!   `ValidationResult` and the Compiler runs. It derives every Approval_Status again from the
//!   Rulebook and the ledger and refuses an artifact whose gate Diagnostics disagree
//!   (`RT-APR-0005`, Req 9.12).
//!
//! A stored Warning with a Source_Span Location is read back at document level, because a
//! verified span exists only next to its Source_Model, which `compile` is not given. A Warning
//! never changes compilability.

use crate::args::CompileArgs;
use crate::io::{commit_files, Fs};
use crate::validate::{collect, exit_code_of, input_id, read_input, Done};
use rt_foundation::{
    CliLocation, Code, Diagnostic, DiagnosticState, DocumentId, ExitCode, InputId, InputLocation,
    Location, RevisionNumber, RuleId, RuleRef, MAX_CONTAINER_DEPTH, MAX_LEDGER_BYTES,
    MAX_RULEBOOK_BYTES,
};
use rt_json::{write_compact, Kind, ParseLimits, Position, Value};
use rt_validate::{ConflictClass, RuleMetadata, ValidationResult};
use std::collections::BTreeMap;

type Json = Value<Position>;

/// The largest Validation_Result artifact read: a Rulebook, a ledger and their Diagnostics.
const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;

const TOP_KEYS: [&str; 8] = [
    "format",
    "version",
    "executable",
    "rulebook",
    "ledger",
    "diagnostics",
    "checked_conflict_classes",
    "rule_metadata",
];

const CLASSES: [ConflictClass; 4] = [
    ConflictClass::ContradictoryDefault,
    ConflictClass::DuplicateRankKey,
    ConflictClass::DuplicateTarget,
    ConflictClass::InputShadowing,
];

/// Runs the command.
pub(crate) fn run(args: &CompileArgs, fs: &dyn Fs) -> Done {
    let input = match input_id(&args.validation) {
        Ok(input) => input,
        Err(d) => return fail(vec![*d]),
    };
    let bytes = match read_input(fs, &args.validation, MAX_ARTIFACT_BYTES) {
        Ok(bytes) => bytes,
        Err(d) => return fail(vec![*d]),
    };
    let artifact = match read_artifact(&input, &bytes) {
        Ok(artifact) => artifact,
        Err(found) => return fail(found),
    };
    if artifact.stored.iter().any(|d| d.blocking) {
        // Not executable: the stored Diagnostics are the answer (RN-29).
        return Done {
            state: DiagnosticState::empty(),
            code: ExitCode::Blocked,
            stored: artifact
                .stored
                .iter()
                .filter(|d| d.blocking)
                .map(|d| d.text.clone())
                .collect(),
        };
    }
    let result = match artifact.into_result(&input) {
        Ok(result) => result,
        Err(found) => return fail(found),
    };
    match rt_compile::compile(&result) {
        Err(rejected) => fail(rejected.into_diagnostics().as_slice().to_vec()),
        Ok(staged) => {
            let (compiled, warnings) = staged.into_parts();
            let mut text = compiled.to_canonical_compact();
            text.push('\n');
            if let Err(d) = commit_files(fs, &[(args.out.as_path(), text.as_bytes())]) {
                let mut state = warnings;
                state.push(*d);
                return Done {
                    state: state.sorted(),
                    code: ExitCode::Internal,
                    stored: Vec::new(),
                };
            }
            Done {
                state: warnings.sorted(),
                code: ExitCode::Success,
                stored: Vec::new(),
            }
        }
    }
}

/// A failed command: the exit code is the lowest catalogue code among the blocking Diagnostics.
fn fail(found: Vec<Diagnostic>) -> Done {
    let mut state = DiagnosticState::empty();
    for d in found {
        state.push(d);
    }
    let state = state.sorted();
    let code = match exit_code_of(&state) {
        ExitCode::Success => ExitCode::Internal,
        code => code,
    };
    Done {
        state,
        code,
        stored: Vec::new(),
    }
}

/// One Diagnostic of the artifact: its text form for standard error and its verdict.
struct Stored {
    blocking: bool,
    text: String,
    typed: Diagnostic,
}

struct Artifact {
    rulebook: Json,
    ledger: String,
    stored: Vec<Stored>,
    classes: Vec<ConflictClass>,
    metadata: BTreeMap<RuleId, RuleMetadata>,
}

fn at(input: &InputId, field: &str) -> Location {
    Location::Input(InputLocation {
        input: input.clone(),
        line: None,
        column: None,
        field: Some(field.to_owned()),
    })
}

fn invalid(input: &InputId, field: &str, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::new(
        Code::CmpArtifactInvalid,
        format!("the Validation_Result is invalid: {message}"),
        at(input, field),
    )]
}

fn member<'a>(root: &'a Json, key: &str) -> Option<&'a Json> {
    root.get(key).map(|m| &m.value)
}

fn read_artifact(input: &InputId, bytes: &[u8]) -> Result<Artifact, Vec<Diagnostic>> {
    let limits = ParseLimits::new(MAX_ARTIFACT_BYTES, MAX_CONTAINER_DEPTH);
    let root = rt_json::parse(input, bytes, limits).map_err(|d| vec![*d])?;
    rt_json::check_flag_keys(input, &root)?;
    let Some(members) = root.as_object() else {
        return Err(invalid(input, "", "the top level is not an object"));
    };
    // Req 9.11: one Diagnostic for each missing component.
    let missing: Vec<Diagnostic> = ["rulebook", "ledger", "diagnostics"]
        .into_iter()
        .filter(|k| root.get(k).is_none())
        .map(|k| {
            Diagnostic::new(
                Code::CmpInputIncomplete,
                format!("the Validation_Result has no '{k}'"),
                at(input, k),
            )
        })
        .collect();
    if !missing.is_empty() {
        return Err(missing);
    }
    if let Some(extra) = members.iter().find(|m| !TOP_KEYS.contains(&m.key.as_str())) {
        return Err(invalid(
            input,
            &extra.key,
            &format!(
                "unknown key '{}'",
                rt_foundation::escape_controls(&extra.key)
            ),
        ));
    }
    if member(&root, "format").and_then(Json::as_str) != Some("ruletrace-validation") {
        return Err(invalid(
            input,
            "format",
            "the format is not 'ruletrace-validation'",
        ));
    }
    if member(&root, "version").and_then(Json::as_u64) != Some(1) {
        return Err(invalid(input, "version", "the version is not 1"));
    }
    if !matches!(
        member(&root, "executable").map(Json::kind),
        Some(Kind::Bool(false))
    ) {
        return Err(invalid(input, "executable", "'executable' is not false"));
    }
    let rulebook = member(&root, "rulebook")
        .filter(|v| v.as_object().is_some())
        .ok_or_else(|| invalid(input, "rulebook", "the Rulebook is not an object"))?
        .clone();
    let ledger = member(&root, "ledger")
        .and_then(Json::as_str)
        .ok_or_else(|| invalid(input, "ledger", "the ledger is not a string"))?
        .to_owned();
    let stored = member(&root, "diagnostics")
        .and_then(Json::as_array)
        .ok_or_else(|| invalid(input, "diagnostics", "the Diagnostics are not an array"))?
        .iter()
        .enumerate()
        .map(|(i, d)| {
            stored_diagnostic(d).ok_or_else(|| {
                invalid(
                    input,
                    &format!("diagnostics[{i}]"),
                    "a Diagnostic is malformed",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let classes = read_classes(input, &root)?;
    let metadata = read_metadata(input, &root)?;
    Ok(Artifact {
        rulebook,
        ledger,
        stored,
        classes,
        metadata,
    })
}

fn read_classes(input: &InputId, root: &Json) -> Result<Vec<ConflictClass>, Vec<Diagnostic>> {
    let field = "checked_conflict_classes";
    let items = member(root, field)
        .and_then(Json::as_array)
        .ok_or_else(|| invalid(input, field, "the checked classes are not an array"))?;
    items
        .iter()
        .map(|v| {
            let name = v.as_str().unwrap_or_default();
            CLASSES
                .into_iter()
                .find(|c| c.name() == name)
                .ok_or_else(|| invalid(input, field, "an unknown conflict class is named"))
        })
        .collect()
}

fn names(v: Option<&Json>) -> Option<Vec<rt_ir::Name>> {
    v?.as_array()?
        .iter()
        .map(|n| rt_ir::Name::new(n.as_str()?).ok())
        .collect()
}

fn read_metadata(
    input: &InputId,
    root: &Json,
) -> Result<BTreeMap<RuleId, RuleMetadata>, Vec<Diagnostic>> {
    let field = "rule_metadata";
    let items = member(root, field)
        .and_then(Json::as_array)
        .ok_or_else(|| invalid(input, field, "the rule metadata is not an array"))?;
    let mut out = BTreeMap::new();
    for (i, item) in items.iter().enumerate() {
        let entry = (|| {
            let id = RuleId::new(member(item, "rule")?.as_str()?).ok()?;
            let fields = names(member(item, "input_fields"))?;
            let collections = names(member(item, "collections"))?;
            Some((id, RuleMetadata::new(fields, collections)))
        })();
        match entry {
            Some((id, meta)) => {
                out.insert(id, meta);
            }
            None => {
                return Err(invalid(
                    input,
                    &format!("{field}[{i}]"),
                    "a rule metadata entry is malformed",
                ))
            }
        }
    }
    Ok(out)
}

fn severity_blocking(name: &str) -> Option<bool> {
    match name {
        "error" | "ambiguity" | "unsupported" => Some(true),
        "warning" => Some(false),
        _ => None,
    }
}

fn opt_u32(v: Option<&Json>) -> Option<Option<u32>> {
    match v?.kind() {
        Kind::Null => Some(None),
        Kind::Integer(n) => u32::try_from(*n).ok().map(Some),
        _ => None,
    }
}

fn opt_text(v: Option<&Json>) -> Option<Option<String>> {
    match v?.kind() {
        Kind::Null => Some(None),
        Kind::String(s) => Some(Some(s.clone())),
        _ => None,
    }
}

/// A stored Location, read back. A Source_Span becomes its document (see the module
/// documentation).
fn location(v: &Json) -> Option<Location> {
    let text = |key: &str| member(v, key).and_then(Json::as_str);
    match text("kind")? {
        "source" | "document" => Some(Location::Document(DocumentId::new(text("document")?).ok()?)),
        "input" => Some(Location::Input(InputLocation {
            input: InputId::new(text("input")?).ok()?,
            line: opt_u32(member(v, "line"))?,
            column: opt_u32(member(v, "column"))?,
            field: opt_text(member(v, "field"))?,
        })),
        "cli_argument" => Some(Location::Cli(CliLocation::Argument {
            position: opt_u32(member(v, "position"))??,
            name: opt_text(member(v, "name"))?,
        })),
        "cli_command" => Some(Location::Cli(CliLocation::Command(
            text("command")?.to_owned(),
        ))),
        "none" => Some(Location::None(Code::from_id(text("reason")?)?)),
        _ => None,
    }
}

fn rule_ref(v: &Json) -> Option<RuleRef> {
    Some(RuleRef {
        rule: RuleId::new(member(v, "rule")?.as_str()?).ok()?,
        revision: match opt_u32(member(v, "revision"))? {
            Some(n) => Some(RevisionNumber::new(n).ok()?),
            None => None,
        },
    })
}

fn stored_diagnostic(v: &Json) -> Option<Stored> {
    let code = Code::from_id(member(v, "code")?.as_str()?)?;
    let blocking = severity_blocking(member(v, "severity")?.as_str()?)?;
    // The catalogue fixes the Severity of a code, so a stored Severity must agree with it.
    if blocking != code.severity().is_blocking() {
        return None;
    }
    let message = member(v, "message")?.as_str()?.to_owned();
    let primary = location(member(v, "primary")?)?;
    let related = member(v, "related")?
        .as_array()?
        .iter()
        .map(location)
        .collect::<Option<Vec<_>>>()?;
    let rules = member(v, "rules")?
        .as_array()?
        .iter()
        .map(rule_ref)
        .collect::<Option<Vec<_>>>()?;
    let text = crate::render::stored(v)?;
    Some(Stored {
        blocking,
        text,
        typed: Diagnostic::new(code, message, primary)
            .with_related(related)
            .with_rules(rules),
    })
}

impl Artifact {
    /// Reads the Rulebook and the ledger back and builds the `ValidationResult`.
    fn into_result(self, input: &InputId) -> Result<ValidationResult, Vec<Diagnostic>> {
        let mut problems = DiagnosticState::empty();
        let rulebook_bytes = write_compact(&self.rulebook);
        let rulebook = collect(
            rt_ir::parse_rulebook(
                rulebook_bytes.as_bytes(),
                input,
                MAX_RULEBOOK_BYTES,
                MAX_CONTAINER_DEPTH,
            )
            .map(|staged| staged.into_parts().0)
            .map_err(|r| r.into_diagnostics().as_slice().to_vec()),
            &mut problems,
        );
        let ledger = collect(
            rt_ledger::parse_ledger(
                input,
                self.ledger.as_bytes(),
                ParseLimits::new(MAX_LEDGER_BYTES, MAX_CONTAINER_DEPTH),
            ),
            &mut problems,
        );
        match (rulebook, ledger) {
            (Some(rulebook), Some(ledger)) if problems.as_slice().is_empty() => {
                let mut state = DiagnosticState::empty();
                for d in self.stored {
                    state.push(d.typed);
                }
                Ok(ValidationResult::from_parts(
                    rulebook,
                    ledger,
                    state.sorted(),
                    self.classes,
                    self.metadata,
                ))
            }
            _ => Err(problems.as_slice().to_vec()),
        }
    }
}
