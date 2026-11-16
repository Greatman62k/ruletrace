//! The provider-neutral Proposal schema, version "2" (UD-37, Req 23.39,
//! design 3.7.3).
//!
//! One flat node-table schema is sent to every provider. An expression is
//! a table of closed node objects that name their children by index, so
//! the schema has no `$ref`, no recursion, and a fixed nesting depth.
//! Every object is closed and lists every property as required. `anyOf`
//! appears only below the root. The local parser stays the authority:
//! nothing here is relied on for validation.

use crate::vocab::{Ud32Option, ROUNDING_MODES, RULE_KINDS};
use rt_json::{write_compact, Value};

/// The value of the `schema_version` member that this schema describes.
pub const FLAT_SCHEMA_VERSION: &str = "2";

/// The name sent beside the schema in `text.format`.
pub const SCHEMA_NAME: &str = "ruletrace_proposals";

const ADVISORY_KINDS: [&str; 4] = [
    "clause_interpretation",
    "clause_relationship",
    "conflict_highlight",
    "candidate_test",
];
const BINARY_OPS: [&str; 4] = ["add", "sub", "mul", "div"];
const CMP_OPS: [&str; 6] = ["eq", "ne", "lt", "le", "gt", "ge"];
const AGG_FNS: [&str; 4] = ["sum", "min", "max", "count"];
const DIRECTIONS: [&str; 2] = ["asc", "desc"];

type Prop = (&'static str, Value<()>);

// Sentences that the schema says to the model. Each states a rule that the
// local parser or the provenance check enforces, and the tests of
// `tests_prompt_contract` back each one.
const DOCUMENT_TEXT: &str = "Document identifier, copied from the clause.";
const SHA256_TEXT: &str = "The sha256 that the request gives for that document.";
const START_TEXT: &str = "UTF-8 byte offset in the whole document where the quote starts.";
const END_TEXT: &str = "UTF-8 byte offset in the whole document after the last byte of the quote.";
const LINE_TEXT: &str = "1-based line of start; CR, LF and CRLF each end a line.";
const COLUMN_TEXT: &str = "1-based column of start, counted in characters.";
const QUOTE_TEXT: &str = "The document bytes from start to end, copied exactly.";
const SPANS_TEXT: &str = "Must repeat the evidence one for one in the same order.";
const NODES_TEXT: &str = "Expression nodes. A member that holds an expression holds an index into this array. Every index must exist, a node's children have greater indexes than the node, and every node is named exactly once by the rule or by one other node.";
const UNRECOGNIZED_NODE_TEXT: &str = "Use only when no other node expresses the operation. A rule that holds it is stored with a blocking Unsupported finding.";
const ELIGIBILITY_TYPE_TEXT: &str =
    "Must be boolean: an eligibility rule is true or false for each participant.";
const DERIVED_TYPE_TEXT: &str = "Must be number, because a derived rule computes a number; use an eligibility rule for a condition that is true or false.";
const WEIGHTED_TYPE_TEXT: &str = "Must be number, because a weighted_score rule computes a number.";
const RANKING_TYPE_TEXT: &str = "Must be ranking, not number, even though the key is a number.";
const DEFAULT_TYPE_TEXT: &str = "Must be the declared type of the target input.";
const UNRECOGNIZED_TYPE_TEXT: &str = "Must still be exactly one of number, boolean, text, date, ranking or enum:<Name>; any other spelling refuses the whole response.";
const NODE_TYPE_TEXT: &str =
    "The type that this node computes, spelled exactly as number, boolean, text, date or enum:<Name>.";
const UNRECOGNIZED_RULE_TEXT: &str = "Use only when no rule kind expresses the clause. The rule is stored with a blocking Unsupported finding.";

// The schema object with a `description` member right after its first
// member, so that a reviewer reads the type and then the sentence.
fn described(schema: Option<Value<()>>, text: &str) -> Option<Value<()>> {
    let schema = schema?;
    let mut members: Vec<(String, Value<()>)> = schema
        .as_object()?
        .iter()
        .map(|m| (m.key.clone(), m.value.clone()))
        .collect();
    members.insert(1, ("description".to_owned(), Value::string(text)));
    object(members)
}

fn object(members: Vec<(String, Value<()>)>) -> Option<Value<()>> {
    Value::object(members).ok()
}

fn simple(kind: &str) -> Option<Value<()>> {
    object(vec![("type".to_owned(), Value::string(kind))])
}

fn text() -> Option<Value<()>> {
    simple("string")
}

fn integer() -> Option<Value<()>> {
    simple("integer")
}

fn single(name: &str) -> Option<Value<()>> {
    choice(&[name])
}

fn choice(names: &[&str]) -> Option<Value<()>> {
    object(vec![
        ("type".to_owned(), Value::string("string")),
        (
            "enum".to_owned(),
            Value::array(names.iter().map(|n| Value::string(*n)).collect()),
        ),
    ])
}

fn array_of(items: Value<()>) -> Option<Value<()>> {
    object(vec![
        ("type".to_owned(), Value::string("array")),
        ("items".to_owned(), items),
    ])
}

fn any_of(variants: Vec<Value<()>>) -> Option<Value<()>> {
    object(vec![("anyOf".to_owned(), Value::array(variants))])
}

fn closed(props: Vec<Prop>) -> Option<Value<()>> {
    let required = props.iter().map(|(k, _)| Value::string(*k)).collect();
    let properties = object(props.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())?;
    object(vec![
        ("type".to_owned(), Value::string("object")),
        ("additionalProperties".to_owned(), Value::boolean(false)),
        ("required".to_owned(), Value::array(required)),
        ("properties".to_owned(), properties),
    ])
}

fn node(name: &str, extra: Vec<Prop>) -> Option<Value<()>> {
    let mut props = vec![
        ("node", single(name)?),
        ("t", described(text(), NODE_TYPE_TEXT)?),
    ];
    props.extend(extra);
    closed(props)
}

fn nullable_text() -> Option<Value<()>> {
    any_of(vec![text()?, simple("null")?])
}

fn node_variant(name: &str) -> Option<Value<()>> {
    match name {
        "num" | "text" | "date" => node(name, vec![("value", text()?)]),
        "bool" => node(name, vec![("value", simple("boolean")?)]),
        "enum" => node(name, vec![("enum", text()?), ("member", text()?)]),
        "field" => node(name, vec![("name", text()?)]),
        "ref" | "rank_reference" => node(name, vec![("target", text()?)]),
        "binary" => node(
            name,
            vec![
                ("op", choice(&BINARY_OPS)?),
                ("left", integer()?),
                ("right", integer()?),
            ],
        ),
        "cmp" => node(
            name,
            vec![
                ("op", choice(&CMP_OPS)?),
                ("left", integer()?),
                ("right", integer()?),
            ],
        ),
        "and" | "or" => node(name, vec![("args", array_of(integer()?)?)]),
        "not" => node(name, vec![("arg", integer()?)]),
        "in" => node(name, vec![("value", integer()?), ("set", text()?)]),
        "agg" => node(
            name,
            vec![
                ("fn", choice(&AGG_FNS)?),
                ("collection", text()?),
                ("field", nullable_text()?),
            ],
        ),
        "round" => node(
            name,
            vec![
                ("arg", integer()?),
                ("places", integer()?),
                ("mode", choice(&ROUNDING_MODES)?),
            ],
        ),
        _ => None,
    }
}

fn unrecognized_node() -> Option<Value<()>> {
    described(
        closed(vec![
            ("node", single("unrecognized")?),
            ("name", text()?),
            ("detail", text()?),
        ]),
        UNRECOGNIZED_NODE_TEXT,
    )
}

// Node names come from the decoder table, so a node that the decoder
// learns about fails the generator loudly until it has a variant.
fn nodes_table(option: Ud32Option) -> Option<Value<()>> {
    let mut variants = Vec::new();
    for (name, _) in rt_ir::NODE_FIELDS {
        if name == "rank_reference" && option == Ud32Option::ThresholdOnly {
            continue;
        }
        variants.push(node_variant(name)?);
    }
    variants.push(unrecognized_node()?);
    array_of(any_of(variants)?)
}

fn span() -> Option<Value<()>> {
    closed(vec![
        ("document", text()?),
        ("start", integer()?),
        ("end", integer()?),
        ("line", integer()?),
        ("column", integer()?),
    ])
}

fn marker() -> Option<Value<()>> {
    any_of(vec![
        closed(vec![
            ("kind", single("ambiguous")?),
            ("interpretations", array_of(text()?)?),
        ])?,
        closed(vec![
            ("kind", single("incomplete")?),
            ("missing", array_of(text()?)?),
        ])?,
    ])
}

fn kind_members(kind: &str) -> Option<Vec<Prop>> {
    Some(match kind {
        "eligibility" | "derived" => vec![("expr", integer()?)],
        "weighted_score" => vec![
            (
                "terms",
                array_of(closed(vec![("weight", integer()?), ("value", integer()?)])?)?,
            ),
            (
                "rounding",
                any_of(vec![
                    simple("null")?,
                    closed(vec![
                        ("mode", choice(&ROUNDING_MODES)?),
                        ("places", integer()?),
                    ])?,
                ])?,
            ),
        ],
        "ranking" => vec![
            ("key", integer()?),
            ("direction", choice(&DIRECTIONS)?),
            (
                "tie_breakers",
                array_of(closed(vec![
                    ("expr", integer()?),
                    ("direction", choice(&DIRECTIONS)?),
                ])?)?,
            ),
            ("ties", single("group")?),
        ],
        "default" => vec![("value", integer()?)],
        _ => return None,
    })
}

// The sentence that sits on the `type` member of a rule of this kind.
fn rule_type_text(kind: &str) -> &'static str {
    match kind {
        "eligibility" => ELIGIBILITY_TYPE_TEXT,
        "derived" => DERIVED_TYPE_TEXT,
        "weighted_score" => WEIGHTED_TYPE_TEXT,
        "ranking" => RANKING_TYPE_TEXT,
        "default" => DEFAULT_TYPE_TEXT,
        _ => UNRECOGNIZED_TYPE_TEXT,
    }
}

fn rule_draft(kind: &str, option: Ud32Option) -> Option<Value<()>> {
    let mut props = vec![
        ("id", text()?),
        ("kind", single(kind)?),
        ("target", text()?),
        ("type", described(text(), rule_type_text(kind))?),
    ];
    if kind == "unrecognized" {
        props.push(("name", text()?));
        props.push(("detail", text()?));
    } else {
        props.extend(kind_members(kind)?);
        props.push(("nodes", described(nodes_table(option), NODES_TEXT)?));
    }
    props.push(("markers", array_of(marker()?)?));
    props.push(("spans", described(array_of(span()?), SPANS_TEXT)?));
    let rule = closed(props);
    if kind == "unrecognized" {
        described(rule, UNRECOGNIZED_RULE_TEXT)
    } else {
        rule
    }
}

fn evidence() -> Option<Value<()>> {
    closed(vec![
        ("document", described(text(), DOCUMENT_TEXT)?),
        ("sha256", described(text(), SHA256_TEXT)?),
        ("start", described(integer(), START_TEXT)?),
        ("end", described(integer(), END_TEXT)?),
        ("line", described(integer(), LINE_TEXT)?),
        ("column", described(integer(), COLUMN_TEXT)?),
        ("quote", described(text(), QUOTE_TEXT)?),
    ])
}

fn proposal(kind: &str, rule: Option<Value<()>>) -> Option<Value<()>> {
    let mut props = vec![
        ("kind", single(kind)?),
        ("proposer", text()?),
        ("rationale", text()?),
        ("evidence", array_of(evidence()?)?),
    ];
    if let Some(rule) = rule {
        props.push(("rule", rule));
    }
    closed(props)
}

/// The schema value for the vocabulary selected by `option`.
///
/// `None` only if the vocabulary names a rule kind or node that this
/// generator has no variant for, which the golden test catches.
#[must_use]
pub fn proposal_schema(option: Ud32Option) -> Option<Value<()>> {
    let mut variants = Vec::new();
    for kind in ADVISORY_KINDS {
        variants.push(proposal(kind, None)?);
    }
    let rule_kinds = RULE_KINDS.iter().chain(["unrecognized"].iter());
    for kind in rule_kinds {
        variants.push(proposal(
            "rule_suggestion",
            Some(rule_draft(kind, option)?),
        )?);
    }
    closed(vec![
        ("schema_version", single(FLAT_SCHEMA_VERSION)?),
        ("proposals", array_of(any_of(variants)?)?),
    ])
}

/// The canonical compact bytes of [`proposal_schema`], which is what a
/// provider integration places in its request.
#[must_use]
pub fn proposal_schema_bytes(option: Ud32Option) -> Option<Vec<u8>> {
    proposal_schema(option).map(|v| write_compact(&v).into_bytes())
}
