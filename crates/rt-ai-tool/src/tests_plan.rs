//! Request building, Request_Identifier and `--dry-run` tests (Req 23.2,
//! 23.3, 23.24, 23.32, 28.14, 29.7, 29.16). Expected request bytes are
//! written by hand, and their SHA-256 values come from the external
//! `sha256sum` tool, not from the code under test.

use crate::tests::{MapEnv, BASE};
use crate::{
    build_plan, dry_run, load, AiConfig, Consent, Out, PlanInput, RunPlan, Ud32Option,
    MAX_CLAUSE_BYTES, MAX_REQUEST_BYTES,
};
use rt_foundation::{
    CliLocation, Code, Diagnostic, DocumentId, ExitCode, InputId, Location, Severity,
};
use rt_ir::{parse_rulebook, Rulebook};
use rt_srcmodel::{CandidateClause, DocumentKind, Element, ElementKind, SourceModel, SourceModels};
use std::collections::BTreeSet;

const RULES_TEXT: &str = "1. Age 16.\n2. Fee 20.\n";
// sha256sum of RULES_TEXT.
const RULES_HASH: &str = "55887559197c793ed287815f2b33d45f454d0a045b92edf84d7d60ba82f1d29b";
// sha256sum of the expected bytes with each vocabulary.
const ID_RANK: &str = "caa07c4e3a8015480edcb28b348329db8acef723c12e144fca7a417e6027d952";
const ID_THRESHOLD: &str = "354981454504a7648334cde6dfc516cbe143ea114c51cc9bbc740365f079e38d";

pub(crate) const HEAD: &str = r#"{"v":1,"task":"propose_rules","model":"model-1","limits":{"deadline_s":60,"attempt_s":20,"retries":2,"response_bytes":1048576},"clauses":["#;
const DECLS: &str = r#","declarations":{"ir_version":"1","inputs":[{"name":"age","type":"number"}],"enums":[],"sets":[],"collections":[],"highest_revision":{}},"#;
const TAIL: &str = r#","revise":null}"#;
pub(crate) const VOCAB_RANK: &str = r#""vocabulary":{"rule_kinds":["eligibility","derived","weighted_score","ranking","default"],"operations":["add","sub","mul","div","eq","ne","lt","le","gt","ge","and","or","not","in","field","ref","sum","min","max","count","round","rank_reference"],"types":["number","boolean","text","date","enum"],"rounding_modes":["half_even","half_away_from_zero","toward_zero"],"unsupported":[{"name":"bracket_generation","capability":"bracket generation"},{"name":"prize_allocation","capability":"prize allocation"},{"name":"progression","capability":"tournament progression"},{"name":"seeding","capability":"seeding"},{"name":"slot_allocation","capability":"slot-based qualification"}]}"#;
const VOCAB_THRESHOLD: &str = r#""vocabulary":{"rule_kinds":["eligibility","derived","weighted_score","ranking","default"],"operations":["add","sub","mul","div","eq","ne","lt","le","gt","ge","and","or","not","in","field","ref","sum","min","max","count","round"],"types":["number","boolean","text","date","enum"],"rounding_modes":["half_even","half_away_from_zero","toward_zero"],"unsupported":[{"name":"bracket_generation","capability":"bracket generation"},{"name":"prize_allocation","capability":"prize allocation"},{"name":"progression","capability":"tournament progression"},{"name":"rank_reference","capability":"rank reference"},{"name":"seeding","capability":"seeding"},{"name":"slot_allocation","capability":"slot-based qualification"}]}"#;

fn clauses_text() -> String {
    format!(
        r#"{{"document":"rules.md","sha256":"{RULES_HASH}","start":0,"end":10,"line":1,"column":1,"text":"1. Age 16."}},{{"document":"rules.md","sha256":"{RULES_HASH}","start":11,"end":21,"line":2,"column":1,"text":"2. Fee 20."}}]"#
    )
}

fn expected(vocab: &str) -> String {
    format!("{HEAD}{}{DECLS}{vocab}{TAIL}", clauses_text())
}

fn doc(name: &str) -> DocumentId {
    DocumentId::new(name).unwrap()
}

fn element(end: u32) -> Element {
    Element {
        id: 0,
        kind: ElementKind::Paragraph,
        level: None,
        label: None,
        start: 0,
        end,
        line: 1,
        column: 1,
        parent: None,
    }
}

fn model(name: &str, text: &str) -> SourceModel {
    let end = u32::try_from(text.len()).unwrap();
    SourceModel::new(
        doc(name),
        DocumentKind::Text,
        text.to_owned(),
        vec![element(end)],
    )
    .unwrap()
}

fn models(list: &[&SourceModel]) -> SourceModels {
    let mut all = SourceModels::new();
    for m in list {
        all.insert((*m).clone()).unwrap();
    }
    all
}

fn clause(m: &SourceModel, start: u32, end: u32) -> CandidateClause {
    CandidateClause::new(m, start, end, None).unwrap()
}

fn config_with(extra: &str) -> AiConfig {
    let text = format!("{BASE}{extra}");
    let documents: BTreeSet<DocumentId> = [doc("rules.md")].into();
    let at = Location::Cli(CliLocation::Argument {
        position: 3,
        name: Some("--consent".to_owned()),
    });
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", "k")]);
    load(
        &InputId::new("ai.toml").unwrap(),
        &text,
        &Consent::named(documents.clone(), at),
        &documents,
        &env,
    )
    .unwrap()
    .config
}

fn rulebook(rules: &str) -> Rulebook {
    let text = format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"n","version":"1","documents":[{{"id":"rules.md","sha256":"{RULES_HASH}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{rules}],"precedence":[]}}"#
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

fn candidates_at() -> Location {
    Location::Cli(CliLocation::Argument {
        position: 5,
        name: Some("--candidates".to_owned()),
    })
}

fn plan_for(
    config: &AiConfig,
    all: &SourceModels,
    candidates: &[CandidateClause],
    rb: &Rulebook,
    option: Ud32Option,
) -> Result<RunPlan, Vec<Diagnostic>> {
    build_plan(&PlanInput {
        config,
        models: all,
        candidates,
        rulebook: rb,
        option,
        revise: None,
        candidates_at: candidates_at(),
    })
}

struct Rig {
    config: AiConfig,
    all: SourceModels,
    candidates: Vec<CandidateClause>,
    rb: Rulebook,
}

fn rig() -> Rig {
    let m = model("rules.md", RULES_TEXT);
    Rig {
        config: config_with(""),
        all: models(&[&m]),
        candidates: vec![clause(&m, 0, 10), clause(&m, 11, 21)],
        rb: rulebook(""),
    }
}

fn plan(r: &Rig, option: Ud32Option) -> RunPlan {
    plan_for(&r.config, &r.all, &r.candidates, &r.rb, option).unwrap()
}

#[test]
fn canonical_bytes_and_identifier_match_the_hand_written_request() {
    let p = plan(&rig(), Ud32Option::RankReference);
    assert_eq!(p.requests().len(), 1);
    let r = &p.requests()[0];
    assert_eq!(
        String::from_utf8(r.bytes().to_vec()).unwrap(),
        expected(VOCAB_RANK)
    );
    assert_eq!(r.id().as_str(), ID_RANK);
    assert_eq!(r.bytes().len(), 1282);
}

#[test]
fn threshold_only_vocabulary_lists_rank_reference_as_unsupported() {
    let p = plan(&rig(), Ud32Option::ThresholdOnly);
    let r = &p.requests()[0];
    assert_eq!(
        String::from_utf8(r.bytes().to_vec()).unwrap(),
        expected(VOCAB_THRESHOLD)
    );
    assert_eq!(r.id().as_str(), ID_THRESHOLD);
    assert_ne!(ID_RANK, ID_THRESHOLD);
}

#[test]
fn identical_inputs_give_identical_bytes_and_identifiers() {
    let r = rig();
    let a = plan(&r, Ud32Option::CONFIRMED);
    let b = plan(&r, Ud32Option::CONFIRMED);
    assert_eq!(a.requests()[0].bytes(), b.requests()[0].bytes());
    assert_eq!(a.requests()[0].id(), b.requests()[0].id());
}

#[test]
fn one_changed_clause_text_changes_the_identifier() {
    let m = model("rules.md", "1. Age 17.\n2. Fee 20.\n");
    let mut r = rig();
    r.all = models(&[&m]);
    r.candidates = vec![clause(&m, 0, 10), clause(&m, 11, 21)];
    let id = plan(&r, Ud32Option::CONFIRMED).requests()[0].id().clone();
    assert_ne!(id.as_str(), ID_RANK);
}

#[test]
fn model_and_limits_changes_change_the_identifier() {
    let mut r = rig();
    r.config = config_with("total_deadline_secs = 100\nattempt_timeout_secs = 30\nmax_retries = 1\nmax_response_bytes = 2048\n");
    let p = plan(&r, Ud32Option::CONFIRMED);
    let text = String::from_utf8(p.requests()[0].bytes().to_vec()).unwrap();
    assert!(
        text.contains(
            r#""limits":{"deadline_s":100,"attempt_s":30,"retries":1,"response_bytes":2048}"#
        ),
        "{text}"
    );
    assert_ne!(p.requests()[0].id().as_str(), ID_RANK);
    let policy = r.config.policy;
    assert_eq!(policy.deadline().as_secs(), 100);
    assert_eq!(policy.attempt_timeout().as_secs(), 30);
    assert_eq!(policy.retries(), 1);
}

#[test]
fn limits_and_policy_agree_for_the_default_configuration() {
    let c = rig().config;
    assert_eq!(
        u64::from(c.limits.deadline_s),
        c.policy.deadline().as_secs()
    );
    assert_eq!(
        u64::from(c.limits.attempt_s),
        c.policy.attempt_timeout().as_secs()
    );
    assert_eq!(c.limits.retries, u32::from(c.policy.retries()));
}

#[test]
fn candidate_order_is_start_then_document_bytes_then_end() {
    let a = model("a.md", "0123456789");
    let b = model("b.md", "0123456789");
    let mut r = rig();
    r.all = models(&[&a, &b]);
    r.candidates = vec![
        clause(&b, 0, 3),
        clause(&a, 4, 6),
        clause(&a, 0, 9),
        clause(&a, 0, 5),
    ];
    let p = plan(&r, Ud32Option::CONFIRMED);
    let order: Vec<(String, u32, u32)> = p.requests()[0]
        .clauses()
        .iter()
        .map(|c| (c.document.as_str().to_owned(), c.start, c.end))
        .collect();
    let want = vec![
        ("a.md".to_owned(), 0, 5),
        ("a.md".to_owned(), 0, 9),
        ("b.md".to_owned(), 0, 3),
        ("a.md".to_owned(), 4, 6),
    ];
    assert_eq!(order, want);
    let reversed: Vec<CandidateClause> = r.candidates.iter().rev().cloned().collect();
    let again = plan_for(&r.config, &r.all, &reversed, &r.rb, Ud32Option::CONFIRMED).unwrap();
    assert_eq!(again.requests()[0].bytes(), p.requests()[0].bytes());
}

#[test]
fn clauses_split_into_consecutive_requests_of_max_clauses() {
    let text = "0123456789";
    let m = model("rules.md", text);
    let mut r = rig();
    r.config = config_with("max_clauses_per_request = 2\n");
    r.all = models(&[&m]);
    r.candidates = (0..5).rev().map(|i| clause(&m, i * 2, i * 2 + 2)).collect();
    let p = plan(&r, Ud32Option::CONFIRMED);
    let shape: Vec<Vec<u32>> = p
        .requests()
        .iter()
        .map(|q| q.clauses().iter().map(|c| c.start).collect())
        .collect();
    assert_eq!(shape, vec![vec![0, 2], vec![4, 6], vec![8]]);
    let ids: BTreeSet<&str> = p.requests().iter().map(|q| q.id().as_str()).collect();
    assert_eq!(ids.len(), 3);
}

fn long_model() -> SourceModel {
    model(
        "rules.md",
        &"a".repeat(MAX_REQUEST_BYTES + MAX_CLAUSE_BYTES),
    )
}

fn many(m: &SourceModel, count: u32, size: u32, last: u32) -> Vec<CandidateClause> {
    let mut out: Vec<CandidateClause> = (0..count - 1)
        .map(|i| clause(m, i * size, (i + 1) * size))
        .collect();
    out.push(clause(m, (count - 1) * size, (count - 1) * size + last));
    out
}

fn requests_for_last(m: &SourceModel, last: u32) -> (usize, usize) {
    let mut r = rig();
    r.config = config_with("max_clauses_per_request = 100\n");
    r.all = models(&[m]);
    r.candidates = many(m, 33, 7_900, last);
    let p = plan(&r, Ud32Option::CONFIRMED);
    (p.requests().len(), p.requests()[0].bytes().len())
}

#[test]
fn request_of_exactly_256_kib_is_kept_whole_and_one_byte_more_splits() {
    let m = long_model();
    let (mut low, mut high) = (1u32, 7_900u32);
    while low < high {
        let mid = (low + high) / 2;
        if requests_for_last(&m, mid).0 == 1 {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    let first_split = low;
    let (count, size) = requests_for_last(&m, first_split - 1);
    assert_eq!(count, 1);
    assert_eq!(size, MAX_REQUEST_BYTES);
    let (count, size) = requests_for_last(&m, first_split);
    assert_eq!(count, 2);
    assert!(size <= MAX_REQUEST_BYTES);
}

#[test]
fn every_split_request_is_within_the_size_limit_and_greedy() {
    let m = long_model();
    let mut r = rig();
    r.config = config_with("max_clauses_per_request = 100\n");
    r.all = models(&[&m]);
    r.candidates = many(&m, 90, 3_000, 3_000);
    let p = plan(&r, Ud32Option::CONFIRMED);
    assert!(p.requests().len() >= 2);
    let total: usize = p.requests().iter().map(|q| q.clauses().len()).sum();
    assert_eq!(total, 90);
    let last = p.requests().len() - 1;
    for (i, q) in p.requests().iter().enumerate() {
        assert!(q.bytes().len() <= MAX_REQUEST_BYTES);
        if i != last {
            // The next clause has 3,000 text bytes plus one comma.
            assert!(q.bytes().len() + 3_001 > MAX_REQUEST_BYTES, "{i}");
        }
    }
}

fn expect_invalid(result: Result<RunPlan, Vec<Diagnostic>>) -> Vec<Diagnostic> {
    let found = result.unwrap_err();
    for d in &found {
        assert_eq!(d.code(), Code::AiRequestInvalid);
        assert_eq!(d.severity(), Severity::Error);
        assert_eq!(d.code().exit_code(), ExitCode::InvalidInput);
        assert_eq!(d.primary(), &candidates_at());
    }
    found
}

#[test]
fn clause_of_8192_bytes_is_accepted_and_8193_rejects_the_run() {
    let m = model("rules.md", &"a".repeat(MAX_CLAUSE_BYTES + 1));
    let mut r = rig();
    r.all = models(&[&m]);
    r.candidates = vec![clause(&m, 0, 8_192)];
    plan(&r, Ud32Option::CONFIRMED);
    r.candidates = vec![clause(&m, 0, 8_192), clause(&m, 0, 8_193)];
    let found = expect_invalid(plan_for(
        &r.config,
        &r.all,
        &r.candidates,
        &r.rb,
        Ud32Option::CONFIRMED,
    ));
    assert_eq!(found.len(), 1);
    assert!(
        found[0].message().contains("8193 bytes"),
        "{}",
        found[0].message()
    );
}

#[test]
fn empty_candidates_missing_model_and_stale_text_are_invalid() {
    let r = rig();
    expect_invalid(plan_for(
        &r.config,
        &r.all,
        &[],
        &r.rb,
        Ud32Option::CONFIRMED,
    ));
    let other = model("other.md", "xx yy zz");
    let missing = [clause(&other, 0, 2)];
    let found = expect_invalid(plan_for(
        &r.config,
        &r.all,
        &missing,
        &r.rb,
        Ud32Option::CONFIRMED,
    ));
    assert!(found[0].message().contains("no Source_Model"));
    let changed = model("rules.md", "1. Age 17.\n2. Fee 20.\n");
    let stale = [clause(&changed, 0, 10)];
    let found = expect_invalid(plan_for(
        &r.config,
        &r.all,
        &stale,
        &r.rb,
        Ud32Option::CONFIRMED,
    ));
    assert!(found[0].message().contains("differs from its Source_Model"));
}

#[test]
fn only_clause_text_is_sent() {
    let m = model("rules.md", "SECRET-PREFIX 1. Age 16. SECRET-SUFFIX");
    let other = model("other.md", "UNSENT-DOCUMENT-TEXT");
    let mut r = rig();
    r.all = models(&[&m, &other]);
    r.candidates = vec![clause(&m, 14, 24)];
    let p = plan(&r, Ud32Option::CONFIRMED);
    let text = String::from_utf8(p.requests()[0].bytes().to_vec()).unwrap();
    assert!(text.contains(r#""text":"1. Age 16.""#));
    assert!(!text.contains("SECRET"));
    assert!(!text.contains("UNSENT"));
    assert!(!text.contains("other.md"));
    let sent: BTreeSet<DocumentId> = [doc("rules.md")].into();
    assert_eq!(p.documents(), sent);
}

#[test]
fn highest_revision_lists_the_largest_revision_of_each_rule() {
    let rule = |rev: u32| {
        format!(
            r#"{{"id":"elig.basic","revision":{rev},"origin":{{"kind":"manual","author":null}},"kind":"eligibility","target":"eligible","type":"boolean","expr":{{"node":"cmp","t":"boolean","op":"ge","left":{{"node":"field","t":"number","name":"age"}},"right":{{"node":"num","t":"number","value":"16"}}}},"markers":[],"spans":[{{"document":"rules.md","start":0,"end":10,"line":1,"column":1}}]}}"#
        )
    };
    let mut r = rig();
    r.rb = rulebook(&format!("{},{}", rule(1), rule(2)));
    let p = plan(&r, Ud32Option::CONFIRMED);
    let text = String::from_utf8(p.requests()[0].bytes().to_vec()).unwrap();
    assert!(
        text.contains(r#""highest_revision":{"elig.basic":2}"#),
        "{text}"
    );
}

// The highest revision must win whatever the listing order. A "last
// revision wins" implementation passes only when the list is ascending.
#[test]
fn highest_revision_wins_when_a_higher_revision_is_listed_first() {
    let rule = |rev: u32| {
        format!(
            r#"{{"id":"elig.basic","revision":{rev},"origin":{{"kind":"manual","author":null}},"kind":"eligibility","target":"eligible","type":"boolean","expr":{{"node":"cmp","t":"boolean","op":"ge","left":{{"node":"field","t":"number","name":"age"}},"right":{{"node":"num","t":"number","value":"16"}}}},"markers":[],"spans":[{{"document":"rules.md","start":0,"end":10,"line":1,"column":1}}]}}"#
        )
    };
    let rb = rulebook(&format!("{},{}", rule(2), rule(1)));
    let listed: Vec<u32> = rb.revisions.iter().map(|r| r.key.revision.get()).collect();
    assert_eq!(
        listed,
        vec![2, 1],
        "the rulebook must keep the listing order"
    );
    let found = crate::decls::declarations(&rb).unwrap();
    assert_eq!(
        rt_json::write_compact(&found),
        r#"{"ir_version":"1","inputs":[{"name":"age","type":"number"}],"enums":[],"sets":[],"collections":[],"highest_revision":{"elig.basic":2}}"#
    );
}

fn expect_duplicates(r: &Rig, names: &[&str]) {
    let found = expect_invalid(plan_for(
        &r.config,
        &r.all,
        &r.candidates,
        &r.rb,
        Ud32Option::CONFIRMED,
    ));
    let messages: Vec<&str> = found.iter().map(Diagnostic::message).collect();
    assert_eq!(messages.len(), names.len(), "{messages:?}");
    for (message, name) in messages.iter().zip(names) {
        assert_eq!(*message, *name);
    }
}

#[test]
fn an_exact_duplicate_candidate_clause_rejects_the_run() {
    let m = model("rules.md", RULES_TEXT);
    let mut r = rig();
    r.candidates = vec![clause(&m, 0, 10), clause(&m, 11, 21), clause(&m, 0, 10)];
    expect_duplicates(
        &r,
        &["clause rules.md:0..10 is a duplicate Candidate_Clause"],
    );
}

#[test]
fn every_extra_copy_of_a_duplicate_is_named_in_input_order() {
    let m = model("rules.md", RULES_TEXT);
    let mut r = rig();
    r.candidates = vec![
        clause(&m, 11, 21),
        clause(&m, 0, 10),
        clause(&m, 11, 21),
        clause(&m, 11, 21),
    ];
    expect_duplicates(
        &r,
        &[
            "clause rules.md:11..21 is a duplicate Candidate_Clause",
            "clause rules.md:11..21 is a duplicate Candidate_Clause",
        ],
    );
}

#[test]
fn same_span_in_another_document_or_another_end_is_not_a_duplicate() {
    let a = model("rules.md", RULES_TEXT);
    let b = model("other.md", RULES_TEXT);
    let mut r = rig();
    r.all = models(&[&a, &b]);
    r.candidates = vec![
        clause(&a, 0, 10),
        clause(&b, 0, 10),
        clause(&a, 0, 9),
        clause(&a, 11, 21),
    ];
    let p = plan(&r, Ud32Option::CONFIRMED);
    let sent: usize = p.requests().iter().map(|q| q.clauses().len()).sum();
    assert_eq!(sent, 4);
}

fn run_dry(r: &Rig) -> (String, String, RunPlan) {
    let p = plan(r, Ud32Option::CONFIRMED);
    let (mut o, mut e) = (Vec::new(), Vec::new());
    let mut out = Out::without_credential(&mut o, &mut e);
    dry_run(&r.config, &p, &mut out).unwrap();
    (
        String::from_utf8(o).unwrap(),
        String::from_utf8(e).unwrap(),
        p,
    )
}

#[test]
fn dry_run_prints_the_identifier_and_the_announcement() {
    let (stdout, stderr, _) = run_dry(&rig());
    assert_eq!(stdout, format!("{ID_RANK}\n"));
    assert_eq!(
        stderr,
        format!(
            "dry run, nothing is sent: request {ID_RANK}: provider openai-responses, endpoint host api.openai.com, documents ['rules.md'], clauses 2, bytes 1282\n"
        )
    );
}

#[test]
fn dry_run_identifiers_follow_the_request_order_and_write_no_log() {
    let scratch = crate::tests_scratch::ScratchDir::new("plan-dry");
    let log = scratch.path().join("dry.jsonl");
    let m = model("rules.md", "0123456789");
    let mut r = rig();
    r.config = config_with("max_clauses_per_request = 1\n");
    r.config.transmission_log = log.to_string_lossy().into_owned();
    r.all = models(&[&m]);
    r.candidates = vec![clause(&m, 0, 4), clause(&m, 5, 9)];
    let (stdout, stderr, p) = run_dry(&r);
    let want: String = p
        .requests()
        .iter()
        .map(|q| format!("{}\n", q.id().as_str()))
        .collect();
    assert_eq!(p.requests().len(), 2);
    assert_eq!(stdout, want);
    assert_eq!(stderr.lines().count(), 2);
    assert!(!log.exists(), "dry run must not create a Transmission_Log");
}
