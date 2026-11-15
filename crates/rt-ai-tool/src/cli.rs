//! The hand-written argument parser of `ruletrace-ai` (design 3.9.2).
//!
//! Options may appear in any order. A repeated option is an error except
//! `--source-model` and `--consent`. The form `--name=value` and `--` are
//! not supported. Every problem is a Diagnostic with a `Cli_Location`, and
//! all problems of one command line are reported. Parsing reads no file.

use crate::config::disabled_without_configuration;
use crate::consent::Consent;
use rt_foundation::{
    escape_controls, CliLocation, Code, Diagnostic, DocumentId, Location, RevisionNumber, RuleId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::PathBuf;

const OPTIONS: [&str; 11] = [
    "--config",
    "--rulebook",
    "--source-model",
    "--candidates",
    "--store",
    "--created-at",
    "--consent",
    "--revise-rule",
    "--revise-revision",
    "--feedback-file",
    "--dry-run",
];

const REPEATABLE: [&str; 2] = ["--source-model", "--consent"];
const FLAGS: [&str; 1] = ["--dry-run"];

/// A request to propose a revision of one named Rule_Revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviseArgs {
    /// The rule to revise.
    pub rule: RuleId,
    /// The revision the feedback is about.
    pub revision: RevisionNumber,
    /// The reviewer feedback file.
    pub feedback: PathBuf,
    /// Command-line position of the `--revise-rule` value.
    pub rule_at: usize,
    /// Command-line position of the `--revise-revision` value.
    pub revision_at: usize,
    /// Command-line position of the `--feedback-file` value.
    pub feedback_at: usize,
}

/// The parsed arguments of `ruletrace-ai propose`.
#[derive(Debug)]
pub struct ProposeArgs {
    /// The AI_Configuration file.
    pub config: PathBuf,
    /// The Rulebook whose declarations are sent.
    pub rulebook: PathBuf,
    /// The Source_Model files in the order given.
    pub source_models: Vec<PathBuf>,
    /// The Candidate_Clause list.
    pub candidates: PathBuf,
    /// The Proposal_Store directory.
    pub store: PathBuf,
    /// The caller-supplied creation and log time, `YYYY-MM-DDTHH:MM:SSZ`.
    pub created_at: String,
    /// The consent argument, naming the documents sent.
    pub consent: Consent,
    /// Print the requests that a run would send, and send nothing.
    pub dry_run: bool,
    /// A revision request, when given.
    pub revise: Option<ReviseArgs>,
}

pub(crate) fn at_argument(index: usize, name: Option<&str>) -> Location {
    Location::Cli(CliLocation::Argument {
        position: u32::try_from(index).unwrap_or(u32::MAX),
        name: name.map(str::to_owned),
    })
}

fn command_at() -> Location {
    Location::Cli(CliLocation::Command("propose".to_owned()))
}

fn invalid(index: usize, name: Option<&str>, message: String) -> Diagnostic {
    Diagnostic::new(Code::CliArgumentInvalid, message, at_argument(index, name))
}

fn missing(name: &str) -> Diagnostic {
    Diagnostic::new(
        Code::CliMissingArgument,
        format!("required option '{name}' is missing"),
        command_at(),
    )
}

/// Whether `text` has the shape `YYYY-MM-DDTHH:MM:SSZ` with ASCII digits.
/// The shape is the only check; no calendar or clock is consulted.
fn time_shape_ok(text: &str) -> bool {
    let shape = b"dddd-dd-ddTdd:dd:ddZ";
    text.len() == shape.len()
        && text.bytes().zip(shape).all(|(b, s)| match s {
            b'd' => b.is_ascii_digit(),
            other => b == *other,
        })
}

type Given<'a> = BTreeMap<&'static str, (usize, &'a str)>;
type Repeated<'a> = Vec<(usize, &'static str, &'a str)>;

fn scan<'a>(text: &'a [String], problems: &mut Vec<Diagnostic>) -> (Given<'a>, Repeated<'a>) {
    let mut given: Given<'a> = BTreeMap::new();
    let mut repeated: Repeated<'a> = Vec::new();
    let mut index = 1;
    while let Some(arg) = text.get(index) {
        let Some(name) = OPTIONS.iter().find(|k| **k == arg.as_str()) else {
            let message = if arg == "--" {
                "'--' is not supported".to_owned()
            } else if arg.starts_with("--") {
                format!("unknown option '{}'", escape_controls(arg))
            } else {
                format!("unexpected argument '{}'", escape_controls(arg))
            };
            problems.push(invalid(index, Some(&escape_controls(arg)), message));
            index += 1;
            continue;
        };
        if FLAGS.contains(name) {
            if given.insert(name, (index, "")).is_some() {
                problems.push(invalid(
                    index,
                    Some(name),
                    format!("option '{name}' is given more than once"),
                ));
            }
            index += 1;
            continue;
        }
        let value = text.get(index + 1).filter(|v| !v.starts_with("--"));
        let Some(value) = value else {
            problems.push(invalid(
                index,
                Some(name),
                format!("option '{name}' needs a value"),
            ));
            index += 1;
            continue;
        };
        if value.is_empty() {
            problems.push(invalid(
                index + 1,
                Some(name),
                format!("option '{name}' has an empty value"),
            ));
        } else if REPEATABLE.contains(name) {
            repeated.push((index + 1, name, value.as_str()));
        } else if given.contains_key(name) {
            problems.push(invalid(
                index,
                Some(name),
                format!("option '{name}' is given more than once"),
            ));
        } else {
            given.insert(name, (index + 1, value.as_str()));
        }
        index += 2;
    }
    (given, repeated)
}

fn consent_of(repeated: &Repeated<'_>, problems: &mut Vec<Diagnostic>) -> Consent {
    let mut documents: BTreeSet<DocumentId> = BTreeSet::new();
    let mut first: Option<usize> = None;
    for (at, _, value) in repeated.iter().filter(|(_, n, _)| *n == "--consent") {
        first.get_or_insert(*at);
        match DocumentId::new(*value) {
            Ok(id) => {
                documents.insert(id);
            }
            Err(e) => problems.push(invalid(
                *at,
                Some("--consent"),
                format!(
                    "'{}' is not a valid document identifier: {e}",
                    escape_controls(value)
                ),
            )),
        }
    }
    match first {
        Some(at) => Consent::named(documents, at_argument(at, Some("--consent"))),
        None => Consent::absent(command_at()),
    }
}

fn revise_of(given: &Given<'_>, problems: &mut Vec<Diagnostic>) -> Option<ReviseArgs> {
    let names = ["--revise-rule", "--revise-revision", "--feedback-file"];
    if names.iter().all(|n| !given.contains_key(n)) {
        return None;
    }
    for name in names.iter().filter(|n| !given.contains_key(*n)) {
        problems.push(missing(name));
    }
    let (Some((rule_at, rule)), Some((rev_at, rev)), Some((feedback_at, feedback))) = (
        given.get("--revise-rule"),
        given.get("--revise-revision"),
        given.get("--feedback-file"),
    ) else {
        return None;
    };
    let rule = RuleId::new(*rule).map_err(|e| {
        problems.push(invalid(
            *rule_at,
            Some("--revise-rule"),
            format!("'{}' is not a rule identifier: {e}", escape_controls(rule)),
        ));
    });
    let revision = rev
        .bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| rev.parse::<u32>().ok())
        .flatten()
        .and_then(|n| RevisionNumber::new(n).ok())
        .ok_or_else(|| {
            problems.push(invalid(
                *rev_at,
                Some("--revise-revision"),
                format!(
                    "'{}' is not a revision number (1 to 4294967295)",
                    escape_controls(rev)
                ),
            ));
        });
    Some(ReviseArgs {
        rule: rule.ok()?,
        revision: revision.ok()?,
        feedback: PathBuf::from(feedback),
        rule_at: *rule_at,
        revision_at: *rev_at,
        feedback_at: *feedback_at,
    })
}

/// Parses the arguments after the program name. The command name is at
/// position 0.
///
/// # Errors
/// All Diagnostics of the command line, in the order found. A missing
/// `--config` is `RT-AI-0001` (no configuration named), the others are
/// `RT-CLI-0001`, `RT-CLI-0002` or `RT-CLI-0004`. All have exit code 2.
pub fn parse(args: &[OsString]) -> Result<ProposeArgs, Vec<Diagnostic>> {
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
    match text.first().map(String::as_str) {
        None => {
            return Err(vec![Diagnostic::new(
                Code::CliMissingArgument,
                "no command given; the command is: propose".to_owned(),
                at_argument(0, None),
            )])
        }
        Some("propose") => {}
        Some(other) => {
            return Err(vec![Diagnostic::new(
                Code::CliUnknownCommand,
                format!(
                    "unknown command '{}'; the command is: propose",
                    escape_controls(other)
                ),
                Location::Cli(CliLocation::Command(escape_controls(other))),
            )])
        }
    }
    let mut problems = Vec::new();
    let (given, repeated) = scan(&text, &mut problems);
    let path =
        |name: &'static str| -> Option<PathBuf> { given.get(name).map(|(_, v)| PathBuf::from(*v)) };
    let config = path("--config");
    let rulebook = path("--rulebook");
    let candidates = path("--candidates");
    let store = path("--store");
    let source_models: Vec<PathBuf> = repeated
        .iter()
        .filter(|(_, n, _)| *n == "--source-model")
        .map(|(_, _, v)| PathBuf::from(*v))
        .collect();
    if config.is_none() {
        problems.push(disabled_without_configuration(command_at()));
    }
    for (name, found) in [
        ("--rulebook", rulebook.is_some()),
        ("--source-model", !source_models.is_empty()),
        ("--candidates", candidates.is_some()),
        ("--store", store.is_some()),
        ("--created-at", given.contains_key("--created-at")),
    ] {
        if !found {
            problems.push(missing(name));
        }
    }
    let created_at = given.get("--created-at").and_then(|(at, v)| {
        if time_shape_ok(v) {
            Some((*v).to_owned())
        } else {
            problems.push(invalid(
                *at,
                Some("--created-at"),
                format!(
                    "'{}' does not have the shape YYYY-MM-DDTHH:MM:SSZ",
                    escape_controls(v)
                ),
            ));
            None
        }
    });
    let consent = consent_of(&repeated, &mut problems);
    let revise = revise_of(&given, &mut problems);
    if !problems.is_empty() {
        return Err(problems);
    }
    match (config, rulebook, candidates, store, created_at) {
        (Some(config), Some(rulebook), Some(candidates), Some(store), Some(created_at)) => {
            Ok(ProposeArgs {
                config,
                rulebook,
                source_models,
                candidates,
                store,
                created_at,
                consent,
                dry_run: given.contains_key("--dry-run"),
                revise,
            })
        }
        _ => Err(vec![Diagnostic::new(
            Code::InternalError,
            "a required argument is missing after the argument checks".to_owned(),
            command_at(),
        )]),
    }
}
