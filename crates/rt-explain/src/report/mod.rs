//! Reports built from Decision_Traces. Task 5.12 provides the Proof_Report; the application
//! report model of design §8.5 is added by the application tasks.

pub mod proof;

pub use proof::{
    build_proof_report, render_proof_human, render_proof_json, ApprovalFacts, ApprovalView,
    DecisionSection, ProofError, ProofHeader, ProofInputs, ProofReport, PublishedFact,
    PROOF_REPORT_FORMAT, PROOF_REPORT_VERSION,
};
