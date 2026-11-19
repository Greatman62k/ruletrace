//! Property 5 (other input kinds): contradictory flag input is rejected.
//!
//! The generator of `contradictory_flags.rs` inserts flag and lifecycle keys
//! into one object of each input kind that exists: a Proposal_Store batch,
//! the validation artifact, the compiled artifact, Competition_Data, Questions
//! and Published_Result. Every parser returns exactly one Error per category
//! present (`RT-DGN-0001`, `RT-DGN-0002` or `RT-DGN-0003`) with the first
//! offending key as the primary location, and returns no value. Through the
//! `ruletrace` command line each of these inputs ends in exit code 2 and
//! names the same codes and the first offending key's line and column.
//!
//! The Proposal_Store base batch holds two rule suggestions and one clause
//! interpretation, so insertion targets include nested proposal, evidence,
//! rule draft and outcome objects.
//!
//! The validation and compiled artifacts have no public parser, and the flag
//! scan runs before decoding, so their bases are minimal objects with the
//! members of the real format rather than complete artifacts.
//!
//! **Validates: Requirements 22.16, 22.17, 22.22, 4.10**

#![allow(clippy::expect_used, clippy::indexing_slicing)]

#[path = "support/flags.rs"]
mod flags;

#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;
use flags::{apply, check, expected, insertions, objects, Edited};
use proptest::prelude::*;
use proptest_floor::property_tests;
use rt_cli::{run, Fs};
use rt_data::{default_parse_limits, parse_data, parse_published, parse_questions, DataLimits};
use rt_foundation::{Code, DiagnosticState, DocumentId, ExitCode, InputId, RuleId};
use rt_hash::{sha256, Sha256Hex};
use rt_ir::Identity;
use rt_proposals::{
    parse_batch, print_batch, proposal_id, Batch, Completion, Evidence, Format, Proposal,
    ProposalKind, RuleDraft, StoredDiagnostic, StoredLocation, StoredResponse,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

const DATA: &str = r#"{"format":"ruletrace-data","version":1,"participants":[{"id":"p1","fields":{"age":"17"},"collections":{"results":[{"place":"1"}]}},{"id":"p2","fields":{"age":"20"},"collections":{}}]}"#;
const QUESTIONS: &str = r#"{"format":"ruletrace-questions","version":1,"questions":[{"kind":"value","target":"a","participant":"p1"},{"kind":"rank","target":"a","participants":null}]}"#;
const PUBLISHED: &str = r#"{"format":"ruletrace-published","version":1,"entries":[{"participant":"p1","target":"a","value":"1"},{"participant":"p2","target":"a","value":true}]}"#;
const VALIDATION: &str = r#"{"format":"ruletrace-validation","rulebook":{"name":"t","documents":[{"id":"rules.md"}]},"ledger":{"lines":[]},"diagnostics":[]}"#;
const COMPILED: &str = r#"{"format":"ruletrace-compiled","compiler_version":"0.1.0","ir_version":"1","documents":[],"inputs":[],"collections":[],"rules":[{"id":"d.a","spans":[]}],"order":[]}"#;

fn proposal(request: &Sha256Hex, index: usize, kind: ProposalKind) -> Proposal {
    let rule = (kind == ProposalKind::RuleSuggestion).then(|| {
        let id = format!("elig.p{index}");
        let draft = RuleDraft {
            id: RuleId::new(id.clone()).expect("rule id"),
            json: format!(r#"{{"id":"{id}","kind":"eligibility","spans":[]}}"#),
        };
        (draft, sha256(format!("draft{index}").as_bytes()))
    });
    let document = DocumentId::new("rules.md").expect("document id");
    Proposal {
        proposal_id: proposal_id(request, index),
        kind,
        proposer: Identity::new("ai.bot").expect("identity"),
        rationale: "Clause 3 sets a minimum age.".to_owned(),
        evidence: vec![Evidence {
            document: document.clone(),
            sha256: sha256(b"rules document"),
            start: 21,
            end: 91,
            line: 3,
            column: 1,
            quote: "Entrants must be 16 or older".to_owned(),
        }],
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
                    document,
                    start: 1,
                    end: 2,
                },
            },
        ],
    }
}

/// A batch of two rule suggestions and one clause interpretation, so that
/// flag keys are inserted into proposal, evidence, rule draft and outcome
/// objects as well as the batch itself.
fn batch() -> Batch {
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
        ai_identities: vec![Identity::new("ai.bot").expect("identity")],
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        proposals: vec![
            proposal(&request_id, 0, ProposalKind::RuleSuggestion),
            proposal(&request_id, 1, ProposalKind::ClauseInterpretation),
            proposal(&request_id, 2, ProposalKind::RuleSuggestion),
        ],
    }
}

fn batch_text() -> String {
    print_batch(&batch())
}

fn input(name: &str) -> InputId {
    InputId::new(name).expect("input id")
}

fn codes_of(state: &DiagnosticState) -> Vec<rt_foundation::Diagnostic> {
    state.as_slice().to_vec()
}

/// An in-memory read-only file system.
struct Files(BTreeMap<PathBuf, Vec<u8>>);

impl Files {
    fn new(files: &[(&str, &str)]) -> Self {
        Self(
            files
                .iter()
                .map(|(p, t)| (PathBuf::from(p), t.as_bytes().to_vec()))
                .collect(),
        )
    }
}

impl Fs for Files {
    fn len(&self, path: &Path) -> io::Result<u64> {
        let bytes = self.0.get(path).ok_or(io::ErrorKind::NotFound)?;
        Ok(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
    }

    fn read(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        let bytes = self.0.get(path).ok_or(io::ErrorKind::NotFound)?;
        Ok(bytes.iter().take(max_bytes as usize).copied().collect())
    }

    fn create_new(&self, _: &Path, _: &[u8]) -> io::Result<()> {
        Err(io::Error::other("read-only"))
    }

    fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
        Err(io::Error::other("read-only"))
    }

    fn remove(&self, _: &Path) -> io::Result<()> {
        Err(io::Error::other("read-only"))
    }
}

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

/// Runs the command line and asserts exit code 2, every expected code and the
/// line and column of the first offending key of each category.
fn check_cli(
    args: &[&str],
    files: &Files,
    name: &str,
    edited: &Edited,
) -> Result<(), TestCaseError> {
    let outcome = run(&os(args), files);
    prop_assert_eq!(outcome.code, ExitCode::InvalidInput, "{}", outcome.stderr);
    for (code, keys) in expected(edited) {
        prop_assert_eq!(code.exit_code(), ExitCode::InvalidInput);
        prop_assert!(outcome.stderr.contains(code.id()), "{}", outcome.stderr);
        let at = edited.text.find(&format!("\"{}\":", keys[0])).expect("key");
        let place = format!("input '{name}' line 1 column {}", at + 1);
        prop_assert!(
            outcome.stderr.contains(&place),
            "{place}: {}",
            outcome.stderr
        );
    }
    Ok(())
}

fn edit(base: &str, ins: &[flags::Insertion], which: usize) -> Edited {
    let objs = objects(base);
    apply(base, &objs[which % objs.len()], ins)
}

property_tests! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn proposal_batch_is_rejected(ins in insertions(), which in any::<usize>()) {
        let edited = edit(&batch_text(), &ins, which);
        match parse_batch(&input("batch.json"), edited.text.as_bytes()) {
            Ok(_) => prop_assert!(false, "batch accepted: {}", edited.text),
            Err(diags) => check(&diags, &edited, &edited.text, Some(1))?,
        }
    }

    #[test]
    fn competition_data_is_rejected(ins in insertions(), which in any::<usize>()) {
        let edited = edit(DATA, &ins, which);
        let result = parse_data(
            &input("data.json"),
            edited.text.as_bytes(),
            DataLimits::default(),
        );
        match result {
            Ok(_) => prop_assert!(false, "data accepted: {}", edited.text),
            Err(state) => check(&codes_of(&state), &edited, &edited.text, Some(1))?,
        }
    }

    #[test]
    fn questions_are_rejected(ins in insertions(), which in any::<usize>()) {
        let edited = edit(QUESTIONS, &ins, which);
        let result = parse_questions(
            &input("questions.json"),
            edited.text.as_bytes(),
            default_parse_limits(),
        );
        match result {
            Ok(_) => prop_assert!(false, "questions accepted: {}", edited.text),
            Err(state) => check(&codes_of(&state), &edited, &edited.text, Some(1))?,
        }
    }

    #[test]
    fn published_result_is_rejected(ins in insertions(), which in any::<usize>()) {
        let edited = edit(PUBLISHED, &ins, which);
        let result = parse_published(
            &input("published.json"),
            edited.text.as_bytes(),
            default_parse_limits(),
        );
        match result {
            Ok(_) => prop_assert!(false, "published accepted: {}", edited.text),
            Err(state) => check(&codes_of(&state), &edited, &edited.text, Some(1))?,
        }
    }

    #[test]
    fn compiled_artifact_is_rejected(ins in insertions(), which in any::<usize>()) {
        let edited = edit(COMPILED, &ins, which);
        let result = rt_compiled::parse_compiled(
            edited.text.as_bytes(),
            &input("compiled.json"),
            8 << 20,
            64,
        );
        match result {
            Ok(_) => prop_assert!(false, "compiled accepted: {}", edited.text),
            Err(state) => check(&codes_of(&state), &edited, &edited.text, Some(1))?,
        }
    }

    #[test]
    fn cli_exits_2_for_every_input_kind(
        ins in insertions(),
        which in any::<usize>(),
        kind in 0usize..5,
    ) {
        let compiled = "compiled.json";
        let data = "data.json";
        let eval = |extra: &[&'static str]| {
            let mut args = vec!["evaluate", "--compiled", compiled, "--data", data, "--out", "r.json"];
            args.extend_from_slice(extra);
            args
        };
        let (name, base, args, others): (&str, &str, Vec<&str>, Vec<(&str, String)>) = match kind {
            0 => (
                "validation.json",
                VALIDATION,
                vec!["compile", "--validation", "validation.json", "--out", "c.json"],
                vec![],
            ),
            1 => (compiled, COMPILED, eval(&[]), vec![(data, DATA.to_owned())]),
            2 => (
                data,
                DATA,
                eval(&[]),
                vec![(compiled, COMPILED.to_owned())],
            ),
            3 => (
                "questions.json",
                QUESTIONS,
                eval(&["--questions", "questions.json"]),
                vec![(data, DATA.to_owned()), (compiled, COMPILED.to_owned())],
            ),
            _ => (
                "published.json",
                PUBLISHED,
                eval(&["--published", "published.json"]),
                vec![(data, DATA.to_owned()), (compiled, COMPILED.to_owned())],
            ),
        };
        let edited = edit(base, &ins, which);
        let mut list: Vec<(&str, &str)> = others.iter().map(|(p, t)| (*p, t.as_str())).collect();
        list.push((name, edited.text.as_str()));
        check_cli(&args, &Files::new(&list), name, &edited)?;
    }
}

#[test]
fn unmodified_inputs_are_accepted() {
    let id = input("x.json");
    assert!(parse_batch(&id, batch_text().as_bytes()).is_ok());
    assert!(parse_data(&id, DATA.as_bytes(), DataLimits::default()).is_ok());
    assert!(parse_questions(&id, QUESTIONS.as_bytes(), default_parse_limits()).is_ok());
    assert!(parse_published(&id, PUBLISHED.as_bytes(), default_parse_limits()).is_ok());
}

#[test]
fn the_batch_base_has_nested_rule_proposals_and_many_insertion_targets() {
    let parsed = parse_batch(&input("x.json"), batch_text().as_bytes()).expect("batch");
    assert_eq!(parsed, batch());
    let with_rule = parsed.proposals.iter().filter(|p| p.rule.is_some()).count();
    assert_eq!(with_rule, 2);
    // Batch, response, three proposals, three evidence, two rule drafts and
    // six outcome diagnostics, with the two source locations.
    assert!(objects(&batch_text()).len() >= 17);
}

#[test]
fn a_flag_key_inside_a_rule_draft_names_its_nested_field() {
    let text = batch_text().replacen(
        r#""kind":"eligibility""#,
        r#""has_errors":false,"kind":"eligibility""#,
        1,
    );
    let diags = parse_batch(&input("batch.json"), text.as_bytes()).expect_err("rejected");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code(), Code::DiagnosticStateContradiction);
    assert!(
        format!("{:?}", diags[0].primary()).contains("has_errors"),
        "{:?}",
        diags[0].primary()
    );
}
