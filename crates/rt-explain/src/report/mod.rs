//! Reports built from Decision_Traces. Task 5.12 provides the Proof_Report;
//! task 9.4 provides the application report model and writers of design §8.5.

mod common;
mod fields;
mod human;
mod machine;
pub mod model;
pub mod proof;

pub use common::{APP_REPORT_FORMAT, APP_REPORT_VERSION};
pub use fields::{FIELD_SET_BRACKET, FIELD_SET_PRIZE};
pub use human::render_human;
pub use machine::render_json;
pub use model::{
    Comparison, EligibilityEntry, Evidence, InputRead, Missing, ProvenanceSummary,
    PublishedDifference, PublishedSection, PublishedUnused, QualificationEntry, ReportApplication,
    ReportDoc, ReportError, ReportHeader, ReportSection, Score, ScoreEntry, Sha256Hex,
    StandingsPlace, StandingsResult, StandingsTable, StatementId, TraceRef, Truth,
};
pub use proof::{
    build_proof_report, render_proof_human, render_proof_json, ApprovalFacts, ApprovalView,
    DecisionSection, ProofError, ProofHeader, ProofInputs, ProofReport, PublishedFact,
    PROOF_REPORT_FORMAT, PROOF_REPORT_VERSION,
};
