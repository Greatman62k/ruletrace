//! Shared runs of the two Workflow_Modules for the isolation test (Req 31.15,
//! 20.6) and the environment test (Req 31.11), task 9.17.
//!
//! Prize runs over the committed `slice-basic` Compiled_Rulebook. Bracket
//! runs over the committed Compiled_Rulebook of the `app-rules.md` source
//! (`tests/fixtures/app-reports/bracket-qualification-weighted-score`). The
//! two differ in document, document hash and spans. Every input is embedded
//! at build time, so a run opens no file.
//!
//! Rules of both rulebooks, from their sources: eligible = age >= 16 and
//! licence; score = 0.6 * judges + 0.4 * technical; standing ranks score
//! descending, then judges descending; the app rulebook adds qualified =
//! score >= 7.7.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use bracket_check::{BracketInputs, BracketRequest};
use prize_check::{PrizeInputs, PrizeRequest, ResultClass};
use rt_compiled::{parse_compiled, CompiledRulebook};
use rt_data::tabular::MappingOptions;
use rt_data::TargetName;
use rt_explain::report::{render_human, render_json, ReportDoc};
use rt_explain::results::{serialize_results, ResultsInput};
use rt_foundation::{InputId, MAX_CONTAINER_DEPTH};
use rt_srcmodel::{default_load_limits, SourceModel, SourceModels};

const BASIC_COMPILED: &[u8] =
    include_bytes!("../../../../tests/fixtures/slice-basic/expected/compiled.json");
const BASIC_MODEL: &[u8] =
    include_bytes!("../../../../tests/fixtures/slice-basic/expected/source-model.json");
const APP_COMPILED: &[u8] = include_bytes!(
    "../../../../tests/fixtures/app-reports/bracket-qualification-weighted-score/compiled.json"
);
const APP_MODEL: &[u8] = include_bytes!(
    "../../../../tests/fixtures/app-reports/bracket-qualification-weighted-score/source-model.json"
);

/// Four participants, every cell present. Scores: p1 0.6*8 + 0.4*7 = 7.6,
/// p2 4.2 + 3.6 = 7.8, p3 3.6 + 2.4 = 6, p4 5.4 + 2.4 = 7.8.
pub const CSV_FULL: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,7,9\np3,15,true,6,6\np4,30,false,9,6\n";

/// As FULL with p4 at judges 7 and technical 9: p2 and p4 equal on score
/// (7.8) and on judges (7).
pub const CSV_TIE: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,7,9\np3,15,true,6,6\np4,30,false,7,9\n";

/// As FULL with an empty age for p2.
pub const CSV_MISSING_AGE: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,,true,7,9\np3,15,true,6,6\np4,30,false,9,6\n";

/// As FULL with an empty judges cell for p2.
pub const CSV_MISSING_JUDGES: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,,9\np3,15,true,6,6\np4,30,false,9,6\n";

/// The header of the participant files.
pub const HEADER: &str = "participant,age,licence,judges,technical\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum App {
    Prize,
    Bracket,
}

/// Which Targets a run requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sel {
    /// Prize: eligible, score, standing. Bracket: standing and qualified.
    All,
    Eligibility,
    Score,
    Ranking,
    Qualification,
}

/// Everything a run produces that a reader or a file would hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub results: Vec<u8>,
    pub human: String,
    pub json: Vec<u8>,
    pub class: ResultClass,
}

/// One named run.
#[derive(Clone, Debug)]
pub struct Job {
    pub name: String,
    pub app: App,
    pub csv: String,
    pub sel: Sel,
}

impl Job {
    pub fn run(&self) -> Snapshot {
        run(self.app, &self.csv, self.sel)
    }
}

fn input(name: &str) -> InputId {
    InputId::new(name).unwrap()
}

fn target(name: &str) -> TargetName {
    TargetName::new(name).unwrap()
}

fn parse(bytes: &[u8]) -> CompiledRulebook {
    parse_compiled(
        bytes,
        &input("compiled.json"),
        64 * 1024 * 1024,
        MAX_CONTAINER_DEPTH,
    )
    .unwrap()
}

fn sources(model: &[u8]) -> SourceModels {
    let model = SourceModel::load(&input("model.json"), model, default_load_limits()).unwrap();
    let mut models = SourceModels::new();
    models.insert(model).unwrap();
    models
}

fn snapshot(
    doc: &ReportDoc,
    evaluation: &rt_engine::Evaluation,
    verifications: &[rt_explain::Verification],
    class: ResultClass,
) -> Snapshot {
    Snapshot {
        results: serialize_results(&ResultsInput {
            outcomes: &evaluation.outcomes,
            traces: &evaluation.traces,
            diagnostics: &evaluation.diagnostics,
            verifications,
        })
        .unwrap(),
        human: render_human(doc),
        json: render_json(doc),
        class,
    }
}

/// Runs one module over a fresh parse of its rulebook. Positions of the
/// request lines are fixed by the order below.
pub fn run(app: App, csv: &str, sel: Sel) -> Snapshot {
    let options = MappingOptions::default();
    let participants = input("participants.csv");
    match app {
        App::Prize => {
            let compiled = parse(BASIC_COMPILED);
            let models = sources(BASIC_MODEL);
            let mut request = PrizeRequest::default();
            let all = sel == Sel::All;
            if all || sel == Sel::Eligibility {
                request.eligibility.insert(target("eligible"), 2);
            }
            if all || sel == Sel::Score {
                request.score.insert(target("score"), 4);
            }
            if all || sel == Sel::Ranking {
                request.ranking.insert(target("standing"), 6);
            }
            let run = prize_check::evaluate(&PrizeInputs {
                compiled: &compiled,
                request: &request,
                files: prize_check::FileInputs {
                    participants: prize_check::NamedFile {
                        bytes: csv.as_bytes(),
                        name: &participants,
                    },
                    collections: None,
                    published: None,
                    options: &options,
                },
                sources: &models,
                statements: &[],
                synthetic_data: Some(true),
            })
            .unwrap_or_else(|e| panic!("prize rejected: {e:?}"));
            let evaluation = run.evaluation();
            snapshot(
                run.report().unwrap(),
                evaluation,
                run.verifications(),
                run.class(),
            )
        }
        App::Bracket => {
            let compiled = parse(APP_COMPILED);
            let models = sources(APP_MODEL);
            let mut request = BracketRequest::default();
            let all = sel == Sel::All;
            if all || sel == Sel::Ranking {
                request.ranking.insert(target("standing"), 2);
            }
            if all || sel == Sel::Qualification {
                request.qualification.insert(target("qualified"), 4);
            }
            let run = bracket_check::evaluate(&BracketInputs {
                compiled: &compiled,
                request: &request,
                files: bracket_check::FileInputs {
                    participants: bracket_check::NamedFile {
                        bytes: csv.as_bytes(),
                        name: &participants,
                    },
                    collections: None,
                    published: None,
                    options: &options,
                },
                sources: &models,
                statements: &[],
                synthetic_data: Some(true),
            })
            .unwrap_or_else(|e| panic!("bracket rejected: {e:?}"));
            let evaluation = run.evaluation();
            snapshot(
                run.report().unwrap(),
                evaluation,
                run.verifications(),
                run.class(),
            )
        }
    }
}

/// The Outcome texts of a results file: the array between `"outcomes":[` and
/// `],"traces"`, split at each `{"state":`.
pub fn outcomes(results: &[u8]) -> Vec<String> {
    let text = String::from_utf8(results.to_vec()).unwrap();
    let start = text.find(r#""outcomes":["#).unwrap() + r#""outcomes":["#.len();
    let end = text.find(r#"],"traces":"#).unwrap();
    text[start..end]
        .split(r#"{"state":"#)
        .skip(1)
        .map(|p| format!(r#"{{"state":{}"#, p.trim_end_matches(',')))
        .collect()
}

pub fn boolean(value: bool) -> String {
    format!(r#"{{"state":"decided","value":{{"type":"bool","value":{value}}}}}"#)
}

pub fn number(text: &str) -> String {
    format!(r#"{{"state":"decided","value":{{"type":"number","value":"{text}"}}}}"#)
}

pub fn missing(participant: &str, field: &str) -> String {
    format!(
        r#"{{"state":"indeterminate","reasons":[{{"kind":"missing_input","participant":"{participant}","field":"{field}"}}]}}"#
    )
}

pub fn standings(places: &[(usize, &[&str])]) -> String {
    let places: Vec<String> = places
        .iter()
        .map(|(position, members)| {
            let members: Vec<String> = members.iter().map(|m| format!("\"{m}\"")).collect();
            format!(
                r#"{{"position":{position},"members":[{}]}}"#,
                members.join(",")
            )
        })
        .collect();
    format!(
        r#"{{"state":"decided","value":{{"type":"standings","places":[{}]}}}}"#,
        places.join(",")
    )
}

/// The data rows of a participant file, each as a one-participant file.
pub fn single_record_files(csv: &str) -> Vec<String> {
    csv.lines()
        .skip(1)
        .map(|row| format!("{HEADER}{row}\n"))
        .collect()
}

/// Whole-file jobs of one module: the full, tie and missing-value data sets.
pub fn whole_jobs(app: App) -> Vec<Job> {
    let missing = match app {
        App::Prize => CSV_MISSING_AGE,
        App::Bracket => CSV_MISSING_JUDGES,
    };
    [("full", CSV_FULL), ("tie", CSV_TIE), ("missing", missing)]
        .into_iter()
        .map(|(name, csv)| Job {
            name: format!("{app:?}-{name}"),
            app,
            csv: csv.to_owned(),
            sel: Sel::All,
        })
        .collect()
}

/// One job per participant record of the full data set.
pub fn record_jobs(app: App) -> Vec<Job> {
    single_record_files(CSV_FULL)
        .into_iter()
        .enumerate()
        .map(|(i, csv)| Job {
            name: format!("{app:?}-record{i}"),
            app,
            csv,
            sel: Sel::All,
        })
        .collect()
}

/// Every job of both modules, in a fixed order.
pub fn all_jobs() -> Vec<Job> {
    let mut jobs = Vec::new();
    for app in [App::Prize, App::Bracket] {
        jobs.extend(whole_jobs(app));
        jobs.extend(record_jobs(app));
    }
    jobs
}

/// The bytes a process prints for the environment test: for every job its
/// class, results, human report and JSON report, each length-prefixed.
pub fn render_all() -> Vec<u8> {
    let mut out = Vec::new();
    for job in all_jobs() {
        let s = job.run();
        out.extend(format!("job {} class {:?}\n", job.name, s.class).as_bytes());
        for (label, bytes) in [
            ("results", s.results.as_slice()),
            ("human", s.human.as_bytes()),
            ("json", s.json.as_slice()),
        ] {
            out.extend(format!("{label} {}\n", bytes.len()).as_bytes());
            out.extend(bytes);
            out.push(b'\n');
        }
    }
    out
}
