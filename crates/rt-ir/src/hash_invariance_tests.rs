//! Property 13: Rule_Content_Hash invariance (task 4.14, design §5).
//!
//! Two Rulebook texts that carry the same Rule_Content but differ in everything
//! that is not content give the same Rule_Content hash and the same Draft hash:
//! the Origin, the span `line` and `column`, the order of the members of every
//! JSON object, the layout, unreferenced documents, other rules and the order
//! of the document list. Approval data and derived status are not inputs of the
//! hash functions at all (the hash takes only the IR_Version, one revision and
//! the declared documents), so no argument of them can change the result; the
//! in-memory part of the property also replaces the Origin and the span
//! positions directly.
//!
//! **Validates: Requirements 3.13, 25.3, 25.9**

use super::*;
use crate::proptest_floor::property_tests;
use proptest::collection::vec;
use proptest::prelude::*;
use rt_foundation::InputId;

const DOC_A: &str = "eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a";
const DOC_B: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const DOC_UNUSED: &str = "2222222222222222222222222222222222222222222222222222222222222222";

/// The content that the hash covers.
#[derive(Clone, Debug)]
struct Content {
    value: String,
    rev: u32,
    markers: Vec<String>,
    /// (use document B, start, length)
    spans: Vec<(bool, u32, u32)>,
}

/// Everything that must not change the hash.
#[derive(Clone, Debug)]
struct Noise {
    origin: String,
    positions: Vec<(u32, u32)>,
    seed: u64,
    extra_rule: bool,
    unused_document: bool,
    reverse_documents: bool,
}

fn content() -> impl Strategy<Value = Content> {
    (
        "-?[0-9]{1,6}(\\.[0-9]{1,4})?",
        1u32..=1,
        vec(
            prop_oneof![
                vec("[a-z ]{0,8}", 0..3).prop_map(|items| format!(
                    r#"{{"kind":"ambiguous","interpretations":[{}]}}"#,
                    quoted(&items)
                )),
                vec("[a-z ]{0,8}", 0..3).prop_map(|items| format!(
                    r#"{{"kind":"incomplete","missing":[{}]}}"#,
                    quoted(&items)
                )),
            ],
            0..3,
        ),
        vec((any::<bool>(), 0u32..100_000, 0u32..1_000), 1..4),
    )
        .prop_map(|(value, rev, markers, spans)| Content {
            value,
            rev,
            markers,
            spans,
        })
}

fn origin() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(r#"{"kind":"manual","author":null}"#.to_owned()),
        "[a-z]{1,8}".prop_map(|a| format!(r#"{{"kind":"manual","author":"{a}"}}"#)),
        ("[a-z]{1,8}", "[a-z]{1,8}", "[a-z0-9]{1,8}").prop_map(|(bot, model, id)| format!(
            r#"{{"kind":"ai_proposal","proposer":"{bot}","provider":"p","model":"{model}","adapter_version":"1","request_hash":"{DOC_A}","response_hash":"{DOC_B}","proposal_id":"{id}","ai_identities":["{bot}"]}}"#
        )),
    ]
}

fn noise() -> impl Strategy<Value = Noise> {
    (
        origin(),
        vec((1u32..10_000, 1u32..500), 4),
        any::<u64>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(origin, positions, seed, extra_rule, unused_document, reverse_documents)| Noise {
                origin,
                positions,
                seed,
                extra_rule,
                unused_document,
                reverse_documents,
            },
        )
}

fn quoted(items: &[String]) -> String {
    items
        .iter()
        .map(|i| format!("\"{i}\""))
        .collect::<Vec<_>>()
        .join(",")
}

/// Joins JSON members in an order given by `seed` (Fisher-Yates over a small
/// LCG). Seed 0 keeps the given (canonical) order.
fn members(mut items: Vec<String>, seed: u64) -> String {
    if seed != 0 {
        let mut state = seed;
        for i in (1..items.len()).rev() {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let j = usize::try_from((state >> 33) % (i as u64 + 1)).unwrap_or(0);
            items.swap(i, j);
        }
    }
    items.join(",")
}

fn rule_text(id: &str, c: &Content, n: &Noise, first: bool) -> String {
    let seed = if first { n.seed } else { n.seed.rotate_left(7) };
    let spans: Vec<String> = c
        .spans
        .iter()
        .enumerate()
        .map(|(i, (b, start, len))| {
            let (line, column) = n.positions[i % n.positions.len()];
            let doc = if *b { "annex.md" } else { "rules.md" };
            format!(
                "{{{}}}",
                members(
                    vec![
                        format!(r#""document":"{doc}""#),
                        format!(r#""start":{start}"#),
                        format!(r#""end":{}"#, start + len),
                        format!(r#""line":{line}"#),
                        format!(r#""column":{column}"#),
                    ],
                    seed.wrapping_add(i as u64)
                )
            )
        })
        .collect();
    let expr = format!(
        "{{{}}}",
        members(
            vec![
                r#""node":"cmp""#.to_owned(),
                r#""t":"boolean""#.to_owned(),
                r#""op":"ge""#.to_owned(),
                r#""left":{"name":"age","t":"number","node":"field"}"#.to_owned(),
                format!(
                    r#""right":{{"value":"{}","t":"number","node":"num"}}"#,
                    c.value
                ),
            ],
            seed.wrapping_add(31)
        )
    );
    format!(
        "{{{}}}",
        members(
            vec![
                format!(r#""id":"{id}""#),
                format!(r#""revision":{}"#, c.rev),
                format!(r#""origin":{}"#, n.origin),
                r#""kind":"eligibility""#.to_owned(),
                r#""target":"eligible""#.to_owned(),
                r#""type":"boolean""#.to_owned(),
                format!(r#""expr":{expr}"#),
                format!(r#""markers":[{}]"#, c.markers.join(",")),
                format!(r#""spans":[{}]"#, spans.join(",")),
            ],
            seed
        )
    )
}

fn rulebook_text(c: &Content, n: &Noise) -> String {
    let mut rules = vec![rule_text("elig.basic", c, n, true)];
    if n.extra_rule {
        // A different rule with different content, placed after the one under
        // test.
        let other = Content {
            value: "99".to_owned(),
            ..c.clone()
        };
        rules.push(rule_text("elig.other", &other, n, false));
    }
    let mut docs = vec![
        format!(r#"{{"id":"rules.md","sha256":"{DOC_A}"}}"#),
        format!(r#"{{"sha256":"{DOC_B}","id":"annex.md"}}"#),
    ];
    if n.unused_document {
        docs.push(format!(r#"{{"id":"unused.md","sha256":"{DOC_UNUSED}"}}"#));
    }
    if n.reverse_documents {
        docs.reverse();
    }
    let types =
        r#"{"inputs":[{"name":"age","type":"number"}],"enums":[],"sets":[],"collections":[]}"#;
    format!(
        "{{{}}}",
        members(
            vec![
                r#""format":"ruletrace-rulebook""#.to_owned(),
                r#""ir_version":"1""#.to_owned(),
                r#""name":"spring-open""#.to_owned(),
                r#""version":"2026.1""#.to_owned(),
                format!(r#""documents":[{}]"#, docs.join(",")),
                format!(r#""types":{types}"#),
                format!(r#""rules":[{}]"#, rules.join(",")),
                r#""precedence":[]"#.to_owned(),
            ],
            n.seed.rotate_left(13)
        )
    )
}

fn parse(text: &str) -> Rulebook {
    let input = InputId::new("rulebook.json").unwrap();
    match parse_rulebook(text.as_bytes(), &input, 8 << 20, 64) {
        Ok(s) => s.into_parts().0,
        Err(r) => panic!(
            "Rulebook was rejected: {:?}\n{text}",
            r.into_diagnostics().as_slice()
        ),
    }
}

fn hashes(book: &Rulebook, id: &str) -> (String, String) {
    let rev = book
        .revisions
        .iter()
        .find(|r| r.key.id.as_str() == id)
        .unwrap();
    let content = rule_content_hash(&book.ir_version, rev, &book.documents)
        .unwrap()
        .as_str()
        .to_owned();
    let draft = draft_hash(&book.ir_version, &rev.key.id, &rev.content, &book.documents)
        .unwrap()
        .as_str()
        .to_owned();
    (content, draft)
}

fn canonical_noise() -> Noise {
    Noise {
        origin: r#"{"kind":"manual","author":null}"#.to_owned(),
        positions: vec![(1, 1); 4],
        seed: 0,
        extra_rule: false,
        unused_document: false,
        reverse_documents: false,
    }
}

property_tests! {
    #![below_floor_config(ProptestConfig::with_cases(128))]

    /// Origin, span line and column, member order, other rules, unreferenced
    /// documents and document order never change either hash.
    ///
    /// **Validates: Requirements 3.13, 25.3, 25.9**
    #[test]
    fn non_content_changes_leave_hashes_unchanged(c in content(), n in noise()) {
        let base = parse(&rulebook_text(&c, &canonical_noise()));
        let varied = parse(&rulebook_text(&c, &n));
        prop_assert_eq!(hashes(&varied, "elig.basic"), hashes(&base, "elig.basic"));

        // The same Rulebook printed in both canonical forms hashes the same
        // after re-parsing.
        for printed in [print_pretty(&varied).unwrap(), print_compact(&varied).unwrap()] {
            prop_assert_eq!(hashes(&parse(&printed), "elig.basic"), hashes(&base, "elig.basic"));
        }
    }

    /// Replacing the Origin and the span positions of a parsed revision in
    /// memory leaves the hashes unchanged.
    ///
    /// **Validates: Requirements 25.3, 25.9**
    #[test]
    fn in_memory_origin_and_position_changes_leave_hashes_unchanged(
        c in content(),
        author in "[a-z]{1,8}",
        line in 1u32..10_000,
        column in 1u32..500,
    ) {
        let base = parse(&rulebook_text(&c, &canonical_noise()));
        let mut changed = base.clone();
        for rev in &mut changed.revisions {
            rev.origin = Origin::Manual { author: Some(Identity::new(author.clone()).unwrap()) };
            rev.loc = Loc::NONE;
            for span in &mut rev.content.spans {
                span.line = line;
                span.column = column;
                span.loc = Loc::NONE;
            }
        }
        prop_assert_eq!(hashes(&changed, "elig.basic"), hashes(&base, "elig.basic"));
    }
}
