//! Property 12: Rule_Content_Hash sensitivity (design §3.8.2,
//! §4.1.7).
//!
//! A revision and its declared documents are generated, then exactly one part
//! of the Rule_Content is changed: a span offset (start or end), a declared
//! document hash, an operand, a rounding declaration (places or mode), a
//! marker, the Revision_Number or the Target. The Rule_Content_Hash must change
//! every time, and the Draft hash must change for every change except the
//! Revision_Number (which the Draft hash excludes by design).
//!
//! The IR_Version is not mutated: `IrVersion` has the single variant `V1` in
//! this build, so no second version can be constructed. The hash input writes
//! `ir_version` first (see `hash.rs`), and the property over the other parts is
//! the one the approval gate relies on today. The "existing approval record
//! becomes non-matching" half of Property 12 belongs to the ledger and status
//! derivation (`rt-ledger`); a record binds this hash, so a changed hash is the
//! precondition it needs.
//!
//! **Validates: Requirements 20.15, 25.4, 25.5, 25.6, 25.8, 25.22, 26.9, 5.6,
//! 5.15**

use super::*;
use proptest::collection::vec;
use proptest::prelude::*;
use rt_foundation::{DocumentId, RevisionNumber, RuleId};
use rt_hash::Sha256Hex;
use rt_numeric::{parse_literal, RoundingMode};

const DOCS: [&str; 2] = ["a.md", "b.md"];

fn mk(ty: Type, kind: ExprKind) -> Expr {
    Expr {
        loc: Loc::NONE,
        ty: Some(ty),
        kind,
    }
}

fn num(v: i32) -> Expr {
    let n = parse_literal(&v.to_string()).unwrap();
    mk(Type::Number, ExprKind::Literal(Literal::Number(n)))
}

fn hash_of(seed: u8) -> Sha256Hex {
    Sha256Hex::parse(&format!("{seed:02x}").repeat(32)).unwrap()
}

fn mode_of(i: u8) -> RoundingMode {
    match i % 3 {
        0 => RoundingMode::HalfEven,
        1 => RoundingMode::HalfAwayFromZero,
        _ => RoundingMode::TowardZero,
    }
}

/// The next mode after `mode_of(i)`, computed on the reduced index so it
/// cannot overflow and always differs from the original.
fn different_mode(i: u8) -> RoundingMode {
    mode_of(i % 3 + 1)
}

/// Everything the property varies, so a failing case prints a readable
/// counterexample.
#[derive(Clone, Debug)]
struct Case {
    hashes: [u8; 2],
    revision: u32,
    weight: i32,
    value: i32,
    places: u64,
    mode: u8,
    markers: Vec<String>,
    spans: Vec<(usize, u32, u32)>,
    ranking: bool,
}

fn case() -> impl Strategy<Value = Case> {
    (
        (any::<u8>(), any::<u8>()),
        1u32..1000,
        -1000i32..1000,
        -1000i32..1000,
        0u64..=27,
        any::<u8>(),
        vec("\\PC{0,8}", 0..3),
        vec((0usize..2, 0u32..100_000, 0u32..500), 1..4),
        any::<bool>(),
    )
        .prop_map(
            |((h0, h1), revision, weight, value, places, mode, markers, spans, ranking)| Case {
                hashes: [h0, h1],
                revision,
                weight,
                value,
                places,
                mode,
                markers,
                spans: spans
                    .into_iter()
                    .map(|(d, start, len)| (d, start, start + len))
                    .collect(),
                ranking,
            },
        )
}

struct Built {
    documents: Vec<DocumentDecl>,
    revision: RuleRevision,
}

fn build(c: &Case) -> Built {
    let documents = DOCS
        .iter()
        .zip(c.hashes)
        .map(|(id, h)| DocumentDecl {
            loc: Loc::NONE,
            id: DocumentId::new(*id).unwrap(),
            sha256: hash_of(h),
        })
        .collect();
    let rounding = Rounding {
        mode: mode_of(c.mode),
        places: Places::new(c.places).unwrap(),
    };
    let body = if c.ranking {
        RuleBody::Ranking {
            key: num(c.value),
            direction: Direction::Desc,
            tie_breakers: vec![TieBreaker {
                loc: Loc::NONE,
                expr: num(c.weight),
                direction: Direction::Asc,
            }],
            ties: Ties::Group,
        }
    } else {
        RuleBody::WeightedScore {
            terms: vec![WeightedTerm {
                loc: Loc::NONE,
                weight: num(c.weight),
                value: num(c.value),
            }],
            rounding: Some(rounding),
        }
    };
    let markers = c
        .markers
        .iter()
        .map(|t| Marker::Incomplete {
            missing: vec![t.clone()],
        })
        .collect();
    let spans = c
        .spans
        .iter()
        .map(|&(d, start, end)| Span {
            loc: Loc::NONE,
            document: DocumentId::new(DOCS[d]).unwrap(),
            start,
            end,
            line: 1,
            column: 1,
        })
        .collect();
    Built {
        documents,
        revision: RuleRevision {
            loc: Loc::NONE,
            key: RuleKey {
                id: RuleId::new("r.one").unwrap(),
                revision: RevisionNumber::new(c.revision).unwrap(),
            },
            origin: Origin::Manual { author: None },
            content: RuleContent {
                target: Name::new("score").unwrap(),
                ty: Some(Type::Number),
                body,
                markers,
                spans,
            },
        },
    }
}

fn content_hash(b: &Built) -> Sha256Hex {
    rule_content_hash(&IrVersion::V1, &b.revision, &b.documents).unwrap()
}

fn draft(b: &Built) -> Sha256Hex {
    let r = &b.revision;
    draft_hash(&IrVersion::V1, &r.key.id, &r.content, &b.documents).unwrap()
}

fn bump(n: i32) -> i32 {
    if n == i32::MAX {
        n - 1
    } else {
        n + 1
    }
}

/// Changes exactly one part. `which` selects it; `idx` picks a span or term
/// where relevant. Returns whether the change is expected to alter the Draft
/// hash too.
fn mutate(b: &mut Built, c: &Case, which: u8, idx: usize) -> bool {
    let content = &mut b.revision.content;
    let span_i = idx % content.spans.len();
    match which % 8 {
        0 => content.spans[span_i].start += 1,
        1 => content.spans[span_i].end += 1,
        2 => {
            let doc = content.spans[span_i].document.clone();
            // XOR with 1 always differs from the declared hash byte.
            for (d, h) in b.documents.iter_mut().zip(c.hashes) {
                if d.id == doc {
                    d.sha256 = hash_of(h ^ 0x01);
                }
            }
        }
        3 => match &mut content.body {
            RuleBody::WeightedScore { terms, .. } => terms[0].value = num(bump(c.value)),
            RuleBody::Ranking { key, .. } => *key = num(bump(c.value)),
            _ => unreachable!("the generator builds only these bodies"),
        },
        4 => match &mut content.body {
            RuleBody::WeightedScore { rounding, .. } => {
                let r = rounding.as_mut().unwrap();
                if c.places.is_multiple_of(2) {
                    r.places = Places::new(if c.places == 28 { 27 } else { c.places + 1 }).unwrap();
                } else {
                    let next = different_mode(c.mode);
                    assert_ne!(next, r.mode, "mutation must pick another mode");
                    assert_ne!(next, mode_of(c.mode));
                    r.mode = next;
                }
            }
            // A ranking has no rounding. Change its tie-breaker operand
            // instead.
            RuleBody::Ranking { tie_breakers, .. } => {
                tie_breakers[0].expr = num(bump(c.weight));
            }
            _ => unreachable!("the generator builds only these bodies"),
        },
        5 => match content.markers.first_mut() {
            Some(Marker::Incomplete { missing }) => missing[0].push('x'),
            Some(Marker::Ambiguous { .. }) => unreachable!("the generator builds Incomplete only"),
            None => content.markers.push(Marker::Ambiguous {
                interpretations: vec!["a".to_owned(), "b".to_owned()],
            }),
        },
        6 => {
            let next = b.revision.key.revision.get() + 1;
            b.revision.key.revision = RevisionNumber::new(next).unwrap();
            return false;
        }
        _ => content.target = Name::new("score2").unwrap(),
    }
    true
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// **Validates: Requirements 20.15, 25.4, 25.5, 25.6, 25.8, 25.22, 26.9,
    /// 5.6, 5.15**
    #[test]
    fn changing_one_part_changes_the_rule_content_hash(
        c in case(),
        which in 0u8..8,
        idx in 0usize..8,
    ) {
        let base = build(&c);
        let before = content_hash(&base);
        let draft_before = draft(&base);
        // Hashing is stable for equal inputs.
        prop_assert_eq!(&before, &content_hash(&build(&c)));

        let mut changed = build(&c);
        let draft_changes = mutate(&mut changed, &c, which, idx);
        prop_assert_ne!(&content_hash(&changed), &before);
        if draft_changes {
            prop_assert_ne!(&draft(&changed), &draft_before);
        } else {
            // The Revision_Number is outside the Draft hash (design §3.8.2).
            prop_assert_eq!(&draft(&changed), &draft_before);
        }
    }
}

/// Regression: `mode + 1` overflowed a `u8` for mode 255 (shrunk case in
/// `proptest-regressions/hash_sensitivity_tests.txt`).
#[test]
fn rounding_mode_change_for_mode_255_changes_the_hash() {
    let c = Case {
        hashes: [0, 0],
        revision: 1,
        weight: 0,
        value: 0,
        places: 11,
        mode: 255,
        markers: vec![],
        spans: vec![(0, 0, 0)],
        ranking: false,
    };
    let before = content_hash(&build(&c));
    let mut changed = build(&c);
    assert!(mutate(&mut changed, &c, 4, 0));
    match &changed.revision.content.body {
        RuleBody::WeightedScore { rounding, .. } => {
            let r = rounding.as_ref().unwrap();
            assert_eq!(r.mode, RoundingMode::HalfAwayFromZero);
            assert_eq!(r.places, Places::new(11).unwrap());
        }
        _ => panic!("expected a weighted score"),
    }
    assert_ne!(content_hash(&changed), before);
}

#[test]
fn different_mode_differs_from_mode_of_for_every_u8() {
    for i in 0..=u8::MAX {
        assert_ne!(different_mode(i), mode_of(i), "mode index {i}");
    }
}
