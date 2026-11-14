//! Loads a fixture's artifacts the way `ruletrace evaluate` does.

use rt_compiled::{parse_compiled, CompiledBody, CompiledRulebook};
use rt_data::{default_questions, parse_data, CompetitionData, DataLimits, Question, TargetName};
use rt_foundation::{InputId, MAX_CONTAINER_DEPTH};
use rt_srcmodel::{default_load_limits, SourceModel, SourceModels};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// The fixtures that compile and evaluate.
pub const EXECUTABLE: [&str; 3] = ["slice-basic", "slice-missing", "slice-tie"];

pub struct Fixture {
    pub name: &'static str,
    pub compiled: CompiledRulebook,
    pub data: CompetitionData,
    pub questions: Vec<Question>,
    pub sources: SourceModels,
}

fn root(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

pub fn load(name: &'static str) -> Fixture {
    let dir = root(name);
    let read = |p: &str| std::fs::read(dir.join(p)).unwrap();
    let input = |p: &str| InputId::new(format!("{name}/{p}")).unwrap();
    let compiled_bytes = read("expected/compiled.json");
    let compiled = parse_compiled(
        &compiled_bytes,
        &input("expected/compiled.json"),
        64 * 1024 * 1024,
        MAX_CONTAINER_DEPTH,
    )
    .unwrap();
    let data = parse_data(
        &input("data.json"),
        &read("data.json"),
        DataLimits::default(),
    )
    .unwrap();
    let model = SourceModel::load(
        &input("expected/source-model.json"),
        &read("expected/source-model.json"),
        default_load_limits(),
    )
    .unwrap();
    let mut sources = SourceModels::new();
    sources.insert(model).unwrap();
    let (mut values, mut rankings) = (BTreeSet::new(), BTreeSet::new());
    for rule in compiled.rules() {
        let target = TargetName::new(rule.target.as_str()).unwrap();
        match rule.body {
            CompiledBody::Ranking { .. } => rankings.insert(target),
            CompiledBody::Default { .. } => false,
            _ => values.insert(target),
        };
    }
    let questions = default_questions(&data, &values, &rankings);
    Fixture {
        name,
        compiled,
        data,
        questions,
        sources,
    }
}
