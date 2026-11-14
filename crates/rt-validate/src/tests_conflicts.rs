//! Tests of pass 9: conflict detection and precedence validation.
//!
//! The ledger has no append function, so these tests give every revision of a
//! Rulebook as Effective to `conflicts::check` directly.

use super::*;
use rt_foundation::Severity;
use rt_ir::Precedence;

fn default_rule(id: &str, target: &str, value: &str, ty: &str) -> String {
    rule(
        id,
        MANUAL,
        target,
        ("default", ty),
        &format!(r#""value":{value}"#),
        "",
        &span_json(CLAUSE1),
    )
}

fn ranking(id: &str, target: &str) -> String {
    rule(
        id,
        MANUAL,
        target,
        ("ranking", "ranking"),
        &format!(
            r#""key":{},"direction":"desc","tie_breakers":[],"ties":"group""#,
            field("age", "number")
        ),
        "",
        &span_json(CLAUSE2),
    )
}

fn d(id: &str, target: &str) -> String {
    derived(id, target, &num("1"), &span_json(CLAUSE2))
}

fn decl(id: &str, target: &str, order: &[&str]) -> Precedence {
    Precedence {
        loc: Loc::NONE,
        id: RuleId::new(id).unwrap(),
        target: rt_ir::Name::new(target).unwrap(),
        order: order.iter().map(|r| RuleId::new(*r).unwrap()).collect(),
        spans: vec![rulebook(&[d("x", "x")]).revisions[0].content.spans[0].clone()],
    }
}

fn run_pass(rb: &Rulebook) -> (Vec<ConflictClass>, DiagnosticState) {
    let effective: BTreeMap<RuleId, RuleKey> = rb
        .revisions
        .iter()
        .map(|r| (r.key.id.clone(), r.key.clone()))
        .collect();
    let mut scratch = DiagnosticState::empty();
    let srcs = sources();
    let decls = Decls::new(rb, &srcs, &BTreeSet::new(), &mut scratch);
    let mut out = DiagnosticState::empty();
    let classes = conflicts::check(rb, &effective, &decls, &mut out);
    (classes, out.sorted())
}

fn codes_of(s: &DiagnosticState) -> Vec<Code> {
    s.as_slice().iter().map(Diagnostic::code).collect()
}

fn rule_ids(d: &Diagnostic) -> Vec<&str> {
    d.rules().iter().map(|r| r.rule.as_str()).collect()
}

#[test]
fn duplicate_target_names_both_rules_and_selects_no_winner() {
    let rb = rulebook(&[d("d.b", "score"), d("d.a", "score"), d("d.c", "other")]);
    let (classes, state) = run_pass(&rb);
    assert_eq!(
        classes,
        [
            ConflictClass::ContradictoryDefault,
            ConflictClass::DuplicateRankKey,
            ConflictClass::DuplicateTarget,
            ConflictClass::InputShadowing
        ]
    );
    let all = state.as_slice();
    assert_eq!(all.len(), 1, "{all:?}");
    let c = &all[0];
    assert_eq!(c.code(), Code::ValConflictDuplicateTarget);
    assert_eq!(c.severity(), Severity::Error);
    assert_eq!(rule_ids(c), ["d.a", "d.b"]);
    assert_eq!(source_start(c), TEXT.find(CLAUSE2).unwrap() as u32);
    assert_eq!(c.related().len(), 1);
    assert!(c.message().contains("'d.a', 'd.b'") && c.message().contains("DuplicateTarget"));
}

#[test]
fn duplicate_rank_key_and_default_classes() {
    let rb = rulebook(&[
        ranking("r.1", "order"),
        ranking("r.2", "order"),
        default_rule("df.1", "age", &num("18"), "number"),
        default_rule("df.2", "age", &num("18"), "number"),
        default_rule(
            "df.3",
            "name",
            r#"{"node":"text","t":"text","value":"a"}"#,
            "text",
        ),
        default_rule(
            "df.4",
            "name",
            r#"{"node":"text","t":"text","value":"b"}"#,
            "text",
        ),
    ]);
    let (_, state) = run_pass(&rb);
    let got: Vec<(Code, Vec<&str>)> = state
        .as_slice()
        .iter()
        .map(|x| (x.code(), rule_ids(x)))
        .collect();
    assert_eq!(got.len(), 3, "{got:?}");
    assert!(got.contains(&(Code::ValConflictDuplicateRankKey, vec!["r.1", "r.2"])));
    // Equal literals: DuplicateTarget. Unequal literals: ContradictoryDefault.
    assert!(got.contains(&(Code::ValConflictDuplicateTarget, vec!["df.1", "df.2"])));
    assert!(got.contains(&(Code::ValConflictContradictoryDefault, vec!["df.3", "df.4"])));
}

#[test]
fn precedence_listing_every_rule_resolves_with_a_warning() {
    let mut rb = rulebook(&[d("d.a", "score"), d("d.b", "score")]);
    rb.precedence
        .push(decl("prec.score", "score", &["d.b", "d.a"]));
    let (_, state) = run_pass(&rb);
    let all = state.as_slice();
    assert_eq!(codes_of(&state), [Code::ValConflictResolved]);
    assert_eq!(all[0].severity(), Severity::Warning);
    assert_eq!(rule_ids(&all[0]), ["d.a", "d.b"]);
    assert!(all[0].message().contains("prec.score"));
    assert_eq!(source_start(&all[0]), TEXT.find(CLAUSE2).unwrap() as u32);

    // A declaration that omits one rule of the conflict does not resolve it.
    let mut rb = rulebook(&[d("d.a", "score"), d("d.b", "score"), d("d.c", "score")]);
    rb.precedence
        .push(decl("prec.score", "score", &["d.b", "d.a"]));
    assert_eq!(
        codes_of(&run_pass(&rb).1),
        [Code::ValConflictDuplicateTarget]
    );
}

#[test]
fn input_shadowing_is_never_resolved_by_precedence() {
    let mut rb = rulebook(&[d("d.age", "age")]);
    rb.precedence.push(decl("prec.age", "age", &["d.age"]));
    let (_, state) = run_pass(&rb);
    let all = state.as_slice();
    assert_eq!(codes_of(&state), [Code::ValConflictInputShadowing]);
    assert_eq!(rule_ids(&all[0]), ["d.age"]);
    assert!(all[0].message().contains("'age'"));
}

#[test]
fn precedence_cycle_and_non_effective_rule_give_0012_and_resolve_nothing() {
    let mut rb = rulebook(&[d("d.a", "score"), d("d.b", "score")]);
    rb.precedence.push(decl("p.1", "score", &["d.a", "d.b"]));
    rb.precedence.push(decl("p.2", "score", &["d.b", "d.a"]));
    let (_, state) = run_pass(&rb);
    let all = state.as_slice();
    assert_eq!(
        codes_of(&state),
        [
            Code::ValConflictDuplicateTarget,
            Code::ValPrecedenceInvalid,
            Code::ValPrecedenceInvalid
        ]
        .iter()
        .copied()
        .filter(|c| all.iter().any(|d| d.code() == *c))
        .collect::<Vec<_>>()
    );
    let invalid: Vec<&Diagnostic> = all
        .iter()
        .filter(|x| x.code() == Code::ValPrecedenceInvalid)
        .collect();
    assert_eq!(invalid.len(), 2);
    assert!(invalid.iter().all(|x| rule_ids(x) == ["d.a", "d.b"]));
    assert!(all
        .iter()
        .any(|x| x.code() == Code::ValConflictDuplicateTarget));

    // A rule that is not Effective (here: absent), listed in ascending order of
    // identifier.
    let mut rb = rulebook(&[d("d.a", "score"), d("d.b", "score")]);
    rb.precedence
        .push(decl("p.3", "score", &["d.b", "zz.gone", "d.a", "aa.gone"]));
    let (_, state) = run_pass(&rb);
    let inv: Vec<&Diagnostic> = state
        .as_slice()
        .iter()
        .filter(|x| x.code() == Code::ValPrecedenceInvalid)
        .collect();
    assert_eq!(inv.len(), 1);
    assert_eq!(rule_ids(inv[0]), ["aa.gone", "zz.gone"]);
    assert!(inv[0].message().contains("p.3"));
    assert!(state
        .as_slice()
        .iter()
        .any(|x| x.code() == Code::ValConflictDuplicateTarget));
}

#[test]
fn duplicate_rule_in_one_order_is_invalid() {
    let mut rb = rulebook(&[d("d.a", "score"), d("d.b", "score")]);
    rb.precedence
        .push(decl("p.dup", "score", &["d.a", "d.b", "d.a"]));
    let (_, state) = run_pass(&rb);
    assert!(codes_of(&state).contains(&Code::ValPrecedenceInvalid));
    assert!(codes_of(&state).contains(&Code::ValConflictDuplicateTarget));
}

#[test]
fn non_effective_revisions_do_not_conflict_and_all_classes_are_recorded() {
    // With an empty ledger nothing is Effective, so two same-Target rules give
    // no conflict, and the checked classes are still recorded.
    let r = check(&[d("d.a", "score"), d("d.b", "score")]);
    assert!(!codes(&r).contains(&Code::ValConflictDuplicateTarget));
    assert_eq!(r.checked_conflict_classes().len(), 4);
    assert!(r.checked_conflict_classes().windows(2).all(|w| w[0] < w[1]));
}
