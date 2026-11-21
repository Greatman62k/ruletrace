//! Cell syntax by declared type (design §8.3). A cell is never trimmed or
//! repaired: it is exactly as declared or it is invalid.

use super::{FieldType, TargetKind};
use crate::{DataValue, ParticipantId, PublishedNumber, PublishedValue};
use rt_numeric::parse_literal;

/// Reads a cell of a typed field. `Ok(None)` is a Missing_Value: an empty cell
/// is missing for every type and is never turned into a default.
pub(super) fn data_cell(ty: &FieldType, text: &str) -> Result<Option<DataValue>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    let value = match ty {
        FieldType::Number => {
            parse_literal(text).map_err(|e| format!("not a valid decimal literal: {e}"))?;
            DataValue::Text(text.to_owned())
        }
        FieldType::Boolean => match text {
            "true" => DataValue::Bool(true),
            "false" => DataValue::Bool(false),
            _ => return Err("expected `true` or `false`".to_owned()),
        },
        FieldType::Date => {
            if !is_calendar_date(text) {
                return Err("expected a valid calendar date YYYY-MM-DD".to_owned());
            }
            DataValue::Text(text.to_owned())
        }
        FieldType::Text => DataValue::Text(text.to_owned()),
        FieldType::Enum(members) => {
            if !members.contains(text) {
                return Err("not a member of the declared enumeration".to_owned());
            }
            DataValue::Text(text.to_owned())
        }
    };
    Ok(Some(value))
}

/// `YYYY-MM-DD` with year 0001 to 9999 and a day that exists in that month.
fn is_calendar_date(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() != 10 || b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') {
        return false;
    }
    let number = |from: usize, to: usize| -> Option<u32> {
        b.get(from..to)?.iter().try_fold(0u32, |acc, d| {
            d.is_ascii_digit().then(|| acc * 10 + u32::from(*d - b'0'))
        })
    };
    let (Some(year), Some(month), Some(day)) = (number(0, 4), number(5, 7), number(8, 10)) else {
        return false;
    };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let last = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year >= 1 && (1..=last).contains(&day)
}

/// Reads a participant identifier cell.
pub(super) fn participant(text: &str) -> Result<ParticipantId, String> {
    ParticipantId::new(text).map_err(|e| e.to_string())
}

/// Reads a published value by the kind of its Target. An empty cell has no
/// Missing_Value form in a Published_Result, so it is invalid.
pub(super) fn published_cell(kind: TargetKind, text: &str) -> Result<PublishedValue, String> {
    match kind {
        TargetKind::Eligibility => match text {
            "true" => Ok(PublishedValue::Bool(true)),
            "false" => Ok(PublishedValue::Bool(false)),
            _ => Err("expected `true` or `false`".to_owned()),
        },
        TargetKind::Derived | TargetKind::Score => {
            PublishedNumber::parse(text).map(PublishedValue::Number)
        }
        TargetKind::Ranking => {
            let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
            if !digits {
                return Err("a Rank_Position is a positive integer".to_owned());
            }
            let n = PublishedNumber::parse(text)?;
            if n.value().is_zero() {
                return Err("a Rank_Position is a positive integer".to_owned());
            }
            Ok(PublishedValue::Number(n))
        }
        // Callers reject this kind before reading a value.
        TargetKind::Other => Err("the Target kind cannot be reported".to_owned()),
    }
}
