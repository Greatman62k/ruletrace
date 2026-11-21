//! The published file: `participant,target,value`.

use super::cells::{participant, published_cell};
use super::header::{index, invalid, missing, row_fits, unknown};
use super::report::{shown, Reporter};
use super::{
    reserved, MappedPublished, Table, TargetKind, TargetKinds, UnusedPublished, UnusedReason,
};
use crate::{CompetitionData, ParticipantId, PublishedResult, PublishedValue, TargetName};
use rt_foundation::{Code, DiagnosticState, Location};
use std::collections::{BTreeMap, BTreeSet};

const COLUMNS: [&str; 3] = ["participant", "target", "value"];

/// The outcome of checking the value cell of a row whose Target is declared.
enum Checked {
    Value(PublishedValue),
    Unchecked,
}

/// Maps a published table. A row whose Target is declared but not in
/// `requested` is returned in `unused` with an [`UnusedReason`] and not in the
/// result; it is not an error, whatever the kind of the Target (design §8.3).
///
/// Requestedness is decided before reportability. The value of an unrequested
/// row is checked against its kind like any other, except that the value of a
/// Target of kind [`TargetKind::Other`] is not checked at all: no value type
/// is known for it. Structure, the participant and repeated (participant,
/// Target) pairs are checked for every row.
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
/// `RT-APP-0005` for an undeclared Target, `RT-APP-0006` for a requested
/// Target of kind [`TargetKind::Other`], and `RT-DGN-0001` to `0003` for a reserved
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
        // Requestedness comes first: reportability only matters for a Target
        // the caller asked for (design §8.3).
        let is_requested = target.as_ref().is_some_and(|t| requested.contains(t));
        let checked = match kind {
            None => {
                let at = rep.cell(tcell, "target");
                rep.push(
                    Code::AppTargetNotDeclared,
                    at,
                    format!("Target `{}` is not declared", shown(&tcell.text)),
                );
                None
            }
            Some(TargetKind::Other) if is_requested => {
                let at = rep.cell(tcell, "target");
                rep.push(
                    Code::AppTargetKindMismatch,
                    at,
                    format!(
                        "Target `{}` has a Rule kind that cannot be reported",
                        shown(&tcell.text)
                    ),
                );
                None
            }
            // No value type is known, so the value cell is not checked.
            Some(TargetKind::Other) => Some(Checked::Unchecked),
            Some(k) => match published_cell(k, &vcell.text) {
                Ok(value) => Some(Checked::Value(value)),
                Err(why) => {
                    invalid(&mut rep, vcell, "value", &why);
                    None
                }
            },
        };
        let (Some(id), Some(target), Some(checked)) = (id, target, checked) else {
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
        match checked {
            Checked::Value(value) if is_requested => {
                entries.insert(key, value);
            }
            Checked::Value(_) | Checked::Unchecked => {
                let (line, column) = row
                    .first()
                    .map_or((pcell.line, pcell.column), |c| (c.line, c.column));
                let reason = match checked {
                    Checked::Unchecked => UnusedReason::NotRequestedKindNotReportable,
                    Checked::Value(_) => UnusedReason::NotRequested,
                };
                unused.push(UnusedPublished {
                    participant: id,
                    target,
                    line,
                    column,
                    reason,
                });
            }
        }
    }
    rep.finish(MappedPublished {
        result: PublishedResult::new(entries),
        unused,
    })
}
