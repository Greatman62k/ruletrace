//! Req 32.5 for the `bracket-check` library: the machine-readable report has
//! exactly the documented fields and none of pairing, round, bracket
//! position, seed, slot, advancement or elimination.
//!
//! The key paths are read from the bytes of reports that the library builds
//! over several runs (a Tie_Group, an Indeterminate position, a published
//! file that splits a tie, a published file that is equal, an unverified
//! provenance). Every path must be in `FIELD_SET_BRACKET`, and no path
//! segment may be on the forbidden list written here. The test also checks
//! that the scan itself flags a forbidden or extra key.
//!
//! **Validates: Requirements 32.5, 32.1, 32.2, 32.6**

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use bracket_check::{evaluate, BracketInputs, BracketRequest, FileInputs, NamedFile};
use rt_compiled::parse_compiled;
use rt_data::tabular::MappingOptions;
use rt_data::TargetName;
use rt_explain::report::{render_json, FIELD_SET_BRACKET};
use rt_foundation::{InputId, MAX_CONTAINER_DEPTH};
use rt_json::{Kind, ParseLimits, Value};
use rt_srcmodel::{default_load_limits, SourceModel, SourceModels};
use std::collections::BTreeSet;
use std::path::Path;

const FIXTURE: &str = "../../tests/fixtures/slice-basic";
const CSV_FULL: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,7,9\np3,15,true,6,6\np4,30,false,9,6\n";
const CSV_TIE: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,7,9\np3,15,true,6,6\np4,30,false,7,9\n";
const CSV_MISSING_JUDGES: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,,9\np3,15,true,6,6\np4,30,false,9,6\n";
const SPLIT: &str =
    "participant,target,value\np2,standing,1\np4,standing,2\np1,standing,3\np3,standing,4\n";
const EQUAL: &str =
    "participant,target,value\np2,standing,1\np4,standing,1\np1,standing,3\np3,standing,4\n";

const QUALIFIED_RULE: &str = r#"{"id":"elig.qual","revision":1,"kind":"eligibility","target":"qualified","type":"boolean","spans":[{"document":"rules.md","sha256":"eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a","start":92,"end":170,"line":4,"column":1}],"nodes":[{"idx":0,"node":"ref","t":"number","target":"score"},{"idx":1,"node":"num","t":"number","value":"7.7"},{"idx":2,"node":"cmp","t":"boolean","op":"ge","left":0,"right":1}],"expr":2},"#;

/// Field names that no Bracket_Check report may have (Req 32.5), and the
/// validity names of Req 22.22.
const FORBIDDEN: [&str; 24] = [
    "pairing",
    "pairings",
    "pair",
    "pairs",
    "round",
    "rounds",
    "bracket",
    "brackets",
    "bracket_position",
    "seed",
    "seeds",
    "seeding",
    "slot",
    "slots",
    "advancement",
    "advances",
    "advance",
    "elimination",
    "eliminated",
    "valid",
    "ok",
    "passed",
    "has_errors",
    "no_errors",
];

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

fn compiled_bytes() -> Vec<u8> {
    let text = String::from_utf8(fixture("expected/compiled.json")).unwrap();
    let after_basic = r#""expr":4},"#;
    let order = r#""order":["elig.basic","score.weighted","rank.standing"]"#;
    text.replace(after_basic, &format!("{after_basic}{QUALIFIED_RULE}"))
        .replace(
            order,
            r#""order":["elig.basic","score.weighted","elig.qual","rank.standing"]"#,
        )
        .into_bytes()
}

fn sources() -> SourceModels {
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

fn report_json(csv: &str, published: Option<&str>, models: &SourceModels) -> Vec<u8> {
    let compiled = parse_compiled(
        &compiled_bytes(),
        &input("compiled.json"),
        64 * 1024 * 1024,
        MAX_CONTAINER_DEPTH,
    )
    .unwrap();
    let request = BracketRequest {
        ranking: [(TargetName::new("standing").unwrap(), 2)].into(),
        qualification: [(TargetName::new("qualified").unwrap(), 4)].into(),
    };
    let (pid, qid, options) = (
        input("participants.csv"),
        input("published.csv"),
        MappingOptions::default(),
    );
    let run = evaluate(&BracketInputs {
        compiled: &compiled,
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
        sources: models,
        statements: &[],
        synthetic_data: None,
    })
    .unwrap();
    render_json(run.report().unwrap())
}

/// Key paths of a JSON document: object keys joined by `.`, `[]` after an
/// array key, empty arrays contribute nothing.
fn key_paths(bytes: &[u8]) -> BTreeSet<String> {
    let limits = ParseLimits::new(1 << 24, 64);
    let v = rt_json::parse(&input("report"), bytes, limits).unwrap();
    let mut out = BTreeSet::new();
    walk(&v, "", &mut out);
    out
}

fn walk<P>(v: &Value<P>, prefix: &str, out: &mut BTreeSet<String>) {
    match v.kind() {
        Kind::Object(members) => {
            for m in members {
                let key = if prefix.is_empty() {
                    m.key.clone()
                } else {
                    format!("{prefix}.{}", m.key)
                };
                walk(&m.value, &key, out);
            }
        }
        Kind::Array(items) => {
            for i in items {
                walk(i, &format!("{prefix}[]"), out);
            }
        }
        _ => {
            out.insert(prefix.to_owned());
        }
    }
}

fn documented() -> BTreeSet<String> {
    FIELD_SET_BRACKET.iter().map(|p| (*p).to_owned()).collect()
}

/// Paths not in the documented set, and paths with a forbidden segment.
fn violations(paths: &BTreeSet<String>) -> (Vec<String>, Vec<String>) {
    let allowed = documented();
    let extra = paths.difference(&allowed).cloned().collect();
    let forbidden = paths
        .iter()
        .filter(|p| {
            p.split('.')
                .any(|s| FORBIDDEN.contains(&s.trim_end_matches("[]")))
        })
        .cloned()
        .collect();
    (extra, forbidden)
}

fn runs() -> Vec<Vec<u8>> {
    let with = sources();
    let none = SourceModels::new();
    vec![
        report_json(CSV_FULL, None, &with),
        report_json(CSV_TIE, None, &with),
        report_json(CSV_TIE, Some(SPLIT), &with),
        report_json(CSV_TIE, Some(EQUAL), &with),
        report_json(CSV_MISSING_JUDGES, None, &with),
        report_json(CSV_FULL, None, &none),
    ]
}

#[test]
fn every_key_path_of_every_report_is_in_the_documented_field_set() {
    for bytes in runs() {
        let paths = key_paths(&bytes);
        let (extra, forbidden) = violations(&paths);
        assert_eq!(extra, Vec::<String>::new());
        assert_eq!(forbidden, Vec::<String>::new());
    }
}

#[test]
fn the_reports_together_use_the_core_fields_so_the_scan_is_not_vacuous() {
    let mut all = BTreeSet::new();
    for bytes in runs() {
        all.extend(key_paths(&bytes));
    }
    for required in [
        "header.application",
        "header.ranking_scope",
        "standings[].target",
        "standings[].places[].position",
        "standings[].places[].members[]",
        "standings[].places[].tie_group",
        "standings[].places[].order_note",
        "standings[].comparisons[].between[]",
        "standings[].comparisons[].criteria[].left",
        "standings[].comparisons[].criteria[].result",
        "standings[].missing[].field",
        "qualification[].participant",
        "qualification[].result",
        "qualification[].inputs_read[].value",
        "qualification[].missing[].participant",
        "qualification[].rules[].rule",
        "qualification[].rules[].revision",
        "qualification[].sources[].start",
        "published.state",
        "published.differences[].kind",
        "published.differences[].code",
        "published.differences[].group[]",
    ] {
        assert!(all.contains(required), "no report has {required}");
    }
}

#[test]
fn the_scan_flags_an_extra_field_and_each_forbidden_name() {
    let base = key_paths(&runs().remove(0));
    assert_eq!(violations(&base), (Vec::new(), Vec::new()));
    for name in FORBIDDEN {
        let mut paths = base.clone();
        paths.insert(format!("standings[].{name}"));
        let (extra, forbidden) = violations(&paths);
        assert_eq!(extra, vec![format!("standings[].{name}")], "{name}");
        assert_eq!(forbidden, vec![format!("standings[].{name}")], "{name}");
    }
    let mut paths = base;
    paths.insert("standings[].advance_to".to_owned());
    let (extra, forbidden) = violations(&paths);
    assert_eq!(extra, vec!["standings[].advance_to".to_owned()]);
    assert_eq!(forbidden, Vec::<String>::new());
}
