//! Tests of passes 1 to 8 (task 5.1).

#[path = "tests_approval.rs"]
mod approval_tests;
#[path = "tests_conflicts.rs"]
mod conflicts_tests;

use super::*;
use rt_foundation::{InputId, SourceSpan};
use rt_ir::{parse_rulebook, Expr, ExprKind, Loc, Type};
use rt_srcmodel::{DocumentKind, SourceModel};

const TEXT: &str = "# Spring Open Rules\n\n1. A competitor is eligible if age is at least 16 and holds a licence.\n2. The score is 0.6 times the judges score.\n";

fn span_json(needle: &str) -> String {
    let start = TEXT.find(needle).unwrap();
    let end = start + needle.len();
    let line = TEXT[..start].matches('\n').count() + 1;
    let col = TEXT[..start].rsplit('\n').next().unwrap().chars().count() + 1;
    format!(r#"{{"document":"rules.md","start":{start},"end":{end},"line":{line},"column":{col}}}"#)
}

const CLAUSE1: &str = "1. A competitor is eligible if age is at least 16 and holds a licence.";
const CLAUSE2: &str = "2. The score is 0.6 times the judges score.";

fn num(v: &str) -> String {
    format!(r#"{{"node":"num","t":"number","value":"{v}"}}"#)
}
fn field(name: &str, t: &str) -> String {
    format!(r#"{{"node":"field","t":"{t}","name":"{name}"}}"#)
}
fn rf(target: &str, t: &str) -> String {
    format!(r#"{{"node":"ref","t":"{t}","target":"{target}"}}"#)
}

const MANUAL: &str = r#"{"kind":"manual","author":null}"#;

fn rule(
    id: &str,
    origin: &str,
    target: &str,
    (kind, ty): (&str, &str),
    body: &str,
    markers: &str,
    spans: &str,
) -> String {
    format!(
        r#"{{"id":"{id}","revision":1,"origin":{origin},"kind":"{kind}","target":"{target}","type":"{ty}",{body},"markers":[{markers}],"spans":[{spans}]}}"#
    )
}

fn elig(id: &str, target: &str, expr: &str) -> String {
    rule(
        id,
        MANUAL,
        target,
        ("eligibility", "boolean"),
        &format!(r#""expr":{expr}"#),
        "",
        &span_json(CLAUSE1),
    )
}

fn derived(id: &str, target: &str, expr: &str, spans: &str) -> String {
    rule(
        id,
        MANUAL,
        target,
        ("derived", "number"),
        &format!(r#""expr":{expr}"#),
        "",
        spans,
    )
}

fn age_ge_16() -> String {
    format!(
        r#"{{"node":"cmp","t":"boolean","op":"ge","left":{},"right":{}}}"#,
        field("age", "number"),
        num("16")
    )
}

fn model() -> SourceModel {
    SourceModel::new(
        rt_foundation::DocumentId::new("rules.md").unwrap(),
        DocumentKind::Markdown,
        TEXT.to_owned(),
        Vec::new(),
    )
    .unwrap()
}

fn sources() -> SourceModels {
    let mut s = SourceModels::new();
    s.insert(model()).unwrap();
    s
}

fn rulebook(rules: &[String]) -> Rulebook {
    let hash = rt_hash::sha256(TEXT.as_bytes());
    let doc = format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"t","version":"1","documents":[{{"id":"rules.md","sha256":"{hash}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}},{{"name":"licence","type":"boolean"}},{{"name":"name","type":"text"}}],"enums":[],"sets":[],"collections":[{{"name":"marks","fields":[{{"name":"points","type":"number"}}]}}]}},"rules":[{}],"precedence":[]}}"#,
        rules.join(",")
    );
    match parse_rulebook(
        doc.as_bytes(),
        &InputId::new("rb.json").unwrap(),
        1 << 22,
        64,
    ) {
        Ok(s) => s.into_parts().0,
        Err(r) => panic!("fixture does not parse: {:?}", r.diagnostics().as_slice()),
    }
}

fn check(rules: &[String]) -> ValidationResult {
    validate_passes_1_to_9(&rulebook(rules), &ApprovalLedger::empty(), &sources())
}

fn codes(r: &ValidationResult) -> Vec<Code> {
    r.state().as_slice().iter().map(Diagnostic::code).collect()
}

fn source_start(d: &Diagnostic) -> u32 {
    match d.primary() {
        Location::Source(s) => s.start().get(),
        other => panic!("not a Source location: {other:?}"),
    }
}

#[test]
fn clean_rulebook_has_no_findings_and_records_dependencies() {
    let r = check(&[
        elig(
            "elig.basic",
            "eligible",
            &format!(
                r#"{{"node":"and","t":"boolean","args":[{},{}]}}"#,
                age_ge_16(),
                field("licence", "boolean")
            ),
        ),
        derived(
            "d.twice",
            "twice",
            &format!(
                r#"{{"node":"binary","t":"number","op":"mul","left":{},"right":{}}}"#,
                rf("base", "number"),
                num("2")
            ),
            &span_json(CLAUSE2),
        ),
        derived(
            "d.base",
            "base",
            &format!(
                r#"{{"node":"binary","t":"number","op":"add","left":{},"right":{{"node":"agg","t":"number","fn":"sum","collection":"marks","field":"points"}}}}"#,
                field("age", "number")
            ),
            &span_json(CLAUSE2),
        ),
    ]);
    assert!(r.state().as_slice().is_empty(), "{:?}", r.state());
    let meta = r.rule_metadata();
    let names = |m: &RuleMetadata| {
        m.input_fields()
            .iter()
            .map(|n| n.as_str().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&meta[&RuleId::new("elig.basic").unwrap()]),
        ["age", "licence"]
    );
    // Reached through the referenced rule, each field once.
    let twice = &meta[&RuleId::new("d.twice").unwrap()];
    assert_eq!(names(twice), ["age"]);
    assert_eq!(twice.collections()[0].as_str(), "marks");
    assert_eq!(r.checked_conflict_classes().len(), 4);
}

#[test]
fn unknown_name_type_mismatch_and_bad_span_give_one_diagnostic_each() {
    let bad_span = r#"{"document":"rules.md","start":10,"end":9999,"line":1,"column":11}"#;
    let r = check(&[
        // Unknown field: the comparison must not add a type finding for the same node.
        elig(
            "a.unknown",
            "ea",
            &format!(
                r#"{{"node":"cmp","t":"boolean","op":"ge","left":{},"right":{}}}"#,
                field("agee", "number"),
                num("16")
            ),
        ),
        // `name` is text, compared with a number.
        elig(
            "b.mismatch",
            "eb",
            &format!(
                r#"{{"node":"cmp","t":"boolean","op":"ge","left":{},"right":{}}}"#,
                field("name", "text"),
                num("16")
            ),
        ),
        derived("c.span", "c", &num("1"), bad_span),
    ]);
    assert_eq!(
        codes(&r)
            .iter()
            .filter(|c| **c != Code::ApprovalUnresolved)
            .count(),
        3,
        "{:?}",
        r.state()
    );
    let by_rule = |id: &str| -> Vec<&Diagnostic> {
        r.state()
            .as_slice()
            .iter()
            .filter(|d| d.rules().iter().any(|x| x.rule.as_str() == id))
            .collect()
    };
    let a = by_rule("a.unknown");
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].code(), Code::ValUndeclaredReference);
    assert!(a[0].message().contains("agee") && a[0].message().contains("field"));
    assert_eq!(source_start(a[0]), TEXT.find(CLAUSE1).unwrap() as u32);
    let b = by_rule("b.mismatch");
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].code(), Code::ValTypeMismatch);
    assert!(b[0].message().contains("number or date") && b[0].message().contains("text"));
    let c = by_rule("c.span");
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].code(), Code::ValSpanOutsideDocument);
    assert_eq!(
        c[0].primary(),
        &Location::Document(rt_foundation::DocumentId::new("rules.md").unwrap())
    );
}

#[test]
fn declared_type_that_differs_from_the_computed_type_is_one_finding_at_the_node() {
    // `cmp` declared as number.
    let e = format!(
        r#"{{"node":"cmp","t":"number","op":"ge","left":{},"right":{}}}"#,
        field("age", "number"),
        num("16")
    );
    let r = check(&[derived("d.x", "x", &e, &span_json(CLAUSE2))]);
    // Declared number at the node (it computes boolean) and the rule type number vs boolean body.
    let c = codes(&r);
    assert_eq!(
        c.iter().filter(|c| **c == Code::ValTypeMismatch).count(),
        2,
        "{:?}",
        r.state()
    );
}

#[test]
fn missing_source_model_and_hash_mismatch_are_reported_once_per_document() {
    let rb = rulebook(&[derived("d.x", "x", &num("1"), &span_json(CLAUSE2))]);
    let none = validate_passes_1_to_9(&rb, &ApprovalLedger::empty(), &SourceModels::new());
    let d: Vec<_> = none.state().as_slice().iter().collect();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].code(), Code::ValSourceModelMissing);
    assert_eq!(d[0].severity(), rt_foundation::Severity::Error);

    let other = SourceModel::new(
        rt_foundation::DocumentId::new("rules.md").unwrap(),
        DocumentKind::Markdown,
        "different".to_owned(),
        Vec::new(),
    )
    .unwrap();
    let mut s = SourceModels::new();
    s.insert(other).unwrap();
    let r = validate_passes_1_to_9(&rb, &ApprovalLedger::empty(), &s);
    assert_eq!(
        r.state()
            .as_slice()
            .iter()
            .map(Diagnostic::code)
            .collect::<Vec<_>>(),
        [Code::ValDocumentHashMismatch]
    );
}

#[test]
fn span_line_and_column_must_match_the_model() {
    let bad = span_json(CLAUSE2).replace(r#""column":1"#, r#""column":2"#);
    let r = check(&[derived("d.x", "x", &num("1"), &bad)]);
    assert_eq!(codes(&r), [Code::ValSpanLineColumnMismatch]);
}

#[test]
fn rule_without_span_and_undeclared_span_document() {
    let mut rb = rulebook(&[derived("d.x", "x", &num("1"), &span_json(CLAUSE2))]);
    rb.revisions[0].content.spans[0].document = rt_foundation::DocumentId::new("other.md").unwrap();
    let r = validate_passes_1_to_9(&rb, &ApprovalLedger::empty(), &sources());
    assert_eq!(codes(&r), [Code::ValSpanInvalid]);
    rb.revisions[0].content.spans.clear();
    let r = validate_passes_1_to_9(&rb, &ApprovalLedger::empty(), &sources());
    let d = &r.state().as_slice()[0];
    assert_eq!(d.code(), Code::ValRuleNoSourceSpan);
    assert_eq!(d.primary(), &Location::None(Code::ValRuleNoSourceSpan));
}

#[test]
fn missing_expression_type_is_pass_1_and_suppresses_nothing_else() {
    let mut rb = rulebook(&[derived("d.x", "x", &num("1"), &span_json(CLAUSE2))]);
    if let rt_ir::RuleBody::Derived { expr } = &mut rb.revisions[0].content.body {
        expr.ty = None;
    }
    let r = validate_passes_1_to_9(&rb, &ApprovalLedger::empty(), &sources());
    assert_eq!(codes(&r), [Code::ValMissingTypeDeclaration]);
    rb.revisions[0].content.ty = None;
    let r = validate_passes_1_to_9(&rb, &ApprovalLedger::empty(), &sources());
    assert_eq!(r.state().as_slice().len(), 2);
}

#[test]
fn reference_cycle_is_one_diagnostic_in_order_of_reference() {
    let r = check(&[
        derived("r.b", "b", &rf("a", "number"), &span_json(CLAUSE2)),
        derived("r.a", "a", &rf("b", "number"), &span_json(CLAUSE1)),
        derived("r.self", "s", &rf("s", "number"), &span_json(CLAUSE1)),
    ]);
    let cycles: Vec<_> = r
        .state()
        .as_slice()
        .iter()
        .filter(|d| d.code() == Code::ValReferenceCycle)
        .collect();
    assert_eq!(cycles.len(), 2, "{:?}", r.state());
    let two = cycles.iter().find(|d| d.rules().len() == 2).unwrap();
    let ids: Vec<_> = two.rules().iter().map(|x| x.rule.as_str()).collect();
    assert_eq!(ids, ["r.a", "r.b"]);
    assert_eq!(source_start(two), TEXT.find(CLAUSE1).unwrap() as u32);
    assert_eq!(two.related().len(), 1);
    let own = cycles.iter().find(|d| d.rules().len() == 1).unwrap();
    assert_eq!(own.rules()[0].rule.as_str(), "r.self");
}

#[test]
fn markers_report_every_interpretation_and_missing_element_in_order() {
    let body = r#""expr":{"node":"bool","t":"boolean","value":true}"#;
    let markers = r#"{"kind":"ambiguous","interpretations":["first","second"]},{"kind":"incomplete","missing":["cap","date"]}"#;
    let r = check(&[rule(
        "m.x",
        MANUAL,
        "m",
        ("eligibility", "boolean"),
        body,
        markers,
        &span_json(CLAUSE1),
    )]);
    let amb = r
        .state()
        .as_slice()
        .iter()
        .find(|d| d.code() == Code::ValAmbiguousInterpretation)
        .unwrap();
    assert!(amb.message().find("'first'").unwrap() < amb.message().find("'second'").unwrap());
    assert_eq!(amb.severity(), rt_foundation::Severity::Ambiguity);
    let inc = r
        .state()
        .as_slice()
        .iter()
        .find(|d| d.code() == Code::ValIncompleteInterpretation)
        .unwrap();
    assert!(inc.message().contains("'cap', 'date'"));
    assert_eq!(r.state().as_slice().len(), 2);

    let one = r#"{"kind":"ambiguous","interpretations":["only"]}"#;
    let r = check(&[rule(
        "m.y",
        MANUAL,
        "m",
        ("eligibility", "boolean"),
        body,
        one,
        &span_json(CLAUSE1),
    )]);
    assert_eq!(codes(&r), [Code::ValUnsupportedConstruct]);
}

#[test]
fn unrecognized_kind_and_node_are_unsupported_with_a_capability_name() {
    let kind = r#"{"id":"u.kind","revision":1,"origin":{"kind":"manual","author":null},"kind":"seeding","target":"x","type":"number","markers":[],"spans":[SP]}"#
        .replace("SP", &span_json(CLAUSE1));
    let node = derived(
        "u.node",
        "y",
        r#"{"node":"rank_reference","t":"number","target":"rank"}"#,
        &span_json(CLAUSE2),
    );
    let other = derived(
        "u.other",
        "z",
        r#"{"node":"foo_bar","t":"number"}"#,
        &span_json(CLAUSE2),
    );
    let r = check(&[kind, node, other]);
    let msgs: Vec<_> = r
        .state()
        .as_slice()
        .iter()
        .filter(|d| d.code() == Code::ValUnsupportedConstruct)
        .map(|d| d.message().to_owned())
        .collect();
    assert_eq!(msgs.len(), 3, "{:?}", r.state());
    assert!(msgs.iter().any(|m| m.contains("seeding")));
    assert!(msgs.iter().any(|m| m.contains("rank reference")));
    assert!(msgs
        .iter()
        .any(|m| m.contains("unrecognized construct 'foo_bar'")));
    assert_eq!(
        r.state()
            .as_slice()
            .iter()
            .filter(|d| d.severity() == rt_foundation::Severity::Unsupported)
            .count(),
        3
    );
}

#[test]
fn origin_does_not_change_any_finding() {
    let ai = r#"{"kind":"ai_proposal","proposer":"ai-1","provider":"mock","model":"m1","adapter_version":"0.1","request_hash":"eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a","response_hash":"eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a","proposal_id":"p-1","ai_identities":["ai-1"]}"#;
    let body = format!(r#""expr":{}"#, field("agee", "number"));
    let mk = |origin: &str| {
        let r = check(&[rule(
            "o.x",
            origin,
            "x",
            ("derived", "number"),
            &body,
            "",
            &span_json(CLAUSE1),
        )]);
        r.state().clone()
    };
    let (a, b) = (mk(MANUAL), mk(ai));
    assert_eq!(a.as_slice().len(), 1); // the unknown field
    assert_eq!(a, b);
}

#[test]
fn structural_only_revisions_report_only_pass_1() {
    let mut rb = rulebook(&[derived(
        "d.x",
        "x",
        &field("agee", "number"),
        &span_json(CLAUSE2),
    )]);
    if let rt_ir::RuleBody::Derived { expr } = &mut rb.revisions[0].content.body {
        expr.ty = None;
    }
    let sources = sources();
    let items = |full| {
        rb.revisions
            .iter()
            .map(|rev| Item {
                id: &rev.key.id,
                revision: Some(rev.key.revision),
                content: &rev.content,
                full,
                current: false,
                report: true,
            })
            .collect::<Vec<_>>()
    };
    let scope = Scope {
        rulebook_checks: true,
        report_docs: BTreeSet::new(),
        cycles_through: None,
    };
    let (structural, _) = run(&rb, &sources, &items(false), &scope);
    assert_eq!(
        structural
            .as_slice()
            .iter()
            .map(Diagnostic::code)
            .collect::<Vec<_>>(),
        [Code::ValMissingTypeDeclaration]
    );
    let (full, _) = run(&rb, &sources, &items(true), &scope);
    assert_eq!(full.as_slice().len(), 2);
}

#[test]
fn deep_library_built_expression_is_one_finding_and_does_not_overflow() {
    let mut rb = rulebook(&[elig(
        "n.deep",
        "n",
        r#"{"node":"bool","t":"boolean","value":true}"#,
    )]);
    let mut e = Expr {
        loc: Loc::NONE,
        ty: Some(Type::Boolean),
        kind: ExprKind::Literal(rt_ir::Literal::Boolean(true)),
    };
    for _ in 0..200 {
        e = Expr {
            loc: Loc::NONE,
            ty: Some(Type::Boolean),
            kind: ExprKind::Not(Box::new(e)),
        };
    }
    rb.revisions[0].content.body = rt_ir::RuleBody::Eligibility { expr: e };
    let r = validate_passes_1_to_9(&rb, &ApprovalLedger::empty(), &sources());
    assert_eq!(codes(&r), [Code::LimitExpressionTooDeep]);
}

#[test]
fn duplicate_revision_in_a_library_built_rulebook() {
    let mut rb = rulebook(&[derived("d.x", "x", &num("1"), &span_json(CLAUSE2))]);
    let again = rb.revisions[0].clone();
    rb.revisions.push(again);
    let r = validate_passes_1_to_9(&rb, &ApprovalLedger::empty(), &sources());
    assert_eq!(codes(&r), [Code::ValDuplicateRevision]);
}

#[test]
fn validate_content_reports_only_the_draft_with_a_verified_location() {
    let rb = rulebook(&[
        derived("other", "o", &field("agee", "number"), &span_json(CLAUSE2)),
        derived("base", "base", &num("1"), &span_json(CLAUSE2)),
    ]);
    let content = |expr: &str| {
        let r = rulebook(&[derived("draft", "dr", expr, &span_json(CLAUSE1))]);
        r.revisions[0].content.clone()
    };
    let draft = RuleDraft {
        id: RuleId::new("draft").unwrap(),
        content: content(&rf("base", "number")),
    };
    assert!(validate_content(&rb, &draft, &sources())
        .as_slice()
        .is_empty());

    let bad = RuleDraft {
        id: RuleId::new("draft").unwrap(),
        content: content(&rf("nope", "number")),
    };
    let state = validate_content(&rb, &bad, &sources());
    assert_eq!(state.as_slice().len(), 1);
    let d = &state.as_slice()[0];
    assert_eq!(d.code(), Code::ValUndeclaredReference);
    assert_eq!(source_start(d), TEXT.find(CLAUSE1).unwrap() as u32);
    assert!(
        matches!(d.primary(), Location::Source(s) if *s == SourceSpan::new(&model(), s.start().get(), s.end().get(), s.line(), s.column()).unwrap())
    );

    // A draft that closes a cycle with an existing rule is reported, an existing cycle is not.
    let cyc = RuleDraft {
        id: RuleId::new("base").unwrap(),
        content: content(&rf("dr", "number")),
    };
    let rb2 = rulebook(&[derived(
        "loop",
        "dr",
        &rf("base", "number"),
        &span_json(CLAUSE2),
    )]);
    let state = validate_content(&rb2, &cyc, &sources());
    assert_eq!(
        state
            .as_slice()
            .iter()
            .filter(|d| d.code() == Code::ValReferenceCycle)
            .count(),
        1
    );
}
