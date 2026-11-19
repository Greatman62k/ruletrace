//! Golden application reports (task 9.16, Req 31.14, 31.15, 32.7, 20.4).
//!
//! Each case under `tests/fixtures/app-reports/<case>/` holds its inputs
//! (`request.txt`, `participants.csv`, optional `published.csv`, a
//! Compiled_Rulebook as `compiled.ref` or `compiled.json`, `source-model.ref`)
//! and two goldens: `report.json` (machine) and `report.txt` (human).
//!
//! The path is the one the Laboratory will use (UD-38): Prepare, then
//! `ruletrace evaluate` through `rt_cli::run` on the canonical bytes, then
//! Present, then the two `rt-explain` writers.
//!
//! Limit: no results-file parser exists before task 13.3, so Present receives
//! the typed values of the library's own evaluation of the same Questions.
//! The test asserts that the results bytes of that run and the results file
//! `ruletrace evaluate` wrote are equal, and that the exit codes agree, so the
//! typed values are those `ruletrace evaluate` produced.
//!
//! Every case also pins hand-derived Outcomes (derivations in comments),
//! written independently of the report writers.
//!
//! Compare mode reads the case files and fails, naming the case, the file and
//! the first differing byte offset, when a file is missing or differs. Bless
//! mode writes the case inputs and goldens into another directory and
//! compares nothing:
//!
//! `RT_APP_GOLDEN_BLESS=<dir> cargo test -p rt-tests-core --test app_report_goldens`
//!
//! The Tie_Group-at-threshold fixture (Rank_Reference, Req 32.7) is not here.
//! It belongs to task 9.3, which is deferred.
//!
//! **Validates: Requirements 31.9 (via 9.16 note), 31.14, 32.7 (partly)**

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

#[path = "support/app_eval.rs"]
mod app_eval;

use app_eval::{identical, Observed, Session};
use rt_compiled::{parse_compiled, CompiledRulebook};
use rt_data::tabular::MappingOptions;
use rt_data::TargetName;
use rt_explain::report::{render_human, render_json};
use rt_explain::results::{serialize_results, ResultsInput};
use rt_foundation::{InputId, MAX_CONTAINER_DEPTH};
use rt_ir::{parse_rulebook, rule_content_hash};
use rt_srcmodel::{default_load_limits, SourceModel, SourceModels};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const SLICE_BASIC: &str = "tests/fixtures/slice-basic";
const COMPILED_REF: &str = "tests/fixtures/slice-basic/expected/compiled.json";
const MODEL_REF: &str = "tests/fixtures/slice-basic/expected/source-model.json";
const CASES_DIR: &str = "tests/fixtures/app-reports";
const BLESS_VAR: &str = "RT_APP_GOLDEN_BLESS";
const CI_VAR: &str = "CI";

/// Decides whether the run blesses. Bless mode is refused when `CI` is set,
/// so a leaked variable can never make the comparison tests pass unchecked,
/// and the target must be an absolute path.
fn bless_target(bless: Option<&str>, ci: Option<&str>) -> Result<Option<PathBuf>, String> {
    let Some(dir) = bless else {
        return Ok(None);
    };
    if ci.is_some() {
        return Err(format!(
            "{BLESS_VAR} is set while {CI_VAR} is set: bless mode is refused in CI"
        ));
    }
    let path = PathBuf::from(dir);
    if !path.is_absolute() {
        return Err(format!("{BLESS_VAR} must be an absolute path, got '{dir}'"));
    }
    Ok(Some(path))
}
const REVIEWER: &str = "Test Reviewer";
const TIME: &str = "2026-11-19T09:00:00Z";

// Participants, columns age, licence, judges, technical. The rules of
// `slice-basic` (read from `rules.md`): eligible = age >= 16 and licence;
// score = 0.6 * judges + 0.4 * technical; standing ranks score descending,
// then judges descending, equal participants forming a Tie_Group.
//
// FULL: scores p1 4.8 + 2.8 = 7.6; p2 4.2 + 3.6 = 7.8; p3 3.6 + 2.4 = 6;
// p4 5.4 + 2.4 = 7.8. p2 and p4 tie on 7.8 and judges separate them (p4 9,
// p2 7), so the standings are p4, p2, p1, p3.
const CSV_FULL: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,7,9\np3,15,true,6,6\np4,30,false,9,6\n";

// TIE: as FULL with p4 judges 7 and technical 9, so p2 and p4 are equal in
// score (7.8) and in judges (7). Competition ranking: {p2, p4} at 1, p1 at 3
// (two ahead), p3 at 4.
const CSV_TIE: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,7,9\np3,15,true,6,6\np4,30,false,7,9\n";

// MISSING_AGE: as FULL with an empty age for p2.
const CSV_MISSING_AGE: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,,true,7,9\np3,15,true,6,6\np4,30,false,9,6\n";

// MISSING_JUDGES: as FULL with an empty judges cell for p2.
const CSV_MISSING_JUDGES: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,,9\np3,15,true,6,6\np4,30,false,9,6\n";

#[derive(Clone, Copy, PartialEq, Eq)]
enum App {
    Prize,
    Bracket,
}

#[derive(Clone, Copy)]
enum Rulebook {
    /// The committed `slice-basic` Compiled_Rulebook.
    Basic,
    /// `slice-basic` plus the test-local Qualification_Predicate
    /// `qualified` (`score >= 7.7`), approved by `ruletrace review approve`.
    WithQualification,
}

struct Case {
    name: &'static str,
    app: App,
    rulebook: Rulebook,
    /// (option, Target) lines of `request.txt`.
    request: &'static [(&'static str, &'static str)],
    participants: &'static str,
    published: Option<&'static str>,
    /// The exit code of `ruletrace evaluate` (Req 31.12).
    code: u8,
    /// Published differences by kind: value differs (RT-EXE-0011), tie
    /// split (RT-EXE-0012), tie not computed (RT-EXE-0013).
    differences: [usize; 3],
    /// The Outcomes as results-file JSON, in Question order.
    outcomes: Vec<String>,
    readme: &'static str,
}

fn boolean(value: bool) -> String {
    format!(r#"{{"state":"decided","value":{{"type":"bool","value":{value}}}}}"#)
}

fn number(text: &str) -> String {
    format!(r#"{{"state":"decided","value":{{"type":"number","value":"{text}"}}}}"#)
}

fn missing(participant: &str, field: &str) -> String {
    format!(
        r#"{{"state":"indeterminate","reasons":[{{"kind":"missing_input","participant":"{participant}","field":"{field}"}}]}}"#
    )
}

fn standings(places: &[(usize, &[&str])]) -> String {
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

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "prize-eligibility",
            app: App::Prize,
            rulebook: Rulebook::Basic,
            request: &[("eligibility", "eligible")],
            participants: CSV_FULL,
            published: None,
            code: 0,
            differences: [0; 3],
            // p1 17 >= 16 and licence: true. p2 20 and licence: true.
            // p3 15 < 16: false. p4 30 but no licence: false.
            outcomes: vec![boolean(true), boolean(true), boolean(false), boolean(false)],
            readme: "Prize eligibility: age >= 16 and a licence. p1 and p2 are eligible, p3 is too young, p4 has no licence.",
        },
        Case {
            name: "prize-weighted-score",
            app: App::Prize,
            rulebook: Rulebook::Basic,
            request: &[("score", "score")],
            participants: CSV_FULL,
            published: None,
            code: 0,
            differences: [0; 3],
            // 0.6 * judges + 0.4 * technical, see CSV_FULL above.
            outcomes: vec![number("7.6"), number("7.8"), number("6"), number("7.8")],
            readme: "Prize weighted score, exact decimals: 0.6 * judges + 0.4 * technical. Scores 7.6, 7.8, 6 and 7.8.",
        },
        Case {
            name: "prize-ranking-tie-group",
            app: App::Prize,
            rulebook: Rulebook::Basic,
            request: &[("ranking", "standing")],
            participants: CSV_TIE,
            published: None,
            code: 0,
            differences: [0; 3],
            // p2 and p4 are equal on score and judges: one Tie_Group at 1.
            outcomes: vec![standings(&[(1, &["p2", "p4"]), (3, &["p1"]), (4, &["p3"])])],
            readme: "Prize ranking with a Tie_Group: p2 and p4 are equal on the score and on the tie-breaker, so they share position 1. The next positions are 3 and 4. The fixture source has no ranking clause: rank.standing cites clause 2, the score clause, and \"verified\" in the report means the cited span resolves against a source with a matching hash, not that the clause states the rule (Req 34.4).",
        },
        Case {
            name: "prize-indeterminate-missing-value",
            app: App::Prize,
            rulebook: Rulebook::Basic,
            request: &[("eligibility", "eligible"), ("score", "score")],
            participants: CSV_MISSING_AGE,
            published: None,
            code: 4,
            differences: [0; 3],
            // Eligibility: p1 true, p2 has no age (Indeterminate, age
            // missing), p3 false, p4 false. Score does not read age:
            // 7.6, 7.8, 6, 7.8.
            outcomes: vec![
                boolean(true),
                missing("p2", "age"),
                boolean(false),
                boolean(false),
                number("7.6"),
                number("7.8"),
                number("6"),
                number("7.8"),
            ],
            readme: "Prize Indeterminate Outcome from a Missing_Value: p2 has no age, so p2's eligibility is Indeterminate and names the age input. The scores do not read age and stay decided. Exit code 4.",
        },
        Case {
            name: "prize-published-equal",
            app: App::Prize,
            rulebook: Rulebook::Basic,
            request: &[("score", "score")],
            participants: CSV_FULL,
            published: Some("participant,target,value\np1,score,7.6\np2,score,7.8\np3,score,6\np4,score,7.8\n"),
            code: 0,
            differences: [0; 3],
            outcomes: vec![number("7.6"), number("7.8"), number("6"), number("7.8")],
            readme: "Prize published result equal to the computed scores: no difference.",
        },
        Case {
            name: "prize-published-differs",
            app: App::Prize,
            rulebook: Rulebook::Basic,
            request: &[("score", "score")],
            participants: CSV_FULL,
            published: Some("participant,target,value\np1,score,7.6\np2,score,7.9\n"),
            code: 0,
            differences: [1, 0, 0],
            // p2 is published at 7.9 but computed 7.8: one value_differs
            // (RT-EXE-0011, a Warning, so the exit code stays 0).
            outcomes: vec![number("7.6"), number("7.8"), number("6"), number("7.8")],
            readme: "Prize published result that differs: p2 is published at 7.9, computed 7.8. One RT-EXE-0011 difference, a Warning, exit code 0.",
        },
        Case {
            name: "bracket-standings-tie-group",
            app: App::Bracket,
            rulebook: Rulebook::Basic,
            request: &[("ranking", "standing")],
            participants: CSV_TIE,
            published: None,
            code: 0,
            differences: [0; 3],
            outcomes: vec![standings(&[(1, &["p2", "p4"]), (3, &["p1"]), (4, &["p3"])])],
            readme: "Tournament standings with a Tie_Group: p2 and p4 share position 1, then 3 and 4. No tie is broken by the application. The fixture source has no ranking clause: rank.standing cites clause 2, the score clause, and \"verified\" in the report means the cited span resolves against a source with a matching hash, not that the clause states the rule (Req 34.4).",
        },
        Case {
            name: "bracket-standings-indeterminate",
            app: App::Bracket,
            rulebook: Rulebook::Basic,
            request: &[("ranking", "standing")],
            participants: CSV_MISSING_JUDGES,
            published: None,
            code: 4,
            differences: [0; 3],
            // p2 has no judges value, so its score and the ranking key are
            // missing: the standings are Indeterminate and name p2's judges.
            // The engine adds its own rank reason after the missing input;
            // it is checked separately below.
            outcomes: vec!["INDETERMINATE-RANK".to_owned()],
            readme: "Tournament standings with an Indeterminate position: p2 has no judges value, so the ranking key is missing and the standings are Indeterminate. The report names p2 and judges. Exit code 4. The fixture source has no ranking clause: rank.standing cites clause 2, the score clause, and \"verified\" in the report means the cited span resolves against a source with a matching hash, not that the clause states the rule (Req 34.4).",
        },
        Case {
            name: "bracket-qualification-weighted-score",
            app: App::Bracket,
            rulebook: Rulebook::WithQualification,
            request: &[("qualification", "qualified")],
            participants: CSV_FULL,
            published: None,
            code: 0,
            differences: [0; 3],
            // qualified = score >= 7.7: p1 7.6 false, p2 7.8 true,
            // p3 6 false, p4 7.8 true.
            outcomes: vec![boolean(false), boolean(true), boolean(false), boolean(true)],
            readme: "Qualification_Predicate over a weighted score. The predicate 'qualified' (score >= 7.7) is test-local: it is added to the slice-basic rulebook as a new revision and approved with `ruletrace review approve` (Reviewer 'Test Reviewer'), so compiled.json is a real compiled artefact. p2 and p4 qualify. The fixture source has no 7.7 threshold: elig.qual cites clause 2, the score clause, and \"verified\" in the report means the cited span resolves against a source with a matching hash, not that the clause states the rule (Req 34.4).",
        },
        Case {
            name: "bracket-published-equal",
            app: App::Bracket,
            rulebook: Rulebook::Basic,
            request: &[("ranking", "standing")],
            participants: CSV_FULL,
            published: Some("participant,target,value\np4,standing,1\np2,standing,2\np1,standing,3\np3,standing,4\n"),
            code: 0,
            differences: [0; 3],
            // Computed: p4 (7.8, judges 9), p2 (7.8, judges 7), p1, p3.
            outcomes: vec![standings(&[(1, &["p4"]), (2, &["p2"]), (3, &["p1"]), (4, &["p3"])])],
            readme: "Published standings equal to the computed standings: p4, p2, p1, p3 at 1 to 4. No difference. The fixture source has no ranking clause: rank.standing cites clause 2, the score clause, and \"verified\" in the report means the cited span resolves against a source with a matching hash, not that the clause states the rule (Req 34.4).",
        },
        Case {
            name: "bracket-published-breaks-tie",
            app: App::Bracket,
            rulebook: Rulebook::Basic,
            request: &[("ranking", "standing")],
            participants: CSV_TIE,
            published: Some("participant,target,value\np2,standing,1\np4,standing,2\np1,standing,3\np3,standing,4\n"),
            code: 0,
            differences: [0, 1, 0],
            // The computed Tie_Group {p2, p4} is at 1. The published file
            // puts p4 at 2: one tie_broken_without_declared_criterion
            // (RT-EXE-0012), a Warning.
            outcomes: vec![standings(&[(1, &["p2", "p4"]), (3, &["p1"]), (4, &["p3"])])],
            readme: "Published standings that break a computed tie: the file puts p4 at 2, but p2 and p4 are one Tie_Group at 1. One RT-EXE-0012 difference, a Warning, exit code 0. The fixture source has no ranking clause: rank.standing cites clause 2, the score clause, and \"verified\" in the report means the cited span resolves against a source with a matching hash, not that the clause states the rule (Req 34.4).",
        },
    ]
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_repo(rel: &str) -> Vec<u8> {
    std::fs::read(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn input(name: &str) -> InputId {
    InputId::new(name).unwrap()
}

fn target(name: &str) -> TargetName {
    TargetName::new(name).unwrap()
}

fn sources() -> SourceModels {
    let model = SourceModel::load(
        &input("model.json"),
        &read_repo(MODEL_REF),
        default_load_limits(),
    )
    .unwrap();
    let mut models = SourceModels::new();
    models.insert(model).unwrap();
    models
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

const QUALIFIED_RULE: &str = r#"{"id":"elig.qual","revision":1,"origin":{"kind":"manual","author":null},"kind":"eligibility","target":"qualified","type":"boolean","expr":{"node":"cmp","t":"boolean","op":"ge","left":{"node":"ref","t":"number","target":"score"},"right":{"node":"num","t":"number","value":"7.7"}},"markers":[],"spans":[{"document":"rules.md","start":92,"end":170,"line":4,"column":1}]}"#;

/// The `slice-basic` rulebook with the test-local rule appended.
fn variant_rulebook() -> String {
    let text = String::from_utf8(read_repo(&format!("{SLICE_BASIC}/rulebook.json"))).unwrap();
    let end = "\n  ],\n  \"precedence\"";
    assert_eq!(text.matches(end).count(), 1);
    text.replace(end, &format!(",{QUALIFIED_RULE}{end}"))
}

/// Approves `elig.qual` with `ruletrace review approve`, then runs
/// `ruletrace validate` and `ruletrace compile`. No ledger is written by any
/// other path.
fn build_variant() -> Vec<u8> {
    let session = Session::default();
    session.put(
        "ledger.jsonl",
        &read_repo(&format!("{SLICE_BASIC}/ledger.jsonl")),
    );
    session.put("model.json", &read_repo(MODEL_REF));
    let rulebook = variant_rulebook();
    session.put("rulebook.json", rulebook.as_bytes());
    let parsed = parse_rulebook(rulebook.as_bytes(), &input("rulebook.json"), 1 << 22, 64)
        .unwrap_or_else(|_| panic!("rulebook rejected"))
        .into_parts()
        .0;
    let revision = parsed
        .revisions
        .iter()
        .find(|r| r.key.id.as_str() == "elig.qual")
        .unwrap();
    let hash = rule_content_hash(&parsed.ir_version, revision, &parsed.documents)
        .unwrap()
        .to_string();
    let (code, stderr) = session.run(&[
        "review",
        "approve",
        "--rulebook",
        "rulebook.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "model.json",
        "--rule",
        "elig.qual",
        "--revision",
        "1",
        "--content-hash",
        &hash,
        "--reviewer",
        REVIEWER,
        "--time",
        TIME,
    ]);
    assert_eq!(code, 0, "approve: {stderr}");
    let (code, listed) = session.run_stdout(&[
        "review",
        "list",
        "--rulebook",
        "rulebook.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "model.json",
        "--rule",
        "elig.qual",
        "--revision",
        "1",
    ]);
    assert_eq!(code, 0, "{listed}");
    assert_eq!(
        listed
            .matches("rule 'elig.qual' revision 1 status Approved\n")
            .count(),
        1,
        "{listed}"
    );
    let (code, stderr) = session.run(&[
        "validate",
        "--rulebook",
        "rulebook.json",
        "--ledger",
        "ledger.jsonl",
        "--source-model",
        "model.json",
        "--out",
        "validation.json",
    ]);
    assert_eq!(code, 0, "validate: {stderr}");
    let (code, stderr) = session.run(&[
        "compile",
        "--validation",
        "validation.json",
        "--out",
        "compiled.json",
    ]);
    assert_eq!(code, 0, "compile: {stderr}");
    session.get("compiled.json").unwrap()
}

fn variant_compiled() -> &'static Vec<u8> {
    static CACHE: OnceLock<Vec<u8>> = OnceLock::new();
    CACHE.get_or_init(build_variant)
}

fn compiled_bytes(rulebook: Rulebook) -> Vec<u8> {
    match rulebook {
        Rulebook::Basic => read_repo(COMPILED_REF),
        Rulebook::WithQualification => variant_compiled().clone(),
    }
}

/// What one run produced.
struct Produced {
    human: String,
    json: Vec<u8>,
    observed: Observed,
    /// The Outcome texts of the results file `ruletrace evaluate` wrote.
    cli_outcomes: Vec<String>,
}

fn difference_counts(comparison: Option<&rt_engine::Comparison>) -> [usize; 3] {
    let mut counts = [0; 3];
    for d in comparison.map_or(&[][..], |c| &c.differences[..]) {
        match d.kind.as_str() {
            "value_differs" => counts[0] += 1,
            "tie_broken_without_declared_criterion" => counts[1] += 1,
            "tie_not_in_computed_result" => counts[2] += 1,
            other => panic!("unexpected difference kind {other}"),
        }
    }
    counts
}

/// The request as the option arguments `ruletrace-lab` would pass: the
/// position of each Target is its line in `request.txt`.
fn request_lines(case: &Case, option: &str) -> BTreeMap<TargetName, u32> {
    case.request
        .iter()
        .enumerate()
        .filter(|(_, (o, _))| *o == option)
        .map(|(i, (_, t))| (target(t), u32::try_from(i).unwrap()))
        .collect()
}

fn produce(case: &Case) -> Produced {
    let compiled_bytes = compiled_bytes(case.rulebook);
    let compiled = parse(&compiled_bytes);
    let models = sources();
    let options = MappingOptions::default();
    let (pid, qid) = (input("participants.csv"), input("published.csv"));
    macro_rules! go {
        ($krate:ident, $inputs:ident, $request:expr) => {{
            use $krate::{evaluate, present, FileInputs, NamedFile, PresentInputs};
            let request = $request;
            let files = FileInputs {
                participants: NamedFile {
                    bytes: case.participants.as_bytes(),
                    name: &pid,
                },
                collections: None,
                published: case.published.map(|text| NamedFile {
                    bytes: text.as_bytes(),
                    name: &qid,
                }),
                options: &options,
            };
            // Prepare, and the typed values of the evaluation (limit above).
            let run = evaluate(&$krate::$inputs {
                compiled: &compiled,
                request: &request,
                files,
                sources: &models,
                statements: &[],
                synthetic_data: Some(true),
            })
            .unwrap_or_else(|e| panic!("{}: rejected: {e:?}", case.name));
            let prepared = run.prepared();
            let evaluation = run.evaluation();
            // Present, from those values.
            let doc = present(&PresentInputs {
                compiled: &compiled,
                outcomes: &evaluation.outcomes,
                traces: &evaluation.traces,
                verifications: run.verifications(),
                diagnostics: &evaluation.diagnostics,
                comparison: evaluation.comparison.as_ref(),
                not_requested: prepared.not_requested(),
                input_hashes: prepared.input_hashes(),
                statements: &[],
                synthetic_data: Some(true),
            })
            .unwrap_or_else(|e| panic!("{}: present: {e:?}", case.name));
            let observed = Observed {
                data: prepared.data_bytes(),
                questions: prepared.questions_bytes(),
                published: prepared.published_bytes(),
                class_code: case.code,
                results: serialize_results(&ResultsInput {
                    outcomes: &evaluation.outcomes,
                    traces: &evaluation.traces,
                    diagnostics: &evaluation.diagnostics,
                    verifications: run.verifications(),
                })
                .unwrap(),
                differences: difference_counts(evaluation.comparison.as_ref()),
            };
            (render_human(&doc), render_json(&doc), observed)
        }};
    }
    let (human, json, observed) = match case.app {
        App::Prize => go!(
            prize_check,
            PrizeInputs,
            prize_check::PrizeRequest {
                eligibility: request_lines(case, "eligibility"),
                score: request_lines(case, "score"),
                ranking: request_lines(case, "ranking"),
            }
        ),
        App::Bracket => go!(
            bracket_check,
            BracketInputs,
            bracket_check::BracketRequest {
                ranking: request_lines(case, "ranking"),
                qualification: request_lines(case, "qualification"),
            }
        ),
    };
    // `ruletrace evaluate` on the canonical bytes: the exit code, the results
    // bytes and the published differences must be what Present was given.
    identical(&compiled_bytes, &observed, Some(&models))
        .unwrap_or_else(|e| panic!("{}: differs from ruletrace evaluate: {e}", case.name));
    let cli_outcomes = split_outcomes(&observed.results);
    Produced {
        human,
        json,
        observed,
        cli_outcomes,
    }
}

/// The Outcome texts of a results file: the array between `"outcomes":[` and
/// `],"traces"`, split at each `{"state":`.
fn split_outcomes(results: &[u8]) -> Vec<String> {
    let text = String::from_utf8(results.to_vec()).unwrap();
    let start = text.find(r#""outcomes":["#).unwrap() + r#""outcomes":["#.len();
    let end = text.find(r#"],"traces":"#).unwrap();
    text[start..end]
        .split(r#"{"state":"#)
        .skip(1)
        .map(|p| format!(r#"{{"state":{}"#, p.trim_end_matches(',')))
        .collect()
}

/// The files of a case: inputs, then goldens.
fn files(case: &Case, produced: &Produced) -> Vec<(&'static str, Vec<u8>)> {
    let mut out: Vec<(&'static str, Vec<u8>)> = Vec::new();
    out.push(("README.md", format!("{}\n", case.readme).into_bytes()));
    let mut request = String::new();
    for (option, name) in case.request {
        request.push_str(&format!("{option} {name}\n"));
    }
    out.push(("request.txt", request.into_bytes()));
    out.push(("participants.csv", case.participants.as_bytes().to_vec()));
    if let Some(published) = case.published {
        out.push(("published.csv", published.as_bytes().to_vec()));
    }
    match case.rulebook {
        Rulebook::Basic => out.push(("compiled.ref", format!("{COMPILED_REF}\n").into_bytes())),
        Rulebook::WithQualification => out.push(("compiled.json", compiled_bytes(case.rulebook))),
    }
    out.push(("source-model.ref", format!("{MODEL_REF}\n").into_bytes()));
    out.push(("report.json", produced.json.clone()));
    out.push(("report.txt", produced.human.clone().into_bytes()));
    out
}

fn first_difference(expected: &[u8], actual: &[u8]) -> usize {
    expected
        .iter()
        .zip(actual)
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| expected.len().min(actual.len()))
}

/// Hand-derived expectations: exit code, differences and Outcomes.
fn check_expectations(case: &Case, produced: &Produced) {
    assert_eq!(
        produced.observed.differences, case.differences,
        "{}: published differences",
        case.name
    );
    if case.outcomes == ["INDETERMINATE-RANK"] {
        // Reasons: the missing input of p2, then the engine's own rank
        // reason `RT-EXE-0006` (Code::ExeRankMissingValue).
        assert_eq!(produced.cli_outcomes.len(), 1, "{}", case.name);
        let outcome = &produced.cli_outcomes[0];
        assert!(
            outcome.starts_with(
                r#"{"state":"indeterminate","reasons":[{"kind":"missing_input","participant":"p2","field":"judges"}"#
            ),
            "{}: {outcome}",
            case.name
        );
        assert!(outcome.contains("RT-EXE-0006"), "{}: {outcome}", case.name);
        return;
    }
    assert_eq!(
        produced.cli_outcomes, case.outcomes,
        "{}: Outcomes",
        case.name
    );
}

fn run_case(name: &str) {
    let case = cases().into_iter().find(|c| c.name == name).unwrap();
    let produced = produce(&case);
    check_expectations(&case, &produced);
    let files = files(&case, &produced);
    let bless = std::env::var(BLESS_VAR).ok();
    let ci = std::env::var(CI_VAR).ok();
    if let Some(dir) =
        bless_target(bless.as_deref(), ci.as_deref()).unwrap_or_else(|e| panic!("{e}"))
    {
        let at = dir.join(case.name);
        std::fs::create_dir_all(&at).unwrap();
        for (file, bytes) in &files {
            std::fs::write(at.join(file), bytes).unwrap();
        }
        return;
    }
    let mut failures = Vec::new();
    for (file, bytes) in &files {
        let path = root().join(CASES_DIR).join(case.name).join(file);
        match std::fs::read(&path) {
            Err(e) => failures.push(format!(
                "case {}: file {file}: missing or unreadable ({e})",
                case.name
            )),
            Ok(committed) if committed != *bytes => failures.push(format!(
                "case {}: file {file}: first differing byte offset {} (committed {} bytes, produced {} bytes)",
                case.name,
                first_difference(&committed, bytes),
                committed.len(),
                bytes.len()
            )),
            Ok(_) => {}
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

macro_rules! golden_case {
    ($test:ident, $name:literal) => {
        #[test]
        fn $test() {
            run_case($name);
        }
    };
}

golden_case!(app_report_golden_prize_eligibility, "prize-eligibility");
golden_case!(
    app_report_golden_prize_weighted_score,
    "prize-weighted-score"
);
golden_case!(
    app_report_golden_prize_ranking_tie_group,
    "prize-ranking-tie-group"
);
golden_case!(
    app_report_golden_prize_indeterminate_missing_value,
    "prize-indeterminate-missing-value"
);
golden_case!(
    app_report_golden_prize_published_equal,
    "prize-published-equal"
);
golden_case!(
    app_report_golden_prize_published_differs,
    "prize-published-differs"
);
golden_case!(
    app_report_golden_bracket_standings_tie_group,
    "bracket-standings-tie-group"
);
golden_case!(
    app_report_golden_bracket_standings_indeterminate,
    "bracket-standings-indeterminate"
);
golden_case!(
    app_report_golden_bracket_qualification_weighted_score,
    "bracket-qualification-weighted-score"
);
golden_case!(
    app_report_golden_bracket_published_equal,
    "bracket-published-equal"
);
golden_case!(
    app_report_golden_bracket_published_breaks_tie,
    "bracket-published-breaks-tie"
);

#[test]
fn app_report_golden_every_case_has_one_test_function() {
    let source = include_str!("app_report_goldens.rs");
    let names: Vec<&str> = cases().iter().map(|c| c.name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "duplicate case name");
    for name in names {
        let literal = format!("\"{name}\"");
        // One use in `cases()` and one in its `golden_case!` line.
        assert_eq!(source.matches(&literal).count(), 2, "{name}");
    }
}

#[test]
fn bless_guard_refuses_ci_and_relative_paths_and_allows_only_absolute_outside_ci() {
    assert_eq!(bless_target(None, None), Ok(None));
    assert_eq!(bless_target(None, Some("true")), Ok(None));
    assert_eq!(
        bless_target(Some("/tmp/x"), None),
        Ok(Some(PathBuf::from("/tmp/x")))
    );
    assert!(bless_target(Some("/tmp/x"), Some("true"))
        .unwrap_err()
        .contains("refused in CI"));
    assert!(bless_target(Some("/tmp/x"), Some(""))
        .unwrap_err()
        .contains("refused in CI"));
    assert!(bless_target(Some("rel/dir"), None)
        .unwrap_err()
        .contains("absolute path"));
    assert!(bless_target(Some(""), None)
        .unwrap_err()
        .contains("absolute path"));
}
