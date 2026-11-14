use super::*;
use proptest::prelude::*;
use rt_foundation::{Code, DocumentId, InputId, RevisionNumber, RuleId};
use rt_hash::sha256;
use rt_ir::{AggFn, BinaryOp, CmpOp, Date, Direction, IrVersion, Name, Places, Rounding, Ties};
use rt_numeric::{parse_literal, RoundingMode};

fn name(s: &str) -> Name {
    Name::new(s).unwrap()
}

fn rid(s: &str) -> RuleId {
    RuleId::new(s).unwrap()
}

fn ni(n: u32) -> NodeIdx {
    NodeIdx::new(n)
}

fn doc_sha() -> rt_hash::Sha256Hex {
    sha256(b"the source document")
}

fn span() -> CompiledSpan {
    CompiledSpan {
        document: DocumentId::new("rules.md").unwrap(),
        sha256: doc_sha(),
        start: 21,
        end: 91,
        line: 3,
        column: 1,
    }
}

fn node(ty: CType, kind: NodeKind) -> CompiledNode {
    CompiledNode { ty, kind }
}

fn num(text: &str) -> NodeKind {
    NodeKind::Literal(CompiledLiteral::Number(
        rt_numeric::parse_canonical(text).unwrap(),
    ))
}

fn rule(
    id: &str,
    target: &str,
    ty: CType,
    nodes: Vec<CompiledNode>,
    body: CompiledBody,
) -> CompiledRule {
    CompiledRule {
        id: rid(id),
        revision: RevisionNumber::new(2).unwrap(),
        target: name(target),
        ty,
        spans: vec![span()],
        nodes,
        body,
    }
}

/// Targets are `eligible`, `extra`, `in_set`, `licence`, `ranking`, `score`, in that order.
const T_RANKING: u32 = 4;
const T_SCORE: u32 = 5;

fn sample_parts() -> CompiledParts {
    use CType::{Boolean, Date as D, Number, Ranking, Text};
    let level = CType::Enum(EnumIdx::new(0));
    let default = rule(
        "default.licence",
        "licence",
        Boolean,
        vec![],
        CompiledBody::Default {
            value: CompiledLiteral::Boolean(false),
        },
    );
    let elig = rule(
        "elig.basic",
        "eligible",
        Boolean,
        vec![
            node(Number, NodeKind::Field(InputIdx::new(0))),
            node(Number, num("16")),
            node(
                Boolean,
                NodeKind::Cmp {
                    op: CmpOp::Ge,
                    left: ni(0),
                    right: ni(1),
                },
            ),
            node(Boolean, NodeKind::Field(InputIdx::new(1))),
            node(Boolean, NodeKind::And(vec![ni(2), ni(3)])),
        ],
        CompiledBody::Eligibility { expr: ni(4) },
    );
    let extra = rule(
        "extra.calc",
        "extra",
        Number,
        vec![
            node(
                Number,
                NodeKind::Agg {
                    func: AggFn::Count,
                    collection: CollectionIdx::new(0),
                    field: None,
                },
            ),
            node(
                Number,
                NodeKind::Round {
                    arg: ni(0),
                    rounding: Rounding {
                        mode: RoundingMode::TowardZero,
                        places: Places::new(0).unwrap(),
                    },
                },
            ),
            node(Number, NodeKind::RankReference(TargetIdx::new(T_RANKING))),
            node(
                Number,
                NodeKind::Binary {
                    op: BinaryOp::Add,
                    left: ni(1),
                    right: ni(2),
                },
            ),
        ],
        CompiledBody::Derived { expr: ni(3) },
    );
    let rank = rule(
        "rank.main",
        "ranking",
        Ranking,
        vec![
            node(Number, NodeKind::Ref(TargetIdx::new(T_SCORE))),
            node(Number, NodeKind::Field(InputIdx::new(2))),
        ],
        CompiledBody::Ranking {
            key: ni(0),
            direction: Direction::Desc,
            tie_breakers: vec![CompiledTieBreaker {
                expr: ni(1),
                direction: Direction::Asc,
            }],
            ties: Ties::Group,
        },
    );
    let score = rule(
        "score.weighted",
        "score",
        Number,
        vec![
            node(Number, num("0.6")),
            node(Number, NodeKind::Field(InputIdx::new(2))),
            node(Number, num("1/3")),
            node(
                Number,
                NodeKind::Agg {
                    func: AggFn::Sum,
                    collection: CollectionIdx::new(0),
                    field: Some(FieldIdx::new(0)),
                },
            ),
        ],
        CompiledBody::WeightedScore {
            terms: vec![
                CompiledTerm {
                    weight: ni(0),
                    value: ni(1),
                },
                CompiledTerm {
                    weight: ni(2),
                    value: ni(3),
                },
            ],
            rounding: Some(Rounding {
                mode: RoundingMode::HalfEven,
                places: Places::new(2).unwrap(),
            }),
        },
    );
    let set = rule(
        "set.check",
        "in_set",
        Boolean,
        vec![
            node(
                Text,
                NodeKind::Literal(CompiledLiteral::Text("nz \"q\"\n".into())),
            ),
            node(
                Boolean,
                NodeKind::In {
                    value: ni(0),
                    set: SetIdx::new(0),
                },
            ),
            node(
                level,
                NodeKind::Literal(CompiledLiteral::Enum {
                    enumeration: EnumIdx::new(0),
                    member: MemberIdx::new(1),
                }),
            ),
            node(level, NodeKind::Field(InputIdx::new(3))),
            node(
                Boolean,
                NodeKind::Cmp {
                    op: CmpOp::Eq,
                    left: ni(3),
                    right: ni(2),
                },
            ),
            node(Boolean, NodeKind::Or(vec![ni(1), ni(4)])),
            node(D, NodeKind::Field(InputIdx::new(4))),
            node(
                D,
                NodeKind::Literal(CompiledLiteral::Date(Date::new(2010, 1, 1).unwrap())),
            ),
            node(
                Boolean,
                NodeKind::Cmp {
                    op: CmpOp::Lt,
                    left: ni(6),
                    right: ni(7),
                },
            ),
            node(Boolean, NodeKind::Not(ni(8))),
            node(Boolean, NodeKind::And(vec![ni(5), ni(9)])),
        ],
        CompiledBody::Eligibility { expr: ni(10) },
    );
    let field = |n: &str, ty| CompiledField { name: name(n), ty };
    let input = |n: &str, ty| CompiledInput { name: name(n), ty };
    CompiledParts {
        compiler_version: "0.1.0".into(),
        ir_version: IrVersion::V1,
        rulebook_hash: sha256(b"rulebook"),
        documents: vec![CompiledDocument {
            id: DocumentId::new("rules.md").unwrap(),
            sha256: doc_sha(),
        }],
        inputs: vec![
            input("age", Number),
            input("licence", Boolean),
            input("judges", Number),
            input("level", level),
            input("born", D),
        ],
        collections: vec![CompiledCollection {
            name: name("results"),
            fields: vec![field("place", Number), field("label", Text)],
        }],
        enums: vec![CompiledEnum {
            name: name("level"),
            members: vec![name("novice"), name("expert")],
        }],
        sets: vec![CompiledSet {
            name: name("countries"),
            element_type: Text,
            members: vec![
                CompiledLiteral::Text("nz".into()),
                CompiledLiteral::Text("au".into()),
            ],
        }],
        order: [0u32, 1, 4, 3, 2, 5].map(RuleIdx::new).to_vec(),
        rules: vec![default, elig, extra, rank, score, set],
        precedence: vec![CompiledPrecedence {
            id: rid("prec.elig"),
            target: TargetIdx::new(0),
            order: vec![RuleIdx::new(1)],
            spans: vec![span()],
        }],
    }
}

fn sample() -> CompiledRulebook {
    CompiledRulebook::from_parts(sample_parts()).unwrap()
}

fn input_id() -> InputId {
    InputId::new("compiled.json").unwrap()
}

fn parse_text(text: &str) -> Result<CompiledRulebook, rt_foundation::DiagnosticState> {
    parse_compiled(text.as_bytes(), &input_id(), 1 << 24, 64)
}

fn codes(text: &str) -> Vec<Code> {
    parse_text(text)
        .unwrap_err()
        .as_slice()
        .iter()
        .map(rt_foundation::Diagnostic::code)
        .collect()
}

#[test]
fn serialize_parse_serialize_is_byte_identical() {
    let rb = sample();
    for text in [rb.to_canonical_compact(), rb.to_canonical_pretty()] {
        let parsed = parse_text(&text).unwrap();
        assert_eq!(parsed, rb);
        assert_eq!(parsed.to_canonical_compact(), rb.to_canonical_compact());
        assert_eq!(parsed.to_canonical_pretty(), rb.to_canonical_pretty());
    }
}

#[test]
fn top_level_keys_are_in_the_design_order() {
    let compact = sample().to_canonical_compact();
    let keys = [
        "format",
        "compiler_version",
        "ir_version",
        "rulebook_hash",
        "documents",
        "inputs",
        "collections",
        "enums",
        "sets",
        "order",
        "rules",
        "precedence",
    ];
    let mut at = 0;
    for key in keys {
        let found = compact[at..].find(&format!("\"{key}\":")).unwrap();
        at += found;
    }
    assert!(
        compact.starts_with("{\"format\":\"ruletrace-compiled\",\"compiler_version\":\"0.1.0\"")
    );
    assert!(compact.contains("\"order\":[\"default.licence\",\"elig.basic\","));
    assert!(compact.contains("\"value\":\"1/3\""));
}

#[test]
fn nothing_about_origin_approval_or_validity_is_written() {
    let compact = sample().to_canonical_compact();
    for key in [
        "origin",
        "approval",
        "has_errors",
        "no_errors",
        "validation",
        "is_approved",
    ] {
        assert!(!compact.contains(key), "{key}");
    }
}

#[test]
fn every_flag_and_lifecycle_key_is_rejected_with_a_dgn_code() {
    let compact = sample().to_canonical_compact();
    let cases = [
        ("has_errors", Code::DiagnosticStateContradiction),
        ("no_errors", Code::DiagnosticStateContradiction),
        ("validation_status", Code::DiagnosticStateContradiction),
        ("approval_status", Code::LifecycleFlagContradiction),
        ("is_approved", Code::LifecycleFlagContradiction),
        ("is_rejected", Code::LifecycleFlagContradiction),
        ("is_proposed", Code::LifecycleFlagContradiction),
        ("approval_record", Code::LifecycleFlagContradiction),
    ];
    for (key, code) in cases {
        // Once at the top level and once inside a rule.
        let top = compact.replacen("{\"format\"", &format!("{{\"{key}\":false,\"format\""), 1);
        assert_eq!(codes(&top), vec![code], "{key} at top");
        let inner = compact.replacen(
            "{\"id\":\"elig.basic\"",
            &format!("{{\"{key}\":null,\"id\":\"elig.basic\""),
            1,
        );
        assert_eq!(codes(&inner), vec![code], "{key} in rule");
    }
    let both = compact.replacen(
        "{\"format\"",
        "{\"has_errors\":false,\"no_errors\":true,\"format\"",
        1,
    );
    assert_eq!(codes(&both), vec![Code::DiagnosticStateBothFlags]);
}

#[test]
fn an_origin_key_is_an_unsupported_name() {
    let compact = sample().to_canonical_compact();
    let text = compact.replacen(
        "{\"id\":\"elig.basic\"",
        "{\"origin\":{\"kind\":\"manual\",\"author\":null},\"id\":\"elig.basic\"",
        1,
    );
    let err = parse_text(&text).unwrap_err();
    assert_eq!(err.as_slice().len(), 1);
    assert_eq!(err.as_slice()[0].code(), Code::IrUnknownField);
}

#[test]
fn version_faults_have_their_own_codes() {
    let compact = sample().to_canonical_compact();
    let unsupported = compact.replacen("\"ir_version\":\"1\"", "\"ir_version\":\"2\"", 1);
    assert_eq!(codes(&unsupported), vec![Code::IrVersionUnsupported]);
    let missing = compact.replacen("\"ir_version\":\"1\",", "", 1);
    assert_eq!(codes(&missing), vec![Code::IrVersionMissingOrRepeated]);
    let wrong_format = compact.replacen("ruletrace-compiled", "ruletrace-rulebook", 1);
    assert_eq!(codes(&wrong_format), vec![Code::IrFieldInvalid]);
}

#[test]
fn a_bare_json_number_is_not_an_exact_number() {
    let compact = sample().to_canonical_compact();
    let text = compact.replacen("\"value\":\"16\"", "\"value\":16", 1);
    assert_eq!(codes(&text), vec![Code::IrNumericLiteralInvalid]);
    let text = compact.replacen("\"value\":\"16\"", "\"value\":\"1e3\"", 1);
    assert_eq!(codes(&text), vec![Code::IrNumericLiteralInvalid]);
}

#[test]
fn unknown_keys_and_node_kinds_report_their_position() {
    let compact = sample().to_canonical_compact();
    let text = compact.replacen(
        "\"idx\":1,\"node\":\"num\"",
        "\"idx\":1,\"node\":\"script\"",
        1,
    );
    assert_eq!(codes(&text), vec![Code::IrUnknownField]);
    let text = compact.replacen("\"column\":1}", "\"column\":1,\"extra\":1}", 1);
    let err = parse_text(&text).unwrap_err();
    assert!(err
        .as_slice()
        .iter()
        .all(|d| d.code() == Code::IrUnknownField));
    let pretty = sample().to_canonical_pretty().replacen(
        "\"column\": 1\n",
        "\"column\": 1,\n\"extra\": 1\n",
        1,
    );
    let err = parse_text(&pretty).unwrap_err();
    match err.as_slice()[0].primary() {
        rt_foundation::Location::Input(l) => assert!(l.line.is_some() && l.column.is_some()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_wrong_node_idx_or_forward_child_is_rejected() {
    let compact = sample().to_canonical_compact();
    let text = compact.replacen(
        "\"idx\":1,\"node\":\"num\"",
        "\"idx\":7,\"node\":\"num\"",
        1,
    );
    assert_eq!(codes(&text), vec![Code::IrFieldInvalid]);
    // `cmp` at index 2 now reads node 3, which comes later.
    let text = compact.replacen("\"left\":0,\"right\":1", "\"left\":0,\"right\":3", 1);
    assert_eq!(codes(&text), vec![Code::IrFieldInvalid]);
}

fn expect_kind(
    mut parts: CompiledParts,
    f: impl FnOnce(&mut CompiledParts),
    kind: CompiledErrorKind,
) {
    f(&mut parts);
    let err = CompiledRulebook::from_parts(parts).unwrap_err();
    assert_eq!(err.kind, kind, "{err}");
}

#[test]
fn from_parts_checks_the_invariants() {
    use CompiledErrorKind as K;
    assert!(CompiledRulebook::from_parts(sample_parts()).is_ok());
    expect_kind(sample_parts(), |p| p.rules.clear(), K::NoRules);
    expect_kind(sample_parts(), |p| p.rules.swap(0, 1), K::NotCanonicalOrder);
    expect_kind(sample_parts(), |p| p.rules[1].spans.clear(), K::BadSpan);
    expect_kind(sample_parts(), |p| p.rules[1].spans[0].end = 21, K::BadSpan);
    expect_kind(
        sample_parts(),
        |p| p.rules[1].spans[0].document = DocumentId::new("other.md").unwrap(),
        K::DocumentMismatch,
    );
    expect_kind(
        sample_parts(),
        |p| {
            p.documents.push(CompiledDocument {
                id: DocumentId::new("zzz.md").unwrap(),
                sha256: doc_sha(),
            })
        },
        K::DocumentMismatch,
    );
    expect_kind(
        sample_parts(),
        |p| {
            p.order.pop();
        },
        K::BadOrder,
    );
    expect_kind(
        sample_parts(),
        |p| p.order[0] = RuleIdx::new(1),
        K::BadOrder,
    );
    expect_kind(
        sample_parts(),
        |p| p.rules[1].nodes[0].kind = NodeKind::Field(InputIdx::new(99)),
        K::BadIndex,
    );
    expect_kind(
        sample_parts(),
        |p| p.rules[1].nodes[1].ty = CType::Text,
        K::TypeMismatch,
    );
    expect_kind(
        sample_parts(),
        |p| p.rules[1].body = CompiledBody::Eligibility { expr: ni(99) },
        K::BadIndex,
    );
    expect_kind(
        sample_parts(),
        |p| p.rules[1].nodes[4].kind = NodeKind::And(vec![ni(2)]),
        K::BadArity,
    );
    expect_kind(
        sample_parts(),
        |p| p.rules[1].nodes[2].kind = NodeKind::Ref(TargetIdx::new(99)),
        K::BadIndex,
    );
    expect_kind(
        sample_parts(),
        |p| p.precedence[0].order = vec![RuleIdx::new(0)],
        K::BadPrecedence,
    );
    expect_kind(
        sample_parts(),
        |p| {
            p.inputs.push(CompiledInput {
                name: name("age"),
                ty: CType::Number,
            })
        },
        K::NotCanonicalOrder,
    );
}

#[test]
fn targets_are_the_sorted_distinct_rule_targets() {
    let rb = sample();
    let names: Vec<&str> = rb.targets().iter().map(Name::as_str).collect();
    assert_eq!(
        names,
        ["eligible", "extra", "in_set", "licence", "ranking", "score"]
    );
    assert_eq!(
        rb.target(TargetIdx::new(T_SCORE)).unwrap().as_str(),
        "score"
    );
}

#[test]
fn two_rulebooks_do_not_share_state() {
    let a = sample();
    let mut parts = sample_parts();
    parts.compiler_version = "9.9.9".into();
    let b = CompiledRulebook::from_parts(parts).unwrap();
    let before = a.to_canonical_compact();
    drop(b);
    assert_eq!(a.to_canonical_compact(), before);
}

fn arb_text() -> impl Strategy<Value = String> {
    proptest::collection::vec(any::<char>(), 0..12).prop_map(|c| c.into_iter().collect())
}

proptest! {
    /// **Validates: Requirements 9.4, 9.5**
    #[test]
    fn serialize_parse_serialize_is_stable_for_generated_rulebooks(
        n in -1_000_000_000i64..1_000_000_000,
        d in 1i64..1_000_000,
        label in arb_text(),
        places in 0u64..=28,
        mode in 0usize..3,
        revision in 1u32..=u32::MAX,
    ) {
        let value = rt_numeric::div(
            &parse_literal(&n.to_string()).unwrap(),
            &parse_literal(&d.to_string()).unwrap(),
        ).unwrap();
        let mode = [RoundingMode::HalfEven, RoundingMode::HalfAwayFromZero, RoundingMode::TowardZero][mode];
        let mut parts = sample_parts();
        if let Some(r) = parts.rules.get_mut(4) {
            r.revision = RevisionNumber::new(revision).unwrap();
            r.nodes[0].kind = NodeKind::Literal(CompiledLiteral::Number(value));
            r.body = CompiledBody::WeightedScore {
                terms: vec![CompiledTerm { weight: ni(0), value: ni(1) }],
                rounding: Some(Rounding { mode, places: Places::new(places).unwrap() }),
            };
        }
        if let Some(r) = parts.rules.get_mut(5) {
            r.nodes[0].kind = NodeKind::Literal(CompiledLiteral::Text(label));
        }
        let rb = CompiledRulebook::from_parts(parts).unwrap();
        let text = rb.to_canonical_compact();
        let parsed = parse_text(&text).unwrap();
        prop_assert_eq!(&parsed, &rb);
        prop_assert_eq!(parsed.to_canonical_compact(), text);
        prop_assert_eq!(parse_text(&rb.to_canonical_pretty()).unwrap().to_canonical_pretty(), rb.to_canonical_pretty());
    }
}
