//! The collections file: `participant,collection,row` plus one column per
//! element field of the declared collections.

use super::cells::{data_cell, participant};
use super::header::{index, invalid, missing, row_fits, unknown};
use super::report::{shown, Reporter};
use super::{reserved, InputSchema, Table};
use crate::{CompetitionData, Element, Participant, ParticipantId};
use rt_foundation::{Code, DiagnosticState, Location};
use std::collections::{BTreeMap, BTreeSet};

const REQUIRED: [&str; 3] = ["participant", "collection", "row"];

/// What the rows of one participant and collection said so far.
#[derive(Default)]
struct Group {
    elements: Vec<Element>,
    first_element: Option<Location>,
    first_empty: Option<Location>,
}

/// Maps a collections table onto the participants of `base`. A collection
/// with no row for a participant stays absent; an `empty` row makes it present
/// with no element; an `element` row adds one element whose empty cells are
/// missing fields. A collection the file names replaces the same collection
/// of `base`; the other collections and fields of `base` are kept.
///
/// A header name of Req 22.22 is reported alone: no later stage reads the
/// file.
///
/// # Errors
/// `RT-APP-0001` for an unknown, empty or repeated column, `RT-APP-0002` for
/// an absent `participant`, `collection`, `row` or element field column,
/// `RT-APP-0004` for a participant that `base` does not have, an undeclared
/// collection, a `row` other than `element` or `empty`, an invalid cell, a
/// cell in a column that is not a field of the row's collection, a non-empty
/// cell of an `empty` row, and an `empty` row beside an `element` row of the
/// same participant and collection, and `RT-DGN-0001` to `0003` for a
/// reserved name.
pub fn map_collections(
    table: &Table,
    schema: &InputSchema,
    base: CompetitionData,
) -> Result<CompetitionData, DiagnosticState> {
    let mut rep = Reporter::new(&table.input);
    if reserved::check_header(table, &mut rep) {
        return Err(rep.into_state());
    }
    let header = index(table, &mut rep);
    let field_names: BTreeSet<&str> = schema
        .collections()
        .values()
        .flat_map(|fields| fields.keys().map(String::as_str))
        .collect();
    for name in REQUIRED {
        if !header.by_name.contains_key(name) {
            missing(table, &mut rep, name);
        }
    }
    for name in &field_names {
        if !header.by_name.contains_key(name) {
            missing(table, &mut rep, name);
        }
    }
    for (i, cell) in table.header.iter().enumerate() {
        let name = cell.text.as_str();
        let first = header.by_name.get(name) == Some(&i);
        if first && !REQUIRED.contains(&name) && !field_names.contains(name) {
            unknown(&mut rep, cell);
        }
    }
    let at = |name: &str| header.by_name.get(name).copied();
    let (Some(pc), Some(cc), Some(rc)) = (at("participant"), at("collection"), at("row")) else {
        return Err(rep.into_state());
    };
    let mut groups: BTreeMap<(ParticipantId, String), Group> = BTreeMap::new();
    for row in &table.rows {
        if !row_fits(table, row, &mut rep) {
            continue;
        }
        let (Some(pcell), Some(ccell), Some(rcell)) = (row.get(pc), row.get(cc), row.get(rc))
        else {
            continue;
        };
        let known = participant(&pcell.text)
            .ok()
            .filter(|id| base.participant(id).is_some());
        if known.is_none() {
            let why = "no such participant in the participants file";
            invalid(&mut rep, pcell, "participant", why);
        }
        let fields = schema.collections().get(&ccell.text);
        if fields.is_none() {
            invalid(&mut rep, ccell, "collection", "not a declared collection");
        }
        let kind = match rcell.text.as_str() {
            "element" => Some(true),
            "empty" => Some(false),
            _ => {
                invalid(&mut rep, rcell, "row", "expected `element` or `empty`");
                None
            }
        };
        let (Some(fields), Some(is_element)) = (fields, kind) else {
            continue;
        };
        let mut element = Element::new();
        let mut cells_ok = true;
        for (i, head) in table.header.iter().enumerate() {
            if REQUIRED.contains(&head.text.as_str())
                || header.by_name.get(head.text.as_str()) != Some(&i)
            {
                continue;
            }
            let Some(cell) = row.get(i) else { continue };
            let name = head.text.as_str();
            if cell.text.is_empty() {
                continue;
            }
            if !is_element {
                invalid(&mut rep, cell, name, "an `empty` row has only empty cells");
                cells_ok = false;
                continue;
            }
            let Some(ty) = fields.get(name) else {
                let why = format!("not a field of collection `{}`", shown(&ccell.text));
                invalid(&mut rep, cell, name, &why);
                cells_ok = false;
                continue;
            };
            match data_cell(ty, &cell.text) {
                Ok(Some(value)) => {
                    element.insert(name.to_owned(), value);
                }
                Ok(None) => {}
                Err(why) => {
                    invalid(&mut rep, cell, name, &why);
                    cells_ok = false;
                }
            }
        }
        let Some(id) = known else { continue };
        if !cells_ok {
            continue;
        }
        let group = groups.entry((id, ccell.text.clone())).or_default();
        let here = rep.cell(rcell, "row");
        if is_element {
            group.first_element.get_or_insert(here);
            group.elements.push(element);
        } else {
            group.first_empty.get_or_insert(here);
        }
    }
    for group in groups.values() {
        if let (Some(empty), Some(element)) = (&group.first_empty, &group.first_element) {
            rep.push_related(
                Code::AppInputValueInvalid,
                empty.clone(),
                "an `empty` row contradicts an `element` row of the same participant and collection"
                    .to_owned(),
                element.clone(),
            );
        }
    }
    if !rep.is_clean() {
        return Err(rep.into_state());
    }
    let mut out = BTreeMap::new();
    for (id, participant) in base.participants() {
        let mut collections = participant.collections().clone();
        for ((owner, name), group) in &groups {
            if owner == id {
                collections.insert(name.clone(), group.elements.clone());
            }
        }
        out.insert(
            id.clone(),
            Participant::new(participant.fields().clone(), collections),
        );
    }
    Ok(CompetitionData::new(out))
}
