//! Published_Result: types, parser and canonical serializer.

use crate::common::{join, load, Ctx, VERSION};
use crate::ids::{ParticipantId, TargetName};
use rt_foundation::{DiagnosticState, InputId};
use rt_json::{write_pretty, Kind, ParseLimits, Position, Value};
use rt_numeric::{parse_literal, to_canonical, ExactNumber};
use std::collections::BTreeMap;

const FORMAT: &str = "ruletrace-published";

/// A published number, always read from a decimal literal (Req 8.27), so it serializes back to
/// a literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedNumber(ExactNumber);

impl PublishedNumber {
    /// Parses a decimal literal within the Exact_Number literal limits.
    ///
    /// # Errors
    /// The numeric fault, as text.
    pub fn parse(text: &str) -> Result<Self, String> {
        parse_literal(text)
            .map(Self)
            .map_err(|e| format!("invalid decimal literal: {e}"))
    }

    /// The exact value.
    #[must_use]
    pub fn value(&self) -> &ExactNumber {
        &self.0
    }
}

/// A published value for one participant and Target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublishedValue {
    /// An eligibility result.
    Bool(bool),
    /// A score, amount or Rank_Position.
    Number(PublishedNumber),
}

/// The published values, keyed by participant then Target, which is the order in which
/// differences are listed (design §3.6.3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PublishedResult {
    entries: BTreeMap<(ParticipantId, TargetName), PublishedValue>,
}

impl PublishedResult {
    /// Builds a result from entries.
    #[must_use]
    pub fn new(entries: BTreeMap<(ParticipantId, TargetName), PublishedValue>) -> Self {
        Self { entries }
    }

    /// The entries in participant then Target order.
    #[must_use]
    pub fn entries(&self) -> &BTreeMap<(ParticipantId, TargetName), PublishedValue> {
        &self.entries
    }

    /// One entry.
    #[must_use]
    pub fn get(&self, participant: &ParticipantId, target: &TargetName) -> Option<&PublishedValue> {
        self.entries.get(&(participant.clone(), target.clone()))
    }
}

/// Parses a Published_Result file.
///
/// # Errors
/// `RT-LIM-0001`, `RT-LIM-0002`, `RT-DGN-0001` to `0003` as for Competition_Data, and
/// `RT-EXE-0009` for any other fault: unknown or missing key, a value that is neither a
/// boolean nor a decimal literal string within the literal limits (a JSON number is refused),
/// a repeated (participant, target) pair, an invalid name.
pub fn parse_published(
    input: &InputId,
    bytes: &[u8],
    limits: ParseLimits,
) -> Result<PublishedResult, DiagnosticState> {
    let root = load(input, bytes, limits)?;
    let mut ctx = Ctx {
        input,
        diags: Vec::new(),
    };
    let out = decode(&mut ctx, &root);
    ctx.finish(out)
}

fn decode(ctx: &mut Ctx<'_>, root: &Value<Position>) -> Option<PublishedResult> {
    let keys = ["format", "version", "entries"];
    let members = ctx.object(root, "", &keys, &keys)?;
    ctx.header(members, FORMAT);
    let list = members.iter().find(|m| m.key == "entries")?;
    let items = ctx.array(&list.value, "entries")?;
    let mut out = BTreeMap::new();
    for (index, item) in items.iter().enumerate() {
        let path = format!("entries[{index}]");
        let keys = ["participant", "target", "value"];
        let Some(m) = ctx.object(item, &path, &keys, &keys) else {
            continue;
        };
        let get = |k: &str| m.iter().find(|x| x.key == k);
        let (Some(p), Some(t), Some(v)) = (get("participant"), get("target"), get("value")) else {
            continue;
        };
        let ppath = join(&path, "participant");
        let participant = ctx.string(&p.value, &ppath).and_then(|s| {
            ParticipantId::new(s)
                .map_err(|e| ctx.bad(Some(*p.value.pos()), &ppath, &e.to_string()))
                .ok()
        });
        let tpath = join(&path, "target");
        let target = ctx.string(&t.value, &tpath).and_then(|s| {
            TargetName::new(s)
                .map_err(|e| ctx.bad(Some(*t.value.pos()), &tpath, &e.to_string()))
                .ok()
        });
        let vpath = join(&path, "value");
        let value = match v.value.kind() {
            Kind::Bool(b) => Some(PublishedValue::Bool(*b)),
            Kind::String(s) => PublishedNumber::parse(s)
                .map(PublishedValue::Number)
                .map_err(|e| ctx.bad(Some(*v.value.pos()), &vpath, &e))
                .ok(),
            _ => {
                ctx.bad(
                    Some(*v.value.pos()),
                    &vpath,
                    "expected a boolean or a decimal literal string",
                );
                None
            }
        };
        if let (Some(participant), Some(target), Some(value)) = (participant, target, value) {
            if out.insert((participant, target), value).is_some() {
                ctx.bad(
                    Some(*item.pos()),
                    &path,
                    "the (participant, target) pair is listed more than once",
                );
            }
        }
    }
    Some(PublishedResult::new(out))
}

/// The canonical bytes of a Published_Result: RT-JSON pretty, fixed key order, entries by
/// participant then Target, numbers in canonical form.
#[must_use]
pub fn serialize_published(result: &PublishedResult) -> Vec<u8> {
    let items = result
        .entries
        .iter()
        .map(|((participant, target), value)| {
            let value = match value {
                PublishedValue::Bool(b) => Value::boolean(*b),
                PublishedValue::Number(n) => Value::string(to_canonical(n.value())),
            };
            Value::object(vec![
                (
                    "participant".to_owned(),
                    Value::string(participant.as_str()),
                ),
                ("target".to_owned(), Value::string(target.as_str())),
                ("value".to_owned(), value),
            ])
            .unwrap_or_else(|_| Value::null())
        })
        .collect();
    let root = Value::object(vec![
        ("format".to_owned(), Value::string(FORMAT)),
        ("version".to_owned(), Value::integer(VERSION)),
        ("entries".to_owned(), Value::array(items)),
    ])
    .unwrap_or_else(|_| Value::null());
    write_pretty(&root).into_bytes()
}
