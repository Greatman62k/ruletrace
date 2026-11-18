//! The Evaluate group: Prepare, one engine call for the complete Question
//! list, Present (design §11.3, Req 31.2).
//!
//! This group exists for the tests of Req 31.4 and 31.5. `ruletrace-lab` must
//! not call it, because every Outcome the Laboratory shows comes from a
//! `ruletrace evaluate` subprocess (Req 36.5).

use crate::classify::{class_of_run, ResultClass};
use crate::mapping::FileInputs;
use crate::prepare::{prepare, Prepared, Rejection};
use crate::questions::PrizeRequest;
use crate::report::{present, PresentError, PresentInputs};
use rt_compiled::CompiledRulebook;
use rt_engine::Evaluation;
use rt_explain::report::{ReportDoc, StatementId};
use rt_explain::{verify_trace, Verification};
use rt_srcmodel::SourceModels;

/// The inputs of an evaluating run.
#[derive(Clone, Copy, Debug)]
pub struct PrizeInputs<'a> {
    /// The Compiled_Rulebook. Every parameter is read from it.
    pub compiled: &'a CompiledRulebook,
    /// The requested Targets.
    pub request: &'a PrizeRequest,
    /// The input files.
    pub files: FileInputs<'a>,
    /// The Source_Models for the Explainer. An empty set means that no
    /// provenance is verified.
    pub sources: &'a SourceModels,
    /// The statements the header shows.
    pub statements: &'a [StatementId],
    /// Whether the data is synthetic, as the caller states it.
    pub synthetic_data: Option<bool>,
}

/// A run that reached the engine.
#[derive(Clone, Debug)]
pub struct Evaluated {
    prepared: Prepared,
    evaluation: Evaluation,
    verifications: Vec<Verification>,
    report: Result<ReportDoc, PresentError>,
}

impl Evaluated {
    /// What Prepare produced.
    #[must_use]
    pub fn prepared(&self) -> &Prepared {
        &self.prepared
    }

    /// What the engine returned.
    #[must_use]
    pub fn evaluation(&self) -> &Evaluation {
        &self.evaluation
    }

    /// The Explainer's verification of each trace.
    #[must_use]
    pub fn verifications(&self) -> &[Verification] {
        &self.verifications
    }

    /// The report, or why none could be built. There is no partial report.
    ///
    /// # Errors
    /// [`PresentError`] as returned by [`present`].
    pub fn report(&self) -> Result<&ReportDoc, &PresentError> {
        self.report.as_ref()
    }

    /// The result classification (Req 31.12).
    #[must_use]
    pub fn class(&self) -> ResultClass {
        class_of_run(
            &self.evaluation.outcomes,
            &self.evaluation.diagnostics,
            &self.verifications,
            self.report.is_ok(),
        )
    }
}

/// Maps the files, builds the Questions, calls `rt_engine::evaluate` once for
/// all of them, verifies each trace and builds the report.
///
/// # Errors
/// [`Rejection`] before any engine call, for invalid input or an unusable
/// Compiled_Rulebook.
pub fn evaluate(inputs: &PrizeInputs<'_>) -> Result<Evaluated, Rejection> {
    let prepared = prepare(inputs.compiled, inputs.request, &inputs.files)?;
    let evaluation = rt_engine::evaluate(
        inputs.compiled,
        prepared.data(),
        prepared.questions(),
        prepared.published(),
    );
    let verifications: Vec<Verification> = evaluation
        .traces
        .iter()
        .map(|trace| verify_trace(trace, inputs.sources))
        .collect();
    let report = present(&PresentInputs {
        compiled: inputs.compiled,
        outcomes: &evaluation.outcomes,
        traces: &evaluation.traces,
        verifications: &verifications,
        diagnostics: &evaluation.diagnostics,
        comparison: evaluation.comparison.as_ref(),
        not_requested: prepared.not_requested(),
        input_hashes: prepared.input_hashes(),
        statements: inputs.statements,
        synthetic_data: inputs.synthetic_data,
    });
    Ok(Evaluated {
        prepared,
        evaluation,
        verifications,
        report,
    })
}
