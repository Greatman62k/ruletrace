//! Property 42: Application layers are byte-identical to `ruletrace evaluate`.
//!
//! Feature: ruletrace-compiler, Property 42: Application layers are
//! byte-identical to `ruletrace evaluate`
//!
//! For at least 256 generated Competition_Data sets per application, the
//! results the Evaluate group of `prize-check` and of `bracket-check`
//! produces equal the file `ruletrace evaluate` (the real `rt_cli::run` over
//! an in-memory file system) writes for the same Compiled_Rulebook, canonical
//! data, Questions, Published_Result and Source_Models, with the same exit
//! code. The canonical bytes handed to `ruletrace evaluate` are those the
//! application's Prepare group wrote. The test also runs every committed
//! fixture that fits the Application_Input_Format.
//!
//! The mapping of a classification to its exit code (0, 4, 2, 70) and the
//! class of each pinned case are written out here, independently of the
//! libraries. The property test generates participant tables with exact
//! decimal literals (including the eligibility threshold 16, the
//! qualification threshold 7.7, equal scores that force Tie_Groups and
//! 28-fractional-digit literals), booleans and empty cells (Missing_Value),
//! and optional published files with equal and differing values.
//!
//! **Validates: Requirements 31.5, 31.12, 15.7**

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

#[path = "support/app_eval.rs"]
mod app_eval;

use app_eval::{identical, Observed};
use proptest::prelude::*;
use rt_compiled::{parse_compiled, CompiledRulebook};
use rt_data::tabular::MappingOptions;
use rt_data::{parse_data, parse_published, DataLimits, DataValue, PublishedValue, TargetName};
use rt_explain::results::{serialize_results, ResultsInput};
use rt_foundation::{DocumentId, InputId, MAX_CONTAINER_DEPTH};
use rt_srcmodel::{default_load_limits, DocumentKind, SourceModel, SourceModels};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SLICE_BASIC: &str = "tests/fixtures/slice-basic";

/// The fewest generated Competition_Data sets per application (Property 42).
const CASES: u32 = 256;

const QUALIFIED_RULE: &str = r#"{"id":"elig.qual","revision":1,"kind":"eligibility","target":"qualified","type":"boolean","spans":[{"document":"rules.md","sha256":"eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a","start":92,"end":170,"line":4,"column":1}],"nodes":[{"idx":0,"node":"ref","t":"number","target":"score"},{"idx":1,"node":"num","t":"number","value":"7.7"},{"idx":2,"node":"cmp","t":"boolean","op":"ge","left":0,"right":1}],"expr":2},"#;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> Vec<u8> {
    std::fs::read(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
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

/// `slice-basic` plus the test-local Qualification_Predicate `qualified`
/// (`score >= 7.7`); no committed fixture changes.
fn bracket_compiled_bytes() -> Vec<u8> {
    let text = String::from_utf8(read(&format!("{SLICE_BASIC}/expected/compiled.json"))).unwrap();
    let after_basic = r#""expr":4},"#;
    let order = r#""order":["elig.basic","score.weighted","rank.standing"]"#;
    assert_eq!(text.matches(after_basic).count(), 1);
    assert_eq!(text.matches(order).count(), 1);
    text.replace(after_basic, &format!("{after_basic}{QUALIFIED_RULE}"))
        .replace(
            order,
            r#""order":["elig.basic","score.weighted","elig.qual","rank.standing"]"#,
        )
        .into_bytes()
}

fn model_sources(model_bytes: &[u8]) -> SourceModels {
    let model =
        SourceModel::load(&input("model.json"), model_bytes, default_load_limits()).unwrap();
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

/// The documented exit codes of Req 31.12, per application type.
macro_rules! exit_code {
    ($class:expr, $ty:ident) => {
        match $class {
            $ty::Complete => 0,
            $ty::Indeterminate => 4,
            $ty::InvalidInput => 2,
            $ty::Internal => 70,
        }
    };
}

/// Published differences by kind, in the order of `Observed::differences`.
fn difference_counts(evaluation: &rt_engine::Evaluation) -> [usize; 3] {
    use rt_engine::DifferenceKind;
    let mut counts = [0; 3];
    for d in evaluation.comparison.iter().flat_map(|c| &c.differences) {
        match d.kind {
            DifferenceKind::ValueDiffers => counts[0] += 1,
            DifferenceKind::TieBrokenWithoutDeclaredCriterion => counts[1] += 1,
            DifferenceKind::TieNotInComputedResult => counts[2] += 1,
        }
    }
    counts
}

/// The two applications behind one signature, so each test body is written
/// once. Every function returns `None` when the library rejects the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum App {
    Prize,
    Bracket,
}

/// Which Targets a run requests. Bit 0, 1, 2 for Prize: eligibility, score,
/// ranking. Bit 0, 1 for Bracket: ranking, qualification.
type Mask = u8;

fn evaluate_app(
    app: App,
    compiled: &CompiledRulebook,
    mask: Mask,
    csv: &str,
    published: Option<&str>,
    sources: Option<&SourceModels>,
) -> Result<Observed, String> {
    let none = SourceModels::new();
    let options = MappingOptions::default();
    let (pid, qid) = (input("participants.csv"), input("published.csv"));
    macro_rules! go {
        ($krate:ident, $inputs:ident, $request:expr) => {{
            use $krate::{evaluate, FileInputs, NamedFile, ResultClass};
            let request = $request;
            let run = evaluate(&$krate::$inputs {
                compiled,
                request: &request,
                files: FileInputs {
                    participants: NamedFile {
                        bytes: csv.as_bytes(),
                        name: &pid,
                    },
                    collections: None,
                    published: published.map(|text| NamedFile {
                        bytes: text.as_bytes(),
                        name: &qid,
                    }),
                    options: &options,
                },
                sources: sources.unwrap_or(&none),
                statements: &[],
                synthetic_data: None,
            })
            .map_err(|e| format!("the application rejected the input: {e:?}"))?;
            let prepared = run.prepared();
            let evaluation = run.evaluation();
            let results = serialize_results(&ResultsInput {
                outcomes: &evaluation.outcomes,
                traces: &evaluation.traces,
                diagnostics: &evaluation.diagnostics,
                verifications: run.verifications(),
            })
            .map_err(|_| "results serialisation failed".to_owned())?;
            Ok(Observed {
                data: prepared.data_bytes(),
                questions: prepared.questions_bytes(),
                published: prepared.published_bytes(),
                class_code: exit_code!(run.class(), ResultClass),
                results,
                differences: difference_counts(evaluation),
            })
        }};
    }
    match app {
        App::Prize => {
            let pick = |bit: u8, name: &str, at: u32| {
                if mask & bit != 0 {
                    [(target(name), at)].into()
                } else {
                    BTreeMap::new()
                }
            };
            go!(
                prize_check,
                PrizeInputs,
                prize_check::PrizeRequest {
                    eligibility: pick(1, "eligible", 2),
                    score: pick(2, "score", 4),
                    ranking: pick(4, "standing", 6),
                }
            )
        }
        App::Bracket => {
            let pick = |bit: u8, name: &str, at: u32| {
                if mask & bit != 0 {
                    [(target(name), at)].into()
                } else {
                    BTreeMap::new()
                }
            };
            go!(
                bracket_check,
                BracketInputs,
                bracket_check::BracketRequest {
                    ranking: pick(1, "standing", 2),
                    qualification: pick(2, "qualified", 4),
                }
            )
        }
    }
}

fn compiled_bytes_of(app: App) -> Vec<u8> {
    match app {
        App::Prize => read(&format!("{SLICE_BASIC}/expected/compiled.json")),
        App::Bracket => bracket_compiled_bytes(),
    }
}

fn mask_max(app: App) -> Mask {
    match app {
        App::Prize => 7,
        App::Bracket => 3,
    }
}

#[derive(Clone, Copy, Debug)]
enum SourceKind {
    Real,
    Absent,
    OtherDocument,
}

#[derive(Clone, Debug)]
struct Row {
    id: u32,
    age: Option<String>,
    licence: Option<bool>,
    judges: Option<String>,
    technical: Option<String>,
}

type PublishedRow = (Option<bool>, Option<String>, Option<String>);

#[derive(Clone, Debug)]
struct Generated {
    prefix: &'static str,
    rows: Vec<Row>,
    /// Per row position: boolean Target cell, score cell, Rank_Position cell.
    published: Option<Vec<PublishedRow>>,
    mask: Mask,
    sources: SourceKind,
}

/// Weight of an empty cell: none in a complete case, one in four otherwise.
fn hole(with_missing: bool) -> u32 {
    if with_missing {
        3
    } else {
        0
    }
}

/// A score input. A narrow case draws only from a few equal-valued literals
/// (7 and 7.0 style spellings included) so Tie_Groups are common.
fn number_cell(with_missing: bool, narrow: bool) -> impl Strategy<Value = Option<String>> {
    let (wide, tight) = if narrow { (0, 12) } else { (1, 0) };
    prop_oneof![
        hole(with_missing) => Just(None),
        tight => prop::sample::select(vec!["7", "8", "7.0", "8.00"])
            .prop_map(|s| Some(s.to_owned())),
        6 * wide => prop::sample::select(vec![
            "0", "10", "6", "7", "7.25", "7.5", "7.50", "8", "9", "7.7", "8.0",
        ])
        .prop_map(|s| Some(s.to_owned())),
        2 * wide => (0u32..=100).prop_map(|n| Some(format!("{}.{}", n / 10, n % 10))),
        wide => Just(Some("7.1234567890123456789012345678".to_owned())),
    ]
}

fn age_cell(with_missing: bool) -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        hole(with_missing) => Just(None),
        4 => prop::sample::select(vec!["15", "16", "16.0", "15.99", "16.01", "0", "99"])
            .prop_map(|s| Some(s.to_owned())),
        2 => (0u32..90).prop_map(|n| Some(n.to_string())),
    ]
}

fn row(with_missing: bool, narrow: bool) -> impl Strategy<Value = Row> {
    (
        1u32..30,
        age_cell(with_missing),
        prop_oneof![
            hole(with_missing) / 2 => Just(None),
            4 => Just(Some(true)),
            2 => Just(Some(false)),
        ],
        number_cell(with_missing, narrow),
        number_cell(with_missing, narrow),
    )
        .prop_map(|(id, age, licence, judges, technical)| Row {
            id,
            age,
            licence,
            judges,
            technical,
        })
}

fn published_score() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        3 => Just(None),
        6 => prop::sample::select(vec!["0", "6", "7.6", "7.7", "7.70", "7.8", "10"])
            .prop_map(|s| Some(s.to_owned())),
    ]
}

// A published Rank_Position is a positive integer.
fn published_rank() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        3 => Just(None),
        6 => (1u32..=6).prop_map(|n| Some(n.to_string())),
    ]
}

fn generated(max_mask: Mask) -> impl Strategy<Value = Generated> {
    // Half of the cases have no empty cell, so decided Outcomes, Tie_Groups
    // and the exit code 0 are reached as often as Indeterminate ones.
    (any::<bool>(), any::<bool>())
        .prop_flat_map(move |(with_missing, narrow)| generated_with(max_mask, with_missing, narrow))
}

fn generated_with(
    max_mask: Mask,
    with_missing: bool,
    narrow: bool,
) -> impl Strategy<Value = Generated> {
    (
        prop::sample::select(vec!["p", "T-", "x_"]),
        prop::collection::vec(row(with_missing, narrow), 1..=12),
        prop::option::of(prop::collection::vec(
            (
                prop_oneof![1 => Just(None), 2 => any::<bool>().prop_map(Some)],
                published_score(),
                published_rank(),
            ),
            12,
        )),
        1..=max_mask,
        prop_oneof![5 => Just(SourceKind::Real), 2 => Just(SourceKind::Absent), 1 => Just(SourceKind::OtherDocument)],
    )
        .prop_map(|(prefix, rows, published, mask, sources)| {
            let mut seen = std::collections::BTreeSet::new();
            let rows = rows.into_iter().filter(|r| seen.insert(r.id)).collect();
            Generated {
                prefix,
                rows,
                published,
                mask,
                sources,
            }
        })
}

fn participants_csv(g: &Generated) -> String {
    let mut text = String::from("participant,age,licence,judges,technical\n");
    for r in &g.rows {
        text.push_str(&format!(
            "{}{},{},{},{},{}\n",
            g.prefix,
            r.id,
            r.age.as_deref().unwrap_or(""),
            r.licence.map_or("", |b| if b { "true" } else { "false" }),
            r.judges.as_deref().unwrap_or(""),
            r.technical.as_deref().unwrap_or(""),
        ));
    }
    text
}

fn published_csv(g: &Generated, names: [&str; 3]) -> Option<String> {
    let cells = g.published.as_ref()?;
    let mut text = String::from("participant,target,value\n");
    for (r, (flag, first, second)) in g.rows.iter().zip(cells) {
        let values = [
            flag.map(|b| if b { "true" } else { "false" }.to_owned()),
            first.clone(),
            second.clone(),
        ];
        for (name, value) in names.iter().zip(values) {
            if let Some(v) = value {
                text.push_str(&format!("{}{},{name},{v}\n", g.prefix, r.id));
            }
        }
    }
    Some(text)
}

fn check_generated(app: App, g: &Generated) -> Result<u8, TestCaseError> {
    let bytes = compiled_bytes_of(app);
    let compiled = parse(&bytes);
    let names = match app {
        App::Prize => ["eligible", "score", "standing"],
        App::Bracket => ["qualified", "score", "standing"],
    };
    let csv = participants_csv(g);
    let published = published_csv(g, names);
    let real = model_sources(&read(&format!("{SLICE_BASIC}/expected/source-model.json")));
    let other = other_document_sources();
    let sources = match g.sources {
        SourceKind::Real => Some(&real),
        SourceKind::Absent => None,
        SourceKind::OtherDocument => Some(&other),
    };
    let observed = evaluate_app(app, &compiled, g.mask, &csv, published.as_deref(), sources)
        .map_err(|e| TestCaseError::fail(format!("{e}\n{csv}\n{published:?}")))?;
    identical(&bytes, &observed, sources)
        .map_err(|e| TestCaseError::fail(format!("{app:?}: {e}\ninput:\n{csv}\n{published:?}")))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn prize_check_evaluate_is_byte_identical_to_ruletrace_evaluate(
        g in generated(mask_max(App::Prize))
    ) {
        check_generated(App::Prize, &g)?;
    }

    #[test]
    fn bracket_check_evaluate_is_byte_identical_to_ruletrace_evaluate(
        g in generated(mask_max(App::Bracket))
    ) {
        check_generated(App::Bracket, &g)?;
    }
}

/// One pinned run per exit class, with the expected code written out here, so
/// the property's comparison is known to be exercised on 0, 4 and 2.
#[test]
fn pinned_runs_reach_every_exit_class_and_stay_identical() {
    let full = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,7,9\np3,15,true,6,6\np4,30,false,7,9\n";
    let missing_judges = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,,9\np3,15,true,6,6\np4,30,false,9,6\n";
    let published = "participant,target,value\np2,standing,1\np4,standing,2\np1,standing,3\n";
    let real = model_sources(&read(&format!("{SLICE_BASIC}/expected/source-model.json")));
    let other = other_document_sources();
    for app in [App::Prize, App::Bracket] {
        let bytes = compiled_bytes_of(app);
        let compiled = parse(&bytes);
        let mask = mask_max(app);
        let table: [(&str, Option<&str>, Option<&SourceModels>, u8); 5] = [
            (full, None, Some(&real), 0),
            (full, Some(published), Some(&real), 0),
            (full, None, None, 0),
            (missing_judges, None, Some(&real), 4),
            (full, None, Some(&other), 2),
        ];
        for (csv, published, sources, expected) in table {
            let observed = evaluate_app(app, &compiled, mask, csv, published, sources).unwrap();
            assert_eq!(observed.class_code, expected, "{app:?} class of {csv}");
            let code = identical(&bytes, &observed, sources).unwrap();
            assert_eq!(code, expected, "{app:?} ruletrace exit of {csv}");
        }
    }
}

/// A committed fixture: its Compiled_Rulebook, Source_Model, canonical data
/// files and canonical Published_Result files, relative to the repository.
struct Fixture {
    name: &'static str,
    compiled: &'static str,
    model: &'static str,
    data: &'static [&'static str],
    published: &'static [&'static str],
    /// The Prize Targets the Compiled_Rulebook declares (bit 0 eligibility,
    /// 1 score, 2 ranking).
    prize_mask: u8,
    /// Whether it declares a ranking Target and an eligibility Target for the
    /// Bracket request.
    ranking: bool,
    qualification: bool,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "slice-basic",
        compiled: "tests/fixtures/slice-basic/expected/compiled.json",
        model: "tests/fixtures/slice-basic/expected/source-model.json",
        data: &["tests/fixtures/slice-basic/data.json"],
        published: &[],
        prize_mask: 7,
        ranking: true,
        qualification: true,
    },
    Fixture {
        name: "slice-missing",
        compiled: "tests/fixtures/slice-missing/expected/compiled.json",
        model: "tests/fixtures/slice-missing/expected/source-model.json",
        data: &["tests/fixtures/slice-missing/data.json"],
        published: &[],
        prize_mask: 3,
        ranking: false,
        qualification: true,
    },
    Fixture {
        name: "slice-tie",
        compiled: "tests/fixtures/slice-tie/expected/compiled.json",
        model: "tests/fixtures/slice-tie/expected/source-model.json",
        data: &["tests/fixtures/slice-tie/data.json"],
        published: &[],
        prize_mask: 6,
        ranking: true,
        qualification: false,
    },
    Fixture {
        name: "slice-ai",
        compiled: "tests/fixtures/slice-ai/expected/compiled.json",
        model: "tests/fixtures/slice-ai/expected/source-model.json",
        data: &["tests/fixtures/slice-ai/data.json"],
        published: &[],
        prize_mask: 7,
        ranking: true,
        qualification: true,
    },
    Fixture {
        name: "stand-in-hand",
        compiled: "demo/stand-in-hand/golden/artifacts/compiled.json",
        model: "demo/stand-in-hand/golden/artifacts/source-model.json",
        data: &[
            "demo/stand-in-hand/data/canonical/data.json",
            "demo/stand-in-hand/data/canonical/data-complete.json",
        ],
        published: &[
            "demo/stand-in-hand/data/canonical/published.json",
            "demo/stand-in-hand/data/canonical/published-seeded.json",
        ],
        prize_mask: 7,
        ranking: true,
        qualification: true,
    },
    Fixture {
        name: "stand-in-ai",
        compiled: "demo/stand-in-ai/golden/artifacts/compiled.json",
        model: "demo/stand-in-ai/golden/artifacts/source-model.json",
        data: &[
            "demo/stand-in-ai/data/canonical/data.json",
            "demo/stand-in-ai/data/canonical/data-complete.json",
        ],
        published: &[
            "demo/stand-in-ai/data/canonical/published.json",
            "demo/stand-in-ai/data/canonical/published-seeded.json",
        ],
        prize_mask: 7,
        ranking: true,
        qualification: true,
    },
];

const COLUMNS: [&str; 4] = ["age", "licence", "judges", "technical"];

/// The Application_Input_Format form of a canonical data file: the columns
/// are the four fields of the committed rulebooks and an absent field is an
/// empty cell. Panics if the file has a collection or another field, because
/// then the fixture does not fit the format and the test must say so.
fn data_to_csv(bytes: &[u8]) -> String {
    let data = parse_data(&input("data.json"), bytes, DataLimits::default())
        .unwrap_or_else(|_| panic!("canonical data does not parse"));
    let mut text = format!("participant,{}\n", COLUMNS.join(","));
    for (id, p) in data.participants() {
        assert!(p.collections().is_empty(), "collections do not fit the CSV");
        assert!(p.fields().keys().all(|k| COLUMNS.contains(&k.as_str())));
        text.push_str(id.as_str());
        for column in COLUMNS {
            text.push(',');
            match p.field(column) {
                Some(DataValue::Bool(b)) => text.push_str(&b.to_string()),
                Some(DataValue::Text(t)) => text.push_str(t),
                None => {}
            }
        }
        text.push('\n');
    }
    text
}

fn published_to_csv(bytes: &[u8]) -> String {
    let published = parse_published(
        &input("published.json"),
        bytes,
        rt_data::default_parse_limits(),
    )
    .unwrap_or_else(|_| panic!("canonical published file does not parse"));
    let mut text = String::from("participant,target,value\n");
    for ((participant, target), value) in published.entries() {
        let cell = match value {
            PublishedValue::Bool(b) => b.to_string(),
            PublishedValue::Number(n) => rt_numeric::to_canonical(n.value()),
        };
        text.push_str(&format!("{participant},{target},{cell}\n"));
    }
    text
}

#[test]
fn every_committed_fixture_is_byte_identical_for_both_applications() {
    let mut runs = 0;
    for fixture in FIXTURES {
        let compiled_bytes = read(fixture.compiled);
        let compiled = parse(&compiled_bytes);
        let models = model_sources(&read(fixture.model));
        let publisheds: Vec<Option<String>> = std::iter::once(None)
            .chain(
                fixture
                    .published
                    .iter()
                    .map(|p| Some(published_to_csv(&read(p)))),
            )
            .collect();
        for data in fixture.data {
            let csv = data_to_csv(&read(data));
            for published in &publisheds {
                for sources in [Some(&models), None] {
                    // Prize: eligibility, score and ranking. Bracket: the
                    // ranking Target and the eligibility Target as the
                    // qualification, because the committed rulebooks hold no
                    // `qualified` rule.
                    let prize = evaluate_app(
                        App::Prize,
                        &compiled,
                        fixture.prize_mask,
                        &csv,
                        published.as_deref(),
                        sources,
                    )
                    .unwrap_or_else(|e| panic!("{}: prize: {e}", fixture.name));
                    identical(&compiled_bytes, &prize, sources)
                        .unwrap_or_else(|e| panic!("{} {data}: prize: {e}", fixture.name));
                    let bracket = evaluate_bracket_with_eligible(
                        &compiled,
                        (fixture.ranking, fixture.qualification),
                        &csv,
                        published.as_deref(),
                        sources,
                    )
                    .unwrap_or_else(|e| panic!("{}: bracket: {e}", fixture.name));
                    identical(&compiled_bytes, &bracket, sources)
                        .unwrap_or_else(|e| panic!("{} {data}: bracket: {e}", fixture.name));
                    runs += 2;
                }
            }
        }
    }
    // Six fixtures; every data file with and without each published file,
    // with and without Source_Models, for two applications.
    assert_eq!(runs, 2 * 2 * (4 + 2 * 2 * 3));
}

fn evaluate_bracket_with_eligible(
    compiled: &CompiledRulebook,
    (ranking, qualification): (bool, bool),
    csv: &str,
    published: Option<&str>,
    sources: Option<&SourceModels>,
) -> Result<Observed, String> {
    use bracket_check::{
        evaluate, BracketInputs, BracketRequest, FileInputs, NamedFile, ResultClass,
    };
    let none = SourceModels::new();
    let options = MappingOptions::default();
    let (pid, qid) = (input("participants.csv"), input("published.csv"));
    let request = BracketRequest {
        ranking: ranking
            .then(|| (target("standing"), 2))
            .into_iter()
            .collect(),
        qualification: qualification
            .then(|| (target("eligible"), 4))
            .into_iter()
            .collect(),
    };
    let run = evaluate(&BracketInputs {
        compiled,
        request: &request,
        files: FileInputs {
            participants: NamedFile {
                bytes: csv.as_bytes(),
                name: &pid,
            },
            collections: None,
            published: published.map(|text| NamedFile {
                bytes: text.as_bytes(),
                name: &qid,
            }),
            options: &options,
        },
        sources: sources.unwrap_or(&none),
        statements: &[],
        synthetic_data: None,
    })
    .map_err(|e| format!("the application rejected the fixture input: {e:?}"))?;
    let prepared = run.prepared();
    let evaluation = run.evaluation();
    let results = serialize_results(&ResultsInput {
        outcomes: &evaluation.outcomes,
        traces: &evaluation.traces,
        diagnostics: &evaluation.diagnostics,
        verifications: run.verifications(),
    })
    .map_err(|_| "results serialisation failed".to_owned())?;
    Ok(Observed {
        data: prepared.data_bytes(),
        questions: prepared.questions_bytes(),
        published: prepared.published_bytes(),
        class_code: exit_code!(run.class(), ResultClass),
        results,
        differences: difference_counts(evaluation),
    })
}
