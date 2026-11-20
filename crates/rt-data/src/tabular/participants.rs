//! The participants file: `participant` plus one column per declared input.

use super::cells::{data_cell, participant};
use super::header::{index, invalid, missing, row_fits, unknown};
use super::report::{shown, Reporter};
use super::{reserved, InputSchema, MappingOptions, Table};
use crate::{CompetitionData, DataValue, Participant, ParticipantId};
use rt_foundation::{Code, DiagnosticState, Location};
use std::collections::BTreeMap;

const PARTICIPANT: &str = "participant";

/// Maps a participants table: a Missing_Value is an absent field, and every
/// other cell is typed by the schema.
///
/// A header name of Req 22.22 is reported alone: no later stage reads the
/// file. A declared input that is also named by `ignore_columns` is still
/// read, because only columns that are not declared inputs can be ignored.
///
/// # Errors
/// `RT-APP-0001` for an unknown, empty or repeated column, `RT-APP-0002` for
/// an absent `participant` or declared input column, `RT-APP-0003` for a
/// repeated participant (the first row is a related location), `RT-APP-0004`
/// for an invalid cell, and `RT-DGN-0001` to `0003` for a reserved name. Every
/// fault of the file is reported, in canonical order.
pub fn map_participants(
    table: &Table,
    schema: &InputSchema,
    options: &MappingOptions,
) -> Result<CompetitionData, DiagnosticState> {
    let mut rep = Reporter::new(&table.input);
    if reserved::check_header(table, &mut rep) {
        return Err(rep.into_state());
    }
    let header = index(table, &mut rep);
    let id_column = header.by_name.get(PARTICIPANT).copied();
    if id_column.is_none() {
        missing(table, &mut rep, PARTICIPANT);
    }
    for name in schema.inputs().keys() {
        if !header.by_name.contains_key(name.as_str()) {
            missing(table, &mut rep, name);
        }
    }
    for (i, cell) in table.header.iter().enumerate() {
        let name = cell.text.as_str();
        let first = header.by_name.get(name) == Some(&i);
        let known = name == PARTICIPANT
            || schema.inputs().contains_key(name)
            || options.ignore_columns.contains(name);
        if first && !known {
            unknown(&mut rep, cell);
        }
    }
    let columns: Vec<_> = schema
        .inputs()
        .iter()
        .filter_map(|(name, ty)| Some((header.by_name.get(name.as_str()).copied()?, name, ty)))
        .collect();
    let mut first_seen: BTreeMap<ParticipantId, Location> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for row in &table.rows {
        if !row_fits(table, row, &mut rep) {
            continue;
        }
        let id =
            id_column
                .and_then(|i| row.get(i))
                .and_then(|cell| match participant(&cell.text) {
                    Ok(id) => Some((id, cell)),
                    Err(why) => {
                        invalid(&mut rep, cell, PARTICIPANT, &why);
                        None
                    }
                });
        let mut fields: BTreeMap<String, DataValue> = BTreeMap::new();
        for (i, name, ty) in &columns {
            let Some(cell) = row.get(*i) else { continue };
            match data_cell(ty, &cell.text) {
                Ok(Some(value)) => {
                    fields.insert((*name).clone(), value);
                }
                Ok(None) => {}
                Err(why) => invalid(&mut rep, cell, name, &why),
            }
        }
        let Some((id, cell)) = id else { continue };
        let here = rep.cell(cell, PARTICIPANT);
        if let Some(first) = first_seen.get(&id) {
            let message = format!(
                "participant `{}` is listed more than once",
                shown(&cell.text)
            );
            rep.push_related(
                Code::AppInputDuplicateParticipant,
                here,
                message,
                first.clone(),
            );
        } else {
            first_seen.insert(id.clone(), here);
            out.insert(id, Participant::new(fields, BTreeMap::new()));
        }
    }
    rep.finish(CompetitionData::new(out))
}
