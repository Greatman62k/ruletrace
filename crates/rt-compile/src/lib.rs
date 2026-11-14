//! The Compiler, stage 5 (design §3.6.2, Req 9, 22).
//!
//! [`compile`] turns a `ValidationResult` into a `CompiledRulebook` or into blocking
//! Diagnostics, never into a partial value (Req 9.10).
//!
//! 1. The Approval_Status of every revision is derived again from the Rulebook and the ledger
//!    inside the result, and the blocking Diagnostics of passes 9 and 10 are compared with the
//!    supplied state. Any difference gives `RT-APR-0005` and nothing else (Req 9.12).
//! 2. A state with a blocking Diagnostic gives exactly those Diagnostics, unchanged and in order
//!    (Req 9.2, RN-29).
//! 3. No Effective revision gives `RT-APR-0002` (Req 9.13).
//! 4. Exactly the Effective revisions are lowered; Origin and ledger data are dropped (Req 9.1,
//!    9.3, 9.5).
//! 5. The evaluation order is fixed by Kahn's algorithm with the lowest identifier first
//!    (Req 9.8). A cycle gives `RT-CMP-0001` (Req 9.9).
//!
//! The Compiler reads no file, clock, random value or environment variable. Its output is a
//! function of the `ValidationResult` and [`COMPILER_VERSION`] (Req 9.14).
//!
//! Choices the design leaves open:
//!
//! * The comparison of step 1 covers the blocking Diagnostics of passes 9 and 10, which the
//!   design names. The Compiler holds no Source_Model, so passes 1 to 8 are not repeated; their
//!   Diagnostics are returned by step 2 and a construct that cannot be lowered is
//!   `RT-CMP-0003`. For the same reason a Source_Span Location is compared at document level,
//!   and the Locations the Compiler creates (`RT-CMP-0001`, `RT-APR-0002`) are Document
//!   Locations, never an unverified span.
//! * A `ValidationResult` always carries its Rulebook, ledger and state, so `RT-CMP-0002` cannot
//!   arise from this type.
//! * The Rulebook content hash is the SHA-256 of the canonical compact Rule_IR of the Effective
//!   revisions, with a fixed Origin, name and version and without span line and column
//!   (`content_hash`).
//! * Warnings of the state accompany a successful result. They are not part of the artifact.
#![forbid(unsafe_code)]

mod content_hash;
mod lower;
mod order;

use lower::LowerError;
use rt_compiled::{CompiledParts, CompiledRulebook};
use rt_foundation::{
    stage_result, Code, Diagnostic, DiagnosticState, Location, Rejected, StageResult,
};
use rt_ir::Rulebook;
use rt_ledger::{derive_statuses, effective_revisions};
use rt_validate::ValidationResult;

/// The Compiler version recorded in every Compiled_Rulebook. It is build metadata of this
/// crate, not a clock.
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The blocking codes that passes 9 and 10 produce.
const RECHECKED: [Code; 8] = [
    Code::ValConflictDuplicateTarget,
    Code::ValConflictDuplicateRankKey,
    Code::ValConflictContradictoryDefault,
    Code::ValConflictInputShadowing,
    Code::ValPrecedenceInvalid,
    Code::ApprovalUnresolved,
    Code::ApprovalNoExecutableRules,
    Code::ApprovalRejectedRevisionModified,
];

/// The Document Location of the Rulebook (its first declared document), or no location.
fn rulebook_location(rb: &Rulebook, code: Code) -> Location {
    rb.documents
        .first()
        .map_or(Location::None(code), |d| Location::Document(d.id.clone()))
}

/// A Location with a verified span reduced to its document.
fn at_document(l: &Location) -> Location {
    match l {
        Location::Source(s) => Location::Document(s.doc().clone()),
        other => other.clone(),
    }
}

/// The blocking Diagnostics of passes 9 and 10 in canonical order, compared at document level.
fn gate_view(state: &DiagnosticState) -> Vec<Diagnostic> {
    let mut view: Vec<Diagnostic> = state
        .as_slice()
        .iter()
        .filter(|d| RECHECKED.contains(&d.code()))
        .map(|d| {
            Diagnostic::new(d.code(), d.message().to_owned(), at_document(d.primary()))
                .with_related(d.related().iter().map(at_document).collect())
                .with_rules(d.rules().to_vec())
        })
        .collect();
    view.sort();
    view
}

/// True when two states have the same blocking Diagnostics of passes 9 and 10.
fn agree(fresh: &DiagnosticState, supplied: &DiagnosticState) -> bool {
    gate_view(fresh) == gate_view(supplied)
}

/// Step 1: the supplied state agrees with a fresh derivation.
fn rechecks(result: &ValidationResult) -> bool {
    let fresh = rt_validate::validate(
        result.rulebook(),
        result.ledger(),
        &Default::default(),
        None,
    );
    agree(fresh.state(), result.state())
}

fn invalid(e: LowerError, rb: &Rulebook) -> Diagnostic {
    let location = e
        .rule
        .as_ref()
        .and_then(|id| {
            rb.revisions
                .iter()
                .rfind(|r| &r.key.id == id)
                .and_then(|r| r.content.spans.first())
        })
        .map_or_else(
            || rulebook_location(rb, Code::CmpArtifactInvalid),
            |s| Location::Document(s.document.clone()),
        );
    let message = match &e.rule {
        Some(id) => format!("rule '{id}' cannot be compiled: {}", e.message),
        None => format!("the Rulebook cannot be compiled: {}", e.message),
    };
    Diagnostic::new(Code::CmpArtifactInvalid, message, location)
}

fn build(
    result: &ValidationResult,
) -> Result<(CompiledRulebook, DiagnosticState), Vec<Diagnostic>> {
    let rb = result.rulebook();
    let state = result.state();

    // 1. Re-derive and compare.
    if !rechecks(result) {
        return Err(vec![Diagnostic::new(
            Code::ApprovalRecheckFailed,
            "the Approval_Status derived again from the Rulebook and the ledger does not agree with the blocking Diagnostics of the Validation_Result".to_owned(),
            rulebook_location(rb, Code::ApprovalRecheckFailed),
        )]);
    }

    // 2. Blocking Diagnostics, unchanged and in order.
    let blocking: Vec<Diagnostic> = state
        .as_slice()
        .iter()
        .filter(|d| d.severity().is_blocking())
        .cloned()
        .collect();
    if !blocking.is_empty() {
        return Err(blocking);
    }

    // 3. At least one Effective revision.
    let statuses = derive_statuses(rb, result.ledger());
    let effective = effective_revisions(rb, &statuses);
    if effective.is_empty() {
        return Err(vec![Diagnostic::new(
            Code::ApprovalNoExecutableRules,
            "the Rulebook has no Effective_Revision: no Compiled_Rulebook is produced".to_owned(),
            rulebook_location(rb, Code::ApprovalNoExecutableRules),
        )]);
    }

    // 4. Lower exactly the Effective revisions.
    let fail = |e| vec![invalid(e, rb)];
    let lowered = lower::lower(rb, &effective).map_err(fail)?;
    let hash = content_hash::rulebook_hash(rb, &effective).map_err(fail)?;

    // 5. Evaluation order.
    let targets = rt_compiled::targets_of(&lowered.rules);
    let order = order::evaluation_order(&lowered.rules, &targets)?;

    let parts: CompiledParts =
        lowered.into_parts(COMPILER_VERSION.to_owned(), rb.ir_version, hash, order);
    let compiled = CompiledRulebook::from_parts(parts).map_err(|e| {
        vec![Diagnostic::new(
            Code::CmpArtifactInvalid,
            format!("the lowered Rulebook is inconsistent: {e}"),
            rulebook_location(rb, Code::CmpArtifactInvalid),
        )]
    })?;

    let mut warnings = DiagnosticState::empty();
    for d in state.as_slice() {
        warnings.push(d.clone());
    }
    Ok((compiled, warnings))
}

/// Builds the rejection. Every code returned above is blocking, so the list is accepted at
/// once; the fallback only guards against a catalogue change.
fn reject(diags: Vec<Diagnostic>) -> Rejected {
    let mut state = DiagnosticState::empty();
    for d in &diags {
        state.push(d.clone());
    }
    match Rejected::new(state) {
        Ok(rejected) => rejected,
        Err(_) => {
            let mut diags = diags;
            diags.push(Diagnostic::new(
                Code::InternalError,
                "a compile diagnostic was not blocking".to_owned(),
                Location::None(Code::InternalError),
            ));
            reject(diags)
        }
    }
}

/// Compiles a `ValidationResult` (stage 5).
///
/// Returns the whole `CompiledRulebook` with the Warnings of the state, or a rejection with the
/// blocking Diagnostics and no value. Compilation is atomic (Req 9.10).
///
/// # Errors
/// A `Rejected` with `RT-APR-0005`, the blocking Diagnostics of the state, `RT-APR-0002`,
/// `RT-CMP-0001` or `RT-CMP-0003`.
pub fn compile(result: &ValidationResult) -> StageResult<CompiledRulebook> {
    match build(result) {
        Ok((compiled, warnings)) => stage_result(compiled, warnings),
        Err(diags) => Err(reject(diags)),
    }
}

#[cfg(test)]
mod tests;
