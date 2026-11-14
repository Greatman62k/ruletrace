use super::*;
use crate::record::{computed_hash, record_line, Parts};
use rt_foundation::{Code, Diagnostic, InputId, Location};
use rt_hash::sha256;
use rt_json::ParseLimits;

fn input() -> InputId {
    InputId::new("approvals.ledger").unwrap()
}

fn limits() -> ParseLimits {
    ParseLimits::new(1 << 20, 64)
}

fn h(seed: &str) -> String {
    sha256(seed.as_bytes()).as_str().to_owned()
}

/// Everything a test needs to say about one record. The hash is computed, as the append
/// operation of `rt-review` will do.
struct Spec<'a> {
    seq: u32,
    kind: RecordKind,
    rule: &'a str,
    revision: u32,
    content: &'a str,
    reviewer: &'a str,
    reason: Option<&'a str>,
    prev: &'a str,
}

fn spec<'a>(seq: u32, rule: &'a str, prev: &'a str) -> Spec<'a> {
    Spec {
        seq,
        kind: RecordKind::Approval,
        rule,
        revision: 1,
        content: "c1",
        reviewer: "A. Reviewer",
        reason: None,
        prev,
    }
}

fn binding() -> Vec<BindingPair> {
    vec![
        BindingPair {
            document: rt_foundation::DocumentId::new("a.md").unwrap(),
            sha256: rt_hash::Sha256Hex::parse(&h("doc-a")).unwrap(),
        },
        BindingPair {
            document: rt_foundation::DocumentId::new("b.md").unwrap(),
            sha256: rt_hash::Sha256Hex::parse(&h("doc-b")).unwrap(),
        },
    ]
}

/// Returns the canonical line and its hash.
fn seal(s: &Spec<'_>) -> (String, String) {
    let content = h(s.content);
    let binding = binding();
    let mut parts = Parts {
        seq: s.seq,
        kind: s.kind,
        rule: s.rule,
        revision: s.revision,
        content_hash: &content,
        ir_version: "1",
        binding: &binding,
        reviewer: s.reviewer,
        time: "2026-11-14T09:00:00Z",
        reason: s.reason,
        prev: s.prev,
        hash: ZERO_HASH,
    };
    let hash = computed_hash(&parts).as_str().to_owned();
    parts.hash = &hash;
    (record_line(&parts), hash)
}

fn file(lines: &[&str]) -> Vec<u8> {
    let mut out = String::from(HEADER_LINE);
    out.push('\n');
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    out.into_bytes()
}

fn parse_bytes(bytes: &[u8]) -> Result<ApprovalLedger, Vec<Diagnostic>> {
    parse_ledger(&input(), bytes, limits())
}

fn line_of(d: &Diagnostic) -> Option<u32> {
    match d.primary() {
        Location::Input(l) => l.line,
        _ => None,
    }
}

fn field_of(d: &Diagnostic) -> Option<String> {
    match d.primary() {
        Location::Input(l) => l.field.clone(),
        _ => None,
    }
}

/// Parses and expects exactly one diagnostic; returns it.
fn fail(bytes: &[u8]) -> Diagnostic {
    let mut errs = parse_bytes(bytes).unwrap_err();
    assert_eq!(errs.len(), 1, "{errs:?}");
    errs.remove(0)
}

fn two_records() -> (Vec<u8>, String, String) {
    let (l1, h1) = seal(&spec(1, "elig.age", ZERO_HASH));
    let mut s2 = spec(2, "elig.age", &h1);
    s2.revision = 2;
    s2.content = "c2";
    let (l2, _) = seal(&s2);
    (file(&[&l1, &l2]), l1, l2)
}

#[test]
fn header_only_ledger_is_valid_and_empty() {
    let ledger = parse_bytes(&file(&[])).unwrap();
    assert!(ledger.is_empty());
    assert_eq!(ledger.len(), 0);
    assert_eq!(ledger, ApprovalLedger::empty());
    assert_eq!(print_ledger(&ledger).as_bytes(), file(&[]).as_slice());
}

#[test]
fn valid_ledger_round_trips_and_prints_identically_twice() {
    let (bytes, _, _) = two_records();
    let ledger = parse_bytes(&bytes).unwrap();
    assert_eq!(ledger.len(), 2);
    let r = &ledger.records()[1];
    assert_eq!((r.seq(), r.revision().get()), (2, 2));
    assert_eq!(r.kind(), RecordKind::Approval);
    assert_eq!(r.reviewer().as_str(), "A. Reviewer");
    assert_eq!(r.reason(), None);
    assert_eq!(r.binding().pairs().len(), 2);
    let printed = print_ledger(&ledger);
    assert_eq!(printed.as_bytes(), bytes.as_slice());
    assert_eq!(printed, print_ledger(&ledger));
    assert_eq!(parse_bytes(printed.as_bytes()).unwrap(), ledger);
}

#[test]
fn rejection_with_reason_round_trips() {
    let mut s = spec(1, "r.one", ZERO_HASH);
    s.kind = RecordKind::Rejection;
    s.reason = Some("unclear \"threshold\" \u{e9}");
    let (line, _) = seal(&s);
    let ledger = parse_bytes(&file(&[&line])).unwrap();
    assert_eq!(
        ledger.records()[0].reason(),
        Some("unclear \"threshold\" \u{e9}")
    );
    assert_eq!(print_ledger(&ledger).as_bytes(), file(&[&line]).as_slice());
}

#[test]
fn syntax_and_field_failures_are_0101_at_the_line() {
    let (l1, _) = seal(&spec(1, "a", ZERO_HASH));
    let cases: Vec<(String, &str)> = vec![
        ("{not json".to_owned(), "syntax"),
        (l1.replace("\"seq\":1,", ""), "missing"),
        (l1.replace("\"seq\":1", "\"seq\":1,\"extra\":1"), "unknown"),
        (
            l1.replace("2026-11-14T09:00:00Z", "2026-11-14 09:00:00"),
            "time",
        ),
        (l1.replace("\"approval\"", "\"maybe\""), "kind"),
        (
            l1.replace("\"kind\":\"approval\",", "\"kind\":\"approval\", "),
            "whitespace",
        ),
        (
            l1.replace("\"reason\":null", "\"reason\":\"x\""),
            "approval reason",
        ),
        (l1.replace("\"revision\":1", "\"revision\":0"), "revision"),
    ];
    for (line, what) in cases {
        let d = fail(&file(&[&line]));
        assert_eq!(d.code(), Code::LedgerMalformed, "{what}");
        assert_eq!(line_of(&d), Some(2), "{what}");
    }
}

#[test]
fn rejection_needs_a_reason_of_1_to_2000_characters() {
    for reason in [None, Some(""), Some(&*"x".repeat(2001))] {
        let mut s = spec(1, "a", ZERO_HASH);
        s.kind = RecordKind::Rejection;
        s.reason = reason;
        let (line, _) = seal(&s);
        assert_eq!(fail(&file(&[&line])).code(), Code::LedgerMalformed);
    }
    let mut s = spec(1, "a", ZERO_HASH);
    s.kind = RecordKind::Rejection;
    let long = "x".repeat(2000);
    s.reason = Some(&long);
    let (line, _) = seal(&s);
    assert!(parse_bytes(&file(&[&line])).is_ok());
}

#[test]
fn unsorted_or_duplicate_binding_is_malformed() {
    let (l1, _) = seal(&spec(1, "a", ZERO_HASH));
    let a = format!(r#"{{"document":"a.md","sha256":"{}"}}"#, h("doc-a"));
    let b = format!(r#"{{"document":"b.md","sha256":"{}"}}"#, h("doc-b"));
    let sorted = format!("\"binding\":[{a},{b}]");
    assert!(l1.contains(&sorted));
    for bad in [
        format!("\"binding\":[{b},{a}]"),
        format!("\"binding\":[{a},{a}]"),
    ] {
        let d = fail(&file(&[&l1.replace(&sorted, &bad)]));
        assert_eq!(d.code(), Code::LedgerMalformed);
        assert_eq!(field_of(&d).as_deref(), Some("binding[1]"));
    }
}

#[test]
fn binding_constructor_sorts_and_rejects_duplicates() {
    let mut pairs = binding();
    pairs.reverse();
    let b = ApprovalBinding::new(pairs.clone()).unwrap();
    assert_eq!(b.pairs(), binding().as_slice());
    pairs.push(pairs[0].clone());
    assert!(matches!(
        ApprovalBinding::new(pairs),
        Err(BindingError::DuplicateDocument(_))
    ));
}

#[test]
fn sequence_gap_duplicate_and_start_are_0103() {
    let (l1, h1) = seal(&spec(1, "a", ZERO_HASH));
    let (gap, _) = seal(&spec(3, "a", &h1));
    let d = fail(&file(&[&l1, &gap]));
    assert_eq!(d.code(), Code::LedgerSequenceInvalid);
    assert_eq!(
        (line_of(&d), field_of(&d).as_deref()),
        (Some(3), Some("seq"))
    );
    let (dup, _) = seal(&spec(1, "a", &h1));
    assert_eq!(
        fail(&file(&[&l1, &dup])).code(),
        Code::LedgerSequenceInvalid
    );
    let (zero, _) = seal(&spec(0, "a", ZERO_HASH));
    assert_eq!(fail(&file(&[&zero])).code(), Code::LedgerSequenceInvalid);
}

#[test]
fn chain_failures_are_0102() {
    let (l1, h1) = seal(&spec(1, "a", ZERO_HASH));
    // Wrong prev, with a hash that is correct for the record as written.
    let (wrong_prev, _) = seal(&spec(2, "a", &h("other")));
    let d = fail(&file(&[&l1, &wrong_prev]));
    assert_eq!(d.code(), Code::LedgerChainBroken);
    assert_eq!(
        (line_of(&d), field_of(&d).as_deref()),
        (Some(3), Some("prev"))
    );
    // First record must name 64 zeros.
    let (bad_first, _) = seal(&spec(1, "a", &h("x")));
    assert_eq!(fail(&file(&[&bad_first])).code(), Code::LedgerChainBroken);
    // Content edited after sealing (still canonical).
    let (l2, _) = seal(&spec(2, "a", &h1));
    let edited = l2.replace("\"revision\":1", "\"revision\":2");
    let d = fail(&file(&[&l1, &edited]));
    assert_eq!(d.code(), Code::LedgerChainBroken);
    assert_eq!(
        (line_of(&d), field_of(&d).as_deref()),
        (Some(3), Some("hash"))
    );
}

#[test]
fn sequence_is_checked_before_the_chain() {
    let (l1, _) = seal(&spec(1, "a", ZERO_HASH));
    let (bad, _) = seal(&spec(5, "a", &h("not the previous hash")));
    assert_eq!(
        fail(&file(&[&l1, &bad])).code(),
        Code::LedgerSequenceInvalid
    );
}

#[test]
fn invalid_reviewer_is_0105_and_comes_after_the_chain() {
    for reviewer in ["", &"r".repeat(201)] {
        let mut s = spec(1, "a", ZERO_HASH);
        s.reviewer = reviewer;
        let (line, _) = seal(&s);
        let d = fail(&file(&[&line]));
        assert_eq!(d.code(), Code::LedgerReviewerInvalid);
        assert_eq!(field_of(&d).as_deref(), Some("reviewer"));
    }
    // A broken chain wins over an invalid reviewer on the same record.
    let other = h("x");
    let mut s = spec(1, "a", &other);
    s.reviewer = "";
    let (line, _) = seal(&s);
    assert_eq!(fail(&file(&[&line])).code(), Code::LedgerChainBroken);
    // 200 scalar values (multi-byte) are accepted.
    let mut s = spec(1, "a", ZERO_HASH);
    let name = "\u{e9}".repeat(200);
    s.reviewer = &name;
    let (line, _) = seal(&s);
    assert!(parse_bytes(&file(&[&line])).is_ok());
}

fn contradiction(first: impl FnOnce(&mut Spec<'_>), second: impl FnOnce(&mut Spec<'_>)) -> Vec<u8> {
    let mut s1 = spec(1, "elig.age", ZERO_HASH);
    first(&mut s1);
    let (l1, h1) = seal(&s1);
    let mut s2 = spec(2, "elig.age", &h1);
    second(&mut s2);
    let (l2, _) = seal(&s2);
    file(&[&l1, &l2])
}

fn reject(s: &mut Spec<'_>) {
    s.kind = RecordKind::Rejection;
    s.reason = Some("no");
}

#[test]
fn contradictory_decisions_are_0104_naming_rule_revision_and_sequences() {
    let cases = [
        // Same revision, same content hash, twice.
        contradiction(|_| {}, |_| {}),
        // Rejection after approval of the same content.
        contradiction(|_| {}, reject),
        // Approval after a rejection, even for other content.
        contradiction(reject, |s| s.content = "c2"),
        // Second rejection of a rejected revision.
        contradiction(reject, reject),
    ];
    for bytes in cases {
        let d = fail(&bytes);
        assert_eq!(d.code(), Code::LedgerRecordContradictory);
        assert_eq!(line_of(&d), Some(3));
        assert!(d.message().contains("elig.age") && d.message().contains("record 2"));
        assert!(d.message().contains("record 1"));
        assert_eq!(d.rules().len(), 1);
        assert_eq!(d.rules()[0].rule.as_str(), "elig.age");
        assert_eq!(d.rules()[0].revision.map(|r| r.get()), Some(1));
        assert_eq!(d.related().len(), 1);
    }
}

#[test]
fn later_approval_of_changed_content_and_other_revisions_are_not_contradictions() {
    let again = contradiction(|_| {}, |s| s.content = "c2");
    assert_eq!(parse_bytes(&again).unwrap().len(), 2);
    let other_revision = contradiction(reject, |s| s.revision = 2);
    assert_eq!(parse_bytes(&other_revision).unwrap().len(), 2);
    let other_rule = contradiction(reject, |s| s.rule = "elig.other");
    assert_eq!(parse_bytes(&other_rule).unwrap().len(), 2);
    let reject_after_approval = contradiction(
        |_| {},
        |s| {
            reject(s);
            s.content = "c2";
        },
    );
    assert_eq!(parse_bytes(&reject_after_approval).unwrap().len(), 2);
}

#[test]
fn flag_and_lifecycle_keys_give_the_dgn_codes_with_the_file_line() {
    let (l1, _) = seal(&spec(1, "a", ZERO_HASH));
    let with = |key: &str| l1.replacen('{', &format!("{{\"{key}\":true,"), 1);
    let d = fail(&file(&[&with("has_errors")]));
    assert_eq!(d.code(), Code::DiagnosticStateContradiction);
    assert_eq!(line_of(&d), Some(2));
    let d = fail(&file(&[&with("approval_status")]));
    assert_eq!(d.code(), Code::LifecycleFlagContradiction);
    assert_eq!(line_of(&d), Some(2));
    let both = l1.replacen('{', "{\"has_errors\":false,\"no_errors\":true,", 1);
    assert_eq!(fail(&file(&[&both])).code(), Code::DiagnosticStateBothFlags);
    // Also in the header line.
    let header = r#"{"format":"ruletrace-ledger","version":1,"is_approved":true}"#;
    let d = fail(format!("{header}\n").as_bytes());
    assert_eq!(d.code(), Code::LifecycleFlagContradiction);
    assert_eq!(line_of(&d), Some(1));
}

#[test]
fn file_level_failures() {
    assert_eq!(fail(b"").code(), Code::LedgerMalformed);
    assert_eq!(fail(b"\n").code(), Code::LedgerMalformed);
    let no_lf = HEADER_LINE.as_bytes();
    assert_eq!(fail(no_lf).code(), Code::LedgerMalformed);
    let wrong = br#"{"format":"ruletrace-ledger","version":2}"#.to_vec();
    let mut wrong = wrong;
    wrong.push(b'\n');
    assert_eq!(fail(&wrong).code(), Code::LedgerMalformed);
    // Blank line, CRLF and invalid UTF-8.
    let mut blank = file(&[]);
    blank.push(b'\n');
    let d = fail(&blank);
    assert_eq!((d.code(), line_of(&d)), (Code::LedgerMalformed, Some(2)));
    let crlf = format!("{HEADER_LINE}\r\n");
    assert_eq!(fail(crlf.as_bytes()).code(), Code::LedgerMalformed);
    let mut bad_utf8 = file(&[]);
    bad_utf8.extend_from_slice(&[0xff, b'\n']);
    let d = fail(&bad_utf8);
    assert_eq!((d.code(), line_of(&d)), (Code::LedgerMalformed, Some(2)));
    let d = fail_with_limit(&file(&[]), 10);
    assert_eq!(d.code(), Code::LimitFileTooLarge);
}

fn fail_with_limit(bytes: &[u8], max: u64) -> Diagnostic {
    parse_ledger(&input(), bytes, ParseLimits::new(max, 64))
        .unwrap_err()
        .remove(0)
}

#[test]
fn stops_at_the_first_failing_record() {
    let (l1, h1) = seal(&spec(1, "a", ZERO_HASH));
    let (bad2, _) = seal(&spec(7, "a", &h1));
    let (bad3, _) = seal(&spec(9, "a", &h1));
    let errs = parse_bytes(&file(&[&l1, &bad2, &bad3, "garbage"])).unwrap_err();
    assert_eq!(errs.len(), 1);
    assert_eq!(line_of(&errs[0]), Some(3));
}

/// Observable of task 4.7: one changed byte fails, at the record that holds the byte, with a
/// code of the ledger family or malformed.
#[test]
fn every_single_byte_change_is_detected_at_its_own_line() {
    let (bytes, _, _) = two_records();
    assert!(parse_bytes(&bytes).is_ok());
    for i in 0..bytes.len() {
        let line = u32::try_from(bytes[..i].iter().filter(|b| **b == b'\n').count()).unwrap() + 1;
        for replacement in [b'X', 0xff, b'0', b'1'] {
            if bytes[i] == replacement {
                continue;
            }
            let mut changed = bytes.clone();
            changed[i] = replacement;
            let errs = parse_bytes(&changed)
                .expect_err(&format!("byte {i} -> {replacement:#x} was accepted"));
            assert_eq!(
                line_of(&errs[0]),
                Some(line),
                "byte {i} -> {replacement:#x}: {:?}",
                errs[0]
            );
        }
    }
}

#[test]
fn a_changed_byte_in_a_hashed_field_is_a_chain_failure() {
    let (bytes, l1, _) = two_records();
    let text = String::from_utf8(bytes).unwrap();
    let edited = text.replacen(&l1, &l1.replace("elig.age", "elig.agf"), 1);
    let d = fail(edited.as_bytes());
    assert_eq!(d.code(), Code::LedgerChainBroken);
    assert_eq!(line_of(&d), Some(2));
}

#[test]
fn diagnostic_messages_escape_control_characters_of_keys() {
    let (l1, _) = seal(&spec(1, "a", ZERO_HASH));
    let line = l1.replacen('{', "{\"x\\u001b[31m\":1,", 1);
    let d = fail(&file(&[&line]));
    assert_eq!(d.code(), Code::LedgerMalformed);
    assert!(!d.message().contains('\u{1b}'));
}

// ---- Source scan: the crate has no function that appends or writes a record ----

fn production(src: &str) -> String {
    let cut = src.find("#[cfg(test)]").unwrap_or(src.len());
    src[..cut]
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn sources() -> Vec<(&'static str, String)> {
    vec![
        ("lib.rs", production(include_str!("lib.rs"))),
        ("parse.rs", production(include_str!("parse.rs"))),
        ("print.rs", production(include_str!("print.rs"))),
        ("record.rs", production(include_str!("record.rs"))),
    ]
}

#[test]
fn no_public_function_creates_a_record_or_a_ledger_from_parts() {
    // The only public functions that return a record type or a ledger are `parse_ledger` and
    // `ApprovalLedger::empty`. Accessors return references.
    let allowed = ["parse_ledger", "empty"];
    for (file, src) in sources() {
        for (n, line) in src.lines().enumerate() {
            let t = line.trim_start();
            if !t.starts_with("pub fn") {
                continue;
            }
            let name = t["pub fn ".len()..].split(['(', '<']).next().unwrap();
            let creates = [
                "-> LedgerRecord",
                "-> ApprovalLedger",
                "-> Self",
                "Result<LedgerRecord",
                "Result<ApprovalLedger",
                "Option<LedgerRecord",
            ]
            .iter()
            .any(|r| t.contains(r));
            // Multi-line signatures: look at the next lines up to the body.
            let sig: String = src
                .lines()
                .skip(n)
                .take_while(|l| !l.contains('{') && !l.trim_end().ends_with(';'))
                .chain(src.lines().skip(n).find(|l| l.contains('{')))
                .collect::<Vec<_>>()
                .join(" ");
            let creates_multi = sig.contains("-> Result<ApprovalLedger")
                || sig.contains("-> Result<LedgerRecord")
                || sig.contains("-> LedgerRecord")
                || sig.contains("-> ApprovalLedger");
            if (creates || creates_multi) && !allowed.contains(&name) {
                // `ApprovalBinding::new` returns Self but is not a record.
                assert!(
                    t.contains("ApprovalBinding") || file == "record.rs" && name == "new",
                    "{file}:{}: `{name}` creates a record or ledger",
                    n + 1
                );
            }
        }
    }
}

#[test]
fn no_append_write_or_io_names_in_production_source() {
    for (file, src) in sources() {
        for forbidden in [
            "fn append",
            "fn push_record",
            "fn write_record",
            "fn add_record",
            "fn seal",
            "std::fs",
            "std::env",
            "std::net",
            "std::process",
            "SystemTime",
            "Instant",
            "f64",
            "f32",
            "static mut",
            "thread_local",
            "lazy",
        ] {
            assert!(!src.contains(forbidden), "{file} contains `{forbidden}`");
        }
    }
}

#[test]
fn record_fields_are_not_publicly_settable() {
    let src = production(include_str!("record.rs"));
    let from = src.find("pub struct LedgerRecord").unwrap();
    let to = src.find("impl LedgerRecord").unwrap();
    let body = &src[from..to];
    assert!(body.contains("pub(crate) seq"));
    assert!(!body.contains("    pub seq") && !body.contains("    pub hash"));
    assert!(!src.contains("pub records"));
}
