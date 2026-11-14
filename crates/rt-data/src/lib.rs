//! Competition_Data, Questions and Published_Result: typed values, strict parsers with the
//! limits of design §4.1.5, and canonical RT-JSON serializers (design §3.6.3, §3.9.1).
//!
//! The crate does no file access: callers pass bytes and an [`InputId`](rt_foundation::InputId).
//! It holds no competition knowledge. Field types come from the Compiled_Rulebook schema and
//! are applied by the engine, so a data value is only a boolean or text here. A number is text
//! (Req 8.27) that the engine parses with `rt-numeric` against the declared type.
//!
//! # File formats (all canonical RT-JSON, pretty form, fixed key order)
//!
//! Competition_Data:
//! `{"format":"ruletrace-data","version":1,"participants":[{"id":"p1","fields":{"age":"17",
//! "licence":true},"collections":{"results":[{"place":"1"}]}}]}`.
//! Participants are sorted by id. A field that is absent or `null` is a Missing_Value and is
//! not stored, so it can never be mistaken for zero, `false` or `""`. A collection that is
//! absent is missing information; `[]` is a present, empty collection. Elements of a
//! collection are a multiset and are stored sorted.
//!
//! Questions: `{"format":"ruletrace-questions","version":1,"questions":[{"kind":"value",
//! "target":"..","participant":".."},{"kind":"rank","target":"..","participants":null}]}`.
//!
//! Published_Result: `{"format":"ruletrace-published","version":1,"entries":[{"participant":"..",
//! "target":"..","value":true|"<decimal literal>"}]}`, sorted by participant then target.
//! A rank position is a decimal literal string like every other number.
#![forbid(unsafe_code)]

mod common;
mod data;
mod ids;
mod published;
mod questions;

pub use common::default_parse_limits;
pub use data::{
    parse_data, serialize_data, CompetitionData, DataLimits, DataValue, Element, Participant,
};
pub use ids::{NameError, ParticipantId, TargetName};
pub use published::{
    parse_published, serialize_published, PublishedNumber, PublishedResult, PublishedValue,
};
pub use questions::{default_questions, parse_questions, serialize_questions, Question};

#[cfg(test)]
mod tests;
