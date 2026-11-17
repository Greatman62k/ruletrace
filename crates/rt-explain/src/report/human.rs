//! The human form of the application report (design §8.5).

use crate::report::common::{
    ascending, esc, location_text, missing_text, value_parts, APP_REPORT_FORMAT,
    APP_REPORT_VERSION, NO_SOURCES_TEXT, RANKING_SCOPE_LABEL, TIE_ORDER_NOTE,
};
use crate::report::model::{
    Comparison, EligibilityEntry, Evidence, Missing, ProvenanceSummary, PublishedDifference,
    PublishedSection, QualificationEntry, ReportApplication, ReportDoc, ReportSection, Score,
    ScoreEntry, StandingsResult, StandingsTable, Truth,
};
use crate::verify::SpanResolution;
use rt_foundation::Code;
use rt_numeric::to_canonical;
use rt_trace::CriterionResult;
use std::fmt::Write as _;

fn header_lines(out: &mut String, doc: &ReportDoc) {
    let h = doc.header();
    let _ = writeln!(
        out,
        "RuleTrace {} report   format {APP_REPORT_FORMAT} v{APP_REPORT_VERSION}",
        h.application.name()
    );
    let _ = writeln!(out, "rulebook sha256:{}", h.rulebook_hash.as_str());
    for (role, hash) in &h.input_hashes {
        let _ = writeln!(out, "input {} sha256:{}", esc(role), hash.as_str());
    }
    let _ = writeln!(out, "provenance: {}", provenance_text(&h.provenance));
    let ids: Vec<String> = h.statements.iter().map(|s| esc(s.as_str())).collect();
    let _ = writeln!(out, "statements: {}", ids.join(" "));
    let synthetic = match h.synthetic_data {
        Some(true) => "synthetic",
        Some(false) => "not synthetic",
        None => "not stated",
    };
    let _ = writeln!(out, "data: {synthetic}");
    if h.application == ReportApplication::PrizeCheck {
        let _ = writeln!(out, "ranking: {RANKING_SCOPE_LABEL}");
    }
}

fn codes_text(codes: &[Code]) -> String {
    codes.iter().map(|c| c.id()).collect::<Vec<_>>().join(", ")
}

fn provenance_text(p: &ProvenanceSummary) -> String {
    match p {
        ProvenanceSummary::Verified { traces } => {
            format!("verified ({traces} of {traces} traces)")
        }
        ProvenanceSummary::NotVerified {
            codes,
            verified,
            total,
        } => format!(
            "not verified ({verified} of {total} traces verified; {})",
            if codes.is_empty() {
                "no diagnostic".to_owned()
            } else {
                codes_text(codes)
            }
        ),
        ProvenanceSummary::NotVerifiedNoSources => format!(
            "{NO_SOURCES_TEXT} (warning {})",
            Code::ProvNotVerifiedNoSources.id()
        ),
    }
}

fn missing_list(missing: &[Missing]) -> String {
    missing
        .iter()
        .map(|m| esc(&missing_text(m)))
        .collect::<Vec<_>>()
        .join("; ")
}

fn rules_text(e: &Evidence) -> String {
    if e.rules.is_empty() {
        return "none".to_owned();
    }
    e.rules
        .iter()
        .map(|(r, rev)| format!("{}@{}", esc(r.as_str()), rev.get()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn evidence_lines(out: &mut String, e: &Evidence) {
    let verdict = if e.verified {
        "verified"
    } else {
        "not verified"
    };
    let _ = writeln!(
        out,
        "      trace #{}  {verdict}  rules {}",
        e.trace,
        rules_text(e)
    );
    for s in &e.sources {
        let rev = s
            .revision
            .map_or_else(String::new, |r| format!("@{}", r.get()));
        let _ = writeln!(
            out,
            "      source {}{rev} {} {}..{} line {}:{}",
            esc(s.rule.as_str()),
            esc(s.span.document.as_str()),
            s.span.start,
            s.span.end,
            s.span.line,
            s.span.column
        );
        match &s.resolution {
            SpanResolution::Resolved(text) => {
                let _ = writeln!(out, "        \"{}\"", esc(text));
            }
            SpanResolution::Unresolved(code) => {
                let _ = writeln!(out, "        unresolved: {}", code.id());
            }
        }
    }
}

fn truth_text(t: &Truth) -> String {
    match t {
        Truth::True => "true".to_owned(),
        Truth::False => "false".to_owned(),
        Truth::Indeterminate(m) => format!("indeterminate   {}", missing_list(m)),
    }
}

fn eligibility(out: &mut String, entries: &[EligibilityEntry]) {
    let _ = writeln!(out, "\nELIGIBILITY");
    if entries.is_empty() {
        let _ = writeln!(out, "  none");
    }
    for e in entries {
        let _ = writeln!(
            out,
            "  {}  {}  {}",
            esc(&e.participant),
            esc(&e.target),
            truth_text(&e.result)
        );
        evidence_lines(out, &e.evidence);
    }
}

fn scores(out: &mut String, entries: &[ScoreEntry]) {
    let _ = writeln!(out, "\nSCORES");
    if entries.is_empty() {
        let _ = writeln!(out, "  none");
    }
    for e in entries {
        let value = match &e.result {
            Score::Exact(n) => to_canonical(n),
            Score::Indeterminate(m) => format!("indeterminate   {}", missing_list(m)),
        };
        let _ = writeln!(
            out,
            "  {}  {}  {value}",
            esc(&e.participant),
            esc(&e.target)
        );
        evidence_lines(out, &e.evidence);
    }
}

fn comparison_line(out: &mut String, c: &Comparison) {
    let e = &c.0;
    let verdict = if e.first_ahead {
        format!("{} ahead of {}", esc(&e.between.0), esc(&e.between.1))
    } else {
        format!("{} tied with {}", esc(&e.between.0), esc(&e.between.1))
    };
    let _ = writeln!(out, "    compare {verdict}");
    for k in &e.criteria {
        let result = match k.result {
            CriterionResult::First => format!("{} ahead", esc(&e.between.0)),
            CriterionResult::Second => format!("{} ahead", esc(&e.between.1)),
            CriterionResult::Equal => "equal".to_owned(),
        };
        let _ = writeln!(
            out,
            "      criterion {} ({}, rule {}): {} vs {} -> {result}",
            k.index,
            k.direction.name(),
            esc(k.rule.as_str()),
            esc(&value_parts(&k.left).1),
            esc(&value_parts(&k.right).1)
        );
    }
}

fn standings(out: &mut String, tables: &[StandingsTable], with_comparisons: bool) {
    if tables.is_empty() {
        let _ = writeln!(out, "\nSTANDINGS\n  none");
    }
    for t in tables {
        let _ = writeln!(out, "\nSTANDINGS {}", esc(&t.target));
        match &t.result {
            StandingsResult::Places(places) => {
                for p in places {
                    let members = ascending(&p.members);
                    if members.len() == 1 {
                        let _ = writeln!(out, "  {}  {}", p.position, esc(&members.join("")));
                    } else {
                        let list: Vec<String> = members.iter().map(|m| esc(m)).collect();
                        let _ = writeln!(
                            out,
                            "  {}  {{{}}}   tie group: members share position {}; {TIE_ORDER_NOTE}",
                            p.position,
                            list.join(", "),
                            p.position
                        );
                    }
                }
            }
            StandingsResult::Indeterminate(m) => {
                let _ = writeln!(out, "  indeterminate   {}", missing_list(m));
            }
        }
        if with_comparisons {
            for c in &t.comparisons {
                comparison_line(out, c);
            }
        }
        evidence_lines(out, &t.evidence);
    }
}

fn qualification(out: &mut String, entries: &[QualificationEntry]) {
    let _ = writeln!(out, "\nQUALIFICATION");
    if entries.is_empty() {
        let _ = writeln!(out, "  none");
    }
    for e in entries {
        let _ = writeln!(
            out,
            "  {}  {}  {}",
            esc(&e.participant),
            esc(&e.target),
            truth_text(&e.result)
        );
        for r in &e.inputs_read {
            let who = r
                .participant
                .as_deref()
                .map_or_else(String::new, |p| format!(" ({})", esc(p)));
            let (kind, text) = value_parts(&r.value);
            let _ = writeln!(
                out,
                "      input read: {}{who} = {} [{kind}]",
                esc(&r.name),
                esc(&text)
            );
        }
        evidence_lines(out, &e.evidence);
    }
}

fn difference(out: &mut String, d: &PublishedDifference) {
    let computed = d
        .computed
        .as_deref()
        .map_or_else(|| "indeterminate".to_owned(), esc);
    let _ = writeln!(
        out,
        "  differs: {} {} published {}, computed {computed}   kind {} ({})   trace #{}",
        esc(&d.participant),
        esc(&d.target),
        esc(&d.published),
        d.kind,
        d.code.id(),
        d.trace
    );
    if let Some(entry) = d.entry {
        let _ = writeln!(out, "      trace entry {entry}");
    }
    if d.non_terminating {
        let _ = writeln!(out, "      computed value has a non-terminating expansion");
    }
    if !d.group.is_empty() {
        let list: Vec<String> = ascending(&d.group).iter().map(|m| esc(m)).collect();
        let _ = writeln!(out, "      group {{{}}}", list.join(", "));
    }
}

fn published(out: &mut String, p: &PublishedSection) {
    let _ = writeln!(out, "\nPUBLISHED CONSISTENCY");
    match p {
        PublishedSection::NotSupplied => {
            let _ = writeln!(out, "  not supplied");
        }
        PublishedSection::Compared {
            differences,
            unused,
        } => {
            if differences.is_empty() {
                let _ = writeln!(out, "  equal");
            }
            for d in differences {
                difference(out, d);
            }
            for u in unused {
                let _ = writeln!(
                    out,
                    "  published_unused: {} {}",
                    esc(&u.participant),
                    esc(&u.target)
                );
            }
        }
    }
}

fn diagnostics(out: &mut String, list: &[rt_foundation::Diagnostic]) {
    let _ = writeln!(out, "\nDIAGNOSTICS");
    if list.is_empty() {
        let _ = writeln!(out, "  none");
    }
    for d in list {
        let _ = writeln!(
            out,
            "  {} {}: {}   at {}",
            d.code().id(),
            crate::results::severity_name(d.severity()),
            esc(d.message()),
            esc(&location_text(d.primary()))
        );
    }
}

/// The human form. Every dynamic character passes through `escape_controls`.
#[must_use]
pub fn render_human(doc: &ReportDoc) -> String {
    let mut out = String::new();
    header_lines(&mut out, doc);
    let bracket = doc.header().application == ReportApplication::BracketCheck;
    for s in doc.sections() {
        match s {
            ReportSection::Eligibility(e) => eligibility(&mut out, e),
            ReportSection::Scores(e) => scores(&mut out, e),
            ReportSection::Standings(t) => standings(&mut out, t, bracket),
            ReportSection::Qualification(e) => qualification(&mut out, e),
            ReportSection::Published(p) => published(&mut out, p),
            ReportSection::Diagnostics(d) => diagnostics(&mut out, d),
        }
    }
    out
}
