//! The machine form of the application report: canonical compact RT-JSON,
//! fixed key order, no float (design §8.5).

use crate::report::common::{
    ascending, location_text, missing_text, provenance_status, some_without_sources_text,
    value_parts, APP_REPORT_FORMAT, APP_REPORT_VERSION, BRACKET_RANKING_SCOPE_LABEL,
    NO_SOURCES_TEXT, RANKING_SCOPE_LABEL, TIE_ORDER_NOTE,
};
use crate::report::model::{
    count, Comparison, EligibilityEntry, Evidence, Missing, ProvenanceSummary, PublishedDifference,
    PublishedSection, QualificationEntry, ReportApplication, ReportDoc, ReportSection, Score,
    ScoreEntry, StandingsResult, StandingsTable, Truth,
};
use crate::results::{obj, opt_text, severity_name, text};
use crate::verify::SpanResolution;
use rt_json::Value;
use rt_numeric::to_canonical;
use rt_trace::CriterionResult;

type Json = Value<()>;

fn int(n: usize) -> Json {
    Value::integer(count(n))
}

fn strings(items: &[String]) -> Json {
    Value::array(items.iter().map(|s| text(s)).collect())
}

fn missing_json(m: &Missing) -> Json {
    let (kind, participant, field, code) = match m {
        Missing::Input { participant, field } => (
            "missing_input",
            Some(participant.as_str()),
            Some(field.as_str()),
            None,
        ),
        Missing::EmptyAggregate { field } => ("empty_aggregate", None, Some(field.as_str()), None),
        Missing::Diagnosed(c) => ("diagnostic", None, None, Some(c.id())),
    };
    obj(vec![
        ("kind", text(kind)),
        ("participant", opt_text(participant)),
        ("field", opt_text(field)),
        ("code", opt_text(code)),
        ("text", text(&missing_text(m))),
    ])
}

fn missing_array(m: &[Missing]) -> Json {
    Value::array(m.iter().map(missing_json).collect())
}

fn evidence_members(e: &Evidence) -> Vec<(&'static str, Json)> {
    let rules = e
        .rules
        .iter()
        .map(|(r, rev)| {
            obj(vec![
                ("rule", text(r.as_str())),
                ("revision", Value::integer(rev.get())),
            ])
        })
        .collect();
    let sources = e
        .sources
        .iter()
        .map(|s| {
            let (txt, code) = match &s.resolution {
                SpanResolution::Resolved(t) => (Some(t.as_str()), None),
                SpanResolution::Unresolved(c) => (None, Some(c.id())),
            };
            obj(vec![
                ("rule", text(s.rule.as_str())),
                (
                    "revision",
                    s.revision
                        .map_or_else(Value::null, |r| Value::integer(r.get())),
                ),
                ("document", text(s.span.document.as_str())),
                ("start", Value::integer(s.span.start)),
                ("end", Value::integer(s.span.end)),
                ("line", Value::integer(s.span.line)),
                ("column", Value::integer(s.span.column)),
                ("text", opt_text(txt)),
                ("code", opt_text(code)),
            ])
        })
        .collect();
    vec![
        ("trace", int(e.trace)),
        ("verified", Value::boolean(e.verified)),
        ("rules", Value::array(rules)),
        ("sources", Value::array(sources)),
    ]
}

fn truth_members(t: &Truth) -> Vec<(&'static str, Json)> {
    match t {
        Truth::True => vec![("result", text("true")), ("missing", missing_array(&[]))],
        Truth::False => vec![("result", text("false")), ("missing", missing_array(&[]))],
        Truth::Indeterminate(m) => vec![
            ("result", text("indeterminate")),
            ("missing", missing_array(m)),
        ],
    }
}

fn eligibility(e: &EligibilityEntry) -> Json {
    let mut m = vec![
        ("participant", text(&e.participant)),
        ("target", text(&e.target)),
    ];
    m.extend(truth_members(&e.result));
    m.extend(evidence_members(&e.evidence));
    obj(m)
}

fn score(e: &ScoreEntry) -> Json {
    let (result, value, missing) = match &e.result {
        Score::Exact(n) => ("exact", text(&to_canonical(n)), missing_array(&[])),
        Score::Indeterminate(m) => ("indeterminate", Value::null(), missing_array(m)),
    };
    let mut m = vec![
        ("participant", text(&e.participant)),
        ("target", text(&e.target)),
        ("result", text(result)),
        ("value", value),
        ("missing", missing),
    ];
    m.extend(evidence_members(&e.evidence));
    obj(m)
}

fn comparison(c: &Comparison) -> Json {
    let e = &c.0;
    let criteria = e
        .criteria
        .iter()
        .map(|k| {
            let (left_type, left) = value_parts(&k.left);
            let (right_type, right) = value_parts(&k.right);
            let result = match k.result {
                CriterionResult::First => "first_ahead",
                CriterionResult::Second => "second_ahead",
                CriterionResult::Equal => "equal",
            };
            obj(vec![
                ("index", Value::integer(k.index)),
                ("rule", text(k.rule.as_str())),
                ("direction", text(k.direction.name())),
                ("left_type", text(left_type)),
                ("left", text(&left)),
                ("right_type", text(right_type)),
                ("right", text(&right)),
                ("result", text(result)),
            ])
        })
        .collect();
    obj(vec![
        (
            "between",
            Value::array(vec![text(&e.between.0), text(&e.between.1)]),
        ),
        ("first_ahead", Value::boolean(e.first_ahead)),
        ("criteria", Value::array(criteria)),
    ])
}

fn table(t: &StandingsTable, with_comparisons: bool) -> Json {
    let (result, places, missing) = match &t.result {
        StandingsResult::Places(places) => {
            let items = places
                .iter()
                .map(|p| {
                    let tie = p.members.len() > 1;
                    obj(vec![
                        ("position", Value::integer(p.position)),
                        ("members", strings(&ascending(&p.members))),
                        ("tie_group", Value::boolean(tie)),
                        ("order_note", opt_text(tie.then_some(TIE_ORDER_NOTE))),
                    ])
                })
                .collect();
            ("ordered", items, missing_array(&[]))
        }
        StandingsResult::Indeterminate(m) => ("indeterminate", Vec::new(), missing_array(m)),
    };
    let mut m = vec![
        ("target", text(&t.target)),
        ("result", text(result)),
        ("places", Value::array(places)),
        ("missing", missing),
    ];
    if with_comparisons {
        m.push((
            "comparisons",
            Value::array(t.comparisons.iter().map(comparison).collect()),
        ));
    }
    m.extend(evidence_members(&t.evidence));
    obj(m)
}

fn qualification(e: &QualificationEntry) -> Json {
    let inputs = e
        .inputs_read
        .iter()
        .map(|r| {
            let (kind, value) = value_parts(&r.value);
            obj(vec![
                ("name", text(&r.name)),
                ("participant", opt_text(r.participant.as_deref())),
                ("type", text(kind)),
                ("value", text(&value)),
            ])
        })
        .collect();
    let mut m = vec![
        ("participant", text(&e.participant)),
        ("target", text(&e.target)),
    ];
    m.extend(truth_members(&e.result));
    m.push(("inputs_read", Value::array(inputs)));
    m.extend(evidence_members(&e.evidence));
    obj(m)
}

fn difference(d: &PublishedDifference) -> Json {
    obj(vec![
        ("participant", text(&d.participant)),
        ("target", text(&d.target)),
        ("published", text(&d.published)),
        ("computed", opt_text(d.computed.as_deref())),
        ("trace", int(d.trace)),
        ("entry", d.entry.map_or_else(Value::null, Value::integer)),
        ("kind", text(d.kind.name())),
        ("code", text(d.kind.code().id())),
        ("non_terminating", Value::boolean(d.non_terminating)),
        ("group", strings(&ascending(&d.group))),
    ])
}

fn published(p: &PublishedSection) -> Json {
    match p {
        PublishedSection::NotSupplied => obj(vec![
            ("state", text("not_supplied")),
            ("differences", Value::array(Vec::new())),
            ("unused", Value::array(Vec::new())),
        ]),
        PublishedSection::Compared {
            differences,
            unused,
        } => obj(vec![
            (
                "state",
                text(if differences.is_empty() {
                    "equal"
                } else {
                    "differs"
                }),
            ),
            (
                "differences",
                Value::array(differences.iter().map(difference).collect()),
            ),
            (
                "unused",
                Value::array(
                    unused
                        .iter()
                        .map(|u| {
                            obj(vec![
                                ("participant", text(&u.participant)),
                                ("target", text(&u.target)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]),
    }
}

fn diagnostic(d: &rt_foundation::Diagnostic) -> Json {
    obj(vec![
        ("code", text(d.code().id())),
        ("severity", text(severity_name(d.severity()))),
        ("message", text(d.message())),
        ("location", text(&location_text(d.primary()))),
    ])
}

fn provenance(p: &ProvenanceSummary) -> Json {
    let (detail, verified, total, codes): (String, Option<u32>, Option<u32>, Vec<&str>) = match p {
        ProvenanceSummary::Verified { traces } => {
            ("verified".to_owned(), Some(*traces), Some(*traces), vec![])
        }
        ProvenanceSummary::NotVerified {
            codes,
            verified,
            total,
        } => (
            "not verified".to_owned(),
            Some(*verified),
            Some(*total),
            codes.iter().map(|c| c.id()).collect(),
        ),
        ProvenanceSummary::NotVerifiedNoSources {
            codes,
            without_sources,
            verified,
            total,
        } => {
            let ids = codes.iter().map(|c| c.id()).collect();
            if without_sources == total {
                (NO_SOURCES_TEXT.to_owned(), None, None, ids)
            } else {
                (
                    some_without_sources_text(*without_sources, *total),
                    Some(*verified),
                    Some(*total),
                    ids,
                )
            }
        }
    };
    obj(vec![
        ("status", text(provenance_status(p))),
        ("text", text(&detail)),
        (
            "verified_traces",
            verified.map_or_else(Value::null, Value::integer),
        ),
        (
            "total_traces",
            total.map_or_else(Value::null, Value::integer),
        ),
        ("codes", Value::array(codes.into_iter().map(text).collect())),
    ])
}

fn header(doc: &ReportDoc) -> Json {
    let h = doc.header();
    let inputs = h
        .input_hashes
        .iter()
        .map(|(role, hash)| obj(vec![("role", text(role)), ("sha256", text(hash.as_str()))]))
        .collect();
    let mut m = vec![
        ("format", text(APP_REPORT_FORMAT)),
        ("version", Value::integer(APP_REPORT_VERSION)),
        ("application", text(h.application.name())),
        (
            "statements",
            Value::array(h.statements.iter().map(|s| text(s.as_str())).collect()),
        ),
        ("rulebook_hash", text(h.rulebook_hash.as_str())),
        ("inputs", Value::array(inputs)),
        ("provenance", provenance(doc.provenance())),
        (
            "synthetic_data",
            h.synthetic_data.map_or_else(Value::null, Value::boolean),
        ),
    ];
    let scope = match h.application {
        ReportApplication::PrizeCheck => RANKING_SCOPE_LABEL,
        ReportApplication::BracketCheck => BRACKET_RANKING_SCOPE_LABEL,
    };
    m.push(("ranking_scope", text(scope)));
    obj(m)
}

/// The machine form: canonical compact RT-JSON with a trailing line feed.
/// Sections follow the fixed order of the model; a Prize_Check report never
/// writes comparisons.
#[must_use]
pub fn render_json(doc: &ReportDoc) -> Vec<u8> {
    let bracket = doc.header().application == ReportApplication::BracketCheck;
    let mut members = vec![("header", header(doc))];
    for s in doc.sections() {
        let value = match s {
            ReportSection::Eligibility(e) => Value::array(e.iter().map(eligibility).collect()),
            ReportSection::Scores(e) => Value::array(e.iter().map(score).collect()),
            ReportSection::Standings(t) => {
                Value::array(t.iter().map(|x| table(x, bracket)).collect())
            }
            ReportSection::Qualification(e) => Value::array(e.iter().map(qualification).collect()),
            ReportSection::Published(p) => published(p),
            ReportSection::Diagnostics(d) => Value::array(d.iter().map(diagnostic).collect()),
        };
        members.push((s.name(), value));
    }
    let mut out = rt_json::write_compact(&obj(members));
    out.push('\n');
    out.into_bytes()
}
