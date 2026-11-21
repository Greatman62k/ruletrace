//! Shared fixtures of the unit tests. The Compiled_Rulebook is the committed
//! `slice-basic` one (read at build time, so the library tests open no file)
//! plus one test-local Qualification_Predicate, and the CSV inputs are written
//! by hand.
//!
//! Rules, read from the Compiled_Rulebook: `score` is
//! `0.6 * judges + 0.4 * technical`; `standing` ranks `score` descending,
//! then `judges` descending (criterion 0 the key, criterion 1 the tie-breaker),
//! tied participants forming a Tie_Group; `qualified` is `score >= 7.7`, citing
//! the score clause of `rules.md` (bytes 92 to 170).
//!
//! The qualification rule is test data only. It is added to the committed
//! text here, so no committed fixture changes.

use crate::{BracketInputs, BracketRequest, Evaluated, FileInputs, NamedFile, Rejection};
use rt_compiled::{parse_compiled, CompiledRulebook};
use rt_data::tabular::MappingOptions;
use rt_data::TargetName;
use rt_foundation::{InputId, MAX_CONTAINER_DEPTH};
use rt_srcmodel::{default_load_limits, SourceModel, SourceModels};

const COMPILED: &str = include_str!("../../../tests/fixtures/slice-basic/expected/compiled.json");
const MODEL: &str = include_str!("../../../tests/fixtures/slice-basic/expected/source-model.json");

const QUALIFIED_RULE: &str = r#"{"id":"elig.qual","revision":1,"kind":"eligibility","target":"qualified","type":"boolean","spans":[{"document":"rules.md","sha256":"eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a","start":92,"end":170,"line":4,"column":1}],"nodes":[{"idx":0,"node":"ref","t":"number","target":"score"},{"idx":1,"node":"num","t":"number","value":"7.7"},{"idx":2,"node":"cmp","t":"boolean","op":"ge","left":0,"right":1}],"expr":2},"#;

/// Four participants, every cell present. Scores: p1 7.6, p2 7.8, p3 6,
/// p4 7.8. p2 and p4 tie on the score and p4 wins on judges (9 against 7).
pub(crate) const CSV_FULL: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,7,9\np3,15,true,6,6\np4,30,false,9,6\n";

/// As [`CSV_FULL`] with p4 equal to p2 in every ranking value (judges 7,
/// technical 9), so p2 and p4 are one Tie_Group.
pub(crate) const CSV_TIE: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,7,9\np3,15,true,6,6\np4,30,false,7,9\n";

/// As [`CSV_FULL`] with an empty `judges` cell for p2.
pub(crate) const CSV_MISSING_JUDGES: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,,9\np3,15,true,6,6\np4,30,false,9,6\n";

pub(crate) fn id(name: &str) -> InputId {
    InputId::new(name).unwrap()
}

pub(crate) fn target(name: &str) -> TargetName {
    TargetName::new(name).unwrap()
}

/// `slice-basic` with the Qualification_Predicate `qualified` added.
pub(crate) fn compiled() -> CompiledRulebook {
    let after_basic = r#""expr":4},"#;
    let order = r#""order":["elig.basic","score.weighted","rank.standing"]"#;
    assert_eq!(COMPILED.matches(after_basic).count(), 1);
    assert_eq!(COMPILED.matches(order).count(), 1);
    let text = COMPILED
        .replace(after_basic, &format!("{after_basic}{QUALIFIED_RULE}"))
        .replace(
            order,
            r#""order":["elig.basic","score.weighted","elig.qual","rank.standing"]"#,
        );
    compiled_from(&text)
}

/// The committed `slice-basic` Compiled_Rulebook, unchanged.
pub(crate) fn compiled_basic() -> CompiledRulebook {
    compiled_from(COMPILED)
}

fn compiled_from(text: &str) -> CompiledRulebook {
    parse_compiled(
        text.as_bytes(),
        &id("compiled.json"),
        64 * 1024 * 1024,
        MAX_CONTAINER_DEPTH,
    )
    .unwrap()
}

pub(crate) fn sources() -> SourceModels {
    let model = SourceModel::load(
        &id("source-model.json"),
        MODEL.as_bytes(),
        default_load_limits(),
    )
    .unwrap();
    let mut models = SourceModels::new();
    models.insert(model).unwrap();
    models
}

/// The ranking at argument 2 and the predicate at argument 4.
pub(crate) fn full_request() -> BracketRequest {
    BracketRequest {
        ranking: [(target("standing"), 2)].into(),
        qualification: [(target("qualified"), 4)].into(),
    }
}

pub(crate) fn run_with(
    participants: &str,
    published: Option<&str>,
    request: &BracketRequest,
    sources: &SourceModels,
) -> Result<Evaluated, Rejection> {
    run_on(&compiled(), participants, published, request, sources)
}

pub(crate) fn run_on(
    compiled: &CompiledRulebook,
    participants: &str,
    published: Option<&str>,
    request: &BracketRequest,
    sources: &SourceModels,
) -> Result<Evaluated, Rejection> {
    let (p_id, pub_id) = (id("participants.csv"), id("published.csv"));
    let options = MappingOptions::default();
    crate::evaluate(&BracketInputs {
        compiled,
        request,
        files: FileInputs {
            participants: NamedFile {
                bytes: participants.as_bytes(),
                name: &p_id,
            },
            collections: None,
            published: published.map(|text| NamedFile {
                bytes: text.as_bytes(),
                name: &pub_id,
            }),
            options: &options,
        },
        sources,
        statements: &[],
        synthetic_data: None,
    })
}
