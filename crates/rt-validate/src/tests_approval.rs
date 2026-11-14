//! Tests of passes 10 and 11 and of the public `validate` function.
//!
//! The ledger has no append function in `rt-ledger`, so these tests write
//! canonical ledger lines by hand (the hash chain is computed here, as
//! `rt-review` will do) and parse them.

use super::*;
use rt_foundation::Severity;
use rt_hash::sha256;
use rt_ir::rule_content_hash;
use rt_json::ParseLimits;
use rt_ledger::parse_ledger;

const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";

struct Rec<'a> {
    rejection: bool,
    rule: &'a str,
    revision: u32,
    content_hash: String,
}

fn approve(rule: &str, revision: u32, content_hash: String) -> Rec<'_> {
    Rec {
        rejection: false,
        rule,
        revision,
        content_hash,
    }
}

fn reject(rule: &str, revision: u32, content_hash: String) -> Rec<'_> {
    Rec {
        rejection: true,
        rule,
        revision,
        content_hash,
    }
}

fn ledger(recs: &[Rec<'_>]) -> ApprovalLedger {
    let doc = sha256(TEXT.as_bytes());
    let mut out = String::from(rt_ledger::HEADER_LINE);
    out.push('\n');
    let mut prev = ZERO.to_owned();
    for (i, r) in recs.iter().enumerate() {
        let kind = if r.rejection { "rejection" } else { "approval" };
        let reason = if r.rejection {
            r#""not a rule""#
        } else {
            "null"
        };
        let base = format!(
            r#"{{"seq":{},"kind":"{kind}","rule":"{}","revision":{},"content_hash":"{}","ir_version":"1","binding":[{{"document":"rules.md","sha256":"{doc}"}}],"reviewer":"A. Reviewer","time":"2026-11-14T09:00:00Z","reason":{reason},"prev":"{prev}""#,
            i + 1,
            r.rule,
            r.revision,
            r.content_hash
        );
        let hash = sha256(format!("{base}}}").as_bytes()).as_str().to_owned();
        out.push_str(&format!("{base},\"hash\":\"{hash}\"}}\n"));
        prev = hash;
    }
    let input = InputId::new("approvals.ledger").unwrap();
    parse_ledger(&input, out.as_bytes(), ParseLimits::new(1 << 20, 64))
        .unwrap_or_else(|e| panic!("ledger fixture does not parse: {e:?}"))
}

fn hash_of(rb: &Rulebook, index: usize) -> String {
    rule_content_hash(&rb.ir_version, &rb.revisions[index], &rb.documents)
        .unwrap()
        .to_string()
}

fn d1(id: &str, target: &str) -> String {
    derived(id, target, &num("1"), &span_json(CLAUSE1))
}

fn at_revision_2(rule: &str) -> String {
    rule.replace(r#""revision":1"#, r#""revision":2"#)
}

fn codes_of(r: &ValidationResult) -> Vec<Code> {
    codes(r)
}

fn run(rb: &Rulebook, l: &ApprovalLedger) -> ValidationResult {
    validate(rb, l, &sources(), None)
}

#[test]
fn unresolved_revision_gives_apr_0001_at_its_first_span_and_a_failed_result() {
    let rb = rulebook(&[d1("d.a", "a")]);
    let r = run(&rb, &ApprovalLedger::empty());
    assert_eq!(
        codes_of(&r),
        [Code::ApprovalUnresolved, Code::ApprovalNoExecutableRules]
    );
    let d = &r.state().as_slice()[0];
    assert_eq!(d.severity(), Severity::Error);
    assert_eq!(source_start(d), TEXT.find(CLAUSE1).unwrap() as u32);
    assert_eq!(d.rules()[0].rule.as_str(), "d.a");
    assert!(r.state().has_errors());
    assert!(r.state().has_unresolved_approval());
}

#[test]
fn matching_approval_makes_the_revision_effective_and_the_result_pass() {
    let rb = rulebook(&[d1("d.a", "a")]);
    let l = ledger(&[approve("d.a", 1, hash_of(&rb, 0))]);
    let r = run(&rb, &l);
    assert!(codes_of(&r).is_empty(), "{:?}", r.state().as_slice());
    assert!(r.state().no_errors());
    assert_eq!(r.checked_conflict_classes().len(), 4);
}

#[test]
fn rejected_revision_gives_only_a_warning_when_another_rule_is_effective() {
    let rb = rulebook(&[d1("d.a", "a"), d1("d.b", "b")]);
    let l = ledger(&[
        approve("d.a", 1, hash_of(&rb, 0)),
        reject("d.b", 1, hash_of(&rb, 1)),
    ]);
    let r = run(&rb, &l);
    assert_eq!(codes_of(&r), [Code::ApprovalRevisionRejected]);
    assert_eq!(r.state().as_slice()[0].severity(), Severity::Warning);
    assert!(r.state().no_errors());
}

#[test]
fn rejected_revision_with_changed_content_is_an_error_and_stays_rejected() {
    let rb = rulebook(&[d1("d.a", "a"), d1("d.b", "b")]);
    let l = ledger(&[
        approve("d.a", 1, hash_of(&rb, 0)),
        reject("d.b", 1, sha256(b"older content").as_str().to_owned()),
    ]);
    let r = run(&rb, &l);
    assert_eq!(
        codes_of(&r),
        [
            Code::ApprovalRejectedRevisionModified,
            Code::ApprovalRevisionRejected
        ]
    );
    assert_eq!(
        source_start(&r.state().as_slice()[0]),
        source_start(&r.state().as_slice()[1])
    );
    assert!(r.state().has_errors());
    // Rejected is not unresolved.
    assert!(!r.state().has_unresolved_approval());
}

#[test]
fn non_matching_approval_is_stale_and_never_counts_as_an_approval() {
    let rb = rulebook(&[d1("d.a", "a"), d1("d.b", "b")]);
    let l = ledger(&[
        approve("d.a", 1, hash_of(&rb, 0)),
        approve("d.b", 1, sha256(b"other content").as_str().to_owned()),
        approve("ghost", 1, sha256(b"x").as_str().to_owned()),
    ]);
    let r = run(&rb, &l);
    assert_eq!(
        codes_of(&r),
        [
            Code::ApprovalUnresolved,
            Code::ApprovalRecordStale,
            Code::ApprovalRecordStale
        ]
    );
    let stale: Vec<&Diagnostic> = r
        .state()
        .as_slice()
        .iter()
        .filter(|d| d.code() == Code::ApprovalRecordStale)
        .collect();
    assert!(stale.iter().all(|d| d.severity() == Severity::Warning));
    let msgs: Vec<&str> = stale.iter().map(|d| d.message()).collect();
    assert!(msgs
        .iter()
        .any(|m| m.contains("'d.b'") && m.contains("revision 1")));
    assert!(msgs.iter().any(|m| m.contains("'ghost'")));
    match stale[0].primary() {
        Location::Input(l) => assert_eq!(l.line, Some(3)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn stale_record_does_not_change_blocking_state() {
    let rb = rulebook(&[d1("d.a", "a")]);
    let good = ledger(&[approve("d.a", 1, hash_of(&rb, 0))]);
    let with_stale = ledger(&[
        approve("d.a", 1, hash_of(&rb, 0)),
        approve("ghost", 1, sha256(b"x").as_str().to_owned()),
    ]);
    let (a, b) = (run(&rb, &good), run(&rb, &with_stale));
    assert_eq!(a.state().has_errors(), b.state().has_errors());
    assert_eq!(codes_of(&b), [Code::ApprovalRecordStale]);
}

#[test]
fn proposed_higher_revision_is_the_only_unresolved_finding_and_names_it() {
    let r1 = d1("d.a", "a");
    let rb = rulebook(&[r1.clone(), at_revision_2(&r1)]);
    let l = ledger(&[approve("d.a", 1, hash_of(&rb, 0))]);
    let r = run(&rb, &l);
    let unresolved: Vec<&Diagnostic> = r
        .state()
        .as_slice()
        .iter()
        .filter(|d| d.code() == Code::ApprovalUnresolved)
        .collect();
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].rules()[0].revision.map(|n| n.get()), Some(2));
    assert!(r.state().has_errors());
}

#[test]
fn unreferenced_candidate_clauses_get_one_warning_each_at_their_span() {
    let rb = rulebook(&[d1("d.a", "a")]);
    let l = ledger(&[approve("d.a", 1, hash_of(&rb, 0))]);
    let m = model();
    let clause = |needle: &str| {
        let start = TEXT.find(needle).unwrap() as u32;
        CandidateClause::new(&m, start, start + needle.len() as u32, None).unwrap()
    };
    let clauses = [clause(CLAUSE1), clause(CLAUSE2)];
    let r = validate(&rb, &l, &sources(), Some(&clauses));
    assert_eq!(codes_of(&r), [Code::CandidateUnreferenced]);
    let d = &r.state().as_slice()[0];
    assert_eq!(d.severity(), Severity::Warning);
    assert_eq!(d.primary(), &Location::Source(clauses[1].span().clone()));
    assert!(r.state().no_errors());
    // Without a candidate list the check does not run.
    assert!(codes_of(&validate(&rb, &l, &sources(), None)).is_empty());
}
