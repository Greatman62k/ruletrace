//! Shared fixtures of the unit tests: the committed `slice-basic` Compiled_Rulebook
//! and Source_Model (read at build time, so the library tests open no file),
//! and hand-written CSV inputs.
//!
//! Rules of `slice-basic`, read from its Compiled_Rulebook: `eligible` is
//! `age >= 16 and licence`; `score` is `0.6 * judges + 0.4 * technical`;
//! `standing` ranks `score` descending, then `judges` descending, tied
//! participants forming a Tie_Group.

use crate::{evaluate, Evaluated, FileInputs, NamedFile, PrizeInputs, PrizeRequest, Rejection};
use rt_compiled::{parse_compiled, CompiledRulebook};
use rt_data::tabular::MappingOptions;
use rt_data::TargetName;
use rt_foundation::{InputId, MAX_CONTAINER_DEPTH};
use rt_srcmodel::{default_load_limits, SourceModel, SourceModels};

const COMPILED: &str = include_str!("../../../tests/fixtures/slice-basic/expected/compiled.json");
const MODEL: &str = include_str!("../../../tests/fixtures/slice-basic/expected/source-model.json");

/// Four participants, every cell present.
pub(crate) const CSV_FULL: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,20,true,7,9\np3,15,true,6,6\np4,30,false,9,6\n";

/// As [`CSV_FULL`] with an empty `age` cell for p2.
pub(crate) const CSV_MISSING_AGE: &str = "participant,age,licence,judges,technical\np1,17,true,8,7\np2,,true,7,9\np3,15,true,6,6\np4,30,false,9,6\n";

pub(crate) fn id(name: &str) -> InputId {
    InputId::new(name).unwrap()
}

pub(crate) fn target(name: &str) -> TargetName {
    TargetName::new(name).unwrap()
}

pub(crate) fn compiled() -> CompiledRulebook {
    compiled_from(COMPILED)
}

/// `slice-basic` with `score` redefined as a derived value equal to `judges`,
/// so a derived Target is available.
pub(crate) fn compiled_with_derived_score() -> CompiledRulebook {
    let weighted_nodes = r#""nodes":[{"idx":0,"node":"num","t":"number","value":"0.6"},{"idx":1,"node":"field","t":"number","input":2},{"idx":2,"node":"num","t":"number","value":"0.4"},{"idx":3,"node":"field","t":"number","input":3}],"terms":[{"weight":0,"value":1},{"weight":2,"value":3}],"rounding":null"#;
    let derived_nodes = r#""nodes":[{"idx":0,"node":"field","t":"number","input":2}],"expr":0"#;
    assert_eq!(COMPILED.matches(weighted_nodes).count(), 1);
    assert_eq!(COMPILED.matches(r#""kind":"weighted_score""#).count(), 1);
    let text = COMPILED
        .replace(weighted_nodes, derived_nodes)
        .replace(r#""kind":"weighted_score""#, r#""kind":"derived""#);
    compiled_from(&text)
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

/// All three Targets, at arguments 2, 4 and 6.
pub(crate) fn full_request() -> PrizeRequest {
    PrizeRequest {
        eligibility: [(target("eligible"), 2)].into(),
        score: [(target("score"), 4)].into(),
        ranking: [(target("standing"), 6)].into(),
    }
}

pub(crate) fn run_with(
    participants: &str,
    published: Option<&str>,
    request: &PrizeRequest,
    sources: &SourceModels,
) -> Result<Evaluated, Rejection> {
    run_on(&compiled(), participants, published, request, sources)
}

pub(crate) fn run_on(
    compiled: &CompiledRulebook,
    participants: &str,
    published: Option<&str>,
    request: &PrizeRequest,
    sources: &SourceModels,
) -> Result<Evaluated, Rejection> {
    let (p_id, pub_id) = (id("participants.csv"), id("published.csv"));
    let options = MappingOptions::default();
    evaluate(&PrizeInputs {
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
