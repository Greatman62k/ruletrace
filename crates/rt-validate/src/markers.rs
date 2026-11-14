//! Pass 7: ambiguity and incompleteness markers (design §4.1.5, Req 6.5, 6.6).

use crate::ctx::Site;
use rt_foundation::{Code, DiagnosticState};
use rt_ir::Marker;

const MAX_ITEMS: usize = 16;
const MAX_CHARS: usize = 1000;

fn well_formed(items: &[String], min: usize) -> bool {
    (min..=MAX_ITEMS).contains(&items.len())
        && items
            .iter()
            .all(|t| (1..=MAX_CHARS).contains(&t.chars().count()))
}

fn list(items: &[String]) -> String {
    items
        .iter()
        .map(|t| format!("'{t}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One `RT-VAL-0004` per rule with ambiguity markers (every interpretation, in recorded order,
/// none selected), one `RT-VAL-0005` per rule with incompleteness markers (every missing
/// element, in recorded order). A marker outside the supported shape (2 to 16 interpretations,
/// 1 to 16 missing names, 1 to 1000 characters each) is `RT-VAL-0003`.
pub(crate) fn check(markers: &[Marker], site: &Site, out: &mut DiagnosticState) {
    let mut interpretations: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for m in markers {
        match m {
            Marker::Ambiguous { interpretations: i } if well_formed(i, 2) => {
                interpretations.extend(i.iter().cloned());
            }
            Marker::Incomplete { missing: x } if well_formed(x, 1) => {
                missing.extend(x.iter().cloned());
            }
            Marker::Ambiguous { .. } | Marker::Incomplete { .. } => out.push(site.diag(
                Code::ValUnsupportedConstruct,
                format!(
                    "unsupported construct: a marker of rule '{}' is outside the supported shape \
                     (2 to 16 interpretations or 1 to 16 missing elements, 1 to 1000 characters each)",
                    site.rule.rule
                ),
            )),
        }
    }
    if !interpretations.is_empty() {
        out.push(site.diag(
            Code::ValAmbiguousInterpretation,
            format!(
                "rule '{}' is marked ambiguous; no interpretation is selected. Interpretations: {}",
                site.rule.rule,
                list(&interpretations)
            ),
        ));
    }
    if !missing.is_empty() {
        out.push(site.diag(
            Code::ValIncompleteInterpretation,
            format!(
                "rule '{}' is marked incomplete. Missing: {}",
                site.rule.rule,
                list(&missing)
            ),
        ));
    }
}
