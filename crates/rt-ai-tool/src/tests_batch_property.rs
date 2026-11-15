//! Whole-batch rejection and strict acceptance (design 3.7.3, 3.7.4;
//! Req 23.5 to 23.8, 23.10, 30.1 to 30.5, 30.10, 30.11).
//!
//! A valid response of several proposals is built from a model of its
//! own. One operator then changes one thing in one proposal. Each operator
//! states its expected code, severity, exit code and structural path
//! itself. Documents, offsets and the line and column of every span are
//! written or computed here, not read from the verifier.

use crate::credential::Credential;
use crate::out::render_diagnostic;
use crate::tests::{MapEnv, BASE};
use crate::tests_memfs::MemFs;
use crate::tests_provenance::{doc, escaped, model};
use crate::tests_response::{FORBIDDEN_NAMES, LIMIT};
use crate::tests_rig::{meta, Harness};
use crate::tests_run_support::{shapes, VecSink};
use crate::{
    accept_batch, build_plan, load, AcceptInput, AiConfig, ClauseId, Consent, PlanInput, Redactor,
    Ud32Option,
};
use proptest::prelude::*;
use rt_ai_adapter::Completion;
use rt_ai_mock::{MockProvider, MockScript, ScriptResult, Step};
use rt_foundation::{CliLocation, Code, DocumentId, InputId, Location, Severity};
use rt_hash::sha256;
use rt_ir::{parse_rulebook, Rulebook};
use rt_proposals::{parse_batch, ProposalKind};
use rt_srcmodel::{CandidateClause, SourceModels};
use std::collections::BTreeSet;
use std::time::Duration;

const CANARY: &str = "sk-canary-Zq7x93LmN";

// Bytes of the first document: "Alpha beta." 0..11, CRLF 11..13, "Gamma
// δ delta." 13..28 (δ at 19..21), CRLF 28..30, "End 日本." 30..41 (日 at
// 34..37, 本 at 37..40), LF 41, "Last é." 42..50 (é at 47..49).
const RULES_TEXT: &str = "Alpha beta.\r\nGamma δ delta.\r\nEnd 日本.\nLast é.";
// Bytes: "Rule one." 0..9, LF 9, "Rule two." 10..19.
const EXTRA_TEXT: &str = "Rule one.\nRule two.";
const DOCS: [(&str, &str); 2] = [("rules.md", RULES_TEXT), ("extra.md", EXTRA_TEXT)];

// The clauses of the single request: document index, start, end.
const CLAUSES: [(usize, u32, u32); 5] =
    [(0, 0, 11), (0, 13, 41), (0, 42, 50), (1, 0, 9), (1, 10, 19)];

// Spans that lie inside one clause. Whole clauses, interior pieces after
// non-ASCII text, and one piece that crosses a CRLF.
const VALID_SPANS: [(usize, u32, u32); 15] = [
    (0, 0, 11),
    (0, 0, 5),
    (0, 6, 11),
    (0, 13, 41),
    (0, 13, 28),
    (0, 22, 28),
    (0, 30, 41),
    (0, 37, 41),
    (0, 19, 21),
    (0, 42, 50),
    (0, 47, 50),
    (1, 0, 9),
    (1, 10, 19),
    (1, 5, 9),
    (1, 14, 19),
];

// Well-formed spans that are not inside any one clause. The last three
// start inside one clause and end in the next gap or clause.
const OUTSIDE_SPANS: [(usize, u32, u32); 7] = [
    (0, 11, 13),
    (0, 9, 13),
    (0, 41, 42),
    (1, 9, 10),
    (0, 5, 17),
    (0, 41, 50),
    (1, 5, 19),
];

const ADVISORY: [&str; 4] = [
    "clause_interpretation",
    "clause_relationship",
    "conflict_highlight",
    "candidate_test",
];

const PROPOSERS: [&str; 4] = ["model-1", "ïden-2", "日本-3", "m"];

const RATIONALES: [&str; 5] = [
    "why",
    "Ünïcode δ 日本 reasoning",
    "line\r\nbreak \"quoted\" back\\slash",
    "a/b <x> & y",
    "see Alpha beta.",
];

// 1-based line and column, counted in scalar values. A CR followed by LF
// belongs to the LF, any other CR or LF ends the line.
fn locate(text: &str, start: u32) -> (u32, u32) {
    let (mut line, mut column) = (1u32, 1u32);
    for (i, c) in text.char_indices() {
        if i >= start as usize {
            break;
        }
        let ends = match c {
            '\n' => true,
            '\r' => !text[i + 1..].starts_with('\n'),
            _ => false,
        };
        if ends {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn text_of(name: &str) -> &'static str {
    DOCS.iter().find(|(n, _)| *n == name).map_or("", |(_, t)| t)
}

fn hash_of(index: usize) -> String {
    let (name, text) = DOCS[index];
    model(name, text).sha256().as_str().to_owned()
}

#[derive(Clone, Debug)]
struct Claim {
    doc: String,
    sha: String,
    start: u32,
    end: u32,
    line: u32,
    column: u32,
    quote: String,
}

impl Claim {
    fn of(index: usize, start: u32, end: u32) -> Self {
        let (name, text) = DOCS[index];
        let (line, column) = locate(text, start);
        Self {
            doc: name.to_owned(),
            sha: hash_of(index),
            start,
            end,
            line,
            column,
            quote: text[start as usize..end as usize].to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuleSpec {
    Eligible(u8),
    Unsupported,
    Undeclared,
    Ambiguous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PKind {
    Advisory(&'static str),
    Rule(RuleSpec),
}

impl PKind {
    fn is_rule(self) -> bool {
        matches!(self, Self::Rule(_))
    }

    fn has_expr(self) -> bool {
        matches!(
            self,
            Self::Rule(RuleSpec::Eligible(_) | RuleSpec::Undeclared | RuleSpec::Ambiguous)
        )
    }

    fn is_advisory(self) -> bool {
        matches!(self, Self::Advisory(_))
    }

    fn is_unsupported(self) -> bool {
        self == Self::Rule(RuleSpec::Unsupported)
    }

    fn blocking(self) -> bool {
        matches!(self, Self::Rule(r) if !matches!(r, RuleSpec::Eligible(_)))
    }

    fn name(self) -> &'static str {
        match self {
            Self::Advisory(n) => n,
            Self::Rule(_) => "rule_suggestion",
        }
    }

    // The findings the validator is expected to store for the proposal.
    fn findings(self) -> Vec<Code> {
        match self {
            Self::Rule(RuleSpec::Unsupported) => vec![Code::ValUnsupportedConstruct],
            Self::Rule(RuleSpec::Undeclared) => vec![Code::ValUndeclaredReference],
            Self::Rule(RuleSpec::Ambiguous) => vec![Code::ValAmbiguousInterpretation],
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
struct PropSpec {
    kind: PKind,
    claims: Vec<Claim>,
    proposer: String,
    rationale: String,
}

type Spec = Vec<PropSpec>;

// A tiny JSON tree, so that an operator edits one place by path.
#[derive(Clone, Debug)]
enum J {
    Obj(Vec<(String, J)>),
    Arr(Vec<J>),
    Str(String),
    Raw(String),
}

impl J {
    fn render(&self, out: &mut String) {
        match self {
            Self::Obj(members) => {
                out.push('{');
                for (i, (key, value)) in members.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push('"');
                    out.push_str(&escaped(key));
                    out.push_str("\":");
                    value.render(out);
                }
                out.push('}');
            }
            Self::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.render(out);
                }
                out.push(']');
            }
            Self::Str(text) => {
                out.push('"');
                out.push_str(&escaped(text));
                out.push('"');
            }
            Self::Raw(text) => out.push_str(text),
        }
    }

    fn text(&self) -> String {
        let mut out = String::new();
        self.render(&mut out);
        out
    }
}

fn s(text: &str) -> J {
    J::Str(text.to_owned())
}

fn num(n: u32) -> J {
    J::Raw(n.to_string())
}

fn obj(members: Vec<(&str, J)>) -> J {
    J::Obj(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
}

#[derive(Clone, Debug)]
enum Seg {
    K(String),
    I(usize),
}

fn key(name: &str) -> Seg {
    Seg::K(name.to_owned())
}

fn path_text(segs: &[Seg]) -> String {
    let mut out = String::new();
    for seg in segs {
        match seg {
            Seg::K(name) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(name);
            }
            Seg::I(i) => out.push_str(&format!("[{i}]")),
        }
    }
    out
}

fn at<'a>(root: &'a mut J, segs: &[Seg]) -> &'a mut J {
    let mut cur = root;
    for seg in segs {
        cur = match (cur, seg) {
            (J::Obj(members), Seg::K(name)) => {
                &mut members
                    .iter_mut()
                    .find(|(k, _)| k == name)
                    .expect("member on the path")
                    .1
            }
            (J::Arr(items), Seg::I(i)) => &mut items[*i],
            _ => panic!("path leaves the tree"),
        };
    }
    cur
}

fn members(node: &mut J) -> &mut Vec<(String, J)> {
    match node {
        J::Obj(m) => m,
        _ => panic!("not an object"),
    }
}

fn evidence_json(c: &Claim) -> J {
    obj(vec![
        ("document", s(&c.doc)),
        ("sha256", s(&c.sha)),
        ("start", num(c.start)),
        ("end", num(c.end)),
        ("line", num(c.line)),
        ("column", num(c.column)),
        ("quote", s(&c.quote)),
    ])
}

fn span_json(c: &Claim) -> J {
    obj(vec![
        ("document", s(&c.doc)),
        ("start", num(c.start)),
        ("end", num(c.end)),
        ("line", num(c.line)),
        ("column", num(c.column)),
    ])
}

fn expr_json(field: &str, threshold: u8) -> J {
    obj(vec![
        ("node", s("cmp")),
        ("t", s("boolean")),
        ("op", s("ge")),
        (
            "left",
            obj(vec![
                ("node", s("field")),
                ("t", s("number")),
                ("name", s(field)),
            ]),
        ),
        (
            "right",
            obj(vec![
                ("node", s("num")),
                ("t", s("number")),
                ("value", s(&threshold.to_string())),
            ]),
        ),
    ])
}

fn rule_json(rule: RuleSpec, claims: &[Claim]) -> J {
    let spans = J::Arr(claims.iter().map(span_json).collect());
    let none = J::Arr(Vec::new());
    match rule {
        RuleSpec::Unsupported => obj(vec![
            ("id", s("elig.new")),
            ("kind", s("unrecognized")),
            ("name", s("rank_reference")),
            ("detail", s("the top four qualify")),
            ("target", s("eligible2")),
            ("type", s("boolean")),
            ("markers", none),
            ("spans", spans),
        ]),
        other => {
            let (field, threshold) = match other {
                RuleSpec::Eligible(t) => ("age", t),
                RuleSpec::Undeclared => ("missing", 16),
                _ => ("age", 16),
            };
            let markers = if other == RuleSpec::Ambiguous {
                J::Arr(vec![obj(vec![
                    ("kind", s("ambiguous")),
                    ("interpretations", J::Arr(vec![s("a"), s("b")])),
                ])])
            } else {
                none
            };
            obj(vec![
                ("id", s("elig.new")),
                ("kind", s("eligibility")),
                ("target", s("eligible2")),
                ("type", s("boolean")),
                ("expr", expr_json(field, threshold)),
                ("markers", markers),
                ("spans", spans),
            ])
        }
    }
}

fn proposal_json(p: &PropSpec) -> J {
    let mut members = vec![
        ("kind", s(p.kind.name())),
        ("proposer", s(&p.proposer)),
        ("rationale", s(&p.rationale)),
        (
            "evidence",
            J::Arr(p.claims.iter().map(evidence_json).collect()),
        ),
    ];
    if let PKind::Rule(rule) = p.kind {
        members.push(("rule", rule_json(rule, &p.claims)));
    }
    obj(members)
}

fn response_json(spec: &Spec) -> J {
    obj(vec![
        ("schema_version", s("1")),
        (
            "proposals",
            J::Arr(spec.iter().map(proposal_json).collect()),
        ),
    ])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Site {
    Top,
    Proposal,
    Evidence,
    Rule,
    RuleSpan,
    ExprRoot,
    ExprLeft,
}

const SITES: [Site; 7] = [
    Site::Top,
    Site::Proposal,
    Site::Evidence,
    Site::Rule,
    Site::RuleSpan,
    Site::ExprRoot,
    Site::ExprLeft,
];

fn site_segs(site: Site, p: usize, e: usize) -> Vec<Seg> {
    let base = vec![key("proposals"), Seg::I(p)];
    let extend = |more: Vec<Seg>| [base.clone(), more].concat();
    match site {
        Site::Top => Vec::new(),
        Site::Proposal => base.clone(),
        Site::Evidence => extend(vec![key("evidence"), Seg::I(e)]),
        Site::Rule => extend(vec![key("rule")]),
        Site::RuleSpan => extend(vec![key("rule"), key("spans"), Seg::I(e)]),
        Site::ExprRoot => extend(vec![key("rule"), key("expr")]),
        Site::ExprLeft => extend(vec![key("rule"), key("expr"), key("left")]),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Need {
    Any,
    Rule,
    Expr,
    Advisory,
    Unsupported,
}

fn site_need(site: Site) -> Need {
    match site {
        Site::Top | Site::Proposal | Site::Evidence => Need::Any,
        Site::Rule | Site::RuleSpan => Need::Rule,
        Site::ExprRoot | Site::ExprLeft => Need::Expr,
    }
}

const UNKNOWN_NAMES: [&str; 6] = [
    "extra",
    "script",
    "command",
    "approved",
    "reference",
    "approval_note",
];

const FORBIDDEN_VALUES: [&str; 7] = ["true", "\"approved\"", "null", "0", "{}", "[]", "{\"a\":1}"];

#[derive(Clone, Debug)]
enum Op {
    Oversize,
    InvalidUtf8,
    Truncate(usize),
    Prose,
    Refused,
    Incomplete,
    Forbidden {
        name: usize,
        site: Site,
        front: bool,
        value: usize,
        nested: bool,
    },
    UnknownField {
        site: Site,
        name: usize,
    },
    Remove {
        site: Site,
        key: &'static str,
    },
    Retype {
        site: Site,
        key: &'static str,
        to: &'static str,
    },
    ItemNotObject,
    Duplicate {
        site: Site,
        key: &'static str,
    },
    Depth,
    TooManyProposals,
    TooManyEvidence,
    ZeroEvidence,
    BadVersion,
    BadKind(usize),
    BadRuleKind,
    BadNode,
    BadUnrecognizedNode,
    BadUnrecognizedRule,
    UnknownDoc,
    HashMismatch(usize),
    EndBeyond(usize),
    ZeroWidth,
    NonBoundaryStart(usize),
    NonBoundaryEnd(usize),
    WrongLine,
    WrongColumn,
    ByteColumn(usize),
    Outside(usize),
    QuoteAppend,
    QuotePrepend,
    QuoteNewline,
    QuoteNormalized,
    Credential {
        fragment: bool,
        proposer: bool,
    },
}

impl Op {
    fn need(&self) -> Need {
        match self {
            Self::Forbidden { site, .. }
            | Self::UnknownField { site, .. }
            | Self::Remove { site, .. }
            | Self::Retype { site, .. }
            | Self::Duplicate { site, .. } => site_need(*site),
            Self::Depth | Self::BadNode | Self::BadUnrecognizedNode => Need::Expr,
            Self::BadRuleKind => Need::Rule,
            Self::BadUnrecognizedRule => Need::Unsupported,
            Self::TooManyEvidence | Self::ZeroEvidence => Need::Advisory,
            _ => Need::Any,
        }
    }
}

fn families() -> Vec<Vec<Op>> {
    let mut forbidden = Vec::new();
    for name in 0..FORBIDDEN_NAMES.len() {
        for (i, site) in SITES.iter().enumerate() {
            forbidden.push(Op::Forbidden {
                name,
                site: *site,
                front: (name + i) % 2 == 0,
                value: (name + i) % FORBIDDEN_VALUES.len(),
                nested: false,
            });
        }
        for site in [Site::Proposal, Site::Evidence, Site::Rule] {
            forbidden.push(Op::Forbidden {
                name,
                site,
                front: false,
                value: name % FORBIDDEN_VALUES.len(),
                nested: true,
            });
        }
    }
    let mut unknown = Vec::new();
    for site in SITES {
        for name in 0..UNKNOWN_NAMES.len() {
            unknown.push(Op::UnknownField { site, name });
        }
    }
    let removed = [
        (Site::Top, "proposals"),
        (Site::Top, "schema_version"),
        (Site::Proposal, "proposer"),
        (Site::Proposal, "rationale"),
        (Site::Proposal, "evidence"),
        (Site::Proposal, "kind"),
        (Site::Evidence, "quote"),
        (Site::Evidence, "document"),
        (Site::Evidence, "sha256"),
        (Site::Evidence, "start"),
        (Site::Evidence, "end"),
        (Site::Evidence, "line"),
        (Site::Evidence, "column"),
        (Site::Rule, "target"),
        (Site::Rule, "id"),
        (Site::Rule, "spans"),
        (Site::RuleSpan, "line"),
    ];
    let retyped = [
        (Site::Evidence, "start", "\"9\""),
        (Site::Evidence, "start", "-1"),
        (Site::Evidence, "start", "1.5"),
        (Site::Evidence, "start", "4294967296"),
        (Site::Evidence, "line", "null"),
        (Site::Evidence, "sha256", "\"ABC\""),
        (Site::Evidence, "quote", "7"),
        (Site::Proposal, "rationale", "7"),
        (Site::Proposal, "proposer", "null"),
        (Site::Proposal, "proposer", "\"\""),
        (Site::Proposal, "evidence", "{}"),
        (Site::Proposal, "kind", "3"),
        (Site::Top, "proposals", "{}"),
        (Site::Top, "schema_version", "1"),
        (Site::Top, "schema_version", "\"1.0\""),
        (Site::Rule, "id", "5"),
        (Site::Rule, "target", "5"),
        (Site::Rule, "markers", "{}"),
    ];
    let repeated = [
        (Site::Top, "schema_version"),
        (Site::Proposal, "rationale"),
        (Site::Proposal, "kind"),
        (Site::Evidence, "quote"),
        (Site::Evidence, "start"),
        (Site::Rule, "id"),
        (Site::ExprRoot, "op"),
    ];
    vec![
        vec![Op::Oversize],
        vec![Op::InvalidUtf8],
        [1, 100, 500, 900, 999].map(Op::Truncate).to_vec(),
        vec![Op::Prose],
        vec![Op::Refused, Op::Incomplete],
        forbidden,
        unknown,
        removed
            .iter()
            .map(|(site, key)| Op::Remove { site: *site, key })
            .collect(),
        retyped
            .iter()
            .map(|(site, key, to)| Op::Retype {
                site: *site,
                key,
                to,
            })
            .chain([Op::ItemNotObject])
            .collect(),
        repeated
            .iter()
            .map(|(site, key)| Op::Duplicate { site: *site, key })
            .collect(),
        vec![
            Op::Depth,
            Op::TooManyProposals,
            Op::TooManyEvidence,
            Op::ZeroEvidence,
            Op::BadVersion,
        ],
        vec![
            Op::BadKind(0),
            Op::BadKind(1),
            Op::BadKind(2),
            Op::BadKind(3),
            Op::BadRuleKind,
            Op::BadNode,
            Op::BadUnrecognizedNode,
            Op::BadUnrecognizedRule,
        ],
        vec![
            Op::UnknownDoc,
            Op::HashMismatch(0),
            Op::HashMismatch(1),
            Op::EndBeyond(0),
            Op::EndBeyond(1),
            Op::EndBeyond(2),
            Op::ZeroWidth,
            Op::NonBoundaryStart(0),
            Op::NonBoundaryStart(1),
            Op::NonBoundaryStart(2),
            Op::NonBoundaryEnd(0),
            Op::NonBoundaryEnd(3),
            Op::WrongLine,
            Op::WrongColumn,
            Op::ByteColumn(0),
            Op::ByteColumn(1),
        ],
        (0..OUTSIDE_SPANS.len())
            .map(Op::Outside)
            .chain([
                Op::QuoteAppend,
                Op::QuotePrepend,
                Op::QuoteNewline,
                Op::QuoteNormalized,
            ])
            .collect(),
        vec![
            Op::Credential {
                fragment: false,
                proposer: false,
            },
            Op::Credential {
                fragment: true,
                proposer: false,
            },
            Op::Credential {
                fragment: false,
                proposer: true,
            },
            Op::Credential {
                fragment: true,
                proposer: true,
            },
        ],
    ]
}

const FAMILY_COUNT: usize = 15;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Field {
    Any,
    Absent,
    Is(String),
}

#[derive(Clone, Debug)]
struct Want {
    id: &'static str,
    exit: u8,
    field: Field,
    pos: Option<(u32, u32)>,
    mention: Option<String>,
}

fn ai(id: &'static str, field: Field) -> Want {
    Want {
        id,
        exit: 6,
        field,
        pos: None,
        mention: None,
    }
}

fn prov(id: &'static str, field: String) -> Want {
    Want {
        id,
        exit: 3,
        field: Field::Is(field),
        pos: None,
        mention: None,
    }
}

struct Built {
    payload: Vec<u8>,
    completion: Completion,
    want: Want,
    canary: bool,
}

fn pick(spec: &Spec, a: usize, need: Need) -> usize {
    let ok = |k: PKind| match need {
        Need::Any => true,
        Need::Rule => k.is_rule(),
        Need::Expr => k.has_expr(),
        Need::Advisory => k.is_advisory(),
        Need::Unsupported => k.is_unsupported(),
    };
    let found: Vec<usize> = (0..spec.len()).filter(|i| ok(spec[*i].kind)).collect();
    found[a % found.len()]
}

fn doc_len(name: &str) -> u32 {
    u32::try_from(text_of(name).len()).unwrap()
}

fn non_boundaries() -> Vec<u32> {
    (1..RULES_TEXT.len())
        .filter(|i| !RULES_TEXT.is_char_boundary(*i))
        .map(|i| u32::try_from(i).unwrap())
        .collect()
}

fn is_boundary(at: u32) -> bool {
    RULES_TEXT.is_char_boundary(at as usize)
}

// Applies one operator to a valid specification. The selectors `a` and
// `b` choose the proposal and the evidence item the operator changes.
fn build(valid: &Spec, op: &Op, a: usize, b: usize) -> Built {
    let mut spec = valid.clone();
    let p = pick(&spec, a, op.need());
    let e = b % spec[p].claims.len();
    let evidence_at = |k: &str| format!("proposals[{p}].evidence[{e}].{k}");
    let mut want: Option<Want> = None;
    match op {
        Op::UnknownDoc => {
            spec[p].claims[e].doc = "zz.md".to_owned();
            want = Some(prov("RT-PRV-0101", evidence_at("document")));
        }
        Op::HashMismatch(how) => {
            let c = &mut spec[p].claims[e];
            c.sha = if *how == 0 {
                let other = usize::from(c.doc == "rules.md");
                hash_of(other)
            } else {
                let first = if c.sha.starts_with('0') { "1" } else { "0" };
                format!("{first}{}", &c.sha[1..])
            };
            want = Some(prov("RT-PRV-0102", evidence_at("sha256")));
        }
        Op::EndBeyond(k) => {
            let c = &mut spec[p].claims[e];
            c.end = [doc_len(&c.doc) + 1, 1_000_000, u32::MAX][*k % 3];
            want = Some(prov("RT-PRV-0101", evidence_at("end")));
        }
        Op::ZeroWidth => {
            let c = &mut spec[p].claims[e];
            c.end = c.start;
            want = Some(prov("RT-PRV-0101", evidence_at("end")));
        }
        Op::NonBoundaryStart(k) => {
            let all = non_boundaries();
            let nb = all[*k % all.len()];
            let mut c = Claim::of(0, 0, 5);
            c.start = nb;
            c.end = (nb + 1..).find(|i| is_boundary(*i)).unwrap();
            spec[p].claims[e] = c;
            want = Some(prov("RT-PRV-0101", evidence_at("start")));
        }
        Op::NonBoundaryEnd(k) => {
            let all = non_boundaries();
            let nb = all[*k % all.len()];
            let mut c = Claim::of(0, 0, 5);
            c.start = (0..nb).rev().find(|i| is_boundary(*i)).unwrap();
            c.end = nb;
            spec[p].claims[e] = c;
            want = Some(prov("RT-PRV-0101", evidence_at("end")));
        }
        Op::WrongLine => {
            let c = &mut spec[p].claims[e];
            let breaks = text_of(&c.doc)[..c.start as usize].matches("\r\n").count();
            c.line += 1 + u32::try_from(breaks).unwrap();
            want = Some(prov("RT-PRV-0101", evidence_at("line")));
        }
        Op::WrongColumn => {
            spec[p].claims[e].column += 1;
            want = Some(prov("RT-PRV-0101", evidence_at("column")));
        }
        Op::ByteColumn(k) => {
            let (start, end, byte_column) = [(22u32, 28u32, 10u32), (37, 41, 8)][*k % 2];
            let mut c = Claim::of(0, start, end);
            c.column = byte_column;
            spec[p].claims[e] = c;
            want = Some(prov("RT-PRV-0101", evidence_at("column")));
        }
        Op::Outside(k) => {
            let (d, start, end) = OUTSIDE_SPANS[*k % OUTSIDE_SPANS.len()];
            spec[p].claims[e] = Claim::of(d, start, end);
            want = Some(prov("RT-PRV-0103", format!("proposals[{p}].evidence[{e}]")));
        }
        Op::QuoteAppend => {
            spec[p].claims[e].quote.push('x');
            want = Some(prov("RT-PRV-0104", evidence_at("quote")));
        }
        Op::QuotePrepend => {
            spec[p].claims[e].quote.insert(0, ' ');
            want = Some(prov("RT-PRV-0104", evidence_at("quote")));
        }
        Op::QuoteNewline => {
            let mut c = Claim::of(0, 13, 41);
            c.quote = c.quote.replace("\r\n", "\n");
            spec[p].claims[e] = c;
            want = Some(prov("RT-PRV-0104", evidence_at("quote")));
        }
        Op::QuoteNormalized => {
            let mut c = Claim::of(0, 47, 50);
            c.quote = "e\u{301}.".to_owned();
            spec[p].claims[e] = c;
            want = Some(prov("RT-PRV-0104", evidence_at("quote")));
        }
        _ => {}
    }

    let mut root = response_json(&spec);
    let mut completion = Completion::Complete;
    let mut canary = false;
    let mut mention = None;
    let prop_segs = vec![key("proposals"), Seg::I(p)];
    match op {
        Op::Forbidden {
            name,
            site,
            front,
            value,
            nested,
        } => {
            let name = FORBIDDEN_NAMES[*name];
            let mut segs = site_segs(*site, p, e);
            let raw = J::Raw(FORBIDDEN_VALUES[*value].to_owned());
            let member = if *nested {
                ("nest".to_owned(), obj(vec![(name, raw)]))
            } else {
                (name.to_owned(), raw)
            };
            let list = members(at(&mut root, &segs));
            if *front {
                list.insert(0, member);
            } else {
                list.push(member);
            }
            if *nested {
                segs.push(key("nest"));
            }
            segs.push(key(name));
            want = Some(ai("RT-AI-0204", Field::Is(path_text(&segs))));
            mention = Some(name.to_owned());
        }
        Op::UnknownField { site, name } => {
            let name = UNKNOWN_NAMES[*name];
            let mut segs = site_segs(*site, p, e);
            members(at(&mut root, &segs)).push((name.to_owned(), J::Raw("1".to_owned())));
            segs.push(key(name));
            want = Some(ai("RT-AI-0202", Field::Is(path_text(&segs))));
        }
        Op::Remove { site, key: name } => {
            let segs = site_segs(*site, p, e);
            members(at(&mut root, &segs)).retain(|(k, _)| k != name);
            // Without `spans` the rule fails the equality with the evidence
            // spans, which is reported at the key (design 3.7.3).
            let field = if segs.is_empty() {
                Field::Absent
            } else if *site == Site::Rule && *name == "spans" {
                Field::Is(path_text(&[segs, vec![key("spans")]].concat()))
            } else {
                Field::Is(path_text(&segs))
            };
            want = Some(ai("RT-AI-0202", field));
            mention = Some((*name).to_owned());
        }
        Op::Retype {
            site,
            key: name,
            to,
        } => {
            let mut segs = site_segs(*site, p, e);
            let list = members(at(&mut root, &segs));
            list.iter_mut().find(|(k, _)| k == name).unwrap().1 = J::Raw((*to).to_owned());
            segs.push(key(name));
            want = Some(ai("RT-AI-0202", Field::Is(path_text(&segs))));
        }
        Op::ItemNotObject => {
            *at(&mut root, &prop_segs) = J::Raw("\"x\"".to_owned());
            want = Some(ai("RT-AI-0202", Field::Is(path_text(&prop_segs))));
        }
        Op::Duplicate { site, key: name } => {
            let mut segs = site_segs(*site, p, e);
            let list = members(at(&mut root, &segs));
            let copy = list.iter().find(|(k, _)| k == name).unwrap().clone();
            list.push(copy);
            segs.push(key(name));
            want = Some(ai("RT-AI-0202", Field::Is(path_text(&segs))));
        }
        Op::Depth => {
            let mut expr = obj(vec![
                ("node", s("bool")),
                ("t", s("boolean")),
                ("value", J::Raw("true".to_owned())),
            ]);
            for _ in 0..28 {
                expr = obj(vec![("node", s("not")), ("t", s("boolean")), ("arg", expr)]);
            }
            let mut segs = site_segs(Site::ExprRoot, p, e);
            *at(&mut root, &segs) = expr;
            segs.extend(std::iter::repeat_with(|| key("arg")).take(28));
            want = Some(ai("RT-AI-0202", Field::Is(path_text(&segs))));
        }
        Op::TooManyProposals => {
            let top = [key("proposals")];
            let J::Arr(items) = at(&mut root, &top) else {
                panic!("proposals is an array");
            };
            let copy = items[p].clone();
            while items.len() < 101 {
                items.push(copy.clone());
            }
            want = Some(ai("RT-AI-0202", Field::Is("proposals".to_owned())));
        }
        Op::TooManyEvidence => {
            let segs = [prop_segs.clone(), vec![key("evidence")]].concat();
            let J::Arr(items) = at(&mut root, &segs) else {
                panic!("evidence is an array");
            };
            let copy = items[0].clone();
            while items.len() < 17 {
                items.push(copy.clone());
            }
            want = Some(ai("RT-AI-0202", Field::Is(path_text(&segs))));
        }
        Op::ZeroEvidence => {
            let segs = [prop_segs.clone(), vec![key("evidence")]].concat();
            *at(&mut root, &segs) = J::Arr(Vec::new());
            want = Some(ai("RT-AI-0202", Field::Is(path_text(&segs))));
        }
        Op::BadVersion => {
            *at(&mut root, &[key("schema_version")]) = s("2");
            want = Some(ai("RT-AI-0202", Field::Is("schema_version".to_owned())));
        }
        Op::BadKind(k) => {
            let text = ["approval", "RULE_SUGGESTION", "", "unrecognized"][*k % 4];
            let segs = [prop_segs.clone(), vec![key("kind")]].concat();
            *at(&mut root, &segs) = s(text);
            want = Some(ai("RT-AI-0202", Field::Is(path_text(&segs))));
        }
        Op::BadRuleKind => {
            let segs = [prop_segs.clone(), vec![key("rule"), key("kind")]].concat();
            *at(&mut root, &segs) = s("shell");
            want = Some(ai("RT-AI-0202", Field::Is(path_text(&segs))));
        }
        Op::BadNode => {
            let segs = [site_segs(Site::ExprRoot, p, e), vec![key("right")]].concat();
            *at(&mut root, &segs) = obj(vec![("node", s("shell_exec")), ("script", s("x"))]);
            let field = path_text(&[segs, vec![key("node")]].concat());
            want = Some(ai("RT-AI-0202", Field::Is(field)));
        }
        Op::BadUnrecognizedNode => {
            let segs = [site_segs(Site::ExprRoot, p, e), vec![key("right")]].concat();
            *at(&mut root, &segs) = obj(vec![
                ("node", s("unrecognized")),
                ("t", s("number")),
                ("name", s("x")),
                ("detail", s("y")),
            ]);
            let field = path_text(&[segs, vec![key("t")]].concat());
            want = Some(ai("RT-AI-0202", Field::Is(field)));
        }
        Op::BadUnrecognizedRule => {
            let segs = site_segs(Site::Rule, p, e);
            members(at(&mut root, &segs)).push(("command".to_owned(), s("rm -rf /")));
            let field = path_text(&[segs, vec![key("command")]].concat());
            want = Some(ai("RT-AI-0202", Field::Is(field)));
        }
        Op::Credential { fragment, proposer } => {
            let text = if *fragment { &CANARY[4..12] } else { CANARY };
            let name = if *proposer { "proposer" } else { "rationale" };
            let segs = [prop_segs.clone(), vec![key(name)]].concat();
            *at(&mut root, &segs) = s(&format!("see {text} here"));
            canary = true;
            want = Some(ai("RT-AI-0206", Field::Any));
        }
        Op::Oversize | Op::InvalidUtf8 => {
            let segs = [prop_segs.clone(), vec![key("rationale")]].concat();
            *at(&mut root, &segs) = s("@@PAD@@");
        }
        _ => {}
    }

    let mut text = root.text();
    let payload: Vec<u8>;
    match op {
        Op::Oversize => {
            let have = u64::try_from(text.len()).unwrap();
            let pad = usize::try_from(LIMIT + 1 - have).unwrap() + "@@PAD@@".len();
            text = text.replace("@@PAD@@", &"a".repeat(pad));
            assert_eq!(u64::try_from(text.len()).unwrap(), LIMIT + 1);
            payload = text.into_bytes();
            want = Some(ai("RT-AI-0203", Field::Absent));
        }
        Op::InvalidUtf8 => {
            let at = text.find("@@PAD@@").unwrap();
            let column = u32::try_from(text[..at].chars().count()).unwrap() + 1;
            payload = [&text.as_bytes()[..at], &[0xFF], &text.as_bytes()[at + 7..]].concat();
            want = Some(Want {
                pos: Some((1, column)),
                ..ai("RT-AI-0201", Field::Is(format!("byte {at}")))
            });
        }
        Op::Truncate(permille) => {
            payload = text.as_bytes()[..text.len() * permille / 1000].to_vec();
            want = Some(ai("RT-AI-0201", Field::Any));
        }
        Op::Prose => {
            payload = format!("Here is the JSON:\n```json\n{text}\n```").into_bytes();
            want = Some(ai("RT-AI-0201", Field::Any));
        }
        Op::Refused | Op::Incomplete => {
            completion = if matches!(op, Op::Refused) {
                Completion::Refused
            } else {
                Completion::Incomplete
            };
            payload = text.into_bytes();
            want = Some(ai("RT-AI-0205", Field::Absent));
        }
        _ => payload = text.into_bytes(),
    }
    let mut want = want.expect("every operator states an expectation");
    want.mention = mention.or(want.mention);
    Built {
        payload,
        completion,
        want,
        canary,
    }
}

fn config() -> AiConfig {
    let documents: BTreeSet<DocumentId> = DOCS.iter().map(|(n, _)| doc(n)).collect();
    let at = Location::Cli(CliLocation::Argument {
        position: 3,
        name: Some("--consent".to_owned()),
    });
    let env = MapEnv::new(&[("PROVIDER_CREDENTIAL", "k")]);
    let mut config = load(
        &InputId::new("ai.toml").unwrap(),
        BASE,
        &Consent::named(documents.clone(), at),
        &documents,
        &env,
    )
    .unwrap()
    .config;
    config.max_clauses_per_request = 8;
    config
}

fn models() -> SourceModels {
    let mut all = SourceModels::new();
    for (name, text) in DOCS {
        all.insert(model(name, text)).unwrap();
    }
    all
}

fn rulebook() -> Rulebook {
    let documents: Vec<String> = DOCS
        .iter()
        .enumerate()
        .map(|(i, (name, _))| format!(r#"{{"id":"{name}","sha256":"{}"}}"#, hash_of(i)))
        .collect();
    let text = format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"n","version":"1","documents":[{}],"types":{{"inputs":[{{"name":"age","type":"number"}}],"enums":[],"sets":[],"collections":[]}},"rules":[],"precedence":[]}}"#,
        documents.join(",")
    );
    parse_rulebook(
        text.as_bytes(),
        &InputId::new("rb.json").unwrap(),
        8 << 20,
        64,
    )
    .map(|parsed| parsed.into_parts().0)
    .map_err(|r| r.into_diagnostics().as_slice().to_vec())
    .unwrap()
}

fn harness(redactor: Redactor) -> Harness {
    let config = config();
    assert_eq!(u64::from(config.limits.response_bytes), LIMIT);
    let all = models();
    let candidates: Vec<CandidateClause> = CLAUSES
        .iter()
        .map(|(d, start, end)| {
            let source = all.get(&doc(DOCS[*d].0)).unwrap();
            CandidateClause::new(source, *start, *end, None).unwrap()
        })
        .collect();
    let rulebook = rulebook();
    let plan = build_plan(&PlanInput {
        config: &config,
        models: &all,
        candidates: &candidates,
        rulebook: &rulebook,
        option: Ud32Option::RankReference,
        candidates_at: Location::Cli(CliLocation::Argument {
            position: 5,
            name: Some("--candidates".to_owned()),
        }),
    })
    .unwrap();
    assert_eq!(plan.requests().len(), 1);
    assert_eq!(plan.requests()[0].clauses().len(), CLAUSES.len());
    Harness {
        config,
        plan,
        models: all,
        rulebook,
        meta: meta(),
        redactor,
    }
}

fn provider(completion: Completion, payload: &[u8]) -> MockProvider {
    MockProvider::new(MockScript::from_steps(vec![Step {
        elapsed: Duration::from_millis(100),
        result: ScriptResult::Response {
            completion,
            payload: payload.to_vec(),
        },
    }]))
}

fn all_clauses(h: &Harness) -> Vec<ClauseId> {
    h.plan.requests()[0].clauses().to_vec()
}

fn field_of(d: &rt_foundation::Diagnostic) -> (Option<String>, Option<u32>, Option<u32>) {
    match d.primary() {
        Location::Input(l) => (l.field.clone(), l.line, l.column),
        other => panic!("not an input location: {other:?}"),
    }
}

fn assert_diagnostic(d: &rt_foundation::Diagnostic, want: &Want, label: &str) {
    assert_eq!(d.code().id(), want.id, "{label}: {}", d.message());
    assert_eq!(d.severity(), Severity::Error, "{label}");
    assert_eq!(d.code().exit_code().value(), want.exit, "{label}");
    let (field, line, column) = field_of(d);
    match &want.field {
        Field::Any => {}
        Field::Absent => assert_eq!(field, None, "{label}"),
        Field::Is(path) => assert_eq!(field.as_deref(), Some(path.as_str()), "{label}"),
    }
    if let Some((l, c)) = want.pos {
        assert_eq!((line, column), (Some(l), Some(c)), "{label}");
    }
    if let Some(text) = &want.mention {
        assert!(
            d.message().contains(text.as_str()),
            "{label}: {}",
            d.message()
        );
    }
}

// Checks one mutated response through `accept_batch` and through the
// whole call loop, and returns the code it was rejected with.
fn check_rejected(valid: &Spec, op: &Op, a: usize, b: usize) -> &'static str {
    let built = build(valid, op, a, b);
    let label = format!("{op:?} a={a} b={b}");
    let redactor = if built.canary {
        Redactor::new(&Credential::new(CANARY.to_owned()).unwrap())
    } else {
        Redactor::no_credential()
    };
    let h = harness(redactor);

    let response_id = InputId::new("response-1").unwrap();
    let request_id = sha256(b"request-bytes");
    let clauses = all_clauses(&h);
    let direct = accept_batch(&AcceptInput {
        response_id: &response_id,
        request_id: &request_id,
        completion: built.completion,
        payload: &built.payload,
        response_limit: LIMIT,
        models: &h.models,
        clauses: &clauses,
        declarations: &h.rulebook,
        redactor: &h.redactor,
    });
    let found = direct.expect_err(&label);
    assert_eq!(found.len(), 1, "{label}: {found:?}");
    assert_diagnostic(&found[0], &built.want, &label);

    let fs = MemFs::new();
    fs.plant("store/batches/sentinel.json", b"{\"kept\":true}");
    let before = fs.files();
    let mut mock = provider(built.completion, &built.payload);
    let sink = VecSink::default();
    let report = h.run(&mut mock, &fs, &sink);
    assert_eq!(fs.files(), before, "{label}: the store changed");
    for op_name in fs.ops() {
        assert!(op_name.starts_with("exists "), "{label}: wrote {op_name}");
    }
    assert_eq!(mock.steps_consumed(), 1, "{label}");
    assert_eq!(report.exit.value(), built.want.exit, "{label}");
    assert_eq!(
        shapes(&report.diagnostics),
        vec![(built.want.id, "error")],
        "{label}"
    );
    assert_diagnostic(&report.diagnostics[0], &built.want, &label);
    assert!(report.summary.stored().is_empty(), "{label}");
    assert!(report.summary.processed().is_empty(), "{label}");
    assert_eq!(report.summary.unprocessed(), clauses.as_slice(), "{label}");
    assert!(!report.summary.is_complete(), "{label}");
    if built.canary {
        for d in &report.diagnostics {
            assert!(!render_diagnostic(d).contains(CANARY), "{label}");
        }
        assert!(!report.summary.render(&h.redactor).contains(CANARY));
        assert!(!sink.text().contains(CANARY), "{label}");
    }
    built.want.id
}

fn raw_prop() -> impl Strategy<Value = (usize, u8, Vec<usize>, usize, usize)> {
    (
        0..8usize,
        0..100u8,
        proptest::collection::vec(0..VALID_SPANS.len(), 1..=4),
        0..PROPOSERS.len(),
        0..RATIONALES.len(),
    )
}

fn make_spec(raw: &[(usize, u8, Vec<usize>, usize, usize)], force: (usize, usize, usize)) -> Spec {
    let n = raw.len();
    let advisory = force.0 % n;
    let eligible = (advisory + 1 + force.1 % (n - 1)) % n;
    let rest: Vec<usize> = (0..n)
        .filter(|i| *i != advisory && *i != eligible)
        .collect();
    let unsupported = rest[force.2 % rest.len()];
    raw.iter()
        .enumerate()
        .map(|(i, (kind, threshold, spans, who, why))| {
            let kind = if i == advisory {
                PKind::Advisory(ADVISORY[*kind % 4])
            } else if i == eligible {
                PKind::Rule(RuleSpec::Eligible(*threshold))
            } else if i == unsupported {
                PKind::Rule(RuleSpec::Unsupported)
            } else {
                match kind {
                    0..=3 => PKind::Advisory(ADVISORY[*kind]),
                    4 => PKind::Rule(RuleSpec::Eligible(*threshold)),
                    5 => PKind::Rule(RuleSpec::Unsupported),
                    6 => PKind::Rule(RuleSpec::Undeclared),
                    _ => PKind::Rule(RuleSpec::Ambiguous),
                }
            };
            PropSpec {
                kind,
                claims: spans
                    .iter()
                    .map(|k| {
                        let (d, start, end) = VALID_SPANS[*k];
                        Claim::of(d, start, end)
                    })
                    .collect(),
                proposer: PROPOSERS[*who].to_owned(),
                rationale: RATIONALES[*why].to_owned(),
            }
        })
        .collect()
}

fn spec_strategy() -> impl Strategy<Value = Spec> {
    (
        proptest::collection::vec(raw_prop(), 3..=8),
        (any::<usize>(), any::<usize>(), any::<usize>()),
    )
        .prop_map(|(raw, force)| make_spec(&raw, force))
}

fn op_strategy() -> impl Strategy<Value = Op> {
    let families = families();
    assert_eq!(families.len(), FAMILY_COUNT);
    (0..FAMILY_COUNT, any::<usize>()).prop_map(move |(f, m)| {
        let family = &families[f];
        family[m % family.len()].clone()
    })
}

// Runs a valid response through the call loop and checks what is stored.
fn check_stored(spec: &Spec) {
    let h = harness(Redactor::no_credential());
    let text = response_json(spec).text();
    let fs = MemFs::new();
    let mut mock = provider(Completion::Complete, text.as_bytes());
    let sink = VecSink::default();
    let report = h.run(&mut mock, &fs, &sink);
    assert_eq!(report.exit.value(), 0, "{:?}", report.diagnostics);
    let blocking = spec.iter().any(|p| p.kind.blocking());
    let expected: Vec<(&str, &str)> = if blocking {
        vec![("RT-AI-0305", "warning")]
    } else {
        Vec::new()
    };
    assert_eq!(shapes(&report.diagnostics), expected);
    assert!(report.summary.is_complete());
    assert_eq!(report.summary.stored(), all_clauses(&h).as_slice());
    let names = fs.names_under("store/batches/");
    assert_eq!(names.len(), 1, "{names:?}");
    let bytes = fs.read(&names[0]).unwrap();
    let batch = parse_batch(&InputId::new("stored").unwrap(), &bytes).unwrap();
    assert_eq!(batch.response.payload, text);
    assert_eq!(batch.proposals.len(), spec.len());
    for (stored, given) in batch.proposals.iter().zip(spec) {
        assert_eq!(
            stored.kind == ProposalKind::RuleSuggestion,
            given.kind.is_rule()
        );
        assert_eq!(stored.rule.is_some(), given.kind.is_rule());
        assert_eq!(stored.proposer.as_str(), given.proposer);
        assert_eq!(stored.rationale, given.rationale);
        let codes: Vec<Code> = stored.outcome.iter().map(|d| d.code).collect();
        assert_eq!(codes, given.kind.findings());
        assert_eq!(stored.evidence.len(), given.claims.len());
        for (cited, claim) in stored.evidence.iter().zip(&given.claims) {
            assert_eq!(cited.document.as_str(), claim.doc);
            assert_eq!(cited.sha256.as_str(), claim.sha);
            assert_eq!((cited.start, cited.end), (claim.start, claim.end));
            assert_eq!((cited.line, cited.column), (claim.line, claim.column));
            assert_eq!(cited.quote, claim.quote);
        }
    }
}

fn rich_spec() -> Spec {
    let raw: Vec<(usize, u8, Vec<usize>, usize, usize)> = vec![
        (0, 0, vec![3, 5, 8], 0, 2),
        (4, 18, vec![0], 1, 1),
        (5, 0, vec![9, 10, 11], 2, 0),
        (6, 0, vec![12, 14], 3, 3),
        (7, 0, vec![4, 7], 0, 4),
        (1, 0, vec![13], 1, 0),
        (2, 0, vec![1, 2], 2, 1),
        (3, 0, vec![6], 3, 2),
    ];
    make_spec(&raw, (0, 0, 0))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn every_single_mutation_rejects_the_whole_batch_and_stores_nothing(
        spec in spec_strategy(),
        op in op_strategy(),
        a in any::<usize>(),
        b in any::<usize>(),
    ) {
        check_rejected(&spec, &op, a, b);
    }

    #[test]
    fn every_valid_response_is_accepted_and_stored(spec in spec_strategy()) {
        check_stored(&spec);
    }
}

#[test]
fn every_operator_and_every_expected_code_occurs() {
    let spec = rich_spec();
    let mut seen = BTreeSet::new();
    let mut operators = 0;
    for family in families() {
        for op in &family {
            for (a, b) in [(0, 0), (3, 1), (5, 2)] {
                seen.insert(check_rejected(&spec, op, a, b));
            }
            operators += 1;
        }
    }
    let want: BTreeSet<&str> = [
        "RT-AI-0201",
        "RT-AI-0202",
        "RT-AI-0203",
        "RT-AI-0204",
        "RT-AI-0205",
        "RT-AI-0206",
        "RT-PRV-0101",
        "RT-PRV-0102",
        "RT-PRV-0103",
        "RT-PRV-0104",
    ]
    .into();
    assert_eq!(seen, want);
    assert!(operators > 200, "{operators}");
    let forbidden_names: BTreeSet<usize> = families()[5]
        .iter()
        .filter_map(|op| match op {
            Op::Forbidden { name, .. } => Some(*name),
            _ => None,
        })
        .collect();
    assert_eq!(forbidden_names.len(), 13);
}

#[test]
fn the_rich_response_is_valid_and_stored() {
    check_stored(&rich_spec());
}

#[test]
fn semantic_findings_are_stored_as_proposed_data_and_never_reject() {
    let raw: Vec<(usize, u8, Vec<usize>, usize, usize)> = vec![
        (4, 18, vec![3], 0, 0),
        (5, 0, vec![5], 0, 0),
        (6, 0, vec![7], 0, 0),
        (7, 0, vec![8], 0, 0),
        (0, 0, vec![0], 0, 0),
    ];
    let spec = make_spec(&raw, (4, 0, 0));
    let kinds: Vec<PKind> = spec.iter().map(|p| p.kind).collect();
    assert!(kinds.contains(&PKind::Rule(RuleSpec::Unsupported)));
    assert!(kinds.contains(&PKind::Rule(RuleSpec::Undeclared)));
    assert!(kinds.contains(&PKind::Rule(RuleSpec::Ambiguous)));
    check_stored(&spec);

    let h = harness(Redactor::no_credential());
    let text = response_json(&spec).text();
    let response_id = InputId::new("response-1").unwrap();
    let request_id = sha256(b"request-bytes");
    let clauses = all_clauses(&h);
    let accepted = accept_batch(&AcceptInput {
        response_id: &response_id,
        request_id: &request_id,
        completion: Completion::Complete,
        payload: text.as_bytes(),
        response_limit: LIMIT,
        models: &h.models,
        clauses: &clauses,
        declarations: &h.rulebook,
        redactor: &h.redactor,
    })
    .unwrap();
    assert!(accepted.has_blocking_findings());
    let warning = accepted.blocking_warning().unwrap();
    assert_eq!(warning.code().id(), "RT-AI-0305");
    assert_eq!(warning.severity(), Severity::Warning);
    assert_eq!(warning.code().exit_code().value(), 0);
    let blocking: Vec<Severity> = accepted
        .proposals()
        .iter()
        .flat_map(|p| p.outcome.iter().map(|d| d.code.severity()))
        .collect();
    assert_eq!(
        blocking,
        vec![Severity::Unsupported, Severity::Error, Severity::Ambiguity]
    );
}

#[test]
fn a_seven_character_run_of_the_credential_is_not_an_echo() {
    let mut spec = rich_spec();
    spec[0].rationale = format!("see {} here", &CANARY[4..11]);
    let h = harness(Redactor::new(&Credential::new(CANARY.to_owned()).unwrap()));
    let text = response_json(&spec).text();
    let response_id = InputId::new("response-1").unwrap();
    let request_id = sha256(b"request-bytes");
    let clauses = all_clauses(&h);
    let input = |payload: &[u8]| {
        accept_batch(&AcceptInput {
            response_id: &response_id,
            request_id: &request_id,
            completion: Completion::Complete,
            payload,
            response_limit: LIMIT,
            models: &h.models,
            clauses: &clauses,
            declarations: &h.rulebook,
            redactor: &h.redactor,
        })
    };
    assert!(input(text.as_bytes()).is_ok());
    let eight = text.replace(&CANARY[4..11], &CANARY[4..12]);
    let found = input(eight.as_bytes()).unwrap_err();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code().id(), "RT-AI-0206");
}

#[test]
fn the_fixture_positions_are_the_hand_computed_ones() {
    assert_eq!(RULES_TEXT.len(), 50);
    assert_eq!(EXTRA_TEXT.len(), 19);
    assert_eq!(locate(RULES_TEXT, 0), (1, 1));
    assert_eq!(locate(RULES_TEXT, 6), (1, 7));
    assert_eq!(locate(RULES_TEXT, 13), (2, 1));
    assert_eq!(locate(RULES_TEXT, 22), (2, 9));
    assert_eq!(locate(RULES_TEXT, 30), (3, 1));
    assert_eq!(locate(RULES_TEXT, 37), (3, 6));
    assert_eq!(locate(RULES_TEXT, 42), (4, 1));
    assert_eq!(locate(RULES_TEXT, 47), (4, 6));
    assert_eq!(locate(EXTRA_TEXT, 14), (2, 5));
    assert_eq!(&RULES_TEXT[22..28], "delta.");
    assert_eq!(&RULES_TEXT[37..41], "本.");
    assert_eq!(non_boundaries(), vec![20, 35, 36, 38, 39, 48]);
}
