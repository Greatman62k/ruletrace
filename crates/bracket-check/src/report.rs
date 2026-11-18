//! The Present group: the Tournament standings report from a
//! Compiled_Rulebook and the Outcomes, Decision_Traces and verifications of an
//! evaluation (design §8.5, §8.7, Req 32.1, 32.2, 32.6, 31.7, 31.13, 31.14).
//!
//! The inputs are typed values. `Evaluate` passes what the engine returned,
//! and the results-file reader of the Laboratory passes what it parsed. The
//! module copies every value into the report model of `rt-explain` and
//! computes, compares and orders nothing. Entries follow the order of the
//! Outcomes.

use crate::mapping::target_kinds_by_name;
use rt_compiled::CompiledRulebook;
use rt_data::tabular::{TargetKind, UnusedPublished};
use rt_data::PublishedValue;
use rt_engine::{Comparison, Computed, DecisionTrace, Difference, Outcome, TraceValue};
pub use rt_explain::report::PresentError;
use rt_explain::report::{
    assemble_report, compared_section, AssembleInputs, DifferenceFacts, PublishedSection,
    PublishedUnused, QualificationEntry, ReportApplication, ReportDoc, SectionEntry, Sha256Hex,
    StandingsTable, StatementId, TraceRef,
};
use rt_explain::Verification;
use rt_foundation::DiagnosticState;
use rt_numeric::to_canonical;

/// Everything Present reads.
#[derive(Clone, Copy, Debug)]
pub struct PresentInputs<'a> {
    /// The Compiled_Rulebook the evaluation ran on. It gives the report its
    /// hash and the kind of each Target.
    pub compiled: &'a CompiledRulebook,
    /// One Outcome per Question, in Question order.
    pub outcomes: &'a [Outcome],
    /// The Decision_Trace of each Outcome, in the same order.
    pub traces: &'a [DecisionTrace],
    /// The Explainer's verification of each trace, in the same order.
    pub verifications: &'a [Verification],
    /// The Diagnostics of the evaluation itself.
    pub diagnostics: &'a DiagnosticState,
    /// The comparison with the Published_Result, when one was supplied.
    pub comparison: Option<&'a Comparison>,
    /// Published rows for Targets that were not requested.
    pub not_requested: &'a [UnusedPublished],
    /// The SHA-256 of each input by role.
    pub input_hashes: &'a [(&'static str, Sha256Hex)],
    /// The statements the header shows.
    pub statements: &'a [StatementId],
    /// Whether the data is synthetic, as the caller states it. A label only.
    pub synthetic_data: Option<bool>,
}

fn published_text(value: &PublishedValue) -> String {
    match value {
        PublishedValue::Bool(b) => b.to_string(),
        PublishedValue::Number(n) => to_canonical(n.value()),
    }
}

fn computed_text(difference: &Difference) -> Result<Option<String>, PresentError> {
    match &difference.computed {
        Computed::Indeterminate => Ok(None),
        Computed::Value(TraceValue::Number(n)) => Ok(Some(to_canonical(n))),
        Computed::Value(TraceValue::Bool(b)) => Ok(Some(b.to_string())),
        Computed::Value(_) => Err(PresentError::ComputedValueNotComparable {
            question: difference.question,
        }),
    }
}

fn published_section(
    comparison: Option<&Comparison>,
    not_requested: &[UnusedPublished],
) -> Result<PublishedSection, PresentError> {
    let Some(comparison) = comparison else {
        return Ok(PublishedSection::NotSupplied);
    };
    let mut differences = Vec::new();
    for d in &comparison.differences {
        let facts = DifferenceFacts {
            participant: d.participant.as_str().to_owned(),
            target: d.target.as_str().to_owned(),
            published: published_text(&d.published),
            computed: computed_text(d)?,
            trace: d.question,
            entry: d.entry,
            non_terminating: d.non_terminating,
            group: d.group.iter().map(|p| p.as_str().to_owned()).collect(),
        };
        differences.push((d.kind.as_str(), facts));
    }
    let engine_unused = comparison.unused.iter().map(|(p, t)| PublishedUnused {
        participant: p.as_str().to_owned(),
        target: t.as_str().to_owned(),
    });
    let file_unused = not_requested.iter().map(|u| PublishedUnused {
        participant: u.participant.as_str().to_owned(),
        target: u.target.as_str().to_owned(),
    });
    compared_section(differences, engine_unused.chain(file_unused).collect())
}

/// Builds the Tournament standings report.
///
/// Each rank trace becomes one Standings table with its tie-breaking
/// comparisons, and each value trace of an eligibility Target becomes one
/// Qualification entry. Entries stay in the order of the Outcomes. The header
/// states provenance as verified only when the Explainer verified every
/// trace, and states that nothing was verified when no Source_Model was
/// supplied.
///
/// # Errors
/// [`PresentError`] when the lists are inconsistent, a trace answers a Target
/// that cannot be reported, or the report model refuses an entry. No partial
/// report is returned.
pub fn present(inputs: &PresentInputs<'_>) -> Result<ReportDoc, PresentError> {
    let kinds = target_kinds_by_name(inputs.compiled);
    let entry_of = |index: usize, trace: &DecisionTrace, at: &TraceRef<'_>| match kinds
        .get(&trace.question().target)
    {
        Some(TargetKind::Ranking) => Ok(SectionEntry::Standings(StandingsTable::from_trace(
            at, true,
        )?)),
        Some(TargetKind::Eligibility) => Ok(SectionEntry::Qualification(
            QualificationEntry::from_trace(at)?,
        )),
        Some(TargetKind::Derived | TargetKind::Score | TargetKind::Other) | None => {
            Err(PresentError::TargetNotReportable { trace: index })
        }
    };
    let shared = AssembleInputs {
        application: ReportApplication::BracketCheck,
        outcomes: inputs.outcomes,
        traces: inputs.traces,
        verifications: inputs.verifications,
        diagnostics: inputs.diagnostics,
        rulebook_hash: inputs.compiled.rulebook_hash().as_str(),
        input_hashes: inputs.input_hashes,
        statements: inputs.statements,
        synthetic_data: inputs.synthetic_data,
    };
    assemble_report(&shared, entry_of, || {
        published_section(inputs.comparison, inputs.not_requested)
    })
}
