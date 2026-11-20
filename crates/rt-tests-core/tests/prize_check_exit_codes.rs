//! Req 31.12 and 31.5 for the `prize-check` library: its result
//! classification equals the exit code that `ruletrace evaluate` (the real
//! `rt_cli::run` over an in-memory file system) gives for the same
//! Compiled_Rulebook, Competition_Data, Questions, Published_Result and
//! Source_Models, and its results bytes equal the file `ruletrace evaluate`
//! writes.
//!
//! The mapping of the classification to 0, 4, 2 and 70 is written out here,
//! independently of the library.
//!
//! **Validates: Requirements 31.5, 31.12**

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use prize_check::{evaluate, FileInputs, NamedFile, PrizeInputs, PrizeRequest, ResultClass};
use rt_cli::{run, Fs};
use rt_compiled::parse_compiled;
use rt_data::tabular::MappingOptions;
use rt_data::{Question, TargetName};
use rt_explain::results::{serialize_results, ResultsInput};
use rt_explain::verify_trace;
use rt_foundation::{DocumentId, InputId, MAX_CONTAINER_DEPTH};
use rt_srcmodel::{default_load_limits, DocumentKind, SourceModel, SourceModels};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

const FIXTURE: &str = "../../tests/fixtures/slice-basic";
const CSV_FULL: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,7,9\np3,15,true,6,6\np4,30,false,9,6\n";
const CSV_MISSING_AGE: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,,true,7,9\np3,15,true,6,6\np4,30,false,9,6\n";

#[derive(Default)]
struct MemFs {
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
}

struct NoLock;

impl rt_cli::io::LockHeld for NoLock {}

impl MemFs {
    fn put(&self, name: &str, bytes: &[u8]) {
        self.files
            .borrow_mut()
            .insert(PathBuf::from(name), bytes.to_vec());
    }

    fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.files.borrow().get(Path::new(name)).cloned()
    }
}

impl Fs for MemFs {
    fn try_lock(&self, _path: &Path) -> io::Result<Box<dyn rt_cli::io::LockHeld>> {
        Ok(Box::new(NoLock))
    }

    fn try_lock_shared(&self, path: &Path) -> io::Result<Box<dyn rt_cli::io::LockHeld>> {
        if self.exists(path)? {
            Ok(Box::new(NoLock))
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn len(&self, path: &Path) -> io::Result<u64> {
        let files = self.files.borrow();
        let bytes = files.get(path).ok_or(io::ErrorKind::NotFound)?;
        Ok(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
    }

    fn read(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        let files = self.files.borrow();
        let bytes = files.get(path).ok_or(io::ErrorKind::NotFound)?;
        Ok(bytes
            .iter()
            .take(usize::try_from(max_bytes).unwrap_or(usize::MAX))
            .copied()
            .collect())
    }

    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut files = self.files.borrow_mut();
        if files.contains_key(path) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        files.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut files = self.files.borrow_mut();
        let bytes = files.remove(from).ok_or(io::ErrorKind::NotFound)?;
        files.insert(to.to_path_buf(), bytes);
        Ok(())
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        self.files
            .borrow_mut()
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(FIXTURE)
            .join(name),
    )
    .unwrap()
}

fn input(name: &str) -> InputId {
    InputId::new(name).unwrap()
}

fn target(name: &str) -> TargetName {
    TargetName::new(name).unwrap()
}

fn real_sources() -> SourceModels {
    let model = SourceModel::load(
        &input("model.json"),
        &fixture("expected/source-model.json"),
        default_load_limits(),
    )
    .unwrap();
    let mut models = SourceModels::new();
    models.insert(model).unwrap();
    models
}

fn other_document_sources() -> SourceModels {
    let model = SourceModel::new(
        DocumentId::new("other.md").unwrap(),
        DocumentKind::Markdown,
        "unrelated".to_owned(),
        Vec::new(),
    )
    .unwrap();
    let mut models = SourceModels::new();
    models.insert(model).unwrap();
    models
}

/// The documented exit codes of Req 31.12.
fn exit_code(class: ResultClass) -> u8 {
    match class {
        ResultClass::Complete => 0,
        ResultClass::Indeterminate => 4,
        ResultClass::InvalidInput => 2,
        ResultClass::Internal => 70,
    }
}

fn full_request() -> PrizeRequest {
    PrizeRequest {
        eligibility: [(target("eligible"), 2)].into(),
        score: [(target("score"), 4)].into(),
        ranking: [(target("standing"), 6)].into(),
    }
}

/// Runs `ruletrace evaluate` over the files and returns its exit code and the
/// results file, if it wrote one.
fn ruletrace_evaluate(
    data: &[u8],
    questions: &[u8],
    published: Option<&[u8]>,
    model: Option<&SourceModels>,
) -> (u8, Option<Vec<u8>>) {
    let fs = MemFs::default();
    fs.put("compiled.json", &fixture("expected/compiled.json"));
    fs.put("data.json", data);
    fs.put("questions.json", questions);
    let mut args: Vec<String> = [
        "evaluate",
        "--compiled",
        "compiled.json",
        "--data",
        "data.json",
        "--questions",
        "questions.json",
        "--out",
        "results.json",
    ]
    .map(str::to_owned)
    .to_vec();
    if let Some(bytes) = published {
        fs.put("published.json", bytes);
        args.extend(["--published".to_owned(), "published.json".to_owned()]);
    }
    if let Some(models) = model {
        for (i, m) in models.iter().enumerate() {
            let name = format!("model{i}.json");
            fs.put(&name, m.print().as_bytes());
            args.extend(["--source-model".to_owned(), name]);
        }
    }
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let outcome = run(&args, &fs);
    (outcome.code.value(), fs.get("results.json"))
}

struct Case {
    name: &'static str,
    csv: &'static str,
    published: Option<&'static str>,
    sources: Option<SourceModels>,
    expected_code: u8,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "complete, provenance verified",
            csv: CSV_FULL,
            published: None,
            sources: Some(real_sources()),
            expected_code: 0,
        },
        Case {
            name: "complete, no source models (Warning only)",
            csv: CSV_FULL,
            published: None,
            sources: None,
            expected_code: 0,
        },
        Case {
            name: "complete, a published value differs (Warning only)",
            csv: CSV_FULL,
            published: Some("participant,target,value\np2,score,7.9\n"),
            sources: Some(real_sources()),
            expected_code: 0,
        },
        Case {
            name: "indeterminate: p2 has no age",
            csv: CSV_MISSING_AGE,
            published: None,
            sources: Some(real_sources()),
            expected_code: 4,
        },
        Case {
            name: "provenance error: the cited document has no model",
            csv: CSV_FULL,
            published: None,
            sources: Some(other_document_sources()),
            expected_code: 2,
        },
        Case {
            name: "indeterminate outranks a provenance error",
            csv: CSV_MISSING_AGE,
            published: None,
            sources: Some(other_document_sources()),
            expected_code: 4,
        },
    ]
}

#[test]
fn the_classification_equals_the_exit_code_of_ruletrace_evaluate() {
    for case in cases() {
        let compiled = parse_compiled(
            &fixture("expected/compiled.json"),
            &input("compiled.json"),
            64 * 1024 * 1024,
            MAX_CONTAINER_DEPTH,
        )
        .unwrap();
        let none = SourceModels::new();
        let (pid, qid) = (input("participants.csv"), input("published.csv"));
        let options = MappingOptions::default();
        let request = full_request();
        let run = evaluate(&PrizeInputs {
            compiled: &compiled,
            request: &request,
            files: FileInputs {
                participants: NamedFile {
                    bytes: case.csv.as_bytes(),
                    name: &pid,
                },
                collections: None,
                published: case.published.map(|text| NamedFile {
                    bytes: text.as_bytes(),
                    name: &qid,
                }),
                options: &options,
            },
            sources: case.sources.as_ref().unwrap_or(&none),
            statements: &[],
            synthetic_data: None,
        })
        .unwrap();
        let prepared = run.prepared();
        let (cli_code, results) = ruletrace_evaluate(
            &prepared.data_bytes(),
            &prepared.questions_bytes(),
            prepared.published_bytes().as_deref(),
            case.sources.as_ref(),
        );
        assert_eq!(cli_code, case.expected_code, "{}: ruletrace", case.name);
        assert_eq!(
            exit_code(run.class()),
            case.expected_code,
            "{}: library",
            case.name
        );
        let evaluation = run.evaluation();
        let own = serialize_results(&ResultsInput {
            outcomes: &evaluation.outcomes,
            traces: &evaluation.traces,
            diagnostics: &evaluation.diagnostics,
            verifications: run.verifications(),
        })
        .unwrap();
        assert_eq!(results, Some(own), "{}: results bytes", case.name);
    }
}

#[test]
fn a_question_the_data_cannot_answer_classifies_like_the_exit_code_2_of_ruletrace() {
    // p9 is not in the data. `ruletrace evaluate` rejects it as invalid input
    // (exit 2) and writes no results file.
    let compiled = parse_compiled(
        &fixture("expected/compiled.json"),
        &input("compiled.json"),
        64 * 1024 * 1024,
        MAX_CONTAINER_DEPTH,
    )
    .unwrap();
    let (pid, options) = (input("participants.csv"), MappingOptions::default());
    let request = PrizeRequest {
        eligibility: [(target("eligible"), 2)].into(),
        ..PrizeRequest::default()
    };
    let none = SourceModels::new();
    let run = evaluate(&PrizeInputs {
        compiled: &compiled,
        request: &request,
        files: FileInputs {
            participants: NamedFile {
                bytes: CSV_FULL.as_bytes(),
                name: &pid,
            },
            collections: None,
            published: None,
            options: &options,
        },
        sources: &none,
        statements: &[],
        synthetic_data: None,
    })
    .unwrap();
    let data = run.prepared().data();
    let questions = vec![Question::Value {
        target: target("eligible"),
        participant: rt_data::ParticipantId::new("p9").unwrap(),
    }];
    let evaluation = rt_engine::evaluate(&compiled, data, &questions, None);
    let verifications: Vec<_> = evaluation
        .traces
        .iter()
        .map(|t| verify_trace(t, &none))
        .collect();
    let class = ResultClass::of(
        &evaluation.outcomes,
        &evaluation.diagnostics,
        &verifications,
    );
    let (cli_code, results) = ruletrace_evaluate(
        &run.prepared().data_bytes(),
        &rt_data::tabular::questions_bytes(&questions),
        None,
        None,
    );
    assert_eq!(cli_code, 2);
    assert_eq!(results, None);
    assert_eq!(exit_code(class), 2);
}

const CSV_MISSING_JUDGES: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,,9\np3,15,true,6,6\np4,30,false,9,6\n";

/// RN-38 (design §9.4): an engine Error Diagnostic with every Outcome
/// decided is not reachable with Rule_IR version 1. Every Error the engine
/// builds for an Outcome (RT-EXE-0010, RT-DGN-0004, RT-DGN-0005,
/// RT-EXE-0006, a dependency cycle and a fault) is created in the same
/// answer as an Indeterminate Outcome (`rt-engine/src/lib.rs`, `rank.rs`).
/// This test pins the implication over the data of this file and the exit
/// code of `ruletrace evaluate` for each case.
#[test]
fn an_engine_error_always_comes_with_an_indeterminate_outcome_and_exit_4() {
    let compiled = parse_compiled(
        &fixture("expected/compiled.json"),
        &input("compiled.json"),
        64 * 1024 * 1024,
        MAX_CONTAINER_DEPTH,
    )
    .unwrap();
    let none = SourceModels::new();
    let (pid, options) = (input("participants.csv"), MappingOptions::default());
    let request = full_request();
    let mut errors_seen = 0;
    for csv in [CSV_FULL, CSV_MISSING_AGE, CSV_MISSING_JUDGES] {
        let run = evaluate(&PrizeInputs {
            compiled: &compiled,
            request: &request,
            files: FileInputs {
                participants: NamedFile {
                    bytes: csv.as_bytes(),
                    name: &pid,
                },
                collections: None,
                published: None,
                options: &options,
            },
            sources: &none,
            statements: &[],
            synthetic_data: None,
        })
        .unwrap();
        let evaluation = run.evaluation();
        let indeterminate = evaluation
            .outcomes
            .iter()
            .any(|o| matches!(o, rt_trace::Outcome::Indeterminate(_)));
        let prepared = run.prepared();
        let (cli_code, _) = ruletrace_evaluate(
            &prepared.data_bytes(),
            &prepared.questions_bytes(),
            None,
            None,
        );
        if evaluation.diagnostics.has_errors() {
            errors_seen += 1;
            assert!(indeterminate, "an Error without an Indeterminate Outcome");
            assert_eq!(cli_code, 4);
        }
        assert_eq!(indeterminate, cli_code == 4, "{csv}");
    }
    assert_eq!(errors_seen, 2, "the missing-value cases must report Errors");
}
