//! Property tests of provenance verification (Req 23.7). The oracle works
//! on the document as a list of characters and a set of boundary offsets,
//! and applies the checks in the order of the requirement.

use crate::plan::ClauseId;
use crate::provenance::verify_provenance;
use crate::tests_provenance::{doc, escaped, model};
use crate::tests_response::{parse, rid, wrap, SHA};
use proptest::prelude::*;
use rt_foundation::{Code, Location};
use rt_srcmodel::SourceModels;
use std::collections::BTreeSet;

const ALPHABET: [char; 8] = ['a', 'b', ' ', '\n', '\r', 'é', 'δ', '日'];

#[derive(Clone, Debug)]
struct Case {
    text: String,
    clauses: Vec<(u32, u32)>,
    known: bool,
    hash_ok: bool,
    start: u32,
    end: u32,
    line_mode: u8,
    quote_mode: u8,
}

fn boundaries(text: &str) -> BTreeSet<u32> {
    let mut set: BTreeSet<u32> = text
        .char_indices()
        .map(|(i, _)| u32::try_from(i).unwrap())
        .collect();
    set.insert(u32::try_from(text.len()).unwrap());
    set
}

// Walks the characters before `start`. A CR followed by LF leaves the line
// to the LF, any other CR and every LF ends the line.
fn line_column(text: &str, start: u32) -> (u32, u32) {
    let Some(before) = text.get(..start as usize) else {
        return (1, 1);
    };
    let (mut line, mut column) = (1u32, 1u32);
    let mut rest = text.chars().peekable();
    let mut seen = 0usize;
    while let Some(c) = rest.next() {
        if seen >= before.len() {
            break;
        }
        seen += c.len_utf8();
        let breaks = match c {
            '\n' => true,
            '\r' => rest.peek() != Some(&'\n'),
            _ => false,
        };
        if breaks {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn claimed_position(c: &Case) -> (u32, u32) {
    let (line, column) = line_column(&c.text, c.start);
    match c.line_mode {
        1 => (line + 1, column),
        2 => (line, column + 1),
        _ => (line, column),
    }
}

fn claimed_quote(c: &Case) -> String {
    let exact = c
        .text
        .get(c.start as usize..c.end as usize)
        .unwrap_or("q")
        .to_owned();
    match c.quote_mode {
        1 => format!("{exact}x"),
        2 => format!(" {exact}"),
        _ => exact,
    }
}

fn oracle(c: &Case) -> Option<(Code, &'static str)> {
    if !c.known {
        return Some((Code::AiProposalSpanUnresolved, "document"));
    }
    if !c.hash_ok {
        return Some((Code::AiProposalHashMismatch, "sha256"));
    }
    let len = u32::try_from(c.text.len()).unwrap();
    let marks = boundaries(&c.text);
    if c.start >= c.end || c.end > len {
        return Some((Code::AiProposalSpanUnresolved, "end"));
    }
    if !marks.contains(&c.start) {
        return Some((Code::AiProposalSpanUnresolved, "start"));
    }
    if !marks.contains(&c.end) {
        return Some((Code::AiProposalSpanUnresolved, "end"));
    }
    match c.line_mode {
        1 => return Some((Code::AiProposalSpanUnresolved, "line")),
        2 => return Some((Code::AiProposalSpanUnresolved, "column")),
        _ => {}
    }
    if !c.clauses.iter().any(|&(a, b)| a <= c.start && c.end <= b) {
        return Some((Code::AiProposalSpanOutsideContext, ""));
    }
    if c.quote_mode != 0 {
        return Some((Code::AiProposalQuoteMismatch, "quote"));
    }
    None
}

fn payload(c: &Case, sha: &str) -> String {
    let (line, column) = claimed_position(c);
    let name = if c.known { "rules.md" } else { "zz.md" };
    let hash = if c.hash_ok { sha } else { SHA };
    let item = format!(
        r#"{{"document":"{name}","sha256":"{hash}","start":{},"end":{},"line":{line},"column":{column},"quote":"{}"}}"#,
        c.start,
        c.end,
        escaped(&claimed_quote(c))
    );
    wrap(&[format!(
        r#"{{"kind":"clause_relationship","proposer":"model-1","rationale":"why","evidence":[{item}]}}"#
    )])
}

fn verify(c: &Case) -> Vec<rt_foundation::Diagnostic> {
    let m = model("rules.md", &c.text);
    let sha = m.sha256().as_str().to_owned();
    let mut all = SourceModels::new();
    all.insert(m).unwrap();
    let clauses: Vec<ClauseId> = c
        .clauses
        .iter()
        .map(|&(start, end)| ClauseId {
            document: doc("rules.md"),
            start,
            end,
        })
        .collect();
    let parsed = parse(&payload(c, &sha)).unwrap();
    verify_provenance(&rid(), &parsed, &all, &clauses)
}

fn check(c: &Case) {
    let found = verify(c);
    match oracle(c) {
        None => assert!(found.is_empty(), "{c:?}: {found:?}"),
        Some((code, key)) => {
            assert_eq!(found.len(), 1, "{c:?}: {found:?}");
            assert_eq!(found[0].code(), code, "{c:?}");
            let Location::Input(at) = found[0].primary() else {
                panic!("not an input location");
            };
            let path = if key.is_empty() {
                "proposals[0].evidence[0]".to_owned()
            } else {
                format!("proposals[0].evidence[0].{key}")
            };
            assert_eq!(at.field.as_deref(), Some(path.as_str()), "{c:?}");
        }
    }
}

fn case() -> impl Strategy<Value = Case> {
    proptest::collection::vec(proptest::sample::select(ALPHABET.to_vec()), 1..20)
        .prop_flat_map(|chars| {
            let text: String = chars.into_iter().collect();
            let marks: Vec<u32> = boundaries(&text).into_iter().collect();
            let len = u32::try_from(text.len()).unwrap();
            let pair = (0..marks.len(), 0..marks.len())
                .prop_filter_map("ordered pair", move |(a, b)| {
                    (a < b).then(|| (marks[a], marks[b]))
                });
            (
                Just(text),
                proptest::collection::vec(pair, 0..4),
                0u32..=len + 2,
                0u32..=len + 2,
                (any::<u8>(), any::<u8>(), any::<u8>(), any::<u8>()),
            )
        })
        .prop_map(|(text, clauses, start, end, (k, h, l, q))| Case {
            text,
            clauses,
            known: k % 6 != 0,
            hash_ok: h % 6 != 0,
            start,
            end,
            line_mode: if l % 3 == 0 { 1 + l / 3 % 2 } else { 0 },
            quote_mode: if q % 3 == 0 { 1 + q / 3 % 2 } else { 0 },
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn verification_agrees_with_the_independent_oracle(c in case()) {
        check(&c);
    }
}

#[test]
fn the_oracle_space_reaches_every_outcome_and_the_verifier_agrees() {
    let text = "aé\nδ日";
    let len = u32::try_from(text.len()).unwrap();
    let mut seen = BTreeSet::new();
    for start in 0..=len + 2 {
        for end in 0..=len + 2 {
            for (known, hash_ok) in [(true, true), (true, false), (false, true)] {
                for line_mode in 0..3 {
                    for quote_mode in 0..3 {
                        let c = Case {
                            text: text.to_owned(),
                            clauses: vec![(0, 3), (4, len)],
                            known,
                            hash_ok,
                            start,
                            end,
                            line_mode,
                            quote_mode,
                        };
                        seen.insert(oracle(&c).map(|(code, key)| (code.id(), key)));
                        check(&c);
                    }
                }
            }
        }
    }
    let want = [
        None,
        Some(("RT-PRV-0101", "document")),
        Some(("RT-PRV-0102", "sha256")),
        Some(("RT-PRV-0101", "start")),
        Some(("RT-PRV-0101", "end")),
        Some(("RT-PRV-0101", "line")),
        Some(("RT-PRV-0101", "column")),
        Some(("RT-PRV-0103", "")),
        Some(("RT-PRV-0104", "quote")),
    ];
    for outcome in want {
        assert!(seen.contains(&outcome), "never reached {outcome:?}");
    }
}
