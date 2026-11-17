//! The published file: `participant,target,value`.

use super::cells::{participant, published_cell};
use super::header::{index, invalid, missing, row_fits, unknown};
use super::report::{shown, Reporter};
use super::{reserved, MappedPublished, Table, TargetKind, TargetKinds, UnusedPublished};
use crate::{CompetitionData, ParticipantId, PublishedResult, PublishedValue, TargetName};
use rt_foundation::{Code, DiagnosticState, Location};
use std::collections::{BTreeMap, BTreeSet};

const COLUMNS: [&str; 3] = ["participant", "target", "value"];

/// Maps a published table. A row whose Target is declared but not in
/// `requested` is returned in `unused` and not in the result, after its value
/// has been checked like any other.
///
/// `participants` is the participants file already mapped: a published row for
/// a participant it does not hold is invalid (design §8.3).
///
/// A reserved name (Req 22.22) in the header or as a `target` cell is reported
/// alone: no later stage reads the file.
///
/// # Errors
/// `RT-APP-0001` for an unknown, empty or repeated column, `RT-APP-0002` for
/// an absent column, `RT-APP-0004` for an unknown participant, an invalid
/// value (an empty value included) or a repeated (participant, Target) pair,
/// `RT-APP-0005` for an undeclared Target, `RT-APP-0006` for a declared Target
/// of kind [`TargetKind::Other`], and `RT-DGN-0001` to `0003` for a reserved
/// name.
pub fn map_published(
    table: &Table,
    kinds: &TargetKinds,
    requested: &BTreeSet<TargetName>,
    participants: &CompetitionData,
) -> Result<MappedPublished, DiagnosticState> {
    let mut rep = Reporter::new(&table.input);
    let mut reserved_found = reserved::check_header(table, &mut rep);
    let header = index(table, &mut rep);
    let column = |name: &str| header.by_name.get(name).copied();
    if let Some(tc) = column("target") {
        for row in &table.rows {
            if let Some(cell) = row.get(tc) {
                reserved_found |= reserved::check_cell(cell, "target", &mut rep);
            }
        }
    }
    if reserved_found {
        return Err(rep.into_state());
    }
    for name in COLUMNS {
        if column(name).is_none() {
            missing(table, &mut rep, name);
        }
    }
    for (i, cell) in table.header.iter().enumerate() {
        let first = header.by_name.get(cell.text.as_str()) == Some(&i);
        if first && !COLUMNS.contains(&cell.text.as_str()) {
            unknown(&mut rep, cell);
        }
    }
    let (Some(pc), Some(tc), Some(vc)) = (column("participant"), column("target"), column("value"))
    else {
        return Err(rep.into_state());
    };
    let mut entries: BTreeMap<(ParticipantId, TargetName), PublishedValue> = BTreeMap::new();
    let mut seen: BTreeMap<(ParticipantId, TargetName), Location> = BTreeMap::new();
    let mut unused = Vec::new();
    for row in &table.rows {
        if !row_fits(table, row, &mut rep) {
            continue;
        }
        let (Some(pcell), Some(tcell), Some(vcell)) = (row.get(pc), row.get(tc), row.get(vc))
        else {
            continue;
        };
        let id = participant(&pcell.text)
            .ok()
            .filter(|id| participants.participant(id).is_some());
        if id.is_none() {
            let why = "no such participant in the participants file";
            invalid(&mut rep, pcell, "participant", why);
        }
        let target = TargetName::new(tcell.text.as_str()).ok();
        let kind = target.as_ref().and_then(|t| kinds.get(t));
        match kind {
            None => {
                let at = rep.cell(tcell, "target");
                rep.push(
                    Code::AppTargetNotDeclared,
                    at,
                    format!("Target `{}` is not declared", shown(&tcell.text)),
                );
            }
            Some(TargetKind::Other) => {
                let at = rep.cell(tcell, "target");
                rep.push(
                    Code::AppTargetKindMismatch,
                    at,
                    format!(
                        "Target `{}` has a Rule kind that cannot be reported",
                        shown(&tcell.text)
                    ),
                );
            }
            Some(_) => {}
        }
        let value = match kind {
            Some(k) if k != TargetKind::Other => match published_cell(k, &vcell.text) {
                Ok(value) => Some(value),
                Err(why) => {
                    invalid(&mut rep, vcell, "value", &why);
                    None
                }
            },
            _ => None,
        };
        let (Some(id), Some(target), Some(value)) = (id, target, value) else {
            continue;
        };
        let key = (id.clone(), target.clone());
        let here = rep.cell(tcell, "target");
        if let Some(first) = seen.get(&key) {
            let message = format!(
                "participant `{}` has a published value for `{}` more than once",
                shown(&pcell.text),
                shown(&tcell.text)
            );
            rep.push_related(Code::AppInputValueInvalid, here, message, first.clone());
            continue;
        }
        seen.insert(key.clone(), here);
        if requested.contains(&target) {
            entries.insert(key, value);
        } else {
            let (line, column) = row
                .first()
                .map_or((pcell.line, pcell.column), |c| (c.line, c.column));
            unused.push(UnusedPublished {
                participant: id,
                target,
                line,
                column,
            });
        }
    }
    rep.finish(MappedPublished {
        result: PublishedResult::new(entries),
        unused,
    })
}
