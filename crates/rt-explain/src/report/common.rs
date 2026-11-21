//! Text shared by the human and machine writers of the application report.

use crate::report::model::{Missing, ProvenanceSummary};
use rt_foundation::{escape_controls, CliLocation, Location};
use rt_numeric::to_canonical;
use rt_trace::{Place, TraceValue};

/// Format identifier of the application report.
pub const APP_REPORT_FORMAT: &str = "ruletrace-app-report";
/// Version of the application report format.
pub const APP_REPORT_VERSION: u32 = 1;

/// The statement written beside every Tie_Group (Req 31.13).
pub(crate) const TIE_ORDER_NOTE: &str =
    "members are listed in ascending identifier order, which has no ranking meaning";

/// The value of the `diagnostics_scope` member of an application report. It
/// states that the Diagnostics section holds the evaluation Diagnostics and
/// the provenance-verification Diagnostics, and nothing else.
pub(crate) const DIAGNOSTICS_SCOPE: &str = "evaluation_and_provenance";

/// The heading line of the human Diagnostics section.
pub(crate) const DIAGNOSTICS_HEADING: &str = "DIAGNOSTICS (evaluation and provenance; published-result differences are listed under PUBLISHED CONSISTENCY; warnings that `ruletrace evaluate` writes to standard error are not part of this section)";

/// The header text when no Source_Model was supplied (Req 31.14).
pub(crate) const NO_SOURCES_TEXT: &str = "not verified: no source models were supplied";

/// The header text when only some traces had no Source_Model.
pub(crate) fn some_without_sources_text(without: u32, total: u32) -> String {
    format!("not verified: {without} of {total} traces had no source models")
}

/// The ranking-scope statement of the Bracket_Check header (RN-23). It does
/// not say that the report evaluates eligibility.
pub(crate) const BRACKET_RANKING_SCOPE_LABEL: &str = "Standings are calculated solely from the declared ranking rules; this report does not apply eligibility filtering.";

/// The ranking-scope statement of the Prize_Check header (RN-23). It is the
/// sentence of the Proof_Report.
pub(crate) use crate::report::proof::RANKING_SCOPE_LABEL;

/// Escapes untrusted text through `escape_controls`. A final line feed is
/// escaped too, so one value never ends a line.
pub(crate) fn esc(s: &str) -> String {
    let mut out = escape_controls(s);
    if out.ends_with('\n') {
        out.pop();
        out.push_str("\\u{a}");
    }
    out
}

/// The kind name and the unescaped text of a value, numbers in canonical
/// Exact_Number text.
pub(crate) fn value_parts(v: &TraceValue) -> (&'static str, String) {
    match v {
        TraceValue::Number(n) => ("number", to_canonical(n)),
        TraceValue::Bool(b) => ("boolean", b.to_string()),
        TraceValue::Text(s) => ("text", s.clone()),
        TraceValue::Date(s) => ("date", s.clone()),
        TraceValue::Enum(s) => ("enum", s.clone()),
        TraceValue::Rank(r) => ("rank", r.position().to_string()),
        TraceValue::Standings(places) => ("standings", standings_text(places)),
    }
}

fn standings_text(places: &[Place]) -> String {
    places
        .iter()
        .map(|p| format!("{}: {}", p.position(), p.members().join(", ")))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Members in ascending byte order, for serialization only.
pub(crate) fn ascending(members: &[String]) -> Vec<String> {
    let mut v = members.to_vec();
    v.sort();
    v
}

/// One-line text of a Diagnostic location.
pub(crate) fn location_text(l: &Location) -> String {
    match l {
        Location::Source(s) => format!(
            "{} bytes {}..{} line {} column {}",
            s.doc().as_str(),
            s.start().get(),
            s.end().get(),
            s.line(),
            s.column()
        ),
        Location::Document(d) => format!("document {}", d.as_str()),
        Location::Input(i) => {
            let mut t = format!("input {}", i.input.as_str());
            if let Some(line) = i.line {
                t.push_str(&format!(" line {line}"));
            }
            if let Some(column) = i.column {
                t.push_str(&format!(" column {column}"));
            }
            if let Some(field) = &i.field {
                t.push_str(&format!(" field {field}"));
            }
            t
        }
        Location::Cli(CliLocation::Argument { position, name }) => match name {
            Some(n) => format!("argument {position} {n}"),
            None => format!("argument {position}"),
        },
        Location::Cli(CliLocation::Command(c)) => format!("command {c}"),
        Location::None(code) => format!("no location ({})", code.id()),
    }
}

/// The text of one reason, unescaped.
pub(crate) fn missing_text(m: &Missing) -> String {
    match m {
        Missing::Input { participant, field } => {
            format!("missing input: {field} ({participant})")
        }
        Missing::EmptyAggregate { field } => format!("min or max of empty {field}"),
        Missing::Diagnosed(code) => format!("diagnostic {}", code.id()),
    }
}

/// The status name of the provenance summary in the machine form.
pub(crate) fn provenance_status(p: &ProvenanceSummary) -> &'static str {
    match p {
        ProvenanceSummary::Verified { .. } => "verified",
        ProvenanceSummary::NotVerified { .. } => "not_verified",
        ProvenanceSummary::NotVerifiedNoSources { .. } => "not_verified_no_sources",
    }
}
