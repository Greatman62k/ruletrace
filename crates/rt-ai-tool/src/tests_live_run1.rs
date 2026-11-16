//! The first live batch (prompt-2) pinned as evidence (task 10.8 follow-up).
//!
//! The fixture is the stored batch of the live call. Today's acceptance
//! path, run over the stored response payload with the same document,
//! clauses and declarations, must reproduce every stored proposal, so the
//! original model errors stay on record and the prompt-3 tests are shown to
//! catch them. The expected findings below are written by hand from the
//! stored batch and the validator rules, not read from the output under test.

use crate::accept::{accept_batch, AcceptInput};
use crate::plan::ClauseId;
use crate::redact::Redactor;
use crate::tests_provenance::{doc, model};
use rt_ai_adapter::Completion;
use rt_foundation::InputId;
use rt_ir::parse_rulebook;
use rt_proposals::{parse_batch, Batch};
use rt_srcmodel::SourceModels;

const BATCH: &[u8] = include_bytes!("../tests/fixtures/live-run1-prompt-2/batch.json");
const DOCUMENT: &str = include_str!("../../../demo/stand-in-ai/documents/stand-in-rules.md");
const SEED: &[u8] = include_bytes!("../../../demo/stand-in-ai/rulebook/seed.json");

// The five clauses of the live request, from its candidates file.
const CLAUSES: [(u32, u32); 5] = [(161, 241), (242, 320), (321, 426), (427, 500), (501, 546)];

fn stored() -> Batch {
    parse_batch(&InputId::new("batch.json").unwrap(), BATCH).unwrap()
}

fn rerun(batch: &Batch) -> crate::accept::AcceptedBatch {
    let mut models = SourceModels::new();
    models.insert(model("stand-in-rules.md", DOCUMENT)).unwrap();
    let declarations = parse_rulebook(SEED, &InputId::new("seed.json").unwrap(), 8 << 20, 64)
        .map(|s| s.into_parts().0)
        .unwrap();
    let clauses: Vec<ClauseId> = CLAUSES
        .iter()
        .map(|(start, end)| ClauseId {
            document: doc("stand-in-rules.md"),
            start: *start,
            end: *end,
        })
        .collect();
    accept_batch(&AcceptInput {
        response_id: &InputId::new("response-live").unwrap(),
        request_id: &batch.request_id,
        completion: Completion::Complete,
        payload: batch.response.payload.as_bytes(),
        response_limit: 1 << 20,
        models: &models,
        clauses: &clauses,
        declarations: &declarations,
        redactor: &Redactor::no_credential(),
        revise_rule: None,
    })
    .unwrap_or_else(|d| panic!("refused: {d:?}"))
}

fn findings(proposal: &rt_proposals::Proposal) -> Vec<(String, String)> {
    proposal
        .outcome
        .iter()
        .map(|d| (d.code.id().to_owned(), d.message.clone()))
        .collect()
}

fn pair(code: &str, message: &str) -> (String, String) {
    (code.to_owned(), message.to_owned())
}

#[test]
fn the_fixture_is_the_prompt_2_live_batch_as_recorded() {
    let batch = stored();
    assert_eq!(
        batch.request_id.as_str(),
        "1c574c2d1e2658ba060f68b788c2407f9aa712efd697e23c997d8c8c653b54b2"
    );
    assert_eq!(batch.adapter_version, "responses-1/prompt-2");
    assert_eq!(batch.model, "muse-spark-1.3");
    assert_eq!(batch.provider, "meta");
    assert_eq!(batch.proposals.len(), 5);
    assert_eq!(
        rt_hash::sha256(DOCUMENT.as_bytes()).as_str(),
        "cad9f3cccea2c7470cf25aad74c646ca2412ba48e3eaf66e7ee81aed038b2cce"
    );
}

#[test]
fn the_original_findings_of_the_live_batch_are_as_recorded() {
    let batch = stored();
    let want: [Vec<(String, String)>; 5] = [
        Vec::new(),
        Vec::new(),
        vec![
            pair(
                "RT-VAL-0001",
                "type mismatch at rule type: expected ranking, found number",
            ),
            pair("RT-VAL-0002", "undeclared field 'score' at key"),
        ],
        vec![pair(
            "RT-VAL-0003",
            "unsupported rule kind 'unrecognized': unrecognized construct 'unrecognized'",
        )],
        vec![
            pair(
                "RT-VAL-0001",
                "type mismatch at expr: expected number, found boolean",
            ),
            pair(
                "RT-VAL-0001",
                "type mismatch at rule type: expected number, found boolean",
            ),
        ],
    ];
    for (i, expected) in want.iter().enumerate() {
        assert_eq!(&findings(&batch.proposals[i]), expected, "proposal {i}");
    }
}

#[test]
fn todays_validator_reproduces_every_stored_proposal_exactly() {
    let batch = stored();
    let again = rerun(&batch);
    assert_eq!(again.proposals().len(), batch.proposals.len());
    for (i, (got, was)) in again.proposals().iter().zip(&batch.proposals).enumerate() {
        assert_eq!(got.proposal_id, was.proposal_id, "proposal {i}");
        assert_eq!(got.rule, was.rule, "proposal {i}: draft and hash");
        assert_eq!(findings(got), findings(was), "proposal {i}: findings");
        assert_eq!(got, was, "proposal {i}");
    }
}

// What the evidence shows about the kinds. The model declared rule 3 as
// kind ranking with type number, rule 5 as kind derived with a boolean
// expression, and rule 4 as unrecognized, which is the intended outcome.
#[test]
fn the_declared_kinds_and_types_of_the_live_rules_are_as_recorded() {
    let batch = stored();
    let shape: Vec<(String, String)> = batch
        .proposals
        .iter()
        .map(|p| {
            let json = &p.rule.as_ref().unwrap().0.json;
            let field = |name: &str| {
                let at = json.find(&format!("\"{name}\":\"")).unwrap() + name.len() + 4;
                json[at..].split('"').next().unwrap().to_owned()
            };
            (field("kind"), field("type"))
        })
        .collect();
    let want = [
        ("eligibility", "boolean"),
        ("derived", "number"),
        ("ranking", "number"),
        ("unrecognized", "number"),
        ("derived", "boolean"),
    ];
    let want: Vec<(String, String)> = want
        .iter()
        .map(|(k, t)| ((*k).to_owned(), (*t).to_owned()))
        .collect();
    assert_eq!(shape, want);
}
