//! Provider response parsing and strict schema check (design 3.7.3, 3.7.4
//! steps 1 and 2; Req 23.4 to 23.6, 30.9, 22.16).
//!
//! [`parse_response`] turns untrusted payload bytes into typed proposals or
//! returns exactly one Diagnostic. The checks run in the order of Req 23.5:
//! too large, malformed, incomplete or refusal, forbidden field, schema
//! violation. No partial result is ever returned.
//!
//! Provenance (step 3) and semantic validation (step 4) are separate stages.

use rt_ai_adapter::Completion;
use rt_foundation::{
    escape_controls, Code, Diagnostic, DocumentId, InputId, InputLocation, Location,
    RevisionNumber, RuleId,
};
use rt_hash::Sha256Hex;
use rt_ir::{parse_draft, Identity, Origin, RuleContent};
use rt_json::{parse_keeping_duplicates, Kind, Member, ParseLimits, Position, Value};

/// The only schema version this parser accepts.
pub const SCHEMA_VERSION: &str = "1";
/// Largest number of proposals in one response.
pub const MAX_PROPOSALS: usize = 100;
/// Largest number of evidence items of one proposal.
pub const MAX_EVIDENCE: usize = 16;
/// Container nesting limit of a response (Req 23.5).
pub const MAX_DEPTH: u32 = 32;

/// Names that carry an approval or validation state. Present at any depth
/// they reject the response, whatever the value (Req 23.4, 30.9, 22.16).
pub const FORBIDDEN_FIELDS: [&str; 13] = [
    "approval",
    "approval_status",
    "status",
    "reviewer",
    "approved_by",
    "validation_status",
    "validation_result",
    "has_errors",
    "no_errors",
    "passed",
    "is_approved",
    "is_rejected",
    "is_proposed",
];

mod closed;

const SHOWN_CHARS: usize = 60;

/// A parsed, schema-checked response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedResponse {
    /// The proposals in response order.
    pub proposals: Vec<ParsedProposal>,
}

/// One proposal. Nothing here has been verified against a source document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedProposal {
    /// Kind and kind-specific content.
    pub kind: ProposalKind,
    /// Proposing identity, 1 to 200 scalar values.
    pub proposer: Identity,
    /// Free text explanation. Never trusted and never executed.
    pub rationale: String,
    /// Claimed source spans, 1 to 16.
    pub evidence: Vec<ParsedEvidence>,
}

/// The five proposal kinds of Req 18.2.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProposalKind {
    /// Advisory.
    ClauseInterpretation,
    /// A rule draft. Only this kind can lead to a Rule revision.
    RuleSuggestion(ParsedRule),
    /// Advisory.
    ClauseRelationship,
    /// Advisory.
    ConflictHighlight,
    /// Advisory.
    CandidateTest,
}

impl ProposalKind {
    /// The kind name used in the schema.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::ClauseInterpretation => "clause_interpretation",
            Self::RuleSuggestion(_) => "rule_suggestion",
            Self::ClauseRelationship => "clause_relationship",
            Self::ConflictHighlight => "conflict_highlight",
            Self::CandidateTest => "candidate_test",
        }
    }
}

/// A claimed source span with its quote. Unverified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedEvidence {
    /// Claimed document.
    pub document: DocumentId,
    /// Claimed document hash.
    pub sha256: Sha256Hex,
    /// Claimed start byte offset.
    pub start: u32,
    /// Claimed end byte offset.
    pub end: u32,
    /// Claimed 1-based line of `start`.
    pub line: u32,
    /// Claimed 1-based column of `start`.
    pub column: u32,
    /// Claimed text of the span.
    pub quote: String,
    /// Where each field was written in the response.
    pub site: EvidenceSite,
}

/// Position of an evidence item and of each of its fields in the response.
///
/// Provenance diagnostics use it to point at the offending field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceSite {
    /// Field path of the item, such as `proposals[0].evidence[1]`.
    pub path: String,
    /// Start of the item object.
    pub item: Position,
    /// Value of `document`.
    pub document: Position,
    /// Value of `sha256`.
    pub sha256: Position,
    /// Value of `start`.
    pub start: Position,
    /// Value of `end`.
    pub end: Position,
    /// Value of `line`.
    pub line: Position,
    /// Value of `column`.
    pub column: Position,
    /// Value of `quote`.
    pub quote: Position,
}

/// A rule draft that is in the closed vocabulary, decodes under the Rule_IR
/// decoder and has the same spans as its evidence.
///
/// Nothing else is guaranteed. Types, references and semantics are checked
/// later, and an `unrecognized` kind or node is carried for the validator
/// (RT-VAL-0003).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedRule {
    /// The rule identifier named by the draft.
    pub id: RuleId,
    /// The kind name, for an unrecognized kind the name the model gave.
    pub kind_name: String,
    /// The draft as the provider wrote it, in compact RT-JSON form with the
    /// provider's key order. The stored draft is the canonical print of
    /// `content`, not this text.
    pub json: String,
    /// The decoded Rule_Content, for the semantic validator.
    pub content: Box<RuleContent>,
}

/// Parses and schema-checks one provider response payload.
///
/// `response_id` names the response in every Diagnostic location.
/// `response_limit` is the configured response size limit in bytes.
///
/// # Errors
/// Exactly one Diagnostic, the first applicable of: `RT-AI-0203` (size),
/// `RT-AI-0201` (not UTF-8 or not parseable), `RT-AI-0205` (refusal or
/// incomplete), `RT-AI-0204` (forbidden field), `RT-AI-0202` (every other
/// violation, including a repeated key and nesting beyond [`MAX_DEPTH`]).
///
/// The Diagnostic can quote text of the untrusted payload, such as an
/// unknown key, and so can contain the credential. A caller that prints or
/// stores it goes through [`crate::accept_batch`] or a [`crate::Redactor`].
pub fn parse_response(
    response_id: &InputId,
    completion: Completion,
    payload: &[u8],
    response_limit: u64,
) -> Result<ParsedResponse, Diagnostic> {
    let at = Locator { id: response_id };
    let size = u64::try_from(payload.len()).unwrap_or(u64::MAX);
    if size > response_limit {
        return Err(Diagnostic::new(
            Code::AiResponseTooLarge,
            format!("provider response is {size} bytes, above the limit of {response_limit}"),
            at.plain(None),
        ));
    }
    let text = std::str::from_utf8(payload).map_err(|e| {
        let valid = e.valid_up_to();
        let prefix = payload
            .get(..valid)
            .and_then(|p| std::str::from_utf8(p).ok())
            .unwrap_or("");
        let (line, column) = line_column(prefix);
        Diagnostic::new(
            Code::AiResponseMalformed,
            format!("provider response is not valid UTF-8 at byte {valid}"),
            at.place(Some((line, column)), Some(format!("byte {valid}"))),
        )
    })?;
    let limits = ParseLimits::new(response_limit, u32::MAX);
    let parsed = parse_keeping_duplicates(response_id, payload, limits)
        .map_err(|d| malformed(&at, text, &d))?;
    if completion != Completion::Complete {
        return Err(Diagnostic::new(
            Code::AiResponseIncomplete,
            format!("provider response is not a completed answer: {completion:?}"),
            at.plain(None),
        ));
    }
    let tree = parsed.tree;
    let repeat = parsed
        .first_duplicate
        .as_deref()
        .and_then(|d| match d.primary() {
            Location::Input(l) => l.line.zip(l.column),
            _ => None,
        });
    let found = scan(&tree, repeat);
    if let Some((path, pos)) = found.forbidden {
        let name = path.rsplit('.').next().unwrap_or(&path);
        return Err(Diagnostic::new(
            Code::AiResponseForbiddenField,
            format!(
                "the response contains the forbidden field '{}'",
                shown(name.split('[').next().unwrap_or(name))
            ),
            at.at(pos, &path),
        ));
    }
    if let Some(d) = parsed.first_duplicate {
        let place = found
            .repeated
            .map_or_else(|| d.primary().clone(), |(path, pos)| at.at(pos, &path));
        return Err(Diagnostic::new(
            Code::AiResponseSchemaViolation,
            format!("the response has a repeated key: {}", d.message()),
            place,
        ));
    }
    if let Some((path, pos)) = found.deep {
        return Err(Diagnostic::new(
            Code::AiResponseSchemaViolation,
            format!("the response nests containers deeper than {MAX_DEPTH}"),
            at.at(pos, &path),
        ));
    }
    Dec {
        at,
        limit: response_limit,
    }
    .response(&tree)
}

struct Locator<'a> {
    id: &'a InputId,
}

impl Locator<'_> {
    fn place(&self, lc: Option<(u32, u32)>, field: Option<String>) -> Location {
        Location::Input(InputLocation {
            input: self.id.clone(),
            line: lc.map(|p| p.0),
            column: lc.map(|p| p.1),
            field,
        })
    }

    fn plain(&self, field: Option<String>) -> Location {
        self.place(None, field)
    }

    fn at(&self, pos: Position, path: &str) -> Location {
        let field = if path.is_empty() {
            None
        } else {
            Some(path.to_owned())
        };
        self.place(Some((pos.line, pos.column)), field)
    }
}

fn line_column(prefix: &str) -> (u32, u32) {
    let (mut line, mut column) = (1u32, 1u32);
    for c in prefix.chars() {
        if c == '\n' {
            line = line.saturating_add(1);
            column = 1;
        } else {
            column = column.saturating_add(1);
        }
    }
    (line, column)
}

fn shown(text: &str) -> String {
    let cut: String = text.chars().take(SHOWN_CHARS).collect();
    let more = if text.chars().nth(SHOWN_CHARS).is_some() {
        "..."
    } else {
        ""
    };
    format!("{}{more}", escape_controls(&cut))
}

fn malformed(at: &Locator<'_>, text: &str, d: &Diagnostic) -> Diagnostic {
    let lc = match d.primary() {
        Location::Input(l) => l.line.zip(l.column),
        _ => None,
    };
    let field = lc
        .and_then(|(line, column)| offset_of(text, line, column))
        .map(|o| format!("byte {o}"));
    Diagnostic::new(
        Code::AiResponseMalformed,
        format!("provider response is not parseable: {}", d.message()),
        at.place(lc, field),
    )
}

/// Byte offset of a 1-based line and column counted in scalar values.
fn offset_of(text: &str, line: u32, column: u32) -> Option<usize> {
    let (mut l, mut c) = (1u32, 1u32);
    for (index, ch) in text.char_indices() {
        if l == line && c == column {
            return Some(index);
        }
        if ch == '\n' {
            l = l.saturating_add(1);
            c = 1;
        } else {
            c = c.saturating_add(1);
        }
    }
    None
}

enum Seg<'a> {
    Key(&'a str),
    Index(usize),
}

struct Scan {
    forbidden: Option<(String, Position)>,
    repeated: Option<(String, Position)>,
    deep: Option<(String, Position)>,
}

fn render(arena: &[(Option<usize>, Seg<'_>)], mut node: Option<usize>) -> String {
    let mut segs = Vec::new();
    while let Some((parent, seg)) = node.and_then(|i| arena.get(i)) {
        segs.push(seg);
        node = *parent;
    }
    let mut out = String::new();
    for seg in segs.into_iter().rev() {
        match seg {
            Seg::Key(k) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(&escape_controls(k));
            }
            Seg::Index(n) => out.push_str(&format!("[{n}]")),
        }
    }
    out
}

type Entry<'a> = (
    &'a Value<Position>,
    u32,
    Option<usize>,
    Option<(&'a str, Position)>,
);

/// Finds, in document order, the first forbidden key, the key at the
/// line and column `repeat` and the first container nested deeper than
/// [`MAX_DEPTH`]. Every key of the tree is visited, repeated or not.
/// Iterative, so the depth of the input cannot exhaust the stack.
fn scan(root: &Value<Position>, repeat: Option<(u32, u32)>) -> Scan {
    let mut arena: Vec<(Option<usize>, Seg<'_>)> = Vec::new();
    let mut found = Scan {
        forbidden: None,
        repeated: None,
        deep: None,
    };
    let mut stack: Vec<Entry<'_>> = vec![(root, 0, None, None)];
    while let Some((value, parent_depth, node, key)) = stack.pop() {
        if let Some((name, pos)) = key {
            if found.forbidden.is_none() && FORBIDDEN_FIELDS.contains(&name) {
                found.forbidden = Some((render(&arena, node), pos));
            }
            if found.repeated.is_none() && repeat == Some((pos.line, pos.column)) {
                found.repeated = Some((render(&arena, node), pos));
            }
        }
        let depth = parent_depth.saturating_add(1);
        let is_container = matches!(value.kind(), Kind::Array(_) | Kind::Object(_));
        if is_container && depth > MAX_DEPTH && found.deep.is_none() {
            found.deep = Some((render(&arena, node), *value.pos()));
        }
        match value.kind() {
            Kind::Array(items) => {
                for (i, item) in items.iter().enumerate().rev() {
                    arena.push((node, Seg::Index(i)));
                    stack.push((item, depth, Some(arena.len() - 1), None));
                }
            }
            Kind::Object(members) => {
                for m in members.iter().rev() {
                    arena.push((node, Seg::Key(&m.key)));
                    let k = Some((m.key.as_str(), m.key_pos));
                    stack.push((&m.value, depth, Some(arena.len() - 1), k));
                }
            }
            _ => {}
        }
    }
    found
}

/// Position in `rule` of the key that the decoder reported at the line and
/// column of `d` in `compact`, the compact text of `rule`. Both trees have
/// the same shape, so the two are walked together.
fn key_position(rule: &Value<Position>, compact: &str, d: &Diagnostic) -> Option<Position> {
    let Location::Input(l) = d.primary() else {
        return None;
    };
    let target = l.line.zip(l.column)?;
    let id = InputId::new("compact").ok()?;
    let limits = ParseLimits::new(u64::try_from(compact.len()).ok()?, u32::MAX);
    let copy = rt_json::parse(&id, compact.as_bytes(), limits).ok()?;
    find_key(rule, &copy, target)
}

fn find_key(
    original: &Value<Position>,
    copy: &Value<Position>,
    target: (u32, u32),
) -> Option<Position> {
    match (original.kind(), copy.kind()) {
        (Kind::Array(a), Kind::Array(b)) => {
            a.iter().zip(b).find_map(|(x, y)| find_key(x, y, target))
        }
        (Kind::Object(a), Kind::Object(b)) => a.iter().zip(b).find_map(|(x, y)| {
            if (y.key_pos.line, y.key_pos.column) == target {
                Some(x.key_pos)
            } else {
                find_key(&x.value, &y.value, target)
            }
        }),
        _ => None,
    }
}

struct Dec<'a> {
    at: Locator<'a>,
    limit: u64,
}

type Members<'v> = &'v [Member<Position>];

fn member_pos(members: Members<'_>, owner: &Value<Position>, key: &str) -> Position {
    members
        .iter()
        .find(|m| m.key == key)
        .map_or(*owner.pos(), |m| *m.value.pos())
}

fn child(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

impl Dec<'_> {
    fn violation(&self, pos: Position, path: &str, message: String) -> Diagnostic {
        Diagnostic::new(
            Code::AiResponseSchemaViolation,
            message,
            self.at.at(pos, path),
        )
    }

    fn object<'v>(
        &self,
        value: &'v Value<Position>,
        path: &str,
        allowed: &[&str],
    ) -> Result<Members<'v>, Diagnostic> {
        let Some(members) = value.as_object() else {
            return Err(self.violation(
                *value.pos(),
                path,
                format!(
                    "{} must be an object",
                    if path.is_empty() {
                        "the response"
                    } else {
                        path
                    }
                ),
            ));
        };
        for m in members {
            if !allowed.contains(&m.key.as_str()) {
                return Err(self.violation(
                    m.key_pos,
                    &child(path, &m.key),
                    format!("unknown field '{}'", shown(&m.key)),
                ));
            }
        }
        Ok(members)
    }

    fn need<'v>(
        &self,
        members: Members<'v>,
        owner: &Value<Position>,
        path: &str,
        key: &str,
    ) -> Result<&'v Member<Position>, Diagnostic> {
        members.iter().find(|m| m.key == key).ok_or_else(|| {
            self.violation(
                *owner.pos(),
                path,
                format!("missing required field '{key}'"),
            )
        })
    }

    fn string<'v>(
        &self,
        members: Members<'v>,
        owner: &Value<Position>,
        path: &str,
        key: &str,
    ) -> Result<&'v str, Diagnostic> {
        let m = self.need(members, owner, path, key)?;
        m.value.as_str().ok_or_else(|| {
            self.violation(
                *m.value.pos(),
                &child(path, key),
                format!("field '{key}' must be a string"),
            )
        })
    }

    fn uint(
        &self,
        members: Members<'_>,
        owner: &Value<Position>,
        path: &str,
        key: &str,
    ) -> Result<u32, Diagnostic> {
        let m = self.need(members, owner, path, key)?;
        m.value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| {
                self.violation(
                    *m.value.pos(),
                    &child(path, key),
                    format!("field '{key}' must be an integer from 0 to 4294967295"),
                )
            })
    }

    fn response(&self, root: &Value<Position>) -> Result<ParsedResponse, Diagnostic> {
        let members = self.object(root, "", &["schema_version", "proposals"])?;
        let version = self.string(members, root, "", "schema_version")?;
        if version != SCHEMA_VERSION {
            let pos = self.need(members, root, "", "schema_version")?.value.pos();
            return Err(self.violation(
                *pos,
                "schema_version",
                format!("schema_version must be \"{SCHEMA_VERSION}\""),
            ));
        }
        let list = self.need(members, root, "", "proposals")?;
        let Some(items) = list.value.as_array() else {
            return Err(self.violation(
                *list.value.pos(),
                "proposals",
                "proposals must be an array".to_owned(),
            ));
        };
        if items.len() > MAX_PROPOSALS {
            return Err(self.violation(
                *list.value.pos(),
                "proposals",
                format!("proposals has {} items, above {MAX_PROPOSALS}", items.len()),
            ));
        }
        let mut proposals = Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            proposals.push(self.proposal(item, i)?);
        }
        Ok(ParsedResponse { proposals })
    }

    fn proposal(
        &self,
        value: &Value<Position>,
        index: usize,
    ) -> Result<ParsedProposal, Diagnostic> {
        let path = format!("proposals[{index}]");
        let Some(all) = value.as_object() else {
            return Err(self.violation(*value.pos(), &path, format!("{path} must be an object")));
        };
        let kind_member = self.need(all, value, &path, "kind")?;
        let kind_path = child(&path, "kind");
        let Some(name) = kind_member.value.as_str() else {
            return Err(self.violation(
                *kind_member.value.pos(),
                &kind_path,
                "field 'kind' must be a string".to_owned(),
            ));
        };
        let common = ["kind", "proposer", "rationale", "evidence"];
        let with_rule = ["kind", "proposer", "rationale", "evidence", "rule"];
        let allowed: &[&str] = match name {
            "rule_suggestion" => &with_rule,
            "clause_interpretation"
            | "clause_relationship"
            | "conflict_highlight"
            | "candidate_test" => &common,
            other => {
                return Err(self.violation(
                    *kind_member.value.pos(),
                    &kind_path,
                    format!("unknown proposal kind '{}'", shown(other)),
                ));
            }
        };
        let members = self.object(value, &path, allowed)?;
        let proposer = self.string(members, value, &path, "proposer")?;
        let proposer_member = self.need(members, value, &path, "proposer")?;
        let proposer = Identity::new(proposer).map_err(|e| {
            self.violation(
                *proposer_member.value.pos(),
                &child(&path, "proposer"),
                format!("proposer identity is invalid: {e:?}"),
            )
        })?;
        let rationale = self.string(members, value, &path, "rationale")?.to_owned();
        let evidence = self.evidence(members, value, &path)?;
        let kind = match name {
            "rule_suggestion" => {
                let rule = self.need(members, value, &path, "rule")?;
                let parsed = self.rule(&rule.value, &child(&path, "rule"), &evidence)?;
                ProposalKind::RuleSuggestion(parsed)
            }
            "clause_interpretation" => ProposalKind::ClauseInterpretation,
            "clause_relationship" => ProposalKind::ClauseRelationship,
            "conflict_highlight" => ProposalKind::ConflictHighlight,
            _ => ProposalKind::CandidateTest,
        };
        Ok(ParsedProposal {
            kind,
            proposer,
            rationale,
            evidence,
        })
    }

    fn evidence(
        &self,
        members: Members<'_>,
        owner: &Value<Position>,
        path: &str,
    ) -> Result<Vec<ParsedEvidence>, Diagnostic> {
        let m = self.need(members, owner, path, "evidence")?;
        let here = child(path, "evidence");
        let Some(items) = m.value.as_array() else {
            return Err(self.violation(
                *m.value.pos(),
                &here,
                "evidence must be an array".to_owned(),
            ));
        };
        if items.is_empty() || items.len() > MAX_EVIDENCE {
            return Err(self.violation(
                *m.value.pos(),
                &here,
                format!(
                    "evidence has {} items, expected 1 to {MAX_EVIDENCE}",
                    items.len()
                ),
            ));
        }
        items
            .iter()
            .enumerate()
            .map(|(i, item)| self.span(item, &format!("{here}[{i}]")))
            .collect()
    }

    fn span(&self, value: &Value<Position>, path: &str) -> Result<ParsedEvidence, Diagnostic> {
        let keys = [
            "document", "sha256", "start", "end", "line", "column", "quote",
        ];
        let members = self.object(value, path, &keys)?;
        let document = self.string(members, value, path, "document")?;
        let document = DocumentId::new(document).map_err(|e| {
            let pos = self.need(members, value, path, "document");
            self.violation(
                pos.map_or(*value.pos(), |m| *m.value.pos()),
                &child(path, "document"),
                format!("document identifier is invalid: {e:?}"),
            )
        })?;
        let hash = self.string(members, value, path, "sha256")?;
        let sha256 = Sha256Hex::parse(hash).ok_or_else(|| {
            let pos = self.need(members, value, path, "sha256");
            self.violation(
                pos.map_or(*value.pos(), |m| *m.value.pos()),
                &child(path, "sha256"),
                "sha256 must be 64 lowercase hexadecimal characters".to_owned(),
            )
        })?;
        let site = EvidenceSite {
            path: path.to_owned(),
            item: *value.pos(),
            document: member_pos(members, value, "document"),
            sha256: member_pos(members, value, "sha256"),
            start: member_pos(members, value, "start"),
            end: member_pos(members, value, "end"),
            line: member_pos(members, value, "line"),
            column: member_pos(members, value, "column"),
            quote: member_pos(members, value, "quote"),
        };
        Ok(ParsedEvidence {
            document,
            sha256,
            start: self.uint(members, value, path, "start")?,
            end: self.uint(members, value, path, "end")?,
            line: self.uint(members, value, path, "line")?,
            column: self.uint(members, value, path, "column")?,
            quote: self.string(members, value, path, "quote")?.to_owned(),
            site,
        })
    }

    /// Checks the closed vocabulary, decodes the rule with the Rule_IR draft
    /// decoder, so the field set of a rule is defined in one place, and
    /// checks that the rule spans equal the evidence spans (design 3.7.3).
    ///
    /// A decoder failure is reported at the start of the rule, except an
    /// unknown field that is the first decoder diagnostic, which is
    /// reported at its key. The field path names the offending member,
    /// which is what Req 23.5 requires.
    fn rule(
        &self,
        value: &Value<Position>,
        path: &str,
        evidence: &[ParsedEvidence],
    ) -> Result<ParsedRule, Diagnostic> {
        if let Err(b) = closed::check(value, path) {
            return Err(self.violation(b.pos, &b.path, b.message));
        }
        let json = rt_json::write_compact(value);
        let origin = Origin::Manual { author: None };
        let draft = parse_draft(
            json.as_bytes(),
            self.at.id,
            RevisionNumber::FIRST,
            origin,
            self.limit,
            MAX_DEPTH,
        );
        let rev = match draft {
            Ok(staged) => staged.into_parts().0,
            Err(rejected) => {
                let diagnostics = rejected.into_diagnostics();
                let first = diagnostics.as_slice().first();
                let inner = match first.map(Diagnostic::primary) {
                    Some(Location::Input(l)) => l.field.clone(),
                    _ => None,
                };
                let field = inner.map_or_else(|| path.to_owned(), |f| child(path, &f));
                let why = first.map_or_else(String::new, |d| {
                    format!("{}: {}", d.code().id(), d.message())
                });
                let pos = first
                    .filter(|d| d.code() == Code::IrUnknownField)
                    .and_then(|d| key_position(value, &json, d))
                    .unwrap_or(*value.pos());
                return Err(self.violation(
                    pos,
                    &field,
                    format!("the rule draft is not valid Rule_IR: {why}"),
                ));
            }
        };
        let same = rev.content.spans.len() == evidence.len()
            && rev.content.spans.iter().zip(evidence).all(|(s, e)| {
                s.document == e.document
                    && (s.start, s.end, s.line, s.column) == (e.start, e.end, e.line, e.column)
            });
        if !same {
            return Err(self.violation(
                *value.pos(),
                &child(path, "spans"),
                "the rule spans must equal the evidence spans".to_owned(),
            ));
        }
        Ok(ParsedRule {
            id: rev.key.id.clone(),
            kind_name: rev.content.body.kind_name().to_owned(),
            json,
            content: Box::new(rev.content),
        })
    }
}
