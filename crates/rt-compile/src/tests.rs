//! Tests of the Compiler.
//!
//! Ledgers are written by hand with their hash chain, as in the Validator
//! tests, because `rt-ledger` has no append function.

use super::*;
use rt_compiled::{CompiledBody, CompiledRule, CompiledSpan, NodeKind, TargetIdx};
use rt_foundation::{DocumentId, InputId, RuleId, Severity};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, rule_content_hash, Name};
use rt_json::ParseLimits;
use rt_ledger::{parse_ledger, ApprovalLedger};
use rt_srcmodel::{DocumentKind, SourceModel, SourceModels};

const TEXT: &str = "# Spring Open Rules\n\n1. A competitor is eligible if age is at least 16 and holds a licence.\n2. The score is 0.6 times the judges score.\n";
const CLAUSE1: &str = "1. A competitor is eligible if age is at least 16 and holds a licence.";
const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn span_json(needle: &str) -> String {
    let start = TEXT.find(needle).unwrap();
    let end = start + needle.len();
    let line = TEXT[..start].matches('\n').count() + 1;
    let col = TEXT[..start].rsplit('\n').next().unwrap().chars().count() + 1;
    format!(r#"{{"document":"rules.md","start":{start},"end":{end},"line":{line},"column":{col}}}"#)
}

fn num(v: &str) -> String {
    format!(r#"{{"node":"num","t":"number","value":"{v}"}}"#)
}

fn derived(id: &str, author: &str, target: &str, expr: &str) -> String {
    format!(
        r#"{{"id":"{id}","revision":1,"origin":{{"kind":"manual","author":{author}}},"kind":"derived","target":"{target}","type":"number","expr":{expr},"markers":[],"spans":[{}]}}"#,
        span_json(CLAUSE1)
    )
}

fn rulebook(rules: &[String]) -> Rulebook {
    let hash = sha256(TEXT.as_bytes());
    let doc = format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{hash}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{}],"precedence":[]}}"#,
        rules.join(",")
    );
    let parsed = parse_rulebook(
        doc.as_bytes(),
        &InputId::new("rb.json").unwrap(),
        1 << 22,
        64,
    );
    match parsed {
        Ok(s) => s.into_parts().0,
        Err(r) => panic!("fixture does not parse: {:?}", r.diagnostics().as_slice()),
    }
}

fn sources() -> SourceModels {
    let mut s = SourceModels::new();
    let model = SourceModel::new(
        DocumentId::new("rules.md").unwrap(),
        DocumentKind::Markdown,
        TEXT.to_owned(),
        Vec::new(),
    )
    .unwrap();
    s.insert(model).unwrap();
    s
}

/// `(rule, revision, is_rejection)` records for the revisions at the given
/// positions.
fn ledger(rb: &Rulebook, records: &[(usize, bool)]) -> ApprovalLedger {
    let doc = sha256(TEXT.as_bytes());
    let mut out = format!("{}\n", rt_ledger::HEADER_LINE);
    let mut prev = ZERO.to_owned();
    for (n, (index, rejection)) in records.iter().enumerate() {
        let rev = &rb.revisions[*index];
        let hash = rule_content_hash(&rb.ir_version, rev, &rb.documents).unwrap();
        let (kind, reason) = if *rejection {
            ("rejection", r#""not a rule""#)
        } else {
            ("approval", "null")
        };
        let base = format!(
            r#"{{"seq":{},"kind":"{kind}","rule":"{}","revision":{},"content_hash":"{hash}","ir_version":"1","binding":[{{"document":"rules.md","sha256":"{doc}"}}],"reviewer":"A. Reviewer","time":"2026-11-14T09:00:00Z","reason":{reason},"prev":"{prev}""#,
            n + 1,
            rev.key.id,
            rev.key.revision,
        );
        let chain = sha256(format!("{base}}}").as_bytes());
        out.push_str(&format!("{base},\"hash\":\"{chain}\"}}\n"));
        prev = chain.to_string();
    }
    let input = InputId::new("approvals.ledger").unwrap();
    parse_ledger(&input, out.as_bytes(), ParseLimits::new(1 << 20, 64)).unwrap()
}

fn validated(rb: &Rulebook, records: &[(usize, bool)]) -> ValidationResult {
    rt_validate::validate(rb, &ledger(rb, records), &sources(), None)
}

fn codes(d: &DiagnosticState) -> Vec<Code> {
    d.as_slice().iter().map(Diagnostic::code).collect()
}

fn ok(result: &ValidationResult) -> (CompiledRulebook, DiagnosticState) {
    match compile(result) {
        Ok(s) => s.into_parts(),
        Err(r) => panic!("expected success: {:?}", r.diagnostics().as_slice()),
    }
}

/// `r.a` reads the Target of `r.b`, so `r.b` must come first although `r.a`
/// sorts first.
fn two_rules(author_a: &str) -> Vec<String> {
    vec![
        derived(
            "r.a",
            author_a,
            "score",
            r#"{"node":"ref","t":"number","target":"bonus"}"#,
        ),
        derived("r.b", "null", "bonus", &num("1")),
    ]
}

#[test]
fn unresolved_revision_returns_its_diagnostics_and_no_rulebook() {
    let rb = rulebook(&two_rules("null"));
    let result = validated(&rb, &[(0, false)]);
    let blocking: Vec<Diagnostic> = result
        .state()
        .as_slice()
        .iter()
        .filter(|d| d.severity().is_blocking())
        .cloned()
        .collect();
    assert_eq!(codes(result.state()), vec![Code::ApprovalUnresolved]);
    let rejected = compile(&result).unwrap_err();
    assert_eq!(rejected.diagnostics().as_slice(), blocking.as_slice());
}

#[test]
fn no_effective_revision_returns_the_blocking_diagnostic() {
    let rb = rulebook(&two_rules("null"));
    let result = validated(&rb, &[(0, true), (1, true)]);
    let rejected = compile(&result).unwrap_err();
    assert!(codes(rejected.diagnostics()).contains(&Code::ApprovalNoExecutableRules));
    assert!(rejected.diagnostics().has_errors());
}

#[test]
fn approved_rules_compile_in_dependency_order_with_recorded_metadata() {
    let rb = rulebook(&two_rules("null"));
    let result = validated(&rb, &[(0, false), (1, false)]);
    let (c, warnings) = ok(&result);
    assert!(warnings.as_slice().is_empty());
    assert_eq!(c.rules().len(), 2);
    let ids: Vec<&str> = c
        .order()
        .iter()
        .map(|i| c.rule(*i).unwrap().id.as_str())
        .collect();
    assert_eq!(ids, ["r.b", "r.a"]);
    assert_eq!(c.compiler_version(), COMPILER_VERSION);
    assert_eq!(c.ir_version(), rt_ir::IrVersion::V1);
    assert_eq!(c.documents().len(), 1);
    assert_eq!(c.documents()[0].sha256, sha256(TEXT.as_bytes()));
    assert_eq!(c.rules()[0].spans.len(), 1);
    assert_eq!(c.rules()[0].spans[0].document.as_str(), "rules.md");
}

#[test]
fn independent_rules_are_ordered_by_identifier_whatever_the_input_order() {
    let a = vec![
        derived("z", "null", "tz", &num("1")),
        derived("b", "null", "tb", &num("2")),
        derived("m", "null", "tm", &num("3")),
    ];
    let mut b = a.clone();
    b.reverse();
    let (rb_a, rb_b) = (rulebook(&a), rulebook(&b));
    let all = |rb: &Rulebook| -> Vec<(usize, bool)> {
        (0..rb.revisions.len()).map(|i| (i, false)).collect()
    };
    let (ca, _) = ok(&validated(&rb_a, &all(&rb_a)));
    let (cb, _) = ok(&validated(&rb_b, &all(&rb_b)));
    let ids: Vec<&str> = ca
        .order()
        .iter()
        .map(|i| ca.rule(*i).unwrap().id.as_str())
        .collect();
    assert_eq!(ids, ["b", "m", "z"]);
    assert_eq!(ca.to_canonical_compact(), cb.to_canonical_compact());
    assert_eq!(ca.rulebook_hash(), cb.rulebook_hash());
}

#[test]
fn origin_and_rejected_revisions_do_not_change_the_output() {
    let plain = rulebook(&two_rules("null"));
    let (c1, _) = ok(&validated(&plain, &[(0, false), (1, false)]));

    // A different author, and one more rule that is Rejected and so not
    // Effective.
    let mut rules = two_rules(r#""Someone Else""#);
    rules.push(derived("r.c", "null", "other", &num("9")));
    let busy = rulebook(&rules);
    let result = validated(&busy, &[(0, false), (1, false), (2, true)]);
    let (c2, warnings) = ok(&result);
    assert!(codes(&warnings).contains(&Code::ApprovalRevisionRejected));
    assert_eq!(c1.to_canonical_compact(), c2.to_canonical_compact());
    assert_eq!(c1.rulebook_hash(), c2.rulebook_hash());
}

#[test]
fn a_change_of_effective_content_changes_the_hash() {
    let one = rulebook(&[derived("r.b", "null", "bonus", &num("1"))]);
    let two = rulebook(&[derived("r.b", "null", "bonus", &num("2"))]);
    let (c1, _) = ok(&validated(&one, &[(0, false)]));
    let (c2, _) = ok(&validated(&two, &[(0, false)]));
    assert_ne!(c1.rulebook_hash(), c2.rulebook_hash());
}

#[test]
fn compiled_output_parses_back_and_is_repeatable() {
    let rb = rulebook(&two_rules("null"));
    let result = validated(&rb, &[(0, false), (1, false)]);
    let (a, _) = ok(&result);
    let (b, _) = ok(&result);
    assert_eq!(a, b);
    let text = a.to_canonical_compact();
    assert!(rt_compiled::parse_compiled(
        text.as_bytes(),
        &InputId::new("c.json").unwrap(),
        1 << 22,
        64
    )
    .is_ok());
}

#[test]
fn recheck_detects_a_state_that_differs_in_blocking_diagnostics() {
    let rb = rulebook(&two_rules("null"));
    let unresolved = validated(&rb, &[(0, false)]);
    let approved = validated(&rb, &[(0, false), (1, false)]);
    assert!(agree(unresolved.state(), unresolved.state()));
    assert!(!agree(unresolved.state(), approved.state()));
    assert!(!agree(approved.state(), unresolved.state()));
    // Warnings and diagnostics of other passes do not take part.
    let mut extra = approved.state().clone();
    extra.push(Diagnostic::new(
        Code::ApprovalRecordStale,
        "x".to_owned(),
        Location::None(Code::ApprovalRecordStale),
    ));
    assert!(agree(approved.state(), &extra));
    assert_eq!(Code::ApprovalRecheckFailed.severity(), Severity::Error);
}

fn rule_with(id: &str, target: &str, refs: &[&str], doc: &str) -> CompiledRule {
    let sha = sha256(TEXT.as_bytes());
    let targets: Vec<&str> = vec!["a", "b", "c"];
    let nodes = refs
        .iter()
        .map(|r| {
            let t = targets.iter().position(|x| x == r).unwrap();
            rt_compiled::CompiledNode {
                ty: rt_compiled::CType::Number,
                kind: NodeKind::Ref(TargetIdx::new(t as u32)),
            }
        })
        .chain(std::iter::once(rt_compiled::CompiledNode {
            ty: rt_compiled::CType::Number,
            kind: NodeKind::Literal(rt_compiled::CompiledLiteral::Boolean(true)),
        }))
        .collect::<Vec<_>>();
    CompiledRule {
        id: RuleId::new(id).unwrap(),
        revision: rt_foundation::RevisionNumber::new(1).unwrap(),
        target: Name::new(target).unwrap(),
        ty: rt_compiled::CType::Number,
        spans: vec![CompiledSpan {
            document: DocumentId::new(doc).unwrap(),
            sha256: sha,
            start: 0,
            end: 1,
            line: 1,
            column: 1,
        }],
        nodes,
        body: CompiledBody::Derived {
            expr: rt_compiled::NodeIdx::new(0),
        },
    }
}

fn names() -> Vec<Name> {
    ["a", "b", "c"]
        .iter()
        .map(|n| Name::new(*n).unwrap())
        .collect()
}

#[test]
fn a_cycle_gives_one_diagnostic_naming_each_rule_once() {
    use crate::order::evaluation_order;
    // r1 -> r2 -> r3 -> r1 (a ref to a Target depends on its rule), and r0
    // stands apart.
    let rules = vec![
        rule_with("r0", "z", &[], "rules.md"),
        rule_with("r1", "a", &["b"], "rules.md"),
        rule_with("r2", "b", &["c"], "other.md"),
        rule_with("r3", "c", &["a"], "third.md"),
    ];
    let targets = vec![
        Name::new("a").unwrap(),
        Name::new("b").unwrap(),
        Name::new("c").unwrap(),
        Name::new("z").unwrap(),
    ];
    let diags = evaluation_order(&rules, &targets).unwrap_err();
    assert_eq!(diags.len(), 1);
    let d = &diags[0];
    assert_eq!(d.code(), Code::CmpDependencyCycle);
    assert_eq!(
        d.primary(),
        &Location::Document(DocumentId::new("rules.md").unwrap())
    );
    assert_eq!(
        d.related(),
        [
            Location::Document(DocumentId::new("other.md").unwrap()),
            Location::Document(DocumentId::new("third.md").unwrap())
        ]
    );
    let named: Vec<&str> = d.rules().iter().map(|r| r.rule.as_str()).collect();
    assert_eq!(named, ["r1", "r2", "r3"]);
    assert!(d.message().contains("'r1', 'r2', 'r3'"));
}

#[test]
fn two_separate_cycles_give_two_diagnostics_and_a_dependent_is_not_named() {
    use crate::order::evaluation_order;
    let rules = vec![
        rule_with("r1", "a", &["a"], "rules.md"),
        rule_with("r2", "b", &["c"], "rules.md"),
        rule_with("r3", "c", &["b"], "rules.md"),
        rule_with("r4", "z", &["a"], "rules.md"),
    ];
    let mut targets = names();
    targets.push(Name::new("z").unwrap());
    let diags = evaluation_order(&rules, &targets).unwrap_err();
    assert_eq!(diags.len(), 2);
    let sets: Vec<Vec<&str>> = diags
        .iter()
        .map(|d| d.rules().iter().map(|r| r.rule.as_str()).collect())
        .collect();
    assert_eq!(sets, [vec!["r1"], vec!["r2", "r3"]]);
}

#[test]
fn acyclic_rules_order_by_identifier_among_the_ready() {
    use crate::order::evaluation_order;
    let rules = vec![
        rule_with("r1", "a", &["b"], "rules.md"),
        rule_with("r2", "b", &[], "rules.md"),
        rule_with("r3", "c", &[], "rules.md"),
    ];
    let order = evaluation_order(&rules, &names()).unwrap();
    assert_eq!(order.iter().map(|i| i.get()).collect::<Vec<_>>(), [1, 0, 2]);
}
