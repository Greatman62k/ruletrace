//! Questions: types, parser, canonical serializer and default-question generation.

use crate::common::{join, load, Ctx, VERSION};
use crate::data::CompetitionData;
use crate::ids::{ParticipantId, TargetName};
use rt_foundation::{DiagnosticState, InputId};
use rt_json::{write_pretty, Kind, ParseLimits, Position, Value};
use std::collections::BTreeSet;

const FORMAT: &str = "ruletrace-questions";

/// One item to decide. The engine returns one Outcome per Question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Question {
    /// The value of a Target for one participant.
    Value {
        /// The Target.
        target: TargetName,
        /// The participant.
        participant: ParticipantId,
    },
    /// The ranking of a Target over a participant set.
    Rank {
        /// The ranking Target.
        target: TargetName,
        /// The participants ranked, or `None` for all participants of the data.
        participants: Option<BTreeSet<ParticipantId>>,
    },
}

/// The default questions (RN-6): one `value` Question per participant (ascending id) and per
/// value Target (ascending name), then one `rank` Question per ranking Target over all
/// participants (ascending name).
#[must_use]
pub fn default_questions(
    data: &CompetitionData,
    value_targets: &BTreeSet<TargetName>,
    ranking_targets: &BTreeSet<TargetName>,
) -> Vec<Question> {
    let mut out = Vec::new();
    for participant in data.participants().keys() {
        for target in value_targets {
            out.push(Question::Value {
                target: target.clone(),
                participant: participant.clone(),
            });
        }
    }
    for target in ranking_targets {
        out.push(Question::Rank {
            target: target.clone(),
            participants: None,
        });
    }
    out
}

/// Parses a Questions file. Order is kept as written. Whether a Target or participant exists
/// is checked by the engine (`RT-EXE-0010`).
///
/// # Errors
/// `RT-LIM-0001`, `RT-LIM-0002`, `RT-DGN-0001` to `0003` as for Competition_Data, and
/// `RT-EXE-0009` for any other fault (unknown kind, unknown or missing key, repeated
/// participant in a `rank` Question, invalid name).
pub fn parse_questions(
    input: &InputId,
    bytes: &[u8],
    limits: ParseLimits,
) -> Result<Vec<Question>, DiagnosticState> {
    let root = load(input, bytes, limits)?;
    let mut ctx = Ctx {
        input,
        diags: Vec::new(),
    };
    let out = decode(&mut ctx, &root);
    ctx.finish(out)
}

fn target(ctx: &mut Ctx<'_>, v: &Value<Position>, path: &str) -> Option<TargetName> {
    let text = ctx.string(v, path)?;
    TargetName::new(text)
        .map_err(|e| ctx.bad(Some(*v.pos()), path, &e.to_string()))
        .ok()
}

fn participant(ctx: &mut Ctx<'_>, v: &Value<Position>, path: &str) -> Option<ParticipantId> {
    let text = ctx.string(v, path)?;
    ParticipantId::new(text)
        .map_err(|e| ctx.bad(Some(*v.pos()), path, &e.to_string()))
        .ok()
}

fn decode(ctx: &mut Ctx<'_>, root: &Value<Position>) -> Option<Vec<Question>> {
    let keys = ["format", "version", "questions"];
    let members = ctx.object(root, "", &keys, &keys)?;
    ctx.header(members, FORMAT);
    let list = members.iter().find(|m| m.key == "questions")?;
    let items = ctx.array(&list.value, "questions")?;
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let path = format!("questions[{index}]");
        let Some(kind) = item.get("kind") else {
            ctx.object(item, &path, &["kind"], &["kind"]);
            continue;
        };
        let kind_path = join(&path, "kind");
        match ctx.string(&kind.value, &kind_path) {
            Some("value") => {
                let keys = ["kind", "target", "participant"];
                let Some(m) = ctx.object(item, &path, &keys, &keys) else {
                    continue;
                };
                let t = m.iter().find(|x| x.key == "target");
                let p = m.iter().find(|x| x.key == "participant");
                if let (Some(t), Some(p)) = (t, p) {
                    let t = target(ctx, &t.value, &join(&path, "target"));
                    let p = participant(ctx, &p.value, &join(&path, "participant"));
                    if let (Some(target), Some(participant)) = (t, p) {
                        out.push(Question::Value {
                            target,
                            participant,
                        });
                    }
                }
            }
            Some("rank") => {
                let keys = ["kind", "target", "participants"];
                let Some(m) = ctx.object(item, &path, &keys, &keys) else {
                    continue;
                };
                let t = m.iter().find(|x| x.key == "target");
                let p = m.iter().find(|x| x.key == "participants");
                if let (Some(t), Some(p)) = (t, p) {
                    let t = target(ctx, &t.value, &join(&path, "target"));
                    let ppath = join(&path, "participants");
                    let set = if matches!(p.value.kind(), Kind::Null) {
                        Some(None)
                    } else {
                        decode_set(ctx, &p.value, &ppath).map(Some)
                    };
                    if let (Some(target), Some(participants)) = (t, set) {
                        out.push(Question::Rank {
                            target,
                            participants,
                        });
                    }
                }
            }
            Some(_) => ctx.bad(
                Some(*kind.value.pos()),
                &kind_path,
                "expected \"value\" or \"rank\"",
            ),
            None => {}
        }
    }
    Some(out)
}

fn decode_set(
    ctx: &mut Ctx<'_>,
    v: &Value<Position>,
    path: &str,
) -> Option<BTreeSet<ParticipantId>> {
    let items = ctx.array(v, path)?;
    let mut set = BTreeSet::new();
    for (i, item) in items.iter().enumerate() {
        let at = format!("{path}[{i}]");
        if let Some(id) = participant(ctx, item, &at) {
            if !set.insert(id) {
                ctx.bad(
                    Some(*item.pos()),
                    &at,
                    "participant is listed more than once",
                );
            }
        }
    }
    Some(set)
}

fn object(members: Vec<(&str, Value<()>)>) -> Value<()> {
    Value::object(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
    .unwrap_or_else(|_| Value::null())
}

/// The canonical bytes of a Questions list: RT-JSON pretty, fixed key order, the given order
/// of Questions, participants of a `rank` Question in ascending order.
#[must_use]
pub fn serialize_questions(questions: &[Question]) -> Vec<u8> {
    let items = questions
        .iter()
        .map(|q| match q {
            Question::Value {
                target,
                participant,
            } => object(vec![
                ("kind", Value::string("value")),
                ("target", Value::string(target.as_str())),
                ("participant", Value::string(participant.as_str())),
            ]),
            Question::Rank {
                target,
                participants,
            } => object(vec![
                ("kind", Value::string("rank")),
                ("target", Value::string(target.as_str())),
                (
                    "participants",
                    participants.as_ref().map_or_else(Value::null, |set| {
                        Value::array(set.iter().map(|p| Value::string(p.as_str())).collect())
                    }),
                ),
            ]),
        })
        .collect();
    let root = object(vec![
        ("format", Value::string(FORMAT)),
        ("version", Value::integer(VERSION)),
        ("questions", Value::array(items)),
    ]);
    write_pretty(&root).into_bytes()
}
