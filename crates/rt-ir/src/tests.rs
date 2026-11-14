use super::*;
use rt_foundation::{DocumentId, RevisionNumber, RuleId};
use rt_hash::Sha256Hex;
use rt_json::Position;
use rt_numeric::{parse_literal, RoundingMode};

fn rid(s: &str) -> RuleId {
    RuleId::new(s).unwrap()
}

fn key(id: &str, rev: u32) -> RuleKey {
    RuleKey {
        id: rid(id),
        revision: RevisionNumber::new(rev).unwrap(),
    }
}

fn expr(ty: Type, kind: ExprKind) -> Expr {
    Expr {
        loc: Loc::NONE,
        ty: Some(ty),
        kind,
    }
}

fn span(start: u32, end: u32) -> Span {
    Span {
        loc: Loc::NONE,
        document: DocumentId::new("rules.md").unwrap(),
        start,
        end,
        line: 3,
        column: 1,
    }
}

/// The `elig.basic` rule of design §4.1.7.
fn elig_basic(rev: u32, origin: Origin) -> RuleRevision {
    let age = expr(
        Type::Number,
        ExprKind::Field {
            name: Name::new("age").unwrap(),
        },
    );
    let sixteen = expr(
        Type::Number,
        ExprKind::Literal(Literal::Number(parse_literal("16").unwrap())),
    );
    let ge = expr(
        Type::Boolean,
        ExprKind::Cmp {
            op: CmpOp::Ge,
            left: Box::new(age),
            right: Box::new(sixteen),
        },
    );
    let licence = expr(
        Type::Boolean,
        ExprKind::Field {
            name: Name::new("licence").unwrap(),
        },
    );
    RuleRevision {
        loc: Loc::NONE,
        key: key("elig.basic", rev),
        origin,
        content: RuleContent {
            target: Name::new("eligible").unwrap(),
            ty: Some(Type::Boolean),
            body: RuleBody::Eligibility {
                expr: expr(Type::Boolean, ExprKind::And(vec![ge, licence])),
            },
            markers: vec![],
            spans: vec![span(21, 91)],
        },
    }
}

fn manual() -> Origin {
    Origin::Manual { author: None }
}

fn rulebook(revisions: Vec<RuleRevision>) -> Rulebook {
    Rulebook {
        ir_version: IrVersion::parse("1").unwrap(),
        name: "spring-open".to_owned(),
        version: "2026.1".to_owned(),
        documents: vec![DocumentDecl {
            loc: Loc::NONE,
            id: DocumentId::new("rules.md").unwrap(),
            sha256: Sha256Hex::parse(
                "eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a",
            )
            .unwrap(),
        }],
        types: TypeDecls::default(),
        revisions,
        precedence: vec![],
    }
}

#[test]
fn worked_example_rule_has_five_kinds_and_spans() {
    let rev = elig_basic(1, manual());
    assert_eq!(rev.content.body.kind_name(), "eligibility");
    assert_eq!(rev.content.spans.len(), 1);
    assert_eq!(IrVersion::V1.as_str(), "1");
    assert_eq!(SUPPORTED_IR_VERSIONS, ["1"]);
}

#[test]
fn origin_is_outside_rule_content() {
    let a = elig_basic(1, manual());
    let b = elig_basic(
        1,
        Origin::Manual {
            author: Some(Identity::new("sam").unwrap()),
        },
    );
    assert_ne!(a, b);
    assert_eq!(a.content, b.content);
    assert_eq!(a.key, b.key);
}

#[test]
fn positions_do_not_affect_equality() {
    let mut a = elig_basic(1, manual());
    let b = a.clone();
    a.loc = Loc::at(Position {
        byte: 10,
        line: 2,
        column: 3,
    });
    assert_eq!(a, b);
    assert_eq!(a.loc.position().map(|p| p.line), Some(2));
    assert_eq!(b.loc.position(), None);
}

#[test]
fn rules_group_revisions_in_key_order() {
    let rb = rulebook(vec![
        elig_basic(2, manual()),
        {
            let mut r = elig_basic(1, manual());
            r.key = key("a.first", 1);
            r
        },
        elig_basic(1, manual()),
    ]);
    let rules = rb.rules();
    let shape: Vec<(&str, Vec<u32>)> = rules
        .iter()
        .map(|r| {
            (
                r.id.as_str(),
                r.revisions.iter().map(|v| v.key.revision.get()).collect(),
            )
        })
        .collect();
    assert_eq!(
        shape,
        vec![("a.first", vec![1]), ("elig.basic", vec![1, 2])]
    );
    assert!(rb.revision(&key("elig.basic", 2)).is_some());
    assert!(rb.revision(&key("elig.basic", 3)).is_none());
}

#[test]
fn rule_key_orders_by_id_bytes_then_revision() {
    assert!(key("a", 9) < key("a-", 1));
    assert!(key("b", 1) < key("b", 2));
}

#[test]
fn unrecognized_kind_and_node_are_carried_opaquely() {
    let v = rt_json::parse(
        &rt_foundation::InputId::new("t.json").unwrap(),
        br#"{"window":  7, "x": [1, 2]}"#,
        rt_json::ParseLimits::new(1024, 8),
    )
    .unwrap();
    let a = OpaqueConstruct::from_text("sliding_window", &v).unwrap();
    assert_eq!(a.raw(), r#"{"window":7,"x":[1,2]}"#);
    let node = Expr {
        loc: Loc::NONE,
        ty: None,
        kind: ExprKind::Opaque(a.clone()),
    };
    let body = RuleBody::Unrecognized(a);
    assert_eq!(body.kind_name(), "sliding_window");
    assert!(node.ty.is_none());
    assert!(OpaqueConstruct::from_text("Bad-Name", &v).is_err());
}

#[test]
fn opaque_names_must_be_well_formed() {
    for ok in ["a", "x_1", "sliding_window"] {
        assert!(OpaqueName::new(ok).is_ok(), "{ok}");
    }
    for bad in ["", "1a", "A", "a-b", "a b", "é"] {
        assert_eq!(OpaqueName::new(bad), Err(NameError::NotWellFormed), "{bad}");
    }
}

#[test]
fn types_round_trip_through_text() {
    for t in ["number", "boolean", "text", "date", "ranking", "enum:Level"] {
        assert_eq!(
            Type::parse_declared(t).map(|x| x.text()).as_deref(),
            Some(t)
        );
    }
    for bad in ["", "error", "float", "enum:", "Number", "enum:a\nb"] {
        assert_eq!(Type::parse_declared(bad), None, "{bad:?}");
    }
}

#[test]
fn identity_counts_scalar_values() {
    assert_eq!(Identity::new(""), Err(IdentityError::Empty));
    assert!(Identity::new("é".repeat(200)).is_ok());
    assert_eq!(Identity::new("é".repeat(201)), Err(IdentityError::TooLong));
}

#[test]
fn dates_are_validated() {
    assert_eq!(Date::parse("2024-02-29").unwrap().to_string(), "2024-02-29");
    assert_eq!(Date::parse("2023-02-29"), Err(DateError::DayOutOfRange));
    assert_eq!(Date::parse("1900-02-29"), Err(DateError::DayOutOfRange));
    assert!(Date::parse("2000-02-29").is_ok());
    assert_eq!(Date::parse("2024-13-01"), Err(DateError::MonthOutOfRange));
    assert_eq!(Date::parse("0000-01-01"), Err(DateError::YearOutOfRange));
    for bad in [
        "2024-1-01",
        "24-01-01",
        "2024/01/01",
        "2024-01-0a",
        " 2024-01-01",
        "+024-01-01",
    ] {
        assert_eq!(Date::parse(bad), Err(DateError::BadShape), "{bad}");
    }
    assert!(Date::parse("2024-01-31").unwrap() < Date::parse("2024-02-01").unwrap());
}

#[test]
fn places_are_limited_to_28() {
    assert_eq!(Places::new(0).map(Places::get), Ok(0));
    assert_eq!(Places::new(28).map(Places::get), Ok(28));
    assert_eq!(Places::new(29), Err(PlacesError));
    assert_eq!(Places::new(u64::MAX), Err(PlacesError));
}

#[test]
fn operator_names_round_trip() {
    for n in ["add", "sub", "mul", "div"] {
        assert_eq!(BinaryOp::from_name(n).map(BinaryOp::name), Some(n));
    }
    for n in ["eq", "ne", "lt", "le", "gt", "ge"] {
        assert_eq!(CmpOp::from_name(n).map(CmpOp::name), Some(n));
    }
    for n in ["sum", "min", "max", "count"] {
        assert_eq!(AggFn::from_name(n).map(AggFn::name), Some(n));
    }
    assert_eq!(BinaryOp::from_name("pow"), None);
    assert_eq!(Direction::from_name("desc"), Some(Direction::Desc));
    assert_eq!(Direction::from_name("up"), None);
}

#[test]
fn weighted_score_and_ranking_bodies_build() {
    let w = expr(
        Type::Number,
        ExprKind::Literal(Literal::Number(parse_literal("0.6").unwrap())),
    );
    let v = expr(
        Type::Number,
        ExprKind::Field {
            name: Name::new("judges").unwrap(),
        },
    );
    let body = RuleBody::WeightedScore {
        terms: vec![WeightedTerm {
            loc: Loc::NONE,
            weight: w,
            value: v.clone(),
        }],
        rounding: Some(Rounding {
            mode: RoundingMode::HalfEven,
            places: Places::new(2).unwrap(),
        }),
    };
    assert_eq!(body.kind_name(), "weighted_score");
    let ranking = RuleBody::Ranking {
        key: v,
        direction: Direction::Desc,
        tie_breakers: vec![],
        ties: Ties::Group,
    };
    assert_eq!(ranking.kind_name(), "ranking");
}

/// Non-comment lines of the production sources of this crate.
fn code_lines() -> Vec<String> {
    [
        include_str!("expr.rs"),
        include_str!("loc.rs"),
        include_str!("names.rs"),
        include_str!("rule.rs"),
        include_str!("rulebook.rs"),
        include_str!("status.rs"),
        include_str!("types.rs"),
    ]
    .iter()
    .flat_map(|src| src.lines())
    .filter(|l| !l.trim_start().starts_with("//"))
    .map(str::to_owned)
    .collect()
}

#[test]
fn approval_status_cannot_be_built_from_data() {
    let code = code_lines();
    let joined = code.join("\n");
    for forbidden in [
        "for ApprovalStatus",
        "Deserialize",
        "FromStr",
        "-> ApprovalStatus",
        "-> Option<ApprovalStatus>",
        "-> Result<ApprovalStatus",
    ] {
        assert!(
            !joined.contains(forbidden),
            "found `{forbidden}` in rt-ir production source"
        );
    }
}

#[test]
fn approval_status_has_no_default() {
    let status: String = include_str!("status.rs")
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect();
    assert!(!status.contains("Default"));
}

#[test]
fn no_stored_approval_or_validity_field() {
    for line in code_lines() {
        for forbidden in [
            "approval_status",
            "approval_record",
            "is_approved",
            "is_rejected",
            "is_proposed",
            "has_errors",
            "no_errors",
            "validation_status",
            ": ApprovalStatus",
        ] {
            assert!(!line.contains(forbidden), "`{forbidden}` in line: {line}");
        }
    }
    assert_eq!(ApprovalStatus::Approved.name(), "Approved");
}

#[test]
fn production_code_has_no_float_type() {
    for line in code_lines() {
        assert!(!line.contains("f64") && !line.contains("f32"), "{line}");
    }
}
