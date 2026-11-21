//! Prepare, part 2: the requested Targets and the Questions built from them
//! (design §8.4, Req 31.2, 31.9).
//!
//! The order of Questions comes from the ordered collections only: Targets by
//! name inside each option, participants by identifier. Ranking Questions name
//! no participant set, so every ranking covers all participants of the data
//! and no Outcome ever decides who is ranked (design §11.3).

use rt_data::tabular::{TargetKind, TargetKinds};
use rt_data::{CompetitionData, Question, TargetName};
use rt_foundation::{escape_controls, CliLocation, Code, Diagnostic, Location};
use std::collections::{BTreeMap, BTreeSet};

/// The Targets a user asked for, by option, each with the zero-based position
/// of its argument as the caller states it. The position only locates a
/// Diagnostic.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrizeRequest {
    /// Targets of `--eligibility`: eligibility predicates.
    pub eligibility: BTreeMap<TargetName, u32>,
    /// Targets of `--score`: derived values and weighted scores.
    pub score: BTreeMap<TargetName, u32>,
    /// Targets of `--ranking`: ranking Targets.
    pub ranking: BTreeMap<TargetName, u32>,
}

/// Targets that were declared and have the kind their option reports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CheckedRequest {
    eligibility: BTreeSet<TargetName>,
    score: BTreeSet<TargetName>,
    ranking: BTreeSet<TargetName>,
}

impl CheckedRequest {
    /// Every requested Target.
    pub(crate) fn all(&self) -> BTreeSet<TargetName> {
        self.eligibility
            .iter()
            .chain(&self.score)
            .chain(&self.ranking)
            .cloned()
            .collect()
    }
}

#[derive(Clone, Copy)]
enum RequestOption {
    Eligibility,
    Score,
    Ranking,
}

impl RequestOption {
    fn name(self) -> &'static str {
        match self {
            Self::Eligibility => "--eligibility",
            Self::Score => "--score",
            Self::Ranking => "--ranking",
        }
    }

    fn reports(self, kind: TargetKind) -> bool {
        match self {
            Self::Eligibility => matches!(kind, TargetKind::Eligibility),
            Self::Score => matches!(kind, TargetKind::Derived | TargetKind::Score),
            Self::Ranking => matches!(kind, TargetKind::Ranking),
        }
    }
}

fn check_group(
    option: RequestOption,
    group: &BTreeMap<TargetName, u32>,
    kinds: &TargetKinds,
    found: &mut Vec<Diagnostic>,
) -> BTreeSet<TargetName> {
    let mut accepted = BTreeSet::new();
    for (target, position) in group {
        let at = Location::Cli(CliLocation::Argument {
            position: *position,
            name: Some(option.name().to_owned()),
        });
        let shown = escape_controls(target.as_str());
        match kinds.get(target) {
            None => found.push(Diagnostic::new(
                Code::AppTargetNotDeclared,
                format!("the Target '{shown}' is not declared in the Compiled_Rulebook"),
                at,
            )),
            Some(kind) if option.reports(kind) => {
                accepted.insert(target.clone());
            }
            Some(_) => found.push(Diagnostic::new(
                Code::AppTargetKindMismatch,
                format!(
                    "the Target '{shown}' has a Rule kind that {} does not report",
                    option.name()
                ),
                at,
            )),
        }
    }
    accepted
}

/// Checks every requested Target against the declared kinds and returns the
/// accepted ones with one Diagnostic per fault. The caller builds no Question
/// when any fault is returned.
pub(crate) fn check_request(
    request: &PrizeRequest,
    kinds: &TargetKinds,
) -> (CheckedRequest, Vec<Diagnostic>) {
    let mut found = Vec::new();
    let checked = CheckedRequest {
        eligibility: check_group(
            RequestOption::Eligibility,
            &request.eligibility,
            kinds,
            &mut found,
        ),
        score: check_group(RequestOption::Score, &request.score, kinds, &mut found),
        ranking: check_group(RequestOption::Ranking, &request.ranking, kinds, &mut found),
    };
    (checked, found)
}

/// One `value` Question per Target and participant for the eligibility
/// Targets, then the score Targets, then one `rank` Question per ranking
/// Target over all participants.
pub(crate) fn build_questions(data: &CompetitionData, request: &CheckedRequest) -> Vec<Question> {
    let mut out = Vec::new();
    for group in [&request.eligibility, &request.score] {
        for target in group {
            for participant in data.participants().keys() {
                out.push(Question::Value {
                    target: target.clone(),
                    participant: participant.clone(),
                });
            }
        }
    }
    for target in &request.ranking {
        out.push(Question::Rank {
            target: target.clone(),
            participants: None,
        });
    }
    out
}
