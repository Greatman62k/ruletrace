//! The model catalogue of `GET {endpoint}/v1/models` (Req 35.18).
//!
//! The answer is untrusted. The list is the `id` of each entry of the
//! `data` array, as reported. Members that the parser does not name are
//! ignored, because the catalogue is read for display and for a membership
//! check only. An entry without a string `id`, or an answer that is not
//! one object with a `data` array, is malformed and no partial list is
//! returned.

use rt_foundation::InputId;
use rt_json::{parse, ParseLimits};

const MAX_DEPTH: u32 = 16;

/// The identifiers of a catalogue body, in the order reported.
///
/// # Errors
/// A fixed text naming the first shape violation.
pub(crate) fn parse_catalogue(body: &[u8]) -> Result<Vec<String>, &'static str> {
    let id = InputId::new("model catalogue").map_err(|_| "the catalogue cannot be named")?;
    let limits = ParseLimits::new(u64::try_from(body.len()).unwrap_or(u64::MAX), MAX_DEPTH);
    let root = parse(&id, body, limits).map_err(|_| "the catalogue is not valid JSON")?;
    if root.as_object().is_none() {
        return Err("the catalogue is not an object");
    }
    let data = root
        .get("data")
        .and_then(|m| m.value.as_array())
        .ok_or("the catalogue has no data array")?;
    data.iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(|m| m.value.as_str())
                .map(str::to_owned)
                .ok_or("a catalogue entry has no string id")
        })
        .collect()
}
