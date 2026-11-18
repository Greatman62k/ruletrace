//! `ruletrace evaluate`: stages 6 and 7 (Req 10 to 13, 16.1, 16.9, 16.11,
//! 16.23, 17.1).
//!
//! The command loads every input (all invalid input is exit 2 and writes
//! nothing), runs the engine on in-memory values, lets the Explainer verify
//! each Decision_Trace against the Source_Models, and writes the results file
//! with `rt_explain::results::serialize_results`. `--render` and
//! `--proof-report` add output and never change a byte of the results file (Req
//! 13.19): the results come from the engine and the Explainer only.
//!
//! The exit code follows the earliest failing stage (Req 16.14, 16.17):
//!
//! 1. Invalid input, including a Question that names something the inputs do
//!    not have (`RT-EXE-0010`) and a Rulebook whose Effective revisions do not
//!    hash to the `rulebook_hash` of the Compiled_Rulebook (`RT-PRV-0008`):
//!    exit 2, nothing is written.
//! 2. Stage 6: any Indeterminate Outcome, including `RT-DGN-0004` and
//!    `RT-DGN-0005`, is exit 4 and never 3 (Req 16.23). The complete results
//!    are written.
//! 3. Stage 7: a provenance Error row of Req 13.16 is exit 2. The complete
//!    Outcomes and Decision_Traces are written, each affected trace marked as
//!    not verified (Req 13.18).
//!
//! Warnings (a missing Source_Model set, a published value that differs) never
//! decide.

use crate::args::EvaluateArgs;
use crate::io::{commit_files, input_location, Fs};
use crate::validate::{collect, exit_code_of, input_id, load, load_sources, read_input};
use rt_compiled::{parse_compiled, CompiledBody, CompiledRulebook};
use rt_data::{
    default_parse_limits, default_questions, parse_data, parse_published, parse_questions,
    CompetitionData, DataLimits, ParticipantId, PublishedResult, PublishedValue, Question,
    TargetName,
};
use rt_engine::{evaluate, Comparison, Computed, DifferenceKind, Evaluation};
use rt_explain::report::{
    build_proof_report, render_proof_human, render_proof_json, ApprovalFacts, ApprovalView,
    ProofInputs, PublishedFact,
};
use rt_explain::results::{serialize_results, ResultsInput};
use rt_explain::{classify_evaluation, render_trace, verify_trace, EvaluationClass, Verification};
use rt_foundation::{
    Code, Diagnostic, DiagnosticState, ExitCode, InputId, InputLocation, Location,
    MAX_COMPETITION_DATA_BYTES, MAX_CONTAINER_DEPTH,
};
use rt_ir::Rulebook;
use rt_ledger::{derive_statuses, effective_revisions, ApprovalLedger, RecordKind};
use rt_numeric::to_canonical;
use rt_srcmodel::SourceModels;
use rt_trace::TraceValue;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The largest Compiled_Rulebook file read.
const MAX_COMPILED_BYTES: u64 = 64 * 1024 * 1024;

/// What the command produced.
pub(crate) struct Finished {
    /// Standard output: the rendered traces with `--render`.
    pub(crate) stdout: Vec<u8>,
    /// Every Diagnostic.
    pub(crate) state: DiagnosticState,
    /// The exit code.
    pub(crate) code: ExitCode,
}

fn failed(found: Vec<Diagnostic>) -> Finished {
    let mut state = DiagnosticState::empty();
    for d in found {
        state.push(d);
    }
    let state = state.sorted();
    let code = match exit_code_of(&state) {
        ExitCode::Success => ExitCode::Internal,
        code => code,
    };
    Finished {
        stdout: Vec::new(),
        state,
        code,
    }
}

fn internal(message: String) -> Diagnostic {
    Diagnostic::new(
        Code::InternalError,
        message,
        Location::None(Code::InternalError),
    )
}

/// The files `--proof-report` needs besides the Source_Models.
struct ProofFiles {
    rulebook: Rulebook,
    ledger: ApprovalLedger,
    rulebook_path: PathBuf,
}

struct Loaded {
    compiled: CompiledRulebook,
    data: CompetitionData,
    questions: Option<Vec<Question>>,
    published: Option<PublishedResult>,
    published_input: Option<InputId>,
    sources: SourceModels,
    proof: Option<ProofFiles>,
}

fn read_with<T>(
    fs: &dyn Fs,
    path: &Path,
    limit: u64,
    parse: impl FnOnce(&InputId, &[u8]) -> Result<T, DiagnosticState>,
) -> Result<T, Vec<Diagnostic>> {
    let input = input_id(path).map_err(|d| vec![*d])?;
    let bytes = read_input(fs, path, limit).map_err(|d| vec![*d])?;
    parse(&input, &bytes).map_err(|state| state.as_slice().to_vec())
}

fn load_all(args: &EvaluateArgs, fs: &dyn Fs) -> Result<Loaded, DiagnosticState> {
    let mut problems = DiagnosticState::empty();
    // The Rulebook and the ledger are read once as one pair, or not at all.
    let pair = match &args.proof {
        None => None,
        Some(p) => match crate::commit::read_pair(fs, &p.ledger, &p.rulebook) {
            Ok(pair) => Some(pair),
            Err(blocked) => {
                problems.push(*blocked);
                return Err(problems);
            }
        },
    };
    let compiled = collect(
        read_with(fs, &args.compiled, MAX_COMPILED_BYTES, |input, bytes| {
            parse_compiled(bytes, input, MAX_COMPILED_BYTES, MAX_CONTAINER_DEPTH)
        }),
        &mut problems,
    );
    let data = collect(
        read_with(
            fs,
            &args.data,
            MAX_COMPETITION_DATA_BYTES,
            |input, bytes| parse_data(input, bytes, DataLimits::default()),
        ),
        &mut problems,
    );
    let limits = default_parse_limits();
    let questions = match &args.questions {
        None => Some(None),
        Some(path) => collect(
            read_with(fs, path, limits.max_bytes(), |input, bytes| {
                parse_questions(input, bytes, limits)
            }),
            &mut problems,
        )
        .map(Some),
    };
    let published = match &args.published {
        None => Some(None),
        Some(path) => collect(
            read_with(fs, path, limits.max_bytes(), |input, bytes| {
                parse_published(input, bytes, limits)
            }),
            &mut problems,
        )
        .map(Some),
    };
    let published_input = args
        .published
        .as_deref()
        .and_then(|path| input_id(path).ok());
    // With a proof report the Rulebook and ledger are read with the
    // Source_Models, by the code that `validate` uses, so both commands accept
    // the same files.
    let (sources, proof) = match &args.proof {
        None => (load_sources(&args.source_models, fs, &mut problems), None),
        Some(proof) => {
            let as_validate = crate::args::ValidateArgs {
                rulebook: proof.rulebook.clone(),
                ledger: proof.ledger.clone(),
                source_models: args.source_models.clone(),
                candidates: None,
                out: PathBuf::new(),
                output_format: crate::args::OutputFormat::Human,
                max_bytes: None,
                max_depth: None,
            };
            match load(&as_validate, pair.as_ref().map_or(fs, |p| p as &dyn Fs)) {
                Ok(inputs) => (
                    Some(inputs.sources),
                    Some(ProofFiles {
                        rulebook: inputs.rulebook,
                        ledger: inputs.ledger,
                        rulebook_path: proof.rulebook.clone(),
                    }),
                ),
                Err(found) => {
                    problems.extend(found);
                    (None, None)
                }
            }
        }
    };
    match (compiled, data, questions, published, sources) {
        (Some(compiled), Some(data), Some(questions), Some(published), Some(sources))
            if problems.as_slice().is_empty() =>
        {
            Ok(Loaded {
                compiled,
                data,
                questions,
                published,
                published_input,
                sources,
                proof,
            })
        }
        _ => Err(problems),
    }
}

/// The value and ranking Targets of the Compiled_Rulebook (RN-6).
fn targets(
    compiled: &CompiledRulebook,
) -> Result<(BTreeSet<TargetName>, BTreeSet<TargetName>), Box<Diagnostic>> {
    let mut values = BTreeSet::new();
    let mut rankings = BTreeSet::new();
    for rule in compiled.rules() {
        let set = match rule.body {
            CompiledBody::Eligibility { .. }
            | CompiledBody::Derived { .. }
            | CompiledBody::WeightedScore { .. } => &mut values,
            CompiledBody::Ranking { .. } => &mut rankings,
            CompiledBody::Default { .. } => continue,
        };
        let name = TargetName::new(rule.target.as_str()).map_err(|e| {
            Box::new(internal(format!(
                "the Target '{}' of rule {} is not a valid Target name: {e}",
                rt_foundation::escape_controls(rule.target.as_str()),
                rule.id
            )))
        })?;
        set.insert(name);
    }
    Ok((values, rankings))
}

/// Checks that the Rulebook's Effective revisions are the ones the
/// Compiled_Rulebook was made from, and reads the Approval_Record of each of
/// them (design §3.9.1).
fn approval_facts(
    compiled: &CompiledRulebook,
    proof: &ProofFiles,
    sources: &SourceModels,
) -> Result<ApprovalFacts, Box<Diagnostic>> {
    let rb = &proof.rulebook;
    let result = rt_validate::validate(rb, &proof.ledger, sources, None);
    let same = rt_compile::compile(&result)
        .is_ok_and(|staged| staged.value().rulebook_hash() == compiled.rulebook_hash());
    if !same {
        return Err(Box::new(Diagnostic::new(
            Code::ProofApprovalFactsMismatch,
            "the Effective revisions of the Rulebook, derived from the ledger, do not compile to the Rulebook hash of the Compiled_Rulebook, so no Proof_Report is written".to_owned(),
            input_location(&proof.rulebook_path),
        )));
    }
    let statuses = derive_statuses(rb, &proof.ledger);
    let mut facts = ApprovalFacts::new();
    for (id, key) in effective_revisions(rb, &statuses) {
        let Some(revision) = rb.revisions.iter().find(|r| r.key == key) else {
            continue;
        };
        let Ok(hash) = rt_ir::rule_content_hash(&rb.ir_version, revision, &rb.documents) else {
            continue;
        };
        let record = proof.ledger.records().iter().rev().find(|r| {
            r.kind() == RecordKind::Approval
                && r.rule() == &id
                && r.revision() == key.revision
                && r.content_hash() == &hash
        });
        if let Some(record) = record {
            facts.insert(
                id,
                key.revision,
                ApprovalView {
                    reviewer: record.reviewer().as_str().to_owned(),
                    time: record.time().to_owned(),
                    content_hash: hash.as_str().to_owned(),
                },
            );
        }
    }
    Ok(facts)
}

fn published_text(v: &PublishedValue) -> String {
    match v {
        PublishedValue::Bool(b) => b.to_string(),
        PublishedValue::Number(n) => to_canonical(n.value()),
    }
}

fn computed_text(v: &Computed) -> String {
    match v {
        Computed::Value(TraceValue::Number(n)) => to_canonical(n),
        Computed::Value(TraceValue::Bool(b)) => b.to_string(),
        Computed::Value(_) => "a value of another type".to_owned(),
        Computed::Indeterminate => "Indeterminate".to_owned(),
    }
}

/// The published-result comparison of each Question, rendered as text (design
/// §8.11.9). The first Question that answers a (participant, Target) pair owns
/// the comparison of that pair, as in the engine.
fn published_facts(
    questions: &[Question],
    data: &CompetitionData,
    comparison: &Comparison,
) -> Vec<PublishedFact> {
    let compared: BTreeSet<_> = comparison.compared.iter().collect();
    let differing: BTreeMap<_, _> = comparison
        .differences
        .iter()
        .map(|d| ((&d.participant, &d.target), d))
        .collect();
    let mut claimed: BTreeSet<(&ParticipantId, &TargetName)> = BTreeSet::new();
    questions
        .iter()
        .map(|question| {
            let (target, ids, labelled): (&TargetName, Vec<&ParticipantId>, bool) = match question {
                Question::Value {
                    target,
                    participant,
                } => (target, vec![participant], false),
                Question::Rank {
                    target,
                    participants: Some(set),
                } => (target, set.iter().collect(), true),
                Question::Rank {
                    target,
                    participants: None,
                } => (target, data.participants().keys().collect(), true),
            };
            let mine: Vec<&ParticipantId> = ids
                .into_iter()
                .filter(|id| compared.contains(&((*id).clone(), target.clone())))
                .filter(|id| claimed.insert((*id, target)))
                .collect();
            if mine.is_empty() {
                return PublishedFact::NotCompared;
            }
            let mut published = Vec::new();
            let mut computed = Vec::new();
            for id in mine {
                if let Some(d) = differing.get(&(id, target)) {
                    let (p, c) = (published_text(&d.published), computed_text(&d.computed));
                    if labelled {
                        published.push(format!("{id}: {p}"));
                        computed.push(format!("{id}: {c}"));
                    } else {
                        published.push(p);
                        computed.push(c);
                    }
                }
            }
            if published.is_empty() {
                PublishedFact::Equal
            } else {
                PublishedFact::Differs {
                    published: published.join("; "),
                    computed: computed.join("; "),
                }
            }
        })
        .collect()
}

/// The catalogue code of one difference kind (one failure mode, one code).
///
/// Public so that a cross-crate test can compare it with the Explainer's
/// table of kinds.
pub fn difference_code(kind: DifferenceKind) -> Code {
    match kind {
        DifferenceKind::ValueDiffers => Code::PublishedResultDiffers,
        DifferenceKind::TieBrokenWithoutDeclaredCriterion => Code::PublishedTieGroupSplit,
        DifferenceKind::TieNotInComputedResult => Code::PublishedTieNotComputed,
    }
}

/// One Warning for each published difference, coded by its kind.
fn published_warnings(comparison: &Comparison, input: Option<&InputId>) -> Vec<Diagnostic> {
    comparison
        .differences
        .iter()
        .map(|d| {
            let code = difference_code(d.kind);
            let location = input.map_or(Location::None(code), |input| {
                Location::Input(InputLocation {
                    input: input.clone(),
                    line: None,
                    column: None,
                    field: Some(format!("entries[{}/{}]", d.participant, d.target)),
                })
            });
            Diagnostic::new(code, d.message.clone(), location)
        })
        .collect()
}

/// Runs the command.
pub(crate) fn run(args: &EvaluateArgs, fs: &dyn Fs) -> Finished {
    let loaded = match load_all(args, fs) {
        Ok(loaded) => loaded,
        Err(problems) => return failed(problems.as_slice().to_vec()),
    };
    let approvals = match &loaded.proof {
        None => None,
        Some(proof) => match approval_facts(&loaded.compiled, proof, &loaded.sources) {
            Ok(facts) => Some(facts),
            Err(d) => return failed(vec![*d]),
        },
    };
    let questions = match &loaded.questions {
        Some(questions) => questions.clone(),
        None => match targets(&loaded.compiled) {
            Ok((values, rankings)) => default_questions(&loaded.data, &values, &rankings),
            Err(d) => return failed(vec![*d]),
        },
    };
    let evaluation = evaluate(
        &loaded.compiled,
        &loaded.data,
        &questions,
        loaded.published.as_ref(),
    );
    let verifications: Vec<Verification> = evaluation
        .traces
        .iter()
        .map(|trace| verify_trace(trace, &loaded.sources))
        .collect();
    let (state, code) = classify(&evaluation, &verifications, &loaded);
    if code == ExitCode::InvalidInput && state.as_slice().iter().any(is_question_fault) {
        // A Question the inputs cannot answer is invalid input: nothing is
        // written.
        return Finished {
            stdout: Vec::new(),
            state,
            code,
        };
    }
    let results = match serialize_results(&ResultsInput {
        outcomes: &evaluation.outcomes,
        traces: &evaluation.traces,
        diagnostics: &evaluation.diagnostics,
        verifications: &verifications,
    }) {
        Ok(bytes) => bytes,
        Err(e) => {
            return failed(vec![internal(format!(
                "the results file cannot be built: {e}"
            ))])
        }
    };
    let mut files: Vec<(&Path, Vec<u8>)> = vec![(args.out.as_path(), results)];
    if let (Some(proof), Some(approvals)) = (&args.proof, &approvals) {
        let facts = loaded
            .published
            .as_ref()
            .and(evaluation.comparison.as_ref())
            .map(|c| published_facts(&questions, &loaded.data, c))
            .unwrap_or_default();
        let report = build_proof_report(&ProofInputs {
            traces: &evaluation.traces,
            verifications: &verifications,
            approvals,
            published: &facts,
            rulebook_hash: loaded.compiled.rulebook_hash().as_str(),
            synthetic_data: None,
        });
        let report = match report {
            Ok(report) => report,
            Err(e) => {
                return failed(vec![internal(format!(
                    "the Proof_Report cannot be built: {e}"
                ))])
            }
        };
        files.push((
            proof.report.as_path(),
            render_proof_human(&report).into_bytes(),
        ));
        if let Some(path) = &proof.report_json {
            files.push((path.as_path(), render_proof_json(&report)));
        }
    }
    let write: Vec<(&Path, &[u8])> = files.iter().map(|(p, b)| (*p, b.as_slice())).collect();
    if let Err(d) = commit_files(fs, &write) {
        let mut state = state;
        state.push(*d);
        return Finished {
            stdout: Vec::new(),
            state: state.sorted(),
            code: crate::validate::later_failure(code),
        };
    }
    let stdout = if args.render {
        evaluation
            .traces
            .iter()
            .zip(&verifications)
            .map(|(trace, verification)| render_trace(trace, verification))
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes()
    } else {
        Vec::new()
    };
    Finished {
        stdout,
        state,
        code,
    }
}

fn is_question_fault(d: &Diagnostic) -> bool {
    d.code() == Code::QuestionInvalid
}

/// The Diagnostics to report and the exit code (see the module documentation).
fn classify(
    evaluation: &Evaluation,
    verifications: &[Verification],
    loaded: &Loaded,
) -> (DiagnosticState, ExitCode) {
    let mut all: BTreeSet<Diagnostic> = evaluation.diagnostics.as_slice().iter().cloned().collect();
    for v in verifications {
        all.extend(v.diagnostics().as_slice().iter().cloned());
    }
    if let Some(comparison) = &evaluation.comparison {
        all.extend(published_warnings(
            comparison,
            loaded.published_input.as_ref(),
        ));
    }
    let mut state = DiagnosticState::empty();
    for d in all {
        state.push(d);
    }
    let state = state.sorted();
    let code =
        match classify_evaluation(&evaluation.outcomes, &evaluation.diagnostics, verifications) {
            EvaluationClass::InvalidInput => ExitCode::InvalidInput,
            EvaluationClass::Indeterminate => ExitCode::Indeterminate,
            EvaluationClass::Success => ExitCode::Success,
        };
    (state, code)
}
