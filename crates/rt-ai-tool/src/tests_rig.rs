//! Shared fixtures of the store and call-loop tests: three clauses of one
//! document, one request each, a replay-free mock provider and a log sink
//! that keeps its bytes.

use crate::tests::{MapEnv, BASE};
use crate::tests_provenance::{doc, model, RULES, RULES_HASH};
use crate::{
    build_plan, load, AiConfig, Consent, PlanInput, Redactor, RunPlan, StoreMeta, Ud32Option,
};
use rt_foundation::{CliLocation, DocumentId, InputId, Location};
use rt_ir::{parse_rulebook, Identity, Rulebook};
use rt_srcmodel::{CandidateClause, SourceModels};
use std::collections::BTreeSet;

pub(crate) const ROOT: &str = "store";

/// Start, end, line and quote of the three clauses of `rules.md`.
pub(crate) const CLAUSES: [(u32, u32, u32, &str); 3] = [
    (0, 11, 1, "Alpha beta."),
    (12, 27, 2, "Gamma δ delta."),
    (28, 32, 3, "End."),
];

pub(crate) fn rulebook() -> Rulebook {
    let text = format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"n","version":"1","documents":[{{"id":"rules.md","sha256":"{RULES_HASH}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[],"precedence":[]}}"#
    );
    parse_rulebook(
        text.as_bytes(),
        &InputId::new("rb.json").unwrap(),
        8 << 20,
        64,
    )
    .map(|s| s.into_parts().0)
    .map_err(|r| r.into_diagnostics().as_slice().to_vec())
    .unwrap()
}

pub(crate) fn config(extra: &str) -> AiConfig {
    let documents: BTreeSet<DocumentId> = [doc("rules.md")].into();
    let at = Location::Cli(CliLocation::Argument {
        position: 3,
        name: Some("--consent".to_owned()),
    });
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", "k")]);
    load(
        &InputId::new("ai.toml").unwrap(),
        &format!("{BASE}max_clauses_per_request = 1\n{extra}"),
        &Consent::named(documents.clone(), at),
        &documents,
        &env,
    )
    .unwrap()
    .config
}

pub(crate) fn models() -> SourceModels {
    let mut all = SourceModels::new();
    all.insert(model("rules.md", RULES)).unwrap();
    all
}

pub(crate) fn plan_of(config: &AiConfig, which: &[usize]) -> RunPlan {
    let all = models();
    let source = model("rules.md", RULES);
    let candidates: Vec<CandidateClause> = which
        .iter()
        .map(|i| {
            let (start, end, _, _) = CLAUSES[*i];
            CandidateClause::new(&source, start, end, None).unwrap()
        })
        .collect();
    build_plan(&PlanInput {
        config,
        models: &all,
        candidates: &candidates,
        rulebook: &rulebook(),
        option: Ud32Option::RankReference,
        candidates_at: Location::Cli(CliLocation::Argument {
            position: 5,
            name: Some("--candidates".to_owned()),
        }),
    })
    .unwrap()
}

pub(crate) fn evidence(index: usize) -> String {
    let (start, end, line, quote) = CLAUSES[index];
    format!(
        r#"{{"document":"rules.md","sha256":"{RULES_HASH}","start":{start},"end":{end},"line":{line},"column":1,"quote":"{quote}"}}"#
    )
}

/// An accepted response with one advisory proposal citing clause `index`.
pub(crate) fn payload(index: usize) -> String {
    format!(
        r#"{{"schema_version":"1","proposals":[{{"kind":"clause_interpretation","proposer":"model-1","rationale":"clause {index}","evidence":[{}]}}]}}"#,
        evidence(index)
    )
}

pub(crate) fn meta() -> StoreMeta {
    StoreMeta {
        provider: "openai-responses".to_owned(),
        model: "model-1".to_owned(),
        adapter_version: "mock-1".to_owned(),
        ai_identities: vec![
            Identity::new("openai-responses").unwrap(),
            Identity::new("model-1").unwrap(),
        ],
        created_at: "2026-11-15T00:00:00Z".to_owned(),
    }
}

/// The inputs of one run. Build once, run many times.
pub(crate) struct Harness {
    pub(crate) config: AiConfig,
    pub(crate) plan: RunPlan,
    pub(crate) models: SourceModels,
    pub(crate) rulebook: Rulebook,
    pub(crate) meta: StoreMeta,
    pub(crate) redactor: Redactor,
}

impl Harness {
    pub(crate) fn new(extra: &str) -> Self {
        let config = config(extra);
        let plan = plan_of(&config, &[0, 1, 2]);
        Self {
            config,
            plan,
            models: models(),
            rulebook: rulebook(),
            meta: meta(),
            redactor: Redactor::no_credential(),
        }
    }

    pub(crate) fn id(&self, request: usize) -> String {
        self.plan.requests()[request].id().as_str().to_owned()
    }

    pub(crate) fn batch_file(&self, request: usize) -> String {
        format!("{ROOT}/batches/{}.json", self.id(request))
    }
}
