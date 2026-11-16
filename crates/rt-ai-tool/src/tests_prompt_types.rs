//! Behaviour behind the type instructions of prompt profile 3 (Req 35.23).
//!
//! Each function states what the validator or the parser does with a rule
//! whose declared type follows or breaks the instruction, through the real
//! acceptance path. The expected messages are written by hand from the
//! validator rules: a rule kind yields one type (eligibility boolean,
//! derived and weighted_score number, ranking ranking, default the type of
//! its target input), and a node's `t` equals the type it computes.

use crate::tests_flat::try_accept;
use crate::tests_prompt_contract::{assert_refused, payload_with, WHOLE};
use rt_foundation::{Code, ExitCode, Severity};

const AGE: &str = r#"{"node":"field","t":"number","name":"age"}"#;
const N18: &str = r#"{"node":"num","t":"number","value":"18"}"#;
const N06: &str = r#"{"node":"num","t":"number","value":"0.6"}"#;
const UNDER_18: &str = r#"{"node":"cmp","t":"boolean","op":"lt","left":1,"right":2}"#;
const SCORE: &str = r#"{"node":"binary","t":"number","op":"mul","left":1,"right":2}"#;

/// A rule of `kind` with its own members and nodes. `ty` is written as is.
fn rule(kind: &str, target: &str, ty: &str, members: &str, nodes: &str) -> String {
    format!(
        r#"{{"id":"r1","kind":"{kind}","target":"{target}","type":"{ty}",{members},"nodes":[{nodes}],"markers":[],"spans":[{}]}}"#,
        WHOLE.span()
    )
}

fn eligibility(ty: &str) -> String {
    rule(
        "eligibility",
        "junior",
        ty,
        r#""expr":0"#,
        &[UNDER_18, AGE, N18].join(","),
    )
}

fn derived(ty: &str) -> String {
    rule(
        "derived",
        "score",
        ty,
        r#""expr":0"#,
        &[SCORE, N06, AGE].join(","),
    )
}

// A derived rule over the comparison of the live run: kind derived, a
// boolean expression.
fn derived_condition(ty: &str) -> String {
    rule(
        "derived",
        "junior",
        ty,
        r#""expr":0"#,
        &[UNDER_18, AGE, N18].join(","),
    )
}

fn weighted(ty: &str) -> String {
    rule(
        "weighted_score",
        "score",
        ty,
        r#""terms":[{"weight":0,"value":1}],"rounding":null"#,
        &[N06, AGE].join(","),
    )
}

fn ranking(ty: &str) -> String {
    rule(
        "ranking",
        "entrant",
        ty,
        r#""key":0,"direction":"desc","tie_breakers":[],"ties":"group""#,
        AGE,
    )
}

fn default_rule(ty: &str) -> String {
    rule("default", "age", ty, r#""value":0"#, N18)
}

fn unrecognized(ty: &str) -> String {
    format!(
        r#"{{"id":"r1","kind":"unrecognized","target":"entrant","type":"{ty}","name":"prize_allocation","detail":"ratio 5 to 3 to 2","markers":[],"spans":[{}]}}"#,
        WHOLE.span()
    )
}

fn payload(rule: &str) -> String {
    payload_with("2", "why", &[&WHOLE], rule)
}

/// The findings of the single proposal as `(code id, message)`.
fn findings(rule: &str) -> Vec<(String, String)> {
    let batch = try_accept(&payload(rule)).unwrap_or_else(|d| panic!("refused: {d:?}"));
    assert_eq!(batch.proposals().len(), 1);
    batch.proposals()[0]
        .outcome
        .iter()
        .map(|d| (d.code.id().to_owned(), d.message.clone()))
        .collect()
}

fn mismatch(message: &str) -> (String, String) {
    ("RT-VAL-0001".to_owned(), message.to_owned())
}

fn clean(rule: &str, what: &str) {
    assert_eq!(findings(rule), Vec::new(), "{what}");
}

fn only(rule: &str, expected: &[&str], what: &str) {
    let want: Vec<_> = expected.iter().map(|m| mismatch(m)).collect();
    assert_eq!(findings(rule), want, "{what}");
}

/// The rule type must be the type the kind yields.
pub(crate) fn type_matches_kind() {
    clean(&eligibility("boolean"), "eligibility boolean");
    clean(&derived("number"), "derived number");
    clean(&weighted("number"), "weighted_score number");
    clean(&ranking("ranking"), "ranking ranking");
    clean(&default_rule("number"), "default of a number input");

    let at = "type mismatch at rule type: expected";
    only(
        &eligibility("number"),
        &[&format!("{at} boolean, found number")],
        "eligibility number",
    );
    only(
        &derived("boolean"),
        &[&format!("{at} number, found boolean")],
        "derived boolean",
    );
    only(
        &weighted("boolean"),
        &[&format!("{at} number, found boolean")],
        "weighted_score boolean",
    );
    only(
        &ranking("boolean"),
        &[&format!("{at} ranking, found boolean")],
        "ranking boolean",
    );
    only(
        &default_rule("boolean"),
        &[&format!("{at} number, found boolean")],
        "default against a number input",
    );

    // A node whose `t` differs from the type it computes.
    let wrong_node = rule(
        "eligibility",
        "junior",
        "boolean",
        r#""expr":0"#,
        &[
            r#"{"node":"cmp","t":"number","op":"lt","left":1,"right":2}"#,
            AGE,
            N18,
        ]
        .join(","),
    );
    only(
        &wrong_node,
        &["type mismatch at expr: declared type number, but the node computes boolean"],
        "cmp declared number",
    );
}

/// A ranking rule has the type ranking, whatever its key is.
pub(crate) fn ranking_type() {
    clean(&ranking("ranking"), "ranking with a number key");
    only(
        &ranking("number"),
        &["type mismatch at rule type: expected ranking, found number"],
        "ranking labelled number",
    );
    // Ranking is not a type of any other kind.
    only(
        &derived("ranking"),
        &["type mismatch at rule type: expected number, found ranking"],
        "numeric scoring labelled ranking",
    );
    only(
        &eligibility("ranking"),
        &["type mismatch at rule type: expected boolean, found ranking"],
        "a condition labelled ranking",
    );
}

/// A true or false condition is an eligibility rule of type boolean.
pub(crate) fn boolean_predicate() {
    clean(&eligibility("boolean"), "junior as eligibility");
    only(
        &eligibility("number"),
        &["type mismatch at rule type: expected boolean, found number"],
        "boolean predicate labelled number",
    );
    // The kind of the live run: derived with a boolean expression yields
    // two findings, the expression first and then the rule type.
    only(
        &derived_condition("boolean"),
        &[
            "type mismatch at expr: expected number, found boolean",
            "type mismatch at rule type: expected number, found boolean",
        ],
        "derived kind with a condition",
    );
}

/// A computed number is a derived or weighted_score rule of type number.
pub(crate) fn numeric_score() {
    clean(&derived("number"), "score as derived");
    clean(&weighted("number"), "score as weighted_score");
    only(
        &derived("ranking"),
        &["type mismatch at rule type: expected number, found ranking"],
        "numeric scoring labelled ranking",
    );
    only(
        &weighted("ranking"),
        &["type mismatch at rule type: expected number, found ranking"],
        "weighted score labelled ranking",
    );
}

/// Only the six type spellings are accepted, and a spelling that is not
/// one refuses the whole response.
pub(crate) fn type_names() {
    names_are_exact();
    for ty in ["number", "boolean", "text", "date", "ranking"] {
        let batch = try_accept(&payload(&unrecognized(ty)));
        assert!(batch.is_ok(), "{ty}: {batch:?}");
    }
    for ty in ["numeric", "bool", "Number", "integer", "string", ""] {
        for text in [
            payload(&derived(ty)),
            payload(&unrecognized(ty)),
            payload(&rule(
                "derived",
                "score",
                "number",
                r#""expr":0"#,
                &format!(r#"{{"node":"field","t":"{ty}","name":"age"}}"#),
            )),
        ] {
            let d = assert_refused(&text, Code::AiResponseSchemaViolation, ExitCode::Response);
            assert!(d.message().contains("RT-IR-0010"), "{ty}: {}", d.message());
            assert!(
                d.message().contains("is not a declared type"),
                "{}",
                d.message()
            );
        }
    }
}

/// A kind or node name that the schema does not list refuses the response.
pub(crate) fn names_are_exact() {
    let kind = derived("number").replace(r#""kind":"derived""#, r#""kind":"scoring""#);
    let node = derived("number").replace(r#""node":"binary""#, r#""node":"multiply""#);
    for text in [kind, node] {
        assert_refused(
            &payload(&text),
            Code::AiResponseSchemaViolation,
            ExitCode::Response,
        );
    }
}

/// The kind decides the type, not what the expression contains.
pub(crate) fn kind_not_content() {
    // A comparison inside a derived rule does not make it boolean.
    only(
        &derived_condition("boolean"),
        &[
            "type mismatch at expr: expected number, found boolean",
            "type mismatch at rule type: expected number, found boolean",
        ],
        "comparison inside derived",
    );
    // Number literals inside an eligibility rule do not make it a number.
    clean(&eligibility("boolean"), "numbers inside a condition");
    only(
        &eligibility("number"),
        &["type mismatch at rule type: expected boolean, found number"],
        "numbers inside a condition do not make it a number",
    );
    // A boolean node inside a number rule does not change the kind.
    only(
        &ranking("boolean"),
        &["type mismatch at rule type: expected ranking, found boolean"],
        "ranking is not decided by the key",
    );
}

/// What cannot be decided is stored as unrecognized and stays blocking and
/// unsupported, whatever its type, and is not turned into another rule.
pub(crate) fn no_guess() {
    for ty in ["number", "boolean", "ranking"] {
        let batch = try_accept(&payload(&unrecognized(ty))).unwrap();
        let proposal = &batch.proposals()[0];
        let found: Vec<_> = proposal
            .outcome
            .iter()
            .map(|d| (d.code.id(), d.code.severity()))
            .collect();
        assert_eq!(found, [("RT-VAL-0003", Severity::Unsupported)], "{ty}");
        assert!(
            proposal.outcome[0]
                .message
                .starts_with("unsupported rule kind 'unrecognized'"),
            "{}",
            proposal.outcome[0].message
        );
        assert!(batch.has_blocking_findings());
        let (draft, _) = proposal.rule.as_ref().unwrap();
        assert!(draft.json.contains(r#""kind":"unrecognized""#), "{ty}");
        assert!(draft.json.contains(r#""name":"prize_allocation""#), "{ty}");
    }
    // The variant has no expression, so it cannot be a typed rule in
    // disguise: an expression member is refused.
    let disguised = unrecognized("number").replace(r#""markers":[]"#, r#""expr":0,"markers":[]"#);
    assert_refused(
        &payload(&disguised),
        Code::AiResponseSchemaViolation,
        ExitCode::Response,
    );
}
