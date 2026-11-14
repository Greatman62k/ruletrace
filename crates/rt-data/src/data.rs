//! Competition_Data: types, strict parser and canonical serializer.

use crate::common::{default_parse_limits, describe, join, load, Ctx, VERSION};
use crate::ids::ParticipantId;
use rt_foundation::{
    Code, Diagnostic, DiagnosticState, InputId, Location, MAX_PARTICIPANTS,
    MAX_PER_PARTICIPANT_ELEMENTS,
};
use rt_json::{write_pretty, Kind, ParseLimits, Position, Value};
use std::collections::BTreeMap;

const FORMAT: &str = "ruletrace-data";

/// A present value. A Missing_Value has no representation: it is the absence of an entry.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DataValue {
    /// `true` or `false`. `false` is present.
    Bool(bool),
    /// Text, including numbers (Exact_Number strings), dates and enum members. `""` is present.
    /// The engine checks it against the declared type.
    Text(String),
}

/// One element of a collection: its present fields by name.
pub type Element = BTreeMap<String, DataValue>;

/// One participant. Field and collection names are ordered bytewise.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Participant {
    fields: BTreeMap<String, DataValue>,
    collections: BTreeMap<String, Vec<Element>>,
}

impl Participant {
    /// Builds a participant. Elements of each collection are a multiset, so they are sorted and
    /// input order never matters (Req 10.4).
    #[must_use]
    pub fn new(
        fields: BTreeMap<String, DataValue>,
        mut collections: BTreeMap<String, Vec<Element>>,
    ) -> Self {
        for elements in collections.values_mut() {
            elements.sort();
        }
        Self {
            fields,
            collections,
        }
    }

    /// The value of a field, or `None` when it is a Missing_Value.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&DataValue> {
        self.fields.get(name)
    }

    /// The present fields.
    #[must_use]
    pub fn fields(&self) -> &BTreeMap<String, DataValue> {
        &self.fields
    }

    /// A collection, or `None` when the data does not supply it (missing information, Req 8.29).
    /// `Some` with no element is a present, empty collection.
    #[must_use]
    pub fn collection(&self, name: &str) -> Option<&[Element]> {
        self.collections.get(name).map(Vec::as_slice)
    }

    /// The supplied collections.
    #[must_use]
    pub fn collections(&self) -> &BTreeMap<String, Vec<Element>> {
        &self.collections
    }
}

/// The participants of one evaluation, keyed by identifier.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompetitionData {
    participants: BTreeMap<ParticipantId, Participant>,
}

impl CompetitionData {
    /// Builds data from participants.
    #[must_use]
    pub fn new(participants: BTreeMap<ParticipantId, Participant>) -> Self {
        Self { participants }
    }

    /// The participants in ascending identifier order.
    #[must_use]
    pub fn participants(&self) -> &BTreeMap<ParticipantId, Participant> {
        &self.participants
    }

    /// One participant.
    #[must_use]
    pub fn participant(&self, id: &ParticipantId) -> Option<&Participant> {
        self.participants.get(id)
    }
}

/// The limits of one load (design §4.1.5). A value equal to a limit is accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataLimits {
    /// File size and container depth.
    pub parse: ParseLimits,
    /// Participants (default 100,000).
    pub max_participants: u32,
    /// Collection elements per participant, over all collections (default 100,000).
    pub max_elements_per_participant: u32,
}

impl Default for DataLimits {
    fn default() -> Self {
        Self {
            parse: default_parse_limits(),
            max_participants: MAX_PARTICIPANTS,
            max_elements_per_participant: MAX_PER_PARTICIPANT_ELEMENTS,
        }
    }
}

/// Parses a Competition_Data file. All faults of the file are reported, in canonical order.
///
/// # Errors
/// - `RT-LIM-0001` and `RT-LIM-0002` for size and depth, `RT-LIM-0004` for too many
///   participants or elements.
/// - `RT-EXE-0008` for a repeated participant identifier (the first is a related location).
/// - `RT-EXE-0009` for any other fault: syntax, unknown or missing key, wrong shape, a JSON
///   number where a string is required, an invalid identifier.
/// - `RT-DGN-0001` to `0003` for a flag or lifecycle key (Req 22.22).
pub fn parse_data(
    input: &InputId,
    bytes: &[u8],
    limits: DataLimits,
) -> Result<CompetitionData, DiagnosticState> {
    let root = load(input, bytes, limits.parse)?;
    let mut ctx = Ctx {
        input,
        diags: Vec::new(),
    };
    let data = decode(&mut ctx, &root, limits);
    ctx.finish(data)
}

fn decode(
    ctx: &mut Ctx<'_>,
    root: &Value<Position>,
    limits: DataLimits,
) -> Option<CompetitionData> {
    let members = ctx.object(
        root,
        "",
        &["format", "version", "participants"],
        &["format", "version", "participants"],
    )?;
    ctx.header(members, FORMAT);
    let list = members.iter().find(|m| m.key == "participants")?;
    let items = ctx.array(&list.value, "participants")?;
    if u32::try_from(items.len()).map_or(true, |n| n > limits.max_participants) {
        ctx.push(
            Code::LimitCountExceeded,
            Some(*list.value.pos()),
            "participants",
            &format!(
                "{} participants exceed the limit of {}",
                items.len(),
                limits.max_participants
            ),
        );
        return None;
    }
    let mut out: BTreeMap<ParticipantId, Participant> = BTreeMap::new();
    let mut first_seen: BTreeMap<ParticipantId, Location> = BTreeMap::new();
    for (index, item) in items.iter().enumerate() {
        let path = format!("participants[{index}]");
        let Some(m) = ctx.object(item, &path, &["id", "fields", "collections"], &["id"]) else {
            continue;
        };
        let id = m.iter().find(|x| x.key == "id").and_then(|x| {
            let p = join(&path, "id");
            let text = ctx.string(&x.value, &p)?;
            match ParticipantId::new(text) {
                Ok(id) => Some((id, *x.value.pos())),
                Err(e) => {
                    ctx.bad(Some(*x.value.pos()), &p, &e.to_string());
                    None
                }
            }
        });
        let fields = m
            .iter()
            .find(|x| x.key == "fields")
            .map(|x| decode_fields(ctx, &x.value, &join(&path, "fields")))
            .unwrap_or_default();
        let collections = m
            .iter()
            .find(|x| x.key == "collections")
            .map(|x| decode_collections(ctx, &x.value, &join(&path, "collections")))
            .unwrap_or_default();
        let total: usize = collections.values().map(Vec::len).sum();
        if u32::try_from(total).map_or(true, |n| n > limits.max_elements_per_participant) {
            ctx.push(
                Code::LimitCountExceeded,
                Some(*item.pos()),
                &join(&path, "collections"),
                &format!(
                    "{total} collection elements exceed the limit of {} per participant",
                    limits.max_elements_per_participant
                ),
            );
            continue;
        }
        let Some((id, at)) = id else { continue };
        let here = ctx.location(Some(at), &join(&path, "id"));
        if let Some(first) = first_seen.get(&id) {
            let d = Diagnostic::new(
                Code::DataDuplicateParticipant,
                format!(
                    "{}: participant '{id}' is listed more than once",
                    join(&path, "id")
                ),
                here,
            )
            .with_related(vec![first.clone()]);
            ctx.diags.push(d);
            continue;
        }
        first_seen.insert(id.clone(), here);
        out.insert(id, Participant::new(fields, collections));
    }
    Some(CompetitionData::new(out))
}

fn decode_fields(
    ctx: &mut Ctx<'_>,
    value: &Value<Position>,
    path: &str,
) -> BTreeMap<String, DataValue> {
    let mut out = BTreeMap::new();
    let Some(members) = value.as_object() else {
        ctx.bad(Some(*value.pos()), path, "expected an object");
        return out;
    };
    for m in members {
        let at = join(path, &m.key);
        if m.key.is_empty() {
            ctx.bad(Some(m.key_pos), &at, "field name is empty");
            continue;
        }
        match m.value.kind() {
            Kind::Null => {}
            Kind::Bool(b) => {
                out.insert(m.key.clone(), DataValue::Bool(*b));
            }
            Kind::String(s) => {
                out.insert(m.key.clone(), DataValue::Text(s.clone()));
            }
            other => ctx.bad(
                Some(*m.value.pos()),
                &at,
                &format!(
                    "expected a string, a boolean or null, found {} (numbers are written as strings)",
                    describe(other)
                ),
            ),
        }
    }
    out
}

fn decode_collections(
    ctx: &mut Ctx<'_>,
    value: &Value<Position>,
    path: &str,
) -> BTreeMap<String, Vec<Element>> {
    let mut out = BTreeMap::new();
    let Some(members) = value.as_object() else {
        ctx.bad(Some(*value.pos()), path, "expected an object");
        return out;
    };
    for m in members {
        let at = join(path, &m.key);
        if m.key.is_empty() {
            ctx.bad(Some(m.key_pos), &at, "collection name is empty");
            continue;
        }
        let Some(items) = ctx.array(&m.value, &at) else {
            continue;
        };
        let elements = items
            .iter()
            .enumerate()
            .map(|(i, e)| decode_fields(ctx, e, &format!("{at}[{i}]")))
            .collect();
        out.insert(m.key.clone(), elements);
    }
    out
}

fn object(members: Vec<(&str, Value<()>)>) -> Value<()> {
    // Callers pass distinct constant keys or keys of a map, so building cannot fail.
    Value::object(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
    .unwrap_or_else(|_| Value::null())
}

fn fields_value(fields: &BTreeMap<String, DataValue>) -> Value<()> {
    let members = fields
        .iter()
        .map(|(k, v)| {
            let v = match v {
                DataValue::Bool(b) => Value::boolean(*b),
                DataValue::Text(t) => Value::string(t.as_str()),
            };
            (k.clone(), v)
        })
        .collect();
    Value::object(members).unwrap_or_else(|_| Value::null())
}

/// The canonical bytes of Competition_Data: RT-JSON pretty, fixed key order, participants by
/// ascending identifier, one trailing line feed. Equal data always gives equal bytes.
#[must_use]
pub fn serialize_data(data: &CompetitionData) -> Vec<u8> {
    let participants = data
        .participants
        .iter()
        .map(|(id, p)| {
            let collections = p
                .collections
                .iter()
                .map(|(name, elements)| {
                    (
                        name.clone(),
                        Value::array(elements.iter().map(fields_value).collect()),
                    )
                })
                .collect();
            object(vec![
                ("id", Value::string(id.as_str())),
                ("fields", fields_value(&p.fields)),
                (
                    "collections",
                    Value::object(collections).unwrap_or_else(|_| Value::null()),
                ),
            ])
        })
        .collect();
    let root = object(vec![
        ("format", Value::string(FORMAT)),
        ("version", Value::integer(VERSION)),
        ("participants", Value::array(participants)),
    ]);
    write_pretty(&root).into_bytes()
}
