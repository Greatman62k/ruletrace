//! Property 31, request identifiers (Req 29.7, 29.9, 29.16, 23.3, 23.32).
//!
//! The expected canonical bytes are written by `oracle_bytes` from the form
//! in design 3.7.2 and hashed with `rt_hash::sha256`. The production
//! `canonical_bytes` is never called to produce an expectation. One golden
//! case uses bytes written by hand and a digest from the external
//! `sha256sum` tool.

use crate::tests::{MapEnv, BASE};
use crate::{build_plan, load, AiConfig, Consent, PlanInput, RunPlan, Ud32Option};
use proptest::prelude::*;
use rt_ai_adapter::{ProviderRequest, RequestLimits};
use rt_foundation::{CliLocation, DocumentId, InputId, Location};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, Rulebook};
use rt_json::Value;
use rt_srcmodel::{CandidateClause, DocumentKind, Element, ElementKind, SourceModel, SourceModels};
use std::collections::BTreeSet;

const NAMES: [&str; 2] = ["a.md", "b.md"];
const RULES_HASH: &str = "55887559197c793ed287815f2b33d45f454d0a045b92edf84d7d60ba82f1d29b";
const ALPHABET: [char; 12] = [
    'a', 'b', '1', ' ', '"', '\\', '\n', '\t', 'é', '中', '\u{7f}', '\u{1}',
];

const DECLS: &str = r#","declarations":{"ir_version":"1","inputs":[{"name":"age","type":"number"}],"enums":[],"sets":[],"collections":[],"highest_revision":{}},"#;
const TAIL: &str = r#","revise":null}"#;
const VOCAB_RANK: &str = r#""vocabulary":{"rule_kinds":["eligibility","derived","weighted_score","ranking","default"],"operations":["add","sub","mul","div","eq","ne","lt","le","gt","ge","and","or","not","in","field","ref","sum","min","max","count","round","rank_reference"],"types":["number","boolean","text","date","enum"],"rounding_modes":["half_even","half_away_from_zero","toward_zero"],"unsupported":[{"name":"bracket_generation","capability":"bracket generation"},{"name":"prize_allocation","capability":"prize allocation"},{"name":"progression","capability":"tournament progression"},{"name":"seeding","capability":"seeding"},{"name":"slot_allocation","capability":"slot-based qualification"}]}"#;
const VOCAB_THRESHOLD: &str = r#""vocabulary":{"rule_kinds":["eligibility","derived","weighted_score","ranking","default"],"operations":["add","sub","mul","div","eq","ne","lt","le","gt","ge","and","or","not","in","field","ref","sum","min","max","count","round"],"types":["number","boolean","text","date","enum"],"rounding_modes":["half_even","half_away_from_zero","toward_zero"],"unsupported":[{"name":"bracket_generation","capability":"bracket generation"},{"name":"prize_allocation","capability":"prize allocation"},{"name":"progression","capability":"tournament progression"},{"name":"rank_reference","capability":"rank reference"},{"name":"seeding","capability":"seeding"},{"name":"slot_allocation","capability":"slot-based qualification"}]}"#;

// Written independently of `rt-json`: quote, backslash, the three named
// control escapes, and `\u00xx` for every other control character.
fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[derive(Clone, Debug)]
struct Oracle {
    doc: &'static str,
    sha: String,
    start: usize,
    end: usize,
    line: usize,
    column: usize,
    text: String,
}

fn byte_of(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(b, _)| b)
}

fn oracle_clause(doc: usize, text: &str, from: usize, to: usize) -> Oracle {
    let (start, end) = (byte_of(text, from), byte_of(text, to));
    let prefix = &text[..start];
    let tail = prefix.rsplit('\n').next().unwrap_or("");
    Oracle {
        doc: NAMES[doc],
        sha: sha256(text.as_bytes()).as_str().to_owned(),
        start,
        end,
        line: 1 + prefix.matches('\n').count(),
        column: tail.chars().count() + 1,
        text: text[start..end].to_owned(),
    }
}

fn clause_json(c: &Oracle) -> String {
    format!(
        r#"{{"document":{},"sha256":"{}","start":{},"end":{},"line":{},"column":{},"text":{}}}"#,
        quote(c.doc),
        c.sha,
        c.start,
        c.end,
        c.line,
        c.column,
        quote(&c.text)
    )
}

fn oracle_bytes(model: &str, l: RequestLimits, rank: bool, chunk: &[Oracle]) -> Vec<u8> {
    let vocab = if rank { VOCAB_RANK } else { VOCAB_THRESHOLD };
    let clauses: Vec<String> = chunk.iter().map(clause_json).collect();
    format!(
        r#"{{"v":1,"task":"propose_rules","model":{},"limits":{{"deadline_s":{},"attempt_s":{},"retries":{},"response_bytes":{}}},"clauses":[{}]{DECLS}{vocab}{TAIL}"#,
        quote(model),
        l.deadline_s,
        l.attempt_s,
        l.retries,
        l.response_bytes,
        clauses.join(",")
    )
    .into_bytes()
}

// Expected requests in order: sort by (start, document bytes, end), then
// consecutive chunks of at most `max`.
fn expected_chunks(all: &[Oracle], max: usize) -> Vec<Vec<Oracle>> {
    let mut sorted = all.to_vec();
    sorted.sort_by(|a, b| {
        (a.start, a.doc.as_bytes(), a.end).cmp(&(b.start, b.doc.as_bytes(), b.end))
    });
    sorted.chunks(max).map(<[Oracle]>::to_vec).collect()
}

fn doc(name: &str) -> DocumentId {
    DocumentId::new(name).unwrap()
}

fn model(name: &str, text: &str) -> SourceModel {
    let end = u32::try_from(text.len()).unwrap();
    let element = Element {
        id: 0,
        kind: ElementKind::Paragraph,
        level: None,
        label: None,
        start: 0,
        end,
        line: 1,
        column: 1,
        parent: None,
    };
    SourceModel::new(
        doc(name),
        DocumentKind::Text,
        text.to_owned(),
        vec![element],
    )
    .unwrap()
}

fn base_config() -> AiConfig {
    let documents: BTreeSet<DocumentId> = [doc("rules.md")].into();
    let at = Location::Cli(CliLocation::Argument {
        position: 3,
        name: Some("--consent".to_owned()),
    });
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", "k")]);
    load(
        &InputId::new("ai.toml").unwrap(),
        BASE,
        &Consent::named(documents.clone(), at),
        &documents,
        &env,
    )
    .unwrap()
    .config
}

fn empty_rulebook() -> Rulebook {
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

#[derive(Clone, Debug)]
struct Spec {
    texts: Vec<String>,
    // (document, start char, end char), in the order given to the tool.
    clauses: Vec<(usize, usize, usize)>,
    model: String,
    limits: RequestLimits,
    rank: bool,
    max_clauses: u32,
    pick: usize,
}

fn text() -> impl Strategy<Value = String> {
    proptest::collection::vec(proptest::sample::select(ALPHABET.to_vec()), 4..=24)
        .prop_map(|c| c.into_iter().collect())
}

fn limits() -> impl Strategy<Value = RequestLimits> {
    (1u32..=1_000, 1u32..=1_000, 0u32..=9, 1_024u32..=2_000_000).prop_map(|(d, a, r, b)| {
        RequestLimits {
            deadline_s: d,
            attempt_s: a,
            retries: r,
            response_bytes: b,
        }
    })
}

fn spec() -> impl Strategy<Value = Spec> {
    let raw = proptest::collection::vec((0usize..2, any::<u8>(), any::<u8>()), 1..=7);
    (
        proptest::collection::vec(text(), 2..=2),
        raw.prop_shuffle(),
        proptest::collection::vec(proptest::sample::select(ALPHABET.to_vec()), 1..=6),
        limits(),
        any::<bool>(),
        1u32..=4,
        any::<usize>(),
    )
        .prop_map(|(texts, raw, model, limits, rank, max_clauses, pick)| {
            let mut seen = BTreeSet::new();
            let mut clauses = Vec::new();
            for (d, a, b) in raw {
                let len = texts[d].chars().count();
                let start = usize::from(a) % len;
                let end = start + 1 + usize::from(b) % (len - start);
                if seen.insert((d, start, end)) {
                    clauses.push((d, start, end));
                }
            }
            Spec {
                texts,
                clauses,
                model: model.into_iter().collect(),
                limits,
                rank,
                max_clauses,
                pick,
            }
        })
}

struct Rig {
    all: SourceModels,
    candidates: Vec<CandidateClause>,
    oracle: Vec<Oracle>,
}

fn rig(spec: &Spec, texts: &[String]) -> Rig {
    let models: Vec<SourceModel> = (0..2).map(|i| model(NAMES[i], &texts[i])).collect();
    let mut all = SourceModels::new();
    for m in &models {
        all.insert(m.clone()).unwrap();
    }
    let mut candidates = Vec::new();
    let mut oracle = Vec::new();
    for &(d, from, to) in &spec.clauses {
        let start = u32::try_from(byte_of(&texts[d], from)).unwrap();
        let end = u32::try_from(byte_of(&texts[d], to)).unwrap();
        candidates.push(CandidateClause::new(&models[d], start, end, None).unwrap());
        oracle.push(oracle_clause(d, &texts[d], from, to));
    }
    Rig {
        all,
        candidates,
        oracle,
    }
}

fn option(rank: bool) -> Ud32Option {
    if rank {
        Ud32Option::RankReference
    } else {
        Ud32Option::ThresholdOnly
    }
}

fn plan(spec: &Spec, config: &AiConfig, rig: &Rig, candidates: &[CandidateClause]) -> RunPlan {
    build_plan(&PlanInput {
        config,
        models: &rig.all,
        candidates,
        rulebook: &empty_rulebook(),
        option: option(spec.rank),
        candidates_at: Location::Cli(CliLocation::Argument {
            position: 5,
            name: Some("--candidates".to_owned()),
        }),
    })
    .unwrap()
}

fn config_of(spec: &Spec) -> AiConfig {
    let mut config = base_config();
    config.model = spec.model.clone();
    config.limits = spec.limits;
    config.max_clauses_per_request = spec.max_clauses;
    config
}

fn ids(p: &RunPlan) -> Vec<String> {
    p.requests()
        .iter()
        .map(|r| r.id().as_str().to_owned())
        .collect()
}

fn alternative(c: char) -> char {
    match c {
        'a' => 'b',
        'b' => 'a',
        '1' => ' ',
        ' ' => '1',
        '"' => '\\',
        '\\' => '"',
        '\n' => '\t',
        '\t' => '\n',
        'é' => 'ü',
        '中' => '文',
        '\u{7f}' => '\u{1}',
        _ => '\u{7f}',
    }
}

fn replace_char(text: &str, index: usize) -> String {
    text.chars()
        .enumerate()
        .map(|(i, c)| if i == index { alternative(c) } else { c })
        .collect()
}

fn reversed_clauses(r: &ProviderRequest) -> ProviderRequest {
    let mut copy = r.clone();
    for (key, value) in &mut copy.context {
        if key == "clauses" {
            let mut items = value.as_array().unwrap().to_vec();
            items.reverse();
            *value = Value::array(items);
        }
    }
    copy
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    // Requests and identifiers equal the independently written bytes and
    // their digests, whatever the order of the supplied clauses.
    #[test]
    fn requests_equal_the_oracle_and_ignore_input_order(s in spec()) {
        let config = config_of(&s);
        let r = rig(&s, &s.texts);
        let chunks = expected_chunks(&r.oracle, s.max_clauses as usize);
        let p = plan(&s, &config, &r, &r.candidates);
        prop_assert_eq!(p.requests().len(), chunks.len());
        for (got, want) in p.requests().iter().zip(&chunks) {
            let bytes = oracle_bytes(&s.model, s.limits, s.rank, want);
            prop_assert_eq!(got.bytes(), bytes.as_slice());
            let digest = sha256(&bytes);
            prop_assert_eq!(got.id().as_str(), digest.as_str());
        }
        let distinct: BTreeSet<String> = ids(&p).into_iter().collect();
        prop_assert_eq!(distinct.len(), chunks.len());

        let mut reversed = r.candidates.clone();
        reversed.reverse();
        let mut rotated = r.candidates.clone();
        let by = s.pick % rotated.len();
        rotated.rotate_left(by);
        let again = plan(&s, &config, &r, &r.candidates);
        prop_assert_eq!(ids(&again), ids(&p));
        prop_assert_eq!(ids(&plan(&s, &config, &r, &reversed)), ids(&p));
        prop_assert_eq!(ids(&plan(&s, &config, &r, &rotated)), ids(&p));
    }

    // One change of a clause byte, the model, a limit, the vocabulary
    // option, the task or the clause order changes the identifier.
    #[test]
    fn every_single_change_changes_the_identifier(s in spec()) {
        let config = config_of(&s);
        let r = rig(&s, &s.texts);
        let base = plan(&s, &config, &r, &r.candidates);
        let base_ids = ids(&base);

        let chosen = s.pick % s.clauses.len();
        let (d, from, _) = s.clauses[chosen];
        let mut texts = s.texts.clone();
        texts[d] = replace_char(&texts[d], from);
        let changed = rig(&s, &texts);
        let after = plan(&s, &config, &changed, &changed.candidates);
        let mut sorted = r.oracle.clone();
        sorted.sort_by(|a, b| {
            (a.start, a.doc.as_bytes(), a.end).cmp(&(b.start, b.doc.as_bytes(), b.end))
        });
        let position = sorted
            .iter()
            .position(|c| {
                c.doc == r.oracle[chosen].doc
                    && c.start == r.oracle[chosen].start
                    && c.end == r.oracle[chosen].end
            })
            .unwrap();
        let k = position / s.max_clauses as usize;
        prop_assert_ne!(&ids(&after)[k], &base_ids[k]);

        let mut other = config.clone();
        other.model.push('x');
        for (a, b) in base_ids.iter().zip(ids(&plan(&s, &other, &r, &r.candidates))) {
            prop_assert_ne!(a, &b);
        }
        for field in 0..4 {
            let mut other = config.clone();
            match field {
                0 => other.limits.deadline_s += 1,
                1 => other.limits.attempt_s += 1,
                2 => other.limits.retries += 1,
                _ => other.limits.response_bytes += 1,
            }
            for (a, b) in base_ids.iter().zip(ids(&plan(&s, &other, &r, &r.candidates))) {
                prop_assert_ne!(a, &b);
            }
        }
        let flipped = Spec { rank: !s.rank, ..s.clone() };
        for (a, b) in base_ids.iter().zip(ids(&plan(&flipped, &config, &r, &r.candidates))) {
            prop_assert_ne!(a, &b);
        }

        for planned in base.requests() {
            let request = planned.request();
            let mut task = request.clone();
            task.task.push('x');
            prop_assert_ne!(task.request_id().unwrap(), request.request_id().unwrap());
            if planned.clauses().len() > 1 {
                let order = reversed_clauses(request).request_id().unwrap();
                prop_assert_ne!(order, request.request_id().unwrap());
            }
        }
    }
}

// Hand-written bytes for one request, hashed with `sha256sum`.
const GOLDEN_TEXT: &str = "Fee \"20\".";
const GOLDEN_BYTES: &str = concat!(
    r#"{"v":1,"task":"propose_rules","model":"m-2","#,
    r#""limits":{"deadline_s":5,"attempt_s":3,"retries":1,"response_bytes":2048},"#,
    r#""clauses":[{"document":"a.md","sha256":"GOLDEN_DOC","start":0,"end":9,"#,
    r#""line":1,"column":1,"text":"Fee \"20\"."}]"#,
);
const GOLDEN_ID: &str = "91c9eaca3b3cd25b00de4e223822f1f6abf540f73ed65a4f9585d136a0e1ab0d";

#[test]
fn golden_request_matches_hand_written_bytes_and_external_digest() {
    let m = model("a.md", GOLDEN_TEXT);
    let mut all = SourceModels::new();
    all.insert(m.clone()).unwrap();
    let mut config = base_config();
    config.model = "m-2".to_owned();
    config.limits = RequestLimits {
        deadline_s: 5,
        attempt_s: 3,
        retries: 1,
        response_bytes: 2048,
    };
    let clause = CandidateClause::new(&m, 0, 9, None).unwrap();
    let p = build_plan(&PlanInput {
        config: &config,
        models: &all,
        candidates: &[clause],
        rulebook: &empty_rulebook(),
        option: Ud32Option::ThresholdOnly,
        candidates_at: Location::Cli(CliLocation::Argument {
            position: 5,
            name: Some("--candidates".to_owned()),
        }),
    })
    .unwrap();
    let want = format!("{GOLDEN_BYTES}{DECLS}{VOCAB_THRESHOLD}{TAIL}");
    assert_eq!(p.requests().len(), 1);
    assert_eq!(
        String::from_utf8(p.requests()[0].bytes().to_vec()).unwrap(),
        want.replace("GOLDEN_DOC", GOLDEN_DOC_HASH)
    );
    assert_eq!(p.requests()[0].id().as_str(), GOLDEN_ID);
}

// sha256sum of GOLDEN_TEXT.
const GOLDEN_DOC_HASH: &str = "9f77d9492a837a3da0e3ef9ecb2b18230ccbc992dc1346e18e2c7fc1e5960dd4";
