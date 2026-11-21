//! Property 3: Rulebook serialization round trip (task 4.11).
//!
//! The parser keeps document order, the printer sorts revisions by (identifier,
//! revision) and documents by (identifier, hash), and `Rulebook` equality is
//! order-sensitive. The generator therefore builds revisions and documents in
//! an arbitrary (shuffled) construction order, and the comparison is made
//! against the canonical-order form of the generated Rulebook
//! (`canonical_order`). Types, precedence, spans and every other list keep
//! their order, which the printer preserves.
//!
//! **Validates: Requirements 3.10, 3.11, 4.5, 4.6**

use super::*;
use crate::proptest_floor::property_tests;
use proptest::collection::vec;
use proptest::prelude::*;
use rt_foundation::{DocumentId, InputId, RevisionNumber, RuleId};
use rt_hash::Sha256Hex;
use rt_numeric::{parse_literal, RoundingMode};

fn input() -> InputId {
    InputId::new("rulebook.json").unwrap()
}

fn parse(text: &str) -> Rulebook {
    match parse_rulebook(text.as_bytes(), &input(), 8 << 20, 64) {
        Ok(s) => s.into_parts().0,
        Err(r) => panic!(
            "printed Rulebook was rejected: {:?}\n{text}",
            r.into_diagnostics().as_slice()
        ),
    }
}

/// The Rulebook with revisions sorted by key and documents by (identifier,
/// hash): the order the printer writes and the parser then reads back.
fn canonical_order(rb: &Rulebook) -> Rulebook {
    let mut out = rb.clone();
    out.revisions.sort_by(|a, b| a.key.cmp(&b.key));
    out.documents
        .sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.sha256.cmp(&b.sha256)));
    out
}

// ---- generators ----

fn name() -> impl Strategy<Value = Name> {
    "[a-z][a-z0-9_]{0,6}".prop_map(|s| Name::new(s).unwrap())
}

fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        "\\PC{0,10}",
        Just("quote \" back \\ tab \t nl \n ctl \u{1}".to_owned()),
        Just("naïve — 日本 😀".to_owned()),
    ]
}

fn hash() -> impl Strategy<Value = Sha256Hex> {
    "[0-9a-f]{64}".prop_map(|s| Sha256Hex::parse(&s).unwrap())
}

fn identity() -> impl Strategy<Value = Identity> {
    "\\PC{1,12}".prop_map(|s| Identity::new(s).unwrap())
}

fn ty() -> impl Strategy<Value = Type> {
    prop_oneof![
        Just(Type::Number),
        Just(Type::Boolean),
        Just(Type::Text),
        Just(Type::Date),
        Just(Type::Ranking),
        name().prop_map(Type::Enum),
    ]
}

fn number() -> impl Strategy<Value = rt_numeric::ExactNumber> {
    "-?[0-9]{1,12}(\\.[0-9]{1,12})?".prop_map(|s| parse_literal(&s).unwrap())
}

fn date() -> impl Strategy<Value = Date> {
    (1u32..=9999, 1u32..=12, 1u32..=28).prop_map(|(y, m, d)| Date::new(y, m, d).unwrap())
}

fn literal() -> impl Strategy<Value = Literal> {
    prop_oneof![
        number().prop_map(Literal::Number),
        any::<bool>().prop_map(Literal::Boolean),
        text().prop_map(Literal::Text),
        date().prop_map(Literal::Date),
        (name(), name()).prop_map(|(enum_name, member)| Literal::Enum { enum_name, member }),
    ]
}

fn literal_type(l: &Literal) -> Type {
    match l {
        Literal::Number(_) => Type::Number,
        Literal::Boolean(_) => Type::Boolean,
        Literal::Text(_) => Type::Text,
        Literal::Date(_) => Type::Date,
        Literal::Enum { enum_name, .. } => Type::Enum(enum_name.clone()),
    }
}

fn mode() -> impl Strategy<Value = RoundingMode> {
    prop_oneof![
        Just(RoundingMode::HalfEven),
        Just(RoundingMode::HalfAwayFromZero),
        Just(RoundingMode::TowardZero),
    ]
}

fn places() -> impl Strategy<Value = Places> {
    (0u64..=28).prop_map(|p| Places::new(p).unwrap())
}

fn rounding() -> impl Strategy<Value = Rounding> {
    (mode(), places()).prop_map(|(mode, places)| Rounding { mode, places })
}

fn mk(ty: Option<Type>, kind: ExprKind) -> Expr {
    Expr {
        loc: Loc::NONE,
        ty,
        kind,
    }
}

fn expr() -> impl Strategy<Value = Expr> {
    let opt_ty = proptest::option::weighted(0.9, ty()).boxed();
    let leaf = prop_oneof![
        literal().prop_map(|l| mk(Some(literal_type(&l)), ExprKind::Literal(l))),
        (opt_ty.clone(), name()).prop_map(|(t, name)| mk(t, ExprKind::Field { name })),
        (opt_ty.clone(), name()).prop_map(|(t, target)| mk(t, ExprKind::Ref { target })),
        (opt_ty.clone(), name()).prop_map(|(t, target)| mk(t, ExprKind::RankReference { target })),
        (
            opt_ty.clone(),
            prop_oneof![
                Just(AggFn::Sum),
                Just(AggFn::Min),
                Just(AggFn::Max),
                Just(AggFn::Count)
            ],
            name(),
            proptest::option::of(name()),
        )
            .prop_map(|(t, func, collection, field)| mk(
                t,
                ExprKind::Agg {
                    func,
                    collection,
                    field
                }
            )),
    ];
    leaf.prop_recursive(4, 24, 3, move |inner| {
        let ty = opt_ty.clone();
        prop_oneof![
            (
                ty.clone(),
                prop_oneof![
                    Just(BinaryOp::Add),
                    Just(BinaryOp::Sub),
                    Just(BinaryOp::Mul),
                    Just(BinaryOp::Div)
                ],
                inner.clone(),
                inner.clone()
            )
                .prop_map(|(t, op, l, r)| mk(
                    t,
                    ExprKind::Binary {
                        op,
                        left: Box::new(l),
                        right: Box::new(r)
                    }
                )),
            (
                ty.clone(),
                prop_oneof![
                    Just(CmpOp::Eq),
                    Just(CmpOp::Ne),
                    Just(CmpOp::Lt),
                    Just(CmpOp::Le),
                    Just(CmpOp::Gt),
                    Just(CmpOp::Ge)
                ],
                inner.clone(),
                inner.clone()
            )
                .prop_map(|(t, op, l, r)| mk(
                    t,
                    ExprKind::Cmp {
                        op,
                        left: Box::new(l),
                        right: Box::new(r)
                    }
                )),
            (ty.clone(), vec(inner.clone(), 0..4)).prop_map(|(t, a)| mk(t, ExprKind::And(a))),
            (ty.clone(), vec(inner.clone(), 0..4)).prop_map(|(t, a)| mk(t, ExprKind::Or(a))),
            (ty.clone(), inner.clone()).prop_map(|(t, a)| mk(t, ExprKind::Not(Box::new(a)))),
            (ty.clone(), inner.clone(), name()).prop_map(|(t, v, set)| mk(
                t,
                ExprKind::In {
                    value: Box::new(v),
                    set
                }
            )),
            (ty, inner, places(), mode()).prop_map(|(t, a, places, mode)| mk(
                t,
                ExprKind::Round {
                    arg: Box::new(a),
                    places,
                    mode
                }
            )),
        ]
    })
}

fn span() -> impl Strategy<Value = Span> {
    (
        "[a-z]{1,6}\\.md",
        0u32..1_000_000,
        0u32..1_000,
        1u32..10_000,
        1u32..500,
    )
        .prop_map(|(doc, start, len, line, column)| Span {
            loc: Loc::NONE,
            document: DocumentId::new(doc).unwrap(),
            start,
            end: start + len,
            line,
            column,
        })
}

fn marker() -> impl Strategy<Value = Marker> {
    prop_oneof![
        vec(text(), 0..4).prop_map(|interpretations| Marker::Ambiguous { interpretations }),
        vec(text(), 0..4).prop_map(|missing| Marker::Incomplete { missing }),
    ]
}

fn origin() -> impl Strategy<Value = Origin> {
    prop_oneof![
        proptest::option::of(identity()).prop_map(|author| Origin::Manual { author }),
        (
            identity(),
            text(),
            text(),
            text(),
            hash(),
            hash(),
            text(),
            vec(identity(), 0..3)
        )
            .prop_map(
                |(
                    proposer,
                    provider,
                    model,
                    adapter_version,
                    request_hash,
                    response_hash,
                    proposal_id,
                    ai_identities,
                )| {
                    Origin::AiProposal(AiOrigin {
                        proposer,
                        provider,
                        model,
                        adapter_version,
                        request_hash,
                        response_hash,
                        proposal_id,
                        ai_identities,
                    })
                }
            ),
    ]
}

fn direction() -> impl Strategy<Value = Direction> {
    prop_oneof![Just(Direction::Asc), Just(Direction::Desc)]
}

fn body() -> impl Strategy<Value = RuleBody> {
    prop_oneof![
        expr().prop_map(|expr| RuleBody::Eligibility { expr }),
        expr().prop_map(|expr| RuleBody::Derived { expr }),
        (
            vec(
                (expr(), expr()).prop_map(|(weight, value)| WeightedTerm {
                    loc: Loc::NONE,
                    weight,
                    value
                }),
                0..3
            ),
            proptest::option::of(rounding())
        )
            .prop_map(|(terms, rounding)| RuleBody::WeightedScore { terms, rounding }),
        (
            expr(),
            direction(),
            vec(
                (expr(), direction()).prop_map(|(expr, direction)| TieBreaker {
                    loc: Loc::NONE,
                    expr,
                    direction
                }),
                0..3
            )
        )
            .prop_map(|(key, direction, tie_breakers)| RuleBody::Ranking {
                key,
                direction,
                tie_breakers,
                ties: Ties::Group
            }),
        literal().prop_map(|literal| RuleBody::Default {
            value: DefaultValue {
                loc: Loc::NONE,
                literal
            }
        }),
    ]
}

fn rule_id() -> impl Strategy<Value = RuleId> {
    // A small alphabet makes shared identifiers (several revisions of one rule)
    // common.
    "[a-c][a-c0-9_.-]{0,3}".prop_map(|s| RuleId::new(s).unwrap())
}

fn revision() -> impl Strategy<Value = RuleRevision> {
    (
        rule_id(),
        1u32..=5,
        origin(),
        name(),
        proptest::option::of(ty()),
        body(),
        vec(marker(), 0..3),
        vec(span(), 0..4),
    )
        .prop_map(
            |(id, rev, origin, target, ty, body, markers, spans)| RuleRevision {
                loc: Loc::NONE,
                key: RuleKey {
                    id,
                    revision: RevisionNumber::new(rev).unwrap(),
                },
                origin,
                content: RuleContent {
                    target,
                    ty,
                    body,
                    markers,
                    spans,
                },
            },
        )
}

fn types_decls() -> impl Strategy<Value = TypeDecls> {
    (
        vec(
            (name(), ty()).prop_map(|(name, ty)| InputDecl {
                loc: Loc::NONE,
                name,
                ty,
            }),
            0..4,
        ),
        vec(
            (name(), vec(name(), 0..4)).prop_map(|(name, members)| EnumDecl {
                loc: Loc::NONE,
                name,
                members,
            }),
            0..3,
        ),
        vec(
            (name(), ty(), vec(literal(), 0..4)).prop_map(|(name, element_type, members)| {
                SetDecl {
                    loc: Loc::NONE,
                    name,
                    element_type,
                    members,
                }
            }),
            0..3,
        ),
        vec(
            (
                name(),
                vec(
                    (name(), ty()).prop_map(|(name, ty)| Field {
                        loc: Loc::NONE,
                        name,
                        ty,
                    }),
                    0..4,
                ),
            )
                .prop_map(|(name, fields)| CollectionDecl {
                    loc: Loc::NONE,
                    name,
                    fields,
                }),
            0..3,
        ),
    )
        .prop_map(|(inputs, enums, sets, collections)| TypeDecls {
            inputs,
            enums,
            sets,
            collections,
        })
}

fn precedence() -> impl Strategy<Value = Precedence> {
    (rule_id(), name(), vec(rule_id(), 0..4), vec(span(), 0..3)).prop_map(
        |(id, target, order, spans)| Precedence {
            loc: Loc::NONE,
            id,
            target,
            order,
            spans,
        },
    )
}

/// Renumbers the revisions of each rule 1, 2, 3 ... (the parser rejects gaps),
/// in list order; the caller shuffles the numbered list afterwards.
fn number_revisions(revisions: Vec<RuleRevision>) -> Vec<RuleRevision> {
    let mut next: std::collections::BTreeMap<RuleId, u32> = std::collections::BTreeMap::new();
    revisions
        .into_iter()
        .map(|mut r| {
            let n = next.entry(r.key.id.clone()).or_insert(0);
            *n += 1;
            r.key.revision = RevisionNumber::new(*n).unwrap();
            r
        })
        .collect()
}

/// A Rulebook in arbitrary construction order: revisions are unique by key but
/// unsorted, documents are unique by identifier but unsorted.
fn rulebook() -> impl Strategy<Value = Rulebook> {
    (
        text(),
        text(),
        vec(("[a-z]{1,6}\\.md", hash()), 0..4),
        types_decls(),
        vec(revision(), 0..6)
            .prop_map(number_revisions)
            .prop_shuffle(),
        vec(precedence(), 0..3),
    )
        .prop_map(|(name, version, documents, types, revisions, precedence)| {
            let mut seen_docs = std::collections::BTreeSet::new();
            let documents = documents
                .into_iter()
                .filter(|(id, _)| seen_docs.insert(id.clone()))
                .map(|(id, sha256)| DocumentDecl {
                    loc: Loc::NONE,
                    id: DocumentId::new(id).unwrap(),
                    sha256,
                })
                .collect();
            Rulebook {
                ir_version: IrVersion::V1,
                name,
                version,
                documents,
                types,
                revisions,
                precedence,
            }
        })
}

// ---- properties ----

property_tests! {
    #![below_floor_config(ProptestConfig::with_cases(128))]

    /// **Validates: Requirements 3.10, 3.11, 4.5, 4.6**
    #[test]
    fn rulebook_serialization_round_trip(rb in rulebook()) {
        let canonical = canonical_order(&rb);
        for pretty in [true, false] {
            let print = |r: &Rulebook| {
                if pretty { print_pretty(r) } else { print_compact(r) }.unwrap()
            };
            let printed = print(&rb);
            prop_assert_eq!(&printed, &print(&rb));
            // Printing is independent of construction order.
            prop_assert_eq!(&printed, &print(&canonical));
            let reparsed = parse(&printed);
            prop_assert_eq!(&reparsed, &canonical);
            prop_assert_eq!(&print(&reparsed), &printed);
        }
    }

    /// For an already sorted Rulebook the round trip holds with plain equality.
    ///
    /// **Validates: Requirements 3.10, 4.6**
    #[test]
    fn sorted_rulebook_round_trips_exactly(rb in rulebook()) {
        let sorted = canonical_order(&rb);
        let reparsed = parse(&print_pretty(&sorted).unwrap());
        prop_assert_eq!(reparsed, sorted);
    }
}
