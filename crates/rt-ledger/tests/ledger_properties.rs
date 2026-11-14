//! Property 14: ledger round trip and tamper evidence.
//!
//! **Validates: Requirements 26.2, 26.3, 26.4, 26.20, 26.21, 26.23**
//!
//! `rt-ledger` has no public record constructor, so the ledgers are written
//! here in their canonical text form and sealed with an independently computed
//! hash chain (SHA-256 of the compact record without its `hash` key). Every
//! tampered ledger must fail with one of `RT-APR-0101` to `0104` at the first
//! affected record, and yield no ledger at all.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use proptest::prelude::*;
use rt_foundation::{Code, Diagnostic, InputId, Location};
use rt_hash::sha256;
use rt_json::ParseLimits;
use rt_ledger::{parse_ledger, print_ledger, ApprovalLedger, HEADER_LINE, ZERO_HASH};

#[derive(Debug, Clone)]
struct Rec {
    rejection: bool,
    rule: String,
    reviewer: String,
    reason: String,
    content_seed: u8,
    docs: Vec<u8>,
}

fn rec() -> impl Strategy<Value = Rec> {
    (
        any::<bool>(),
        "[a-z][a-z0-9_.]{0,9}",
        "[A-Za-z][A-Za-z0-9 .]{0,15}",
        "[A-Za-z][A-Za-z0-9 .,]{0,30}",
        any::<u8>(),
        proptest::collection::btree_set(0u8..3, 0..3),
    )
        .prop_map(
            |(rejection, rule, reviewer, reason, content_seed, docs)| Rec {
                rejection,
                rule,
                reviewer,
                reason,
                content_seed,
                docs: docs.into_iter().collect(),
            },
        )
}

fn hex(seed: &str) -> String {
    sha256(seed.as_bytes()).as_str().to_owned()
}

/// Builds the canonical record lines with a correct chain. Revision numbers are
/// distinct, so no two records contradict each other.
fn build(recs: &[Rec]) -> Vec<String> {
    let mut prev = ZERO_HASH.to_owned();
    let mut lines = Vec::new();
    for (i, r) in recs.iter().enumerate() {
        let binding: Vec<String> = r
            .docs
            .iter()
            .map(|d| {
                format!(
                    r#"{{"document":"d{d}.md","sha256":"{}"}}"#,
                    hex(&format!("doc{d}"))
                )
            })
            .collect();
        let reason = if r.rejection {
            format!("\"{}\"", r.reason)
        } else {
            "null".to_owned()
        };
        let head = format!(
            r#"{{"seq":{},"kind":"{}","rule":"{}","revision":{},"content_hash":"{}","ir_version":"1","binding":[{}],"reviewer":"{}","time":"2026-11-14T09:00:00Z","reason":{},"prev":"{}""#,
            i + 1,
            if r.rejection { "rejection" } else { "approval" },
            r.rule,
            i + 1,
            hex(&format!("c{}", r.content_seed)),
            binding.join(","),
            r.reviewer,
            reason,
            prev
        );
        let hash = sha256(format!("{head}}}").as_bytes()).as_str().to_owned();
        lines.push(format!(r#"{head},"hash":"{hash}"}}"#));
        prev = hash;
    }
    lines
}

fn file(lines: &[String]) -> Vec<u8> {
    let mut out = String::from(HEADER_LINE);
    out.push('\n');
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    out.into_bytes()
}

fn parse(bytes: &[u8]) -> Result<ApprovalLedger, Vec<Diagnostic>> {
    let input = InputId::new("approvals.ledger").expect("input id");
    parse_ledger(&input, bytes, ParseLimits::new(1 << 20, 64))
}

fn line_of(d: &Diagnostic) -> Option<u32> {
    match d.primary() {
        Location::Input(l) => l.line,
        _ => None,
    }
}

const TAMPER_CODES: [Code; 4] = [
    Code::LedgerMalformed,
    Code::LedgerChainBroken,
    Code::LedgerSequenceInvalid,
    Code::LedgerRecordContradictory,
];

/// Asserts the single diagnostic of a rejected ledger and returns its code and
/// line.
fn rejected(bytes: &[u8]) -> Result<(Code, u32), TestCaseError> {
    let errs = match parse(bytes) {
        Ok(_) => return Err(TestCaseError::fail("tampered ledger was accepted")),
        Err(e) => e,
    };
    prop_assert_eq!(errs.len(), 1);
    let code = errs[0].code();
    prop_assert!(TAMPER_CODES.contains(&code), "unexpected {:?}", code);
    let line = line_of(&errs[0]).ok_or_else(|| TestCaseError::fail("no line"))?;
    Ok((code, line))
}

fn ledger_strategy(min: usize) -> impl Strategy<Value = Vec<Rec>> {
    proptest::collection::vec(rec(), min..6)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn round_trip_and_print_twice(recs in ledger_strategy(0)) {
        let bytes = file(&build(&recs));
        let ledger = parse(&bytes).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
        prop_assert_eq!(ledger.len(), recs.len());
        let printed = print_ledger(&ledger);
        prop_assert_eq!(printed.as_bytes(), bytes.as_slice());
        prop_assert_eq!(&printed, &print_ledger(&ledger));
        let again = parse(printed.as_bytes()).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
        prop_assert_eq!(again, ledger);
    }

    #[test]
    fn any_one_byte_change_is_detected_at_its_record(
        recs in ledger_strategy(1),
        pos in any::<usize>(),
        delta in 1u8..=255,
    ) {
        let lines = build(&recs);
        let mut bytes = file(&lines);
        let start = HEADER_LINE.len() + 1;
        let at = start + pos % (bytes.len() - start);
        bytes[at] = bytes[at].wrapping_add(delta);
        // The affected record is the one whose line contains the changed byte.
        let mut line_no = 2u32;
        let mut offset = start;
        for l in &lines {
            if at <= offset + l.len() {
                break;
            }
            offset += l.len() + 1;
            line_no += 1;
        }
        let (_, line) = rejected(&bytes)?;
        prop_assert_eq!(line, line_no);
    }

    #[test]
    fn removal_of_a_record_before_the_last_is_a_sequence_error(
        recs in ledger_strategy(2),
        idx in any::<usize>(),
    ) {
        let mut lines = build(&recs);
        let i = idx % (lines.len() - 1);
        lines.remove(i);
        let (code, line) = rejected(&file(&lines))?;
        prop_assert_eq!(code, Code::LedgerSequenceInvalid);
        prop_assert_eq!(line, i as u32 + 2);
    }

    #[test]
    fn reordering_is_a_sequence_error_at_the_first_moved_record(
        recs in ledger_strategy(2),
        a in any::<usize>(),
        b in any::<usize>(),
    ) {
        let mut lines = build(&recs);
        let i = a % lines.len();
        let j = (i + 1 + b % (lines.len() - 1)) % lines.len();
        lines.swap(i, j);
        let (code, line) = rejected(&file(&lines))?;
        prop_assert_eq!(code, Code::LedgerSequenceInvalid);
        prop_assert_eq!(line, i.min(j) as u32 + 2);
    }

    #[test]
    fn insertion_of_a_copied_record_is_detected(
        recs in ledger_strategy(1),
        from in any::<usize>(),
        to in any::<usize>(),
    ) {
        let mut lines = build(&recs);
        let j = from % lines.len();
        let p = to % (lines.len() + 1);
        let copy = lines[j].clone();
        lines.insert(p, copy);
        let (code, line) = rejected(&file(&lines))?;
        prop_assert_eq!(code, Code::LedgerSequenceInvalid);
        // A copy placed directly before its original is itself valid; the
        // original after it is the first record out of sequence.
        let expected = if p == j { p + 3 } else { p + 2 };
        prop_assert_eq!(line, expected as u32);
    }
}
