//! Property 43: Applications read every business parameter from the
//! Compiled_Rulebook.
//!
//! Feature: ruletrace-compiler, Property 43: Applications read every
//! business parameter from the Compiled_Rulebook
//!
//! The metamorphic test of Req 31.4 on the Evaluate group of `prize-check`
//! and of `bracket-check`. The base Rulebook is `tests/fixtures/slice-basic`
//! plus one new Rule_Revision of `rank.standing` that declares two
//! tie-breaking criteria (`judges` descending, then `age` ascending). Age is
//! not a function of the score or of the judges, so the second criterion
//! decides ties that the first leaves, and the order of the two can be
//! swapped. Each variant adds exactly one new
//! Rule_Revision to that base: the `judges` weight 0.6 to 0.7, the age
//! threshold 16 to 18, or the criteria order swapped.
//!
//! Every new Rule_Revision is approved through `ruletrace review approve`
//! (the real `rt_cli::run`, in-memory file system, Reviewer `Test Reviewer`,
//! fixed time) appended to the committed `slice-basic` ledger. Then
//! `ruletrace validate` and `ruletrace compile` produce the Compiled_Rulebook.
//! No ledger is written by any other path (Req 33.11), and no committed file
//! changes.
//!
//! The unchanged application gets the variant's Compiled_Rulebook. For each
//! variant the test asserts that the application's results bytes equal
//! `ruletrace evaluate` on it, that the Outcomes equal hand-derived values,
//! that the changed Outcomes are exactly the ones the change reaches, and
//! that all other Outcomes equal those of the base.
//!
//! Two kinds of test: pinned cases with hand-derived Outcomes (the
//! derivations are in comments), and a property over at least 256 generated
//! data sets checked against a small independent reference model written
//! here in integer arithmetic (tenths), not against any production code.
//!
//! **Validates: Requirements 31.4, 15.6, 31.10**

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
use proptest::prelude::*;
use rt_compiled::{parse_compiled, CompiledRulebook};
use rt_data::tabular::MappingOptions;
use rt_data::TargetName;
use rt_explain::results::{serialize_results, ResultsInput};
use rt_foundation::{InputId, MAX_CONTAINER_DEPTH};
use rt_ir::{parse_rulebook, rule_content_hash};
use rt_srcmodel::{default_load_limits, SourceModel, SourceModels};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const SLICE_BASIC: &str = "tests/fixtures/slice-basic";
const REVIEWER: &str = "Test Reviewer";
const TIME: &str = "2026-11-19T09:00:00Z";

/// The fewest generated data sets (Property 43 as a property test).
const CASES: u32 = 256;

/// The floor of the specification. Lowering `CASES` fails the build.
const MIN_CASES: u32 = 256;
const _: () = assert!(CASES >= MIN_CASES);

const MANUAL: &str = r#""origin":{"kind":"manual","author":null}"#;
const SPAN_ELIG: &str =
    r#""spans":[{"document":"rules.md","start":21,"end":91,"line":3,"column":1}]"#;
const SPAN_SCORE: &str =
    r#""spans":[{"document":"rules.md","start":92,"end":170,"line":4,"column":1}]"#;

/// The three parameters that a variant may change, in tenths for weights.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Params {
    judges_weight_tenths: i64,
    technical_weight_tenths: i64,
    age_threshold: i64,
    /// True: `judges` desc then `age` asc. False: `age` asc then `judges`.
    judges_first: bool,
}

const BASE: Params = Params {
    judges_weight_tenths: 6,
    technical_weight_tenths: 4,
    age_threshold: 16,
    judges_first: true,
};

const WEIGHT: Params = Params {
    judges_weight_tenths: 7,
    ..BASE
};

const THRESHOLD: Params = Params {
    age_threshold: 18,
    ..BASE
};

const ORDER: Params = Params {
    judges_first: false,
    ..BASE
};

/// A new Rule_Revision: rule id, revision number and its IR JSON.
struct Revision {
    rule: &'static str,
    number: u32,
    json: String,
}

fn rank_revision(number: u32, judges_first: bool) -> Revision {
    let tb = |(name, direction): (&str, &str)| {
        format!(
            r#"{{"expr":{{"node":"field","t":"number","name":"{name}"}},"direction":"{direction}"}}"#
        )
    };
    let (first, second) = if judges_first {
        (("judges", "desc"), ("age", "asc"))
    } else {
        (("age", "asc"), ("judges", "desc"))
    };
    Revision {
        rule: "rank.standing",
        number,
        json: format!(
            r#"{{"id":"rank.standing","revision":{number},{MANUAL},"kind":"ranking","target":"standing","type":"ranking","key":{{"node":"ref","t":"number","target":"score"}},"direction":"desc","tie_breakers":[{},{}],"ties":"group","markers":[],{SPAN_SCORE}}}"#,
            tb(first),
            tb(second)
        ),
    }
}

fn score_revision(number: u32, judges_tenths: i64) -> Revision {
    let weight = |tenths: i64| format!("0.{tenths}");
    assert!((1..=9).contains(&judges_tenths));
    Revision {
        rule: "score.weighted",
        number,
        json: format!(
            r#"{{"id":"score.weighted","revision":{number},{MANUAL},"kind":"weighted_score","target":"score","type":"number","terms":[{{"weight":{{"node":"num","t":"number","value":"{}"}},"value":{{"node":"field","t":"number","name":"judges"}}}},{{"weight":{{"node":"num","t":"number","value":"0.4"}},"value":{{"node":"field","t":"number","name":"technical"}}}}],"rounding":null,"markers":[],{SPAN_SCORE}}}"#,
            weight(judges_tenths)
        ),
    }
}

fn eligibility_revision(number: u32, threshold: i64) -> Revision {
    Revision {
        rule: "elig.basic",
        number,
        json: format!(
            r#"{{"id":"elig.basic","revision":{number},{MANUAL},"kind":"eligibility","target":"eligible","type":"boolean","expr":{{"node":"and","t":"boolean","args":[{{"node":"cmp","t":"boolean","op":"ge","left":{{"node":"field","t":"number","name":"age"}},"right":{{"node":"num","t":"number","value":"{threshold}"}}}},{{"node":"field","t":"boolean","name":"licence"}}]}},"markers":[],{SPAN_ELIG}}}"#
        ),
    }
}

/// The revisions that turn `slice-basic` into the Rulebook of `params`: the
/// two-criteria ranking is always present (revision 2), and at most one more
/// revision changes exactly one parameter.
fn revisions_for(params: Params) -> Vec<Revision> {
    let mut list = vec![rank_revision(2, true)];
    if params == BASE {
    } else if params == WEIGHT {
        list.push(score_revision(2, params.judges_weight_tenths));
    } else if params == THRESHOLD {
        list.push(eligibility_revision(2, params.age_threshold));
    } else if params == ORDER {
        list.push(rank_revision(3, false));
    } else {
        panic!("not a pinned variant: {params:?}");
    }
    list
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> Vec<u8> {
    std::fs::read(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn input(name: &str) -> InputId {
    InputId::new(name).unwrap()
}

/// The committed rulebook text with the given revisions appended to its
/// `rules` array.
fn rulebook_with(revisions: &[&Revision]) -> String {
    let text = String::from_utf8(read(&format!("{SLICE_BASIC}/rulebook.json"))).unwrap();
    let end = "\n  ],\n  \"precedence\"";
    assert_eq!(text.matches(end).count(), 1);
    let extra: String = revisions.iter().map(|r| format!(",{}", r.json)).collect();
    text.replace(end, &format!("{extra}{end}"))
}

/// The content hash `review approve` asks the Reviewer to confirm.
fn content_hash(rulebook: &str, rule: &str, number: u32) -> String {
    let parsed = parse_rulebook(rulebook.as_bytes(), &input("rulebook.json"), 1 << 22, 64)
        .unwrap_or_else(|_| panic!("rulebook rejected"))
        .into_parts()
        .0;
    let revision = parsed
        .revisions
        .iter()
        .find(|r| r.key.id.as_str() == rule && r.key.revision.to_string() == number.to_string())
        .unwrap();
    rule_content_hash(&parsed.ir_version, revision, &parsed.documents)
        .unwrap()
        .to_string()
}

/// Approves each new revision with `ruletrace review approve`, then runs
/// `ruletrace validate` and `ruletrace compile`. Returns the compiled bytes.
fn build_compiled(revisions: &[Revision]) -> Vec<u8> {
    let session = Session::default();
    session.put(
        "ledger.jsonl",
        &read(&format!("{SLICE_BASIC}/ledger.jsonl")),
    );
    session.put(
        "model.json",
        &read(&format!("{SLICE_BASIC}/expected/source-model.json")),
    );
    let mut so_far: Vec<&Revision> = Vec::new();
    for revision in revisions {
        so_far.push(revision);
        let rulebook = rulebook_with(&so_far);
        session.put("rulebook.json", rulebook.as_bytes());
        let hash = content_hash(&rulebook, revision.rule, revision.number);
        let number = revision.number.to_string();
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
            revision.rule,
            "--revision",
            &number,
            "--content-hash",
            &hash,
            "--reviewer",
            REVIEWER,
            "--time",
            TIME,
        ]);
        assert_eq!(code, 0, "approve {} r{number}: {stderr}", revision.rule);
    }
    assert_ledger_lists_approvals(&session, revisions);
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

/// Reads the variant ledger back. `ruletrace review list` must show each new
/// revision as Approved. It does not print the Reviewer, so the ledger file
/// must also hold one approval record by the Test Reviewer per new revision.
fn assert_ledger_lists_approvals(session: &Session, revisions: &[Revision]) {
    for revision in revisions {
        let number = revision.number.to_string();
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
            revision.rule,
            "--revision",
            &number,
        ]);
        assert_eq!(code, 0, "review list: {listed}");
        let heading = format!(
            "rule '{}' revision {} status Approved\n",
            revision.rule, revision.number
        );
        assert_eq!(listed.matches(&heading).count(), 1, "{heading}{listed}");
    }
    let ledger = String::from_utf8(session.get("ledger.jsonl").unwrap()).unwrap();
    let committed = String::from_utf8(read(&format!("{SLICE_BASIC}/ledger.jsonl"))).unwrap();
    let appended = ledger.strip_prefix(&committed).unwrap();
    let records: Vec<&str> = appended.lines().collect();
    assert_eq!(records.len(), revisions.len(), "{appended}");
    for (record, revision) in records.iter().zip(revisions) {
        assert!(record.contains(r#""kind":"approval""#), "{record}");
        assert!(record.contains(&format!(r#""rule":"{}""#, revision.rule)));
        assert!(record.contains(&format!(r#""revision":{}"#, revision.number)));
        assert!(record.contains(&format!(r#""reviewer":"{REVIEWER}""#)));
    }
}

/// Compiled bytes per pinned variant, built once through the real commands.
fn compiled_of(params: Params) -> &'static Vec<u8> {
    static CACHE: OnceLock<Vec<(Params, Vec<u8>)>> = OnceLock::new();
    let all = CACHE.get_or_init(|| {
        [BASE, WEIGHT, THRESHOLD, ORDER]
            .into_iter()
            .map(|p| (p, build_compiled(&revisions_for(p))))
            .collect()
    });
    &all.iter().find(|(p, _)| *p == params).unwrap().1
}

fn sources() -> SourceModels {
    let bytes = read(&format!("{SLICE_BASIC}/expected/source-model.json"));
    let model = SourceModel::load(&input("model.json"), &bytes, default_load_limits()).unwrap();
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum App {
    Prize,
    Bracket,
}

/// Runs the Evaluate group of `app` over `csv`, requesting every Target the
/// application has: eligibility, score and ranking for Prize, the ranking
/// for Bracket.
fn run_app(app: App, compiled_bytes: &[u8], csv: &str) -> Observed {
    let compiled = parse(compiled_bytes);
    let models = sources();
    let options = MappingOptions::default();
    let participants_id = input("participants.csv");
    let target = |name: &str| TargetName::new(name).unwrap();
    macro_rules! go {
        ($krate:ident, $inputs:ident, $request:expr) => {{
            use $krate::{evaluate, FileInputs, NamedFile, ResultClass};
            let request = $request;
            let run = evaluate(&$krate::$inputs {
                compiled: &compiled,
                request: &request,
                files: FileInputs {
                    participants: NamedFile {
                        bytes: csv.as_bytes(),
                        name: &participants_id,
                    },
                    collections: None,
                    published: None,
                    options: &options,
                },
                sources: &models,
                statements: &[],
                synthetic_data: None,
            })
            .unwrap_or_else(|e| panic!("the application rejected the input: {e:?}"));
            let prepared = run.prepared();
            let evaluation = run.evaluation();
            assert!(matches!(run.class(), ResultClass::Complete));
            Observed {
                data: prepared.data_bytes(),
                questions: prepared.questions_bytes(),
                published: prepared.published_bytes(),
                class_code: 0,
                results: serialize_results(&ResultsInput {
                    outcomes: &evaluation.outcomes,
                    traces: &evaluation.traces,
                    diagnostics: &evaluation.diagnostics,
                    verifications: run.verifications(),
                })
                .unwrap(),
                differences: [0; 3],
            }
        }};
    }
    match app {
        App::Prize => go!(
            prize_check,
            PrizeInputs,
            prize_check::PrizeRequest {
                eligibility: [(target("eligible"), 2)].into(),
                score: [(target("score"), 4)].into(),
                ranking: [(target("standing"), 6)].into(),
            }
        ),
        App::Bracket => go!(
            bracket_check,
            BracketInputs,
            bracket_check::BracketRequest {
                ranking: [(target("standing"), 2)].into(),
                qualification: BTreeMap::new(),
            }
        ),
    }
}

/// The application run on `compiled_bytes` with the check that its results
/// bytes and exit code equal `ruletrace evaluate`'s. Returns the Outcomes as
/// separate JSON texts.
fn outcomes_checked(app: App, compiled_bytes: &[u8], csv: &str) -> Vec<String> {
    let observed = run_app(app, compiled_bytes, csv);
    let code = identical(compiled_bytes, &observed, Some(&sources()))
        .unwrap_or_else(|e| panic!("{app:?} differs from ruletrace evaluate: {e}"));
    assert_eq!(code, 0);
    split_outcomes(&observed.results)
}

/// The Outcome texts of a results file: the array between `"outcomes":[` and
/// `],"traces"`, split at each `{"state":`.
fn split_outcomes(results: &[u8]) -> Vec<String> {
    let text = String::from_utf8(results.to_vec()).unwrap();
    let start = text.find(r#""outcomes":["#).unwrap() + r#""outcomes":["#.len();
    let end = text.find(r#"],"traces":"#).unwrap();
    let body = &text[start..end];
    let mut parts: Vec<String> = body
        .split(r#"{"state":"#)
        .skip(1)
        .map(|p| format!(r#"{{"state":{}"#, p.trim_end_matches(',')))
        .collect();
    assert!(parts.iter().all(|p| p.starts_with(r#"{"state":"decided""#)));
    parts.shrink_to_fit();
    parts
}

fn boolean(value: bool) -> String {
    format!(r#"{{"state":"decided","value":{{"type":"bool","value":{value}}}}}"#)
}

fn number(text: &str) -> String {
    format!(r#"{{"state":"decided","value":{{"type":"number","value":"{text}"}}}}"#)
}

/// A standings Outcome from places given as (position, members).
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

/// Participants a..f: the six eligibility Outcomes, the six scores, then
/// the standings, the order of the Questions of the Prize request.
fn prize_outcomes(eligible: [bool; 6], scores: [&str; 6], order: [&str; 6]) -> Vec<String> {
    let mut list = Vec::new();
    list.extend(eligible.iter().map(|e| boolean(*e)));
    list.extend(scores.iter().map(|s| number(s)));
    let places: Vec<(usize, &[&str])> = order
        .iter()
        .enumerate()
        .map(|(i, m)| (i + 1, std::slice::from_ref(m)))
        .collect();
    list.push(standings(&places));
    list
}

/// Pinned data, columns age, licence, judges, technical.
///
/// ```text
/// id  age licence judges technical
/// a   20  true    8      7
/// b   20  true    7      8.5
/// c   18  true    6      10
/// d   17  true    9      5
/// e   25  false   5      5
/// f   19  true    8      7
/// ```
///
/// f equals a on judges and technical, so f and a tie on every score and on
/// judges, and only age (f 19, a 20) separates them.
const PINNED_CSV: &str = "participant,age,licence,judges,technical\na,20,true,8,7\nb,20,true,7,8.5\nc,18,true,6,10\nd,17,true,9,5\ne,25,false,5,5\nf,19,true,8,7\n";

// Base (judges 0.6, technical 0.4, age >= 16, ties: judges descending, then
// age ascending):
//   eligible: a, b, c, d, f true (age >= 16 and licence); e false (no licence)
//   score a = 0.6*8 + 0.4*7   = 4.8 + 2.8 = 7.6
//   score b = 0.6*7 + 0.4*8.5 = 4.2 + 3.4 = 7.6
//   score c = 0.6*6 + 0.4*10  = 3.6 + 4.0 = 7.6
//   score d = 0.6*9 + 0.4*5   = 5.4 + 2.0 = 7.4
//   score e = 0.6*5 + 0.4*5   = 3.0 + 2.0 = 5
//   score f = 0.6*8 + 0.4*7   = 7.6
//   a, b, c, f tie on 7.6. Judges descending: a and f (8), b (7), c (6).
//   a and f tie on judges too: age ascending puts f (19) before a (20).
//   Order f, a, b, c, then d, e.
fn pinned_base() -> Vec<String> {
    prize_outcomes(
        [true, true, true, true, false, true],
        ["7.6", "7.6", "7.6", "7.4", "5", "7.6"],
        ["f", "a", "b", "c", "d", "e"],
    )
}

// Weight variant (judges 0.7, everything else as the base):
//   score a = 0.7*8 + 2.8 = 5.6 + 2.8 = 8.4
//   score b = 0.7*7 + 3.4 = 4.9 + 3.4 = 8.3
//   score c = 0.7*6 + 4.0 = 4.2 + 4.0 = 8.2
//   score d = 0.7*9 + 2.0 = 6.3 + 2.0 = 8.3
//   score e = 0.7*5 + 2.0 = 3.5 + 2.0 = 5.5
//   score f = 0.7*8 + 2.8 = 8.4
//   Eligibility is unchanged. a and f tie on 8.4 and on judges: f (19)
//   before a (20). b and d tie on 8.3: judges descending puts d (9) before
//   b (7). Order f, a, d, b, c, e.
fn pinned_weight() -> Vec<String> {
    prize_outcomes(
        [true, true, true, true, false, true],
        ["8.4", "8.3", "8.2", "8.3", "5.5", "8.4"],
        ["f", "a", "d", "b", "c", "e"],
    )
}

// Threshold variant (age >= 18):
//   eligible: a (20) true, b (20) true, c (18) true (the boundary is
//   inclusive), d (17) false (was true), e false (no licence), f (19) true.
//   Scores and standings are the base's.
fn pinned_threshold() -> Vec<String> {
    prize_outcomes(
        [true, true, true, false, false, true],
        ["7.6", "7.6", "7.6", "7.4", "5", "7.6"],
        ["f", "a", "b", "c", "d", "e"],
    )
}

// Order variant (age ascending, then judges descending): a, b, c, f still
// tie on 7.6. Age ascending: c (18), f (19), then a and b (both 20), which
// judges descending separates: a (8) before b (7). Order c, f, a, b, then
// d, e. Eligibility and scores are the base's. Against the base, c moves
// from fourth to first and f from first to second.
fn pinned_order() -> Vec<String> {
    prize_outcomes(
        [true, true, true, true, false, true],
        ["7.6", "7.6", "7.6", "7.4", "5", "7.6"],
        ["c", "f", "a", "b", "d", "e"],
    )
}

fn last(list: &[String]) -> &String {
    list.last().unwrap()
}

#[test]
fn prize_check_reads_every_parameter_from_the_compiled_rulebook() {
    let base = outcomes_checked(App::Prize, compiled_of(BASE), PINNED_CSV);
    assert_eq!(base, pinned_base());
    let cases = [
        (WEIGHT, pinned_weight()),
        (THRESHOLD, pinned_threshold()),
        (ORDER, pinned_order()),
    ];
    for (params, expected) in cases {
        let got = outcomes_checked(App::Prize, compiled_of(params), PINNED_CSV);
        assert_eq!(got, expected, "{params:?}");
    }
    // The change is observable, and only where the changed parameter reaches.
    let by_index = |got: &[String], pick: fn(usize) -> bool| -> Vec<String> {
        got.iter()
            .enumerate()
            .filter(|(i, _)| pick(*i))
            .map(|(_, o)| o.clone())
            .collect()
    };
    let is_eligibility = |i: usize| i < 6;
    let is_score = |i: usize| (6..12).contains(&i);
    let weight = outcomes_checked(App::Prize, compiled_of(WEIGHT), PINNED_CSV);
    assert_eq!(
        by_index(&weight, is_eligibility),
        by_index(&base, is_eligibility)
    );
    assert_ne!(by_index(&weight, is_score), by_index(&base, is_score));
    assert_ne!(last(&weight), last(&base));

    let threshold = outcomes_checked(App::Prize, compiled_of(THRESHOLD), PINNED_CSV);
    assert_ne!(
        by_index(&threshold, is_eligibility),
        by_index(&base, is_eligibility)
    );
    assert_eq!(by_index(&threshold, is_score), by_index(&base, is_score));
    assert_eq!(last(&threshold), last(&base));

    let order = outcomes_checked(App::Prize, compiled_of(ORDER), PINNED_CSV);
    assert_eq!(
        by_index(&order, is_eligibility),
        by_index(&base, is_eligibility)
    );
    assert_eq!(by_index(&order, is_score), by_index(&base, is_score));
    assert_ne!(last(&order), last(&base));
}

#[test]
fn bracket_check_reads_every_parameter_from_the_compiled_rulebook() {
    let expect = |params: Params, order: [&str; 6]| {
        let got = outcomes_checked(App::Bracket, compiled_of(params), PINNED_CSV);
        let places: Vec<(usize, &[&str])> = order
            .iter()
            .enumerate()
            .map(|(i, m)| (i + 1, std::slice::from_ref(m)))
            .collect();
        assert_eq!(got, vec![standings(&places)], "{params:?}");
        got
    };
    let base = expect(BASE, ["f", "a", "b", "c", "d", "e"]);
    // The weight change reorders the standings (see `pinned_weight`).
    let weight = expect(WEIGHT, ["f", "a", "d", "b", "c", "e"]);
    assert_ne!(weight, base);
    // The threshold is not an input of the ranking: the standings stay.
    let threshold = expect(THRESHOLD, ["f", "a", "b", "c", "d", "e"]);
    assert_eq!(threshold, base);
    // The swapped criteria reorder the tied a, b, c, f (see `pinned_order`).
    let order = expect(ORDER, ["c", "f", "a", "b", "d", "e"]);
    assert_ne!(order, base);
}

// Reference model, independent of every production crate. Participant i has
// integer age, licence, judges and technical; weights are tenths, so a score
// is an integer number of tenths.

#[derive(Clone, Debug)]
struct Row {
    age: i64,
    licence: bool,
    judges: i64,
    technical: i64,
}

fn tenths_text(tenths: i64) -> String {
    if tenths % 10 == 0 {
        (tenths / 10).to_string()
    } else {
        format!("{}.{}", tenths / 10, tenths % 10)
    }
}

/// Expected Prize Outcomes for rows named p01, p02, ... in table order
/// (sorted by name, which is also numeric order).
fn model_prize(rows: &[Row], p: Params) -> Vec<String> {
    let score =
        |r: &Row| p.judges_weight_tenths * r.judges + p.technical_weight_tenths * r.technical;
    let mut list = Vec::new();
    list.extend(
        rows.iter()
            .map(|r| boolean(r.age >= p.age_threshold && r.licence)),
    );
    list.extend(rows.iter().map(|r| number(&tenths_text(score(r)))));
    list.push(model_standings(rows, p));
    list
}

fn model_standings(rows: &[Row], p: Params) -> String {
    let key = |r: &Row| {
        let score = p.judges_weight_tenths * r.judges + p.technical_weight_tenths * r.technical;
        // Age ascending is descending order of the negated age.
        if p.judges_first {
            (score, r.judges, -r.age)
        } else {
            (score, -r.age, r.judges)
        }
    };
    let mut named: Vec<(String, (i64, i64, i64))> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (format!("p{:02}", i + 1), key(r)))
        .collect();
    named.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));
    let mut places: Vec<(usize, Vec<String>)> = Vec::new();
    let mut before = 0;
    let mut i = 0;
    while i < named.len() {
        let mut j = i;
        while j < named.len() && named[j].1 == named[i].1 {
            j += 1;
        }
        places.push((
            before + 1,
            named[i..j].iter().map(|(n, _)| n.clone()).collect(),
        ));
        before += j - i;
        i = j;
    }
    let refs: Vec<(usize, Vec<&str>)> = places
        .iter()
        .map(|(pos, m)| (*pos, m.iter().map(String::as_str).collect()))
        .collect();
    let slices: Vec<(usize, &[&str])> = refs.iter().map(|(p, m)| (*p, m.as_slice())).collect();
    standings(&slices)
}

fn rows_csv(rows: &[Row], file_order: &[usize]) -> String {
    let mut text = String::from("participant,age,licence,judges,technical\n");
    for &i in file_order {
        let r = &rows[i];
        text.push_str(&format!(
            "p{:02},{},{},{},{}\n",
            i + 1,
            r.age,
            r.licence,
            r.judges,
            r.technical
        ));
    }
    text
}

fn row() -> impl Strategy<Value = Row> {
    // Ages straddle both thresholds (16 and 18). Small ranges make equal
    // scores, equal judges and equal ages common, so each tie-breaker
    // decides some pairs.
    (
        prop_oneof![14i64..=22, Just(16), Just(18), Just(17)],
        any::<bool>(),
        0i64..=4,
        0i64..=4,
    )
        .prop_map(|(age, licence, judges, technical)| Row {
            age,
            licence,
            judges,
            technical,
        })
}

fn data_set() -> impl Strategy<Value = (Vec<Row>, Vec<usize>)> {
    prop::collection::vec(row(), 1..=8).prop_flat_map(|rows| {
        let order: Vec<usize> = (0..rows.len()).collect();
        (Just(rows), Just(order).prop_shuffle())
    })
}

/// For each generated data set and each variant, both applications equal
/// `ruletrace evaluate` on the variant's Compiled_Rulebook and the reference
/// model, which takes the parameters from the variant alone.
///
/// The runner is built explicitly, as in `rt-cli`'s adoption property, so the
/// case count is the one written here and `PROPTEST_CASES` cannot lower it;
/// the cases that actually ran are counted and checked against the floor.
#[test]
fn applications_follow_the_variant_compiled_rulebook() {
    let config = ProptestConfig {
        cases: CASES,
        failure_persistence: None,
        ..ProptestConfig::default()
    };
    let ran = std::cell::Cell::new(0u32);
    let mut runner = proptest::test_runner::TestRunner::new(config);
    let result = runner.run(&data_set(), |(rows, order)| {
        ran.set(ran.get() + 1);
        let csv = rows_csv(&rows, &order);
        for params in [BASE, WEIGHT, THRESHOLD, ORDER] {
            let compiled = compiled_of(params);
            let prize = outcomes_checked(App::Prize, compiled, &csv);
            prop_assert_eq!(&prize, &model_prize(&rows, params), "prize {:?}", params);
            let bracket = outcomes_checked(App::Bracket, compiled, &csv);
            prop_assert_eq!(
                bracket,
                vec![model_standings(&rows, params)],
                "bracket {:?}",
                params
            );
        }
        Ok(())
    });
    if let Err(failure) = result {
        panic!("Property 43: {failure}");
    }
    assert!(
        ran.get() >= MIN_CASES,
        "Property 43 ran {} cases, fewer than {MIN_CASES}",
        ran.get()
    );
}

/// Hand check of the reference model on two ties.
///
/// p01 (age 18) scores 0.6*2 + 0.4*3 = 2.4 and p02 (age 20) scores
/// 0.6*4 + 0.4*0 = 2.4. Judges first puts p02 (4) before p01 (2). Age first
/// puts p01 (18) before p02 (20).
///
/// In the second pair p01 and p02 both score 0.6*3 + 0.4*3 = 3 and have
/// judges 3, so only age separates them: p02 (19) before p01 (20) in both
/// orders.
#[test]
fn the_reference_model_matches_a_hand_derived_tie() {
    let row = |age, judges, technical| Row {
        age,
        licence: true,
        judges,
        technical,
    };
    let tied = vec![row(18, 2, 3), row(20, 4, 0)];
    assert_eq!(
        model_standings(&tied, BASE),
        standings(&[(1, &["p02"]), (2, &["p01"])])
    );
    assert_eq!(
        model_standings(&tied, ORDER),
        standings(&[(1, &["p01"]), (2, &["p02"])])
    );
    let by_age = vec![row(20, 3, 3), row(19, 3, 3)];
    let expected = standings(&[(1, &["p02"]), (2, &["p01"])]);
    assert_eq!(model_standings(&by_age, BASE), expected);
    assert_eq!(model_standings(&by_age, ORDER), expected);
}
