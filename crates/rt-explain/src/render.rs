//! Human-readable rendering of a verified trace (Req 13.9, 13.13, 13.16, 19.5, 19.10).

use crate::verify::{SpanResolution, Verification, VerificationStatus};
use rt_foundation::escape_controls;
use rt_trace::{DecisionTrace, QuestionKind};
use std::fmt::Write as _;

/// Renders the question, the verification status, every span and the verification diagnostics.
///
/// Source text appears only for spans that resolved, exactly as the Source_Model holds it except
/// that every control character is escaped by `escape_controls`. An unresolved span shows its
/// recorded position and document hash only. `verification` must come from `verify_trace` on the
/// same trace.
#[must_use]
pub fn render_trace(trace: &DecisionTrace, verification: &Verification) -> String {
    let mut out = String::new();
    let q = trace.question();
    let kind = match q.kind {
        QuestionKind::Value => "value",
        QuestionKind::Rank => "rank",
    };
    let _ = writeln!(
        out,
        "question: {kind} of {} for {}",
        escape_controls(&q.target),
        q.participant
            .as_deref()
            .map_or_else(|| "(no participant)".to_owned(), escape_controls)
    );
    let status = match verification.status() {
        VerificationStatus::Verified => "verified",
        VerificationStatus::NotVerified => "not verified",
    };
    let _ = writeln!(out, "provenance: {status}");
    for (i, v) in verification.spans().iter().enumerate() {
        let revision = v
            .revision
            .map_or_else(String::new, |r| format!(" revision {r}"));
        let _ = writeln!(
            out,
            "span {}: rule {}{revision}, document {}, bytes {}..{}, line {}, column {}, sha256 {}",
            i + 1,
            escape_controls(v.rule.as_str()),
            escape_controls(v.span.document.as_str()),
            v.span.start,
            v.span.end,
            v.span.line,
            v.span.column,
            v.span.sha256.as_str()
        );
        if let SpanResolution::Resolved(text) = &v.resolution {
            let mut escaped = escape_controls(text);
            if !escaped.ends_with('\n') {
                escaped.push('\n');
            }
            out.push_str("  text: ");
            out.push_str(&escaped);
        }
    }
    for d in verification.diagnostics().as_slice() {
        let _ = writeln!(
            out,
            "diagnostic {} ({:?}): {}",
            d.code(),
            d.severity(),
            escape_controls(d.message())
        );
    }
    out
}
