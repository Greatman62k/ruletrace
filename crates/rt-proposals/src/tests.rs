use super::*;
use rt_foundation::{Code, DocumentId, InputId, RuleId};
use rt_hash::{sha256, Sha256Hex};
use rt_ir::{parse_rulebook, Identity, Rulebook};

const DOC_HASH: &str = "eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a";

fn input() -> InputId {
    InputId::new("batches/x.json").unwrap()
}

fn evidence() -> Evidence {
    Evidence {
        document: DocumentId::new("rules.md").unwrap(),
        sha256: Sha256Hex::parse(DOC_HASH).unwrap(),
        start: 21,
        end: 91,
        line: 3,
        column: 1,
        quote: "Entrants must be \"16\" or older\n".to_owned(),
    }
}

fn proposal(request: &Sha256Hex, index: usize, kind: ProposalKind) -> Proposal {
    let rule = (kind == ProposalKind::RuleSuggestion).then(|| {
        let json = format!(r#"{{"id":"elig.p{index}","kind":"eligibility","spans":[]}}"#);
        let draft = RuleDraft {
            id: RuleId::new(format!("elig.p{index}")).unwrap(),
            json,
        };
        (draft, sha256(format!("draft{index}").as_bytes()))
    });
    Proposal {
        proposal_id: proposal_id(request, index),
        kind,
        proposer: Identity::new("ai.bot").unwrap(),
        rationale: "Clause 3 sets a minimum age.".to_owned(),
        evidence: vec![evidence()],
        rule,
        outcome: vec![
            StoredDiagnostic {
                code: Code::AiStoreInvalid,
                message: "example".to_owned(),
                location: StoredLocation::Field("rule.spans[0]".to_owned()),
            },
            StoredDiagnostic {
                code: Code::AiStoreInvalid,
                message: "example".to_owned(),
                location: StoredLocation::Source {
                    document: DocumentId::new("rules.md").unwrap(),
                    start: 1,
                    end: 2,
                },
            },
        ],
    }
}

pub(crate) fn batch() -> Batch {
    let request_id = sha256(b"request");
    let payload = "{\"schema_version\":\"1\",\"proposals\":[]}";
    Batch {
        format: Format::V1,
        request_id: request_id.clone(),
        response_sha256: sha256(payload.as_bytes()),
        response: StoredResponse {
            completion: Completion::Complete,
            payload: payload.to_owned(),
        },
        provider: "mock".to_owned(),
        model: "m1".to_owned(),
        adapter_version: "0.1".to_owned(),
        ai_identities: vec![Identity::new("ai.bot").unwrap()],
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        proposals: vec![
            proposal(&request_id, 0, ProposalKind::RuleSuggestion),
            proposal(&request_id, 1, ProposalKind::ClauseInterpretation),
            proposal(&request_id, 2, ProposalKind::RuleSuggestion),
        ],
    }
}

fn codes(result: Result<Batch, Vec<rt_foundation::Diagnostic>>) -> Vec<Code> {
    result.unwrap_err().iter().map(|d| d.code()).collect()
}

#[test]
fn a_batch_round_trips_byte_identically() {
    let text = print_batch(&batch());
    let parsed = parse_batch(&input(), text.as_bytes()).unwrap();
    assert_eq!(parsed, batch());
    assert_eq!(print_batch(&parsed), text);
    assert!(text.ends_with("}\n"));
}

#[test]
fn a_flag_key_is_rejected_with_the_dgn_code() {
    let text = print_batch(&batch()).replacen('{', r#"{"has_errors":false,"#, 1);
    assert_eq!(
        codes(parse_batch(&input(), text.as_bytes())),
        vec![Code::DiagnosticStateContradiction]
    );
}

#[test]
fn a_flag_key_inside_a_proposal_is_rejected() {
    let text =
        print_batch(&batch()).replacen(r#""rationale""#, r#""no_errors":true,"rationale""#, 1);
    assert_eq!(
        codes(parse_batch(&input(), text.as_bytes())),
        vec![Code::DiagnosticStateContradiction]
    );
}

#[test]
fn a_lifecycle_key_is_rejected_with_the_dgn_code() {
    let text = print_batch(&batch()).replacen('{', r#"{"approval_status":"approved","#, 1);
    assert_eq!(
        codes(parse_batch(&input(), text.as_bytes())),
        vec![Code::LifecycleFlagContradiction]
    );
}

#[test]
fn transport_members_cannot_be_stored() {
    let text = print_batch(&batch()).replacen('{', r#"{"authorization":"Bearer x","#, 1);
    assert_eq!(
        codes(parse_batch(&input(), text.as_bytes())),
        vec![Code::AiStoreInvalid]
    );
}

#[test]
fn a_wrong_proposal_id_payload_hash_or_form_is_invalid() {
    let good = print_batch(&batch());
    let id = proposal_id(&batch().request_id, 0);
    let other = sha256(b"other");
    let bad_id = good.replacen(id.as_str(), other.as_str(), 1);
    let bad_payload = good.replace("schema_version", "schema_versio");
    let spaced = good.replacen(r#""format":"#, r#""format": "#, 1);
    for text in [bad_id, bad_payload, spaced] {
        assert_eq!(
            codes(parse_batch(&input(), text.as_bytes())),
            vec![Code::AiStoreInvalid]
        );
    }
}

#[test]
fn a_rule_exists_exactly_for_a_rule_suggestion() {
    let mut b = batch();
    b.proposals[1].rule = b.proposals[0].rule.clone();
    let text = print_batch(&b);
    assert_eq!(
        codes(parse_batch(&input(), text.as_bytes())),
        vec![Code::AiStoreInvalid]
    );
}

fn rulebook_adopting(proposal_id: &str) -> Rulebook {
    let text = format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"n","version":"1","documents":[{{"id":"rules.md","sha256":"{DOC_HASH}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{{"id":"elig.p0","revision":1,"origin":{{"kind":"ai_proposal","proposer":"ai.bot","provider":"mock","model":"m1","adapter_version":"0.1","request_hash":"{DOC_HASH}","response_hash":"{DOC_HASH}","proposal_id":"{proposal_id}","ai_identities":["ai.bot"]}},"kind":"eligibility","target":"eligible","type":"boolean","expr":{{"node":"bool","t":"boolean","value":true}},"markers":[],"spans":[{{"document":"rules.md","start":1,"end":2,"line":1,"column":1}}]}}],"precedence":[]}}"#
    );
    let id = InputId::new("rulebook.json").unwrap();
    let parsed = parse_rulebook(text.as_bytes(), &id, 8 << 20, 64);
    parsed.map_err(|_| "rulebook").unwrap().into_parts().0
}

#[test]
fn pending_proposals_are_unadopted_rule_suggestions() {
    let b = batch();
    let adopted = b.proposals[0].proposal_id.as_str().to_owned();
    let batches = [b.clone()];
    let none = rulebook_adopting("unrelated");
    let ids = |rb: &Rulebook| -> Vec<usize> {
        pending_proposals(&batches, rb)
            .iter()
            .map(|p| p.index)
            .collect()
    };
    assert_eq!(ids(&none), vec![0, 2]);
    assert_eq!(ids(&rulebook_adopting(&adopted)), vec![2]);
}

// The limit is 16 MiB, 16777216 bytes, written out and not read from the
// constant under test.
const LIMIT_BYTES: usize = 16_777_216;

#[test]
fn bytes_above_the_limit_are_rt_lim_0001_with_the_length_given() {
    for len in [LIMIT_BYTES + 1, 3 * LIMIT_BYTES + 5] {
        let diags = parse_batch(&input(), &vec![b'x'; len]).unwrap_err();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code(), Code::LimitFileTooLarge);
        assert_eq!(
            diags[0].message(),
            format!(
                "input file 'batches/x.json' is {len} bytes, larger than the effective \
                 limit of 16777216 bytes"
            )
        );
    }
}

#[test]
fn bytes_of_exactly_the_limit_are_parsed_and_not_a_size_fault() {
    let diags = parse_batch(&input(), &vec![b'x'; LIMIT_BYTES]).unwrap_err();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code(), Code::AiStoreInvalid);
    assert!(!diags[0].message().contains("larger than"));
}
