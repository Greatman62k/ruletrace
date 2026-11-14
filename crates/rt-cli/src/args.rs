//! The hand-written argument parser (design §3.9).
//!
//! Options may appear in any order. A repeated option is an error (except
//! `--source-model`), `--` is not supported, and the form `--name=value` is not
//! supported. Every problem is a `Cli_Location` Diagnostic. All problems of one
//! command line are reported, so a user sees them at once; none of them reads a
//! file.

use rt_foundation::{CliLocation, Code, Diagnostic, DocumentId, Location, RevisionNumber, RuleId};
use rt_hash::Sha256Hex;
use rt_srcmodel::DocumentKind;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

/// A parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Invocation {
    /// `ruletrace ingest`.
    Ingest(IngestArgs),
    /// `ruletrace validate`.
    Validate(ValidateArgs),
    /// `ruletrace compile`.
    Compile(CompileArgs),
    /// `ruletrace evaluate`.
    Evaluate(EvaluateArgs),
    /// `ruletrace review list`, `approve` or `reject`.
    Review(ReviewArgs),
}

/// The arguments of `ruletrace review`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewArgs {
    /// The Rule_IR file.
    pub rulebook: PathBuf,
    /// The Approval_Ledger file.
    pub ledger: PathBuf,
    /// The Source_Model files, in the order given.
    pub source_models: Vec<PathBuf>,
    /// The Proposal_Store directory of `approve` and `reject`, when given.
    pub proposals: Option<PathBuf>,
    /// The subcommand.
    pub command: ReviewCommand,
}

/// The subcommand of `ruletrace review`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewCommand {
    /// `review list`: shows revisions, writes nothing.
    List {
        /// Show only this rule, when given.
        rule: Option<RuleId>,
        /// Show only this revision of the rule, when given (needs `rule`).
        revision: Option<RevisionNumber>,
    },
    /// `review approve` or `review reject`: one Review_Decision.
    Decide(DecideArgs),
}

/// The values of one `approve` or `reject` request. Reviewer and time come only
/// from here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecideArgs {
    /// The rule to decide.
    pub rule: RuleId,
    /// The revision to decide.
    pub revision: RevisionNumber,
    /// The Rule_Content_Hash the Reviewer saw.
    pub content_hash: Sha256Hex,
    /// The Reviewer name, validated by the review planner (UD-23).
    pub reviewer: String,
    /// The decision time, stored verbatim.
    pub time: String,
    /// Create the ledger when the file does not exist.
    pub create_ledger: bool,
    /// The reason of a rejection. `None` for an approval.
    pub reject_reason: Option<String>,
}

/// How Diagnostics are written to standard error (`--output-format`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    /// Line groups for a reader (the default).
    Human,
    /// One compact RT-JSON array on one line.
    Json,
}

/// The arguments of `ruletrace validate`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidateArgs {
    /// The Rule_IR file.
    pub rulebook: PathBuf,
    /// The Approval_Ledger file.
    pub ledger: PathBuf,
    /// The Source_Model files, in the order given (at least one).
    pub source_models: Vec<PathBuf>,
    /// The Candidate_Clause list, if given.
    pub candidates: Option<PathBuf>,
    /// Where the Validation_Result is written.
    pub out: PathBuf,
    /// The rendering of Diagnostics on standard error.
    pub output_format: OutputFormat,
    /// The size limit given by `--max-bytes` for the Rulebook and the ledger,
    /// if any.
    pub max_bytes: Option<u64>,
    /// The container depth limit given by `--max-depth` for the Rulebook and
    /// the ledger, if any.
    pub max_depth: Option<u32>,
}

/// The arguments of `ruletrace compile`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileArgs {
    /// The Validation_Result artifact written by `validate`.
    pub validation: PathBuf,
    /// Where the Compiled_Rulebook is written.
    pub out: PathBuf,
}

/// The arguments of `ruletrace evaluate`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluateArgs {
    /// The Compiled_Rulebook artifact.
    pub compiled: PathBuf,
    /// The Competition_Data file.
    pub data: PathBuf,
    /// The Questions file, if given. Without it the default Questions are used
    /// (RN-6).
    pub questions: Option<PathBuf>,
    /// The Published_Result file, if given.
    pub published: Option<PathBuf>,
    /// The Source_Model files, in the order given. Without any, provenance is
    /// not verified.
    pub source_models: Vec<PathBuf>,
    /// Where the results file is written.
    pub out: PathBuf,
    /// The rendering of Diagnostics on standard error.
    pub output_format: OutputFormat,
    /// Whether the rendered traces are written to standard output.
    pub render: bool,
    /// The proof report request, if any.
    pub proof: Option<ProofArgs>,
}

/// The inputs and outputs of a Proof_Report request (`--proof-report`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofArgs {
    /// The Rulebook the Compiled_Rulebook came from.
    pub rulebook: PathBuf,
    /// The Approval_Ledger.
    pub ledger: PathBuf,
    /// Where the human form is written.
    pub report: PathBuf,
    /// Where the machine form is written, if requested.
    pub report_json: Option<PathBuf>,
}

/// The arguments of `ruletrace ingest`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IngestArgs {
    /// The Source_Document file.
    pub document: PathBuf,
    /// The document identifier, given or derived from the path text.
    pub document_id: DocumentId,
    /// The format given by `--format`, if any.
    pub format: Option<DocumentKind>,
    /// Where the Source_Model is written.
    pub out: PathBuf,
    /// Where the Candidate_Clause list is written, if requested.
    pub candidates: Option<PathBuf>,
    /// The size limit given by `--max-bytes`, if any.
    pub max_bytes: Option<u64>,
}

const INGEST_OPTIONS: [&str; 6] = [
    "--document",
    "--document-id",
    "--format",
    "--out",
    "--candidates",
    "--max-bytes",
];

const VALIDATE_OPTIONS: [&str; 8] = [
    "--rulebook",
    "--ledger",
    "--source-model",
    "--candidates",
    "--out",
    "--output-format",
    "--max-bytes",
    "--max-depth",
];

const COMPILE_OPTIONS: [&str; 2] = ["--validation", "--out"];

const EVALUATE_OPTIONS: [&str; 12] = [
    "--compiled",
    "--data",
    "--questions",
    "--published",
    "--source-model",
    "--out",
    "--output-format",
    "--render",
    "--ledger",
    "--rulebook",
    "--proof-report",
    "--proof-report-json",
];

/// Options that may be given more than once. Their values are collected in
/// order.
const REPEATABLE: [&str; 1] = ["--source-model"];

/// Options that take no value.
const FLAGS: [&str; 2] = ["--render", "--create-ledger"];

/// Options whose value may be empty, because the review planner refuses an
/// empty value with its own REVIEW Diagnostic (Req 23.14, 23.15).
const MAY_BE_EMPTY: [&str; 2] = ["--reviewer", "--reason"];

const REVIEW_LIST_OPTIONS: [&str; 5] = [
    "--rulebook",
    "--ledger",
    "--source-model",
    "--rule",
    "--revision",
];

const REVIEW_APPROVE_OPTIONS: [&str; 10] = [
    "--rulebook",
    "--ledger",
    "--source-model",
    "--rule",
    "--revision",
    "--content-hash",
    "--reviewer",
    "--time",
    "--create-ledger",
    "--proposals",
];

const REVIEW_REJECT_OPTIONS: [&str; 11] = [
    "--rulebook",
    "--ledger",
    "--source-model",
    "--rule",
    "--revision",
    "--content-hash",
    "--reviewer",
    "--time",
    "--create-ledger",
    "--proposals",
    "--reason",
];

const COMMAND_LIST: &str = "ingest, validate, compile, evaluate, review";

fn position(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

fn at_argument(index: usize, name: Option<&str>) -> Location {
    Location::Cli(CliLocation::Argument {
        position: position(index),
        name: name.map(str::to_owned),
    })
}

fn invalid(index: usize, name: Option<&str>, message: String) -> Diagnostic {
    Diagnostic::new(Code::CliArgumentInvalid, message, at_argument(index, name))
}

fn display(text: &str) -> String {
    rt_foundation::escape_controls(text)
}

/// Parses the arguments after the program name. The command name is at
/// position 0.
///
/// # Errors
/// All Diagnostics of the command line, in the order found. They have codes
/// `RT-CLI-0001`, `RT-CLI-0002` or `RT-CLI-0004` and a `Cli_Location`.
pub fn parse(args: &[OsString]) -> Result<Invocation, Vec<Diagnostic>> {
    let mut text = Vec::with_capacity(args.len());
    for (index, arg) in args.iter().enumerate() {
        match arg.to_str() {
            Some(s) => text.push(s.to_owned()),
            None => {
                return Err(vec![invalid(
                    index,
                    None,
                    format!("argument {index} is not valid UTF-8"),
                )])
            }
        }
    }
    let Some(command) = text.first() else {
        return Err(vec![Diagnostic::new(
            Code::CliMissingArgument,
            format!("no command given; the commands are: {COMMAND_LIST}"),
            at_argument(0, None),
        )]);
    };
    match command.as_str() {
        "ingest" => parse_ingest(&text),
        "validate" => parse_validate(&text),
        "compile" => parse_compile(&text),
        "evaluate" => parse_evaluate(&text),
        "review" => parse_review(&text),
        other => Err(vec![Diagnostic::new(
            Code::CliUnknownCommand,
            format!(
                "unknown command '{}'; the commands are: {COMMAND_LIST}",
                display(other)
            ),
            Location::Cli(CliLocation::Command(display(other))),
        )]),
    }
}

/// The option values of one command: position and text of each option that was
/// given once.
type Given<'a> = BTreeMap<&'static str, (usize, &'a str)>;

/// Values of the repeatable options, with their positions, in the order given.
type Repeated<'a> = Vec<(usize, &'a str)>;

fn scan<'a>(
    text: &'a [String],
    known: &[&'static str],
    problems: &mut Vec<Diagnostic>,
) -> (Given<'a>, Repeated<'a>) {
    scan_from(text, 1, known, problems)
}

fn scan_from<'a>(
    text: &'a [String],
    start: usize,
    known: &[&'static str],
    problems: &mut Vec<Diagnostic>,
) -> (Given<'a>, Repeated<'a>) {
    let mut given: Given<'a> = BTreeMap::new();
    let mut repeated: Repeated<'a> = Vec::new();
    let mut index = start;
    while let Some(arg) = text.get(index) {
        let Some(name) = known.iter().find(|k| **k == arg.as_str()) else {
            let message = if arg == "--" {
                "'--' is not supported".to_owned()
            } else if arg.starts_with("--") {
                format!("unknown option '{}'", display(arg))
            } else {
                format!("unexpected argument '{}'", display(arg))
            };
            problems.push(invalid(index, Some(&display(arg)), message));
            index += 1;
            continue;
        };
        if FLAGS.contains(name) {
            if given.contains_key(name) {
                problems.push(invalid(
                    index,
                    Some(name),
                    format!("option '{name}' is given more than once"),
                ));
            } else {
                given.insert(name, (index, ""));
            }
            index += 1;
            continue;
        }
        let value = text.get(index + 1).filter(|v| !v.starts_with("--"));
        match value {
            None => {
                problems.push(invalid(
                    index,
                    Some(name),
                    format!("option '{name}' needs a value"),
                ));
                index += 1;
            }
            Some(value) => {
                if REPEATABLE.contains(name) {
                    if value.is_empty() {
                        problems.push(invalid(
                            index + 1,
                            Some(name),
                            format!("option '{name}' has an empty value"),
                        ));
                    } else {
                        repeated.push((index + 1, value.as_str()));
                    }
                } else if given.contains_key(name) {
                    problems.push(invalid(
                        index,
                        Some(name),
                        format!("option '{name}' is given more than once"),
                    ));
                } else if value.is_empty() && !MAY_BE_EMPTY.contains(name) {
                    problems.push(invalid(
                        index + 1,
                        Some(name),
                        format!("option '{name}' has an empty value"),
                    ));
                } else {
                    given.insert(name, (index + 1, value.as_str()));
                }
                index += 2;
            }
        }
    }
    (given, repeated)
}

fn parse_ingest(text: &[String]) -> Result<Invocation, Vec<Diagnostic>> {
    let mut problems = Vec::new();
    let (given, _) = scan(text, &INGEST_OPTIONS, &mut problems);
    let missing_at = text.len();
    let mut require = |name: &'static str| -> Option<(usize, &str)> {
        let found = given.get(name).copied();
        if found.is_none() {
            problems.push(Diagnostic::new(
                Code::CliMissingArgument,
                format!("required option '{name}' is missing"),
                at_argument(missing_at, Some(name)),
            ));
        }
        found
    };
    let document = require("--document");
    let out = require("--out");

    let format = match given.get("--format").copied() {
        None => None,
        Some((_, "text")) => Some(DocumentKind::Text),
        Some((_, "markdown")) => Some(DocumentKind::Markdown),
        Some((at, other)) => {
            problems.push(invalid(
                at,
                Some("--format"),
                format!("format '{}' is not 'text' or 'markdown'", display(other)),
            ));
            None
        }
    };
    let max_bytes = match given.get("--max-bytes").copied() {
        None => None,
        Some((at, value)) => match value.parse::<u64>() {
            Ok(n) if n > 0 && value.bytes().all(|b| b.is_ascii_digit()) => Some(n),
            _ => {
                problems.push(invalid(
                    at,
                    Some("--max-bytes"),
                    format!(
                        "'{}' is not a positive whole number of bytes",
                        display(value)
                    ),
                ));
                None
            }
        },
    };
    let document_id = match (given.get("--document-id").copied(), document) {
        (Some((at, value)), _) => id_from(at, "--document-id", value, &mut problems),
        (None, Some((at, path))) => {
            id_from(at, "--document", &path.replace('\\', "/"), &mut problems)
        }
        (None, None) => None,
    };
    let candidates = given.get("--candidates").copied();
    // An output must never be the input, and the two outputs must differ.
    if let (Some((_, doc)), Some((at, o))) = (document, out) {
        if doc == o {
            problems.push(invalid(
                at,
                Some("--out"),
                "the output file is the input document".to_owned(),
            ));
        }
    }
    if let (Some((_, doc)), Some((at, c))) = (document, candidates) {
        if doc == c {
            problems.push(invalid(
                at,
                Some("--candidates"),
                "the candidates file is the input document".to_owned(),
            ));
        }
    }
    if let (Some((_, o)), Some((at, c))) = (out, candidates) {
        if o == c {
            problems.push(invalid(
                at,
                Some("--candidates"),
                "the candidates file is the same as the --out file".to_owned(),
            ));
        }
    }
    match (problems.is_empty(), document, out, document_id) {
        (true, Some((_, document)), Some((_, out)), Some(document_id)) => {
            Ok(Invocation::Ingest(IngestArgs {
                document: PathBuf::from(document),
                document_id,
                format,
                out: PathBuf::from(out),
                candidates: candidates.map(|(_, c)| PathBuf::from(c)),
                max_bytes,
            }))
        }
        _ => Err(problems),
    }
}

fn id_from(
    at: usize,
    option: &str,
    value: &str,
    problems: &mut Vec<Diagnostic>,
) -> Option<DocumentId> {
    match DocumentId::new(value) {
        Ok(id) => Some(id),
        Err(e) => {
            problems.push(invalid(
                at,
                Some(option),
                format!(
                    "'{}' is not a valid document identifier: {e}",
                    display(value)
                ),
            ));
            None
        }
    }
}

fn parse_validate(text: &[String]) -> Result<Invocation, Vec<Diagnostic>> {
    let mut problems = Vec::new();
    let (given, models) = scan(text, &VALIDATE_OPTIONS, &mut problems);
    let missing_at = text.len();
    let mut require = |name: &'static str| -> Option<(usize, &str)> {
        let found = given.get(name).copied();
        if found.is_none() {
            problems.push(Diagnostic::new(
                Code::CliMissingArgument,
                format!("required option '{name}' is missing"),
                at_argument(missing_at, Some(name)),
            ));
        }
        found
    };
    let rulebook = require("--rulebook");
    let ledger = require("--ledger");
    let out = require("--out");
    if models.is_empty() {
        problems.push(Diagnostic::new(
            Code::CliMissingArgument,
            "required option '--source-model' is missing".to_owned(),
            at_argument(missing_at, Some("--source-model")),
        ));
    }
    let output_format = match given.get("--output-format").copied() {
        None | Some((_, "human")) => OutputFormat::Human,
        Some((_, "json")) => OutputFormat::Json,
        Some((at, other)) => {
            problems.push(invalid(
                at,
                Some("--output-format"),
                format!("format '{}' is not 'human' or 'json'", display(other)),
            ));
            OutputFormat::Human
        }
    };
    let max_bytes = positive(&given, "--max-bytes", "bytes", &mut problems);
    let max_depth = positive(&given, "--max-depth", "levels", &mut problems)
        .and_then(|n| u32::try_from(n).ok().or(Some(u32::MAX)));
    let candidates = given.get("--candidates").copied();
    // The output must be none of the inputs.
    if let Some((at, o)) = out {
        let inputs = rulebook
            .into_iter()
            .chain(ledger)
            .chain(models.iter().copied())
            .chain(candidates);
        if inputs.into_iter().any(|(_, i)| i == o) {
            problems.push(invalid(
                at,
                Some("--out"),
                "the output file is also an input file".to_owned(),
            ));
        }
    }
    match (problems.is_empty(), rulebook, ledger, out) {
        (true, Some((_, rulebook)), Some((_, ledger)), Some((_, out))) => {
            Ok(Invocation::Validate(ValidateArgs {
                rulebook: PathBuf::from(rulebook),
                ledger: PathBuf::from(ledger),
                source_models: models.iter().map(|(_, m)| PathBuf::from(m)).collect(),
                candidates: candidates.map(|(_, c)| PathBuf::from(c)),
                out: PathBuf::from(out),
                output_format,
                max_bytes,
                max_depth,
            }))
        }
        _ => Err(problems),
    }
}

fn parse_compile(text: &[String]) -> Result<Invocation, Vec<Diagnostic>> {
    let mut problems = Vec::new();
    let (given, _) = scan(text, &COMPILE_OPTIONS, &mut problems);
    let missing_at = text.len();
    let mut require = |name: &'static str| -> Option<(usize, &str)> {
        let found = given.get(name).copied();
        if found.is_none() {
            problems.push(Diagnostic::new(
                Code::CliMissingArgument,
                format!("required option '{name}' is missing"),
                at_argument(missing_at, Some(name)),
            ));
        }
        found
    };
    let validation = require("--validation");
    let out = require("--out");
    if let (Some((_, v)), Some((at, o))) = (validation, out) {
        if v == o {
            problems.push(invalid(
                at,
                Some("--out"),
                "the output file is also an input file".to_owned(),
            ));
        }
    }
    match (problems.is_empty(), validation, out) {
        (true, Some((_, validation)), Some((_, out))) => Ok(Invocation::Compile(CompileArgs {
            validation: PathBuf::from(validation),
            out: PathBuf::from(out),
        })),
        _ => Err(problems),
    }
}

fn parse_evaluate(text: &[String]) -> Result<Invocation, Vec<Diagnostic>> {
    let mut problems = Vec::new();
    let (given, models) = scan(text, &EVALUATE_OPTIONS, &mut problems);
    let missing_at = text.len();
    let mut require = |name: &'static str| -> Option<(usize, &str)> {
        let found = given.get(name).copied();
        if found.is_none() {
            problems.push(Diagnostic::new(
                Code::CliMissingArgument,
                format!("required option '{name}' is missing"),
                at_argument(missing_at, Some(name)),
            ));
        }
        found
    };
    let compiled = require("--compiled");
    let data = require("--data");
    let out = require("--out");
    let output_format = match given.get("--output-format").copied() {
        None | Some((_, "human")) => OutputFormat::Human,
        Some((_, "json")) => OutputFormat::Json,
        Some((at, other)) => {
            problems.push(invalid(
                at,
                Some("--output-format"),
                format!("format '{}' is not 'human' or 'json'", display(other)),
            ));
            OutputFormat::Human
        }
    };
    let questions = given.get("--questions").copied();
    let published = given.get("--published").copied();
    let report = given.get("--proof-report").copied();
    let report_json = given.get("--proof-report-json").copied();
    let rulebook = given.get("--rulebook").copied();
    let ledger = given.get("--ledger").copied();
    // The proof report needs its own inputs, and they have no other use.
    let proof_needs: [(&str, bool); 3] = [
        ("--rulebook", rulebook.is_some()),
        ("--ledger", ledger.is_some()),
        ("--source-model", !models.is_empty()),
    ];
    if let Some((at, _)) = report {
        for (name, present) in proof_needs {
            if !present {
                problems.push(invalid(
                    at,
                    Some("--proof-report"),
                    format!("'--proof-report' needs '{name}'"),
                ));
            }
        }
    } else {
        for (name, found) in [
            ("--proof-report-json", report_json),
            ("--rulebook", rulebook),
            ("--ledger", ledger),
        ] {
            if let Some((at, _)) = found {
                problems.push(invalid(
                    at,
                    Some(name),
                    format!("'{name}' is only used with '--proof-report'"),
                ));
            }
        }
    }
    // No output file is an input file or another output file.
    let inputs: Vec<(usize, &str)> = compiled
        .into_iter()
        .chain(data)
        .chain(questions)
        .chain(published)
        .chain(models.iter().copied())
        .chain(rulebook)
        .chain(ledger)
        .collect();
    let outputs: Vec<(usize, &str, &'static str)> = out
        .map(|(at, p)| (at, p, "--out"))
        .into_iter()
        .chain(report.map(|(at, p)| (at, p, "--proof-report")))
        .chain(report_json.map(|(at, p)| (at, p, "--proof-report-json")))
        .collect();
    for (i, (at, path, name)) in outputs.iter().enumerate() {
        if inputs.iter().any(|(_, input)| input == path) {
            problems.push(invalid(
                *at,
                Some(name),
                "the output file is also an input file".to_owned(),
            ));
        } else if outputs.iter().take(i).any(|(_, other, _)| other == path) {
            problems.push(invalid(
                *at,
                Some(name),
                "the output file is also another output file".to_owned(),
            ));
        }
    }
    let proof = match (report, rulebook, ledger) {
        (Some((_, report)), Some((_, rulebook)), Some((_, ledger))) => Some(ProofArgs {
            rulebook: PathBuf::from(rulebook),
            ledger: PathBuf::from(ledger),
            report: PathBuf::from(report),
            report_json: report_json.map(|(_, p)| PathBuf::from(p)),
        }),
        _ => None,
    };
    match (problems.is_empty(), compiled, data, out) {
        (true, Some((_, compiled)), Some((_, data)), Some((_, out))) => {
            Ok(Invocation::Evaluate(EvaluateArgs {
                compiled: PathBuf::from(compiled),
                data: PathBuf::from(data),
                questions: questions.map(|(_, p)| PathBuf::from(p)),
                published: published.map(|(_, p)| PathBuf::from(p)),
                source_models: models.iter().map(|(_, m)| PathBuf::from(m)).collect(),
                out: PathBuf::from(out),
                output_format,
                render: given.contains_key("--render"),
                proof,
            }))
        }
        _ => Err(problems),
    }
}

/// A positive whole number option, or `None` when absent or invalid (a problem
/// is recorded).
fn positive(
    given: &Given<'_>,
    name: &'static str,
    unit: &str,
    problems: &mut Vec<Diagnostic>,
) -> Option<u64> {
    let (at, value) = given.get(name).copied()?;
    match value.parse::<u64>() {
        Ok(n) if n > 0 && value.bytes().all(|b| b.is_ascii_digit()) => Some(n),
        _ => {
            problems.push(invalid(
                at,
                Some(name),
                format!(
                    "'{}' is not a positive whole number of {unit}",
                    display(value)
                ),
            ));
            None
        }
    }
}

fn parse_review(text: &[String]) -> Result<Invocation, Vec<Diagnostic>> {
    let sub = text.get(1).map(String::as_str);
    let (known, kind): (&[&'static str], u8) = match sub {
        Some("list") => (&REVIEW_LIST_OPTIONS, 0),
        Some("approve") => (&REVIEW_APPROVE_OPTIONS, 1),
        Some("reject") => (&REVIEW_REJECT_OPTIONS, 2),
        other => {
            let shown =
                other.map_or_else(|| "review".to_owned(), |o| format!("review {}", display(o)));
            return Err(vec![Diagnostic::new(
                Code::CliUnknownCommand,
                "the review commands are: review list, review approve, review reject".to_owned(),
                Location::Cli(CliLocation::Command(shown)),
            )]);
        }
    };
    let mut problems = Vec::new();
    let (given, models) = scan_from(text, 2, known, &mut problems);
    let missing_at = text.len();
    let mut require = |name: &'static str| -> Option<(usize, &str)> {
        let found = given.get(name).copied();
        if found.is_none() {
            problems.push(Diagnostic::new(
                Code::CliMissingArgument,
                format!("required option '{name}' is missing"),
                at_argument(missing_at, Some(name)),
            ));
        }
        found
    };
    let rulebook = require("--rulebook");
    let ledger = require("--ledger");
    let rule_given = if kind == 0 {
        given.get("--rule").copied()
    } else {
        require("--rule")
    };
    let revision_given = if kind == 0 {
        given.get("--revision").copied()
    } else {
        require("--revision")
    };
    let (hash_given, reviewer_given, time_given) = if kind == 0 {
        (None, None, None)
    } else {
        (
            require("--content-hash"),
            require("--reviewer"),
            require("--time"),
        )
    };
    let reason_given = if kind == 2 { require("--reason") } else { None };
    let rule = rule_given.and_then(|(at, v)| match RuleId::new(v) {
        Ok(id) => Some(id),
        Err(e) => {
            problems.push(invalid(
                at,
                Some("--rule"),
                format!("'{}' is not a rule identifier: {e}", display(v)),
            ));
            None
        }
    });
    let revision = revision_given.and_then(|(at, v)| {
        let parsed = v
            .bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| v.parse::<u32>().ok())
            .flatten()
            .and_then(|n| RevisionNumber::new(n).ok());
        if parsed.is_none() {
            problems.push(invalid(
                at,
                Some("--revision"),
                format!(
                    "'{}' is not a revision number (1 to 4294967295)",
                    display(v)
                ),
            ));
        }
        parsed
    });
    if kind == 0 && revision_given.is_some() && rule_given.is_none() {
        if let Some((at, _)) = revision_given {
            problems.push(invalid(
                at,
                Some("--revision"),
                "'--revision' needs '--rule'".to_owned(),
            ));
        }
    }
    let content_hash = hash_given.and_then(|(at, v)| {
        let parsed = Sha256Hex::parse(v);
        if parsed.is_none() {
            problems.push(invalid(
                at,
                Some("--content-hash"),
                format!("'{}' is not 64 lowercase hexadecimal digits", display(v)),
            ));
        }
        parsed
    });
    match (problems.is_empty(), rulebook, ledger) {
        (true, Some((_, rulebook)), Some((_, ledger))) => {
            let command = if kind == 0 {
                ReviewCommand::List { rule, revision }
            } else {
                match (rule, revision, content_hash, reviewer_given, time_given) {
                    (
                        Some(rule),
                        Some(revision),
                        Some(content_hash),
                        Some((_, r)),
                        Some((_, t)),
                    ) => ReviewCommand::Decide(DecideArgs {
                        rule,
                        revision,
                        content_hash,
                        reviewer: r.to_owned(),
                        time: t.to_owned(),
                        create_ledger: given.contains_key("--create-ledger"),
                        reject_reason: reason_given.map(|(_, v)| v.to_owned()),
                    }),
                    _ => return Err(problems),
                }
            };
            Ok(Invocation::Review(ReviewArgs {
                rulebook: PathBuf::from(rulebook),
                ledger: PathBuf::from(ledger),
                source_models: models.iter().map(|(_, m)| PathBuf::from(m)).collect(),
                proposals: given.get("--proposals").map(|(_, v)| PathBuf::from(*v)),
                command,
            }))
        }
        _ => Err(problems),
    }
}
