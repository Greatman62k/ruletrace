//! Prepare, part 2: the requested Targets and the Questions built from them
//! (design §8.7, Req 31.2, 31.9, 32.1, 32.2).
//!
//! The order of Questions comes from the ordered collections only: Targets by
//! name inside each option, participants by identifier. Ranking Questions name
//! no participant set, so every ranking covers all participants of the data
//! and no Outcome ever decides who is ranked (design §11.3).
//!
//! A Qualification_Predicate is an eligibility Target (Req 14.11). The
//! request has no field for a place count, a slot or a rank cutoff: a
//! rank-based clause is not part of the Rule_IR in this version (task 9.3 is
//! deferred), so the Validator rejects it before a Compiled_Rulebook exists.

use rt_data::tabular::{TargetKind, TargetKinds};
use rt_data::{CompetitionData, Question, TargetName};
use rt_foundation::{escape_controls, CliLocation, Code, Diagnostic, Location};
use std::collections::{BTreeMap, BTreeSet};

/// The Targets a user asked for, by option, each with the zero-based position
/// of its argument as the caller states it. The position only locates a
/// Diagnostic.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BracketRequest {
    /// Targets of `--ranking`: ranking Targets, reported as Standings.
    pub ranking: BTreeMap<TargetName, u32>,
    /// Targets of `--qualification`: Qualification_Predicates, which are
    /// eligibility Targets.
    pub qualification: BTreeMap<TargetName, u32>,
}

/// Targets that were declared and have the kind their option reports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CheckedRequest {
    ranking: BTreeSet<TargetName>,
    qualification: BTreeSet<TargetName>,
}

impl CheckedRequest {
    /// Every requested Target.
    pub(crate) fn all(&self) -> BTreeSet<TargetName> {
        self.ranking
            .iter()
            .chain(&self.qualification)
            .cloned()
            .collect()
    }
}

#[derive(Clone, Copy)]
enum RequestOption {
    Ranking,
    Qualification,
}

impl RequestOption {
    fn name(self) -> &'static str {
        match self {
            Self::Ranking => "--ranking",
            Self::Qualification => "--qualification",
        }
    }

    fn reports(self, kind: TargetKind) -> bool {
        match self {
            Self::Ranking => matches!(kind, TargetKind::Ranking),
            Self::Qualification => matches!(kind, TargetKind::Eligibility),
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
    request: &BracketRequest,
    kinds: &TargetKinds,
) -> (CheckedRequest, Vec<Diagnostic>) {
    let mut found = Vec::new();
    let checked = CheckedRequest {
        ranking: check_group(RequestOption::Ranking, &request.ranking, kinds, &mut found),
        qualification: check_group(
            RequestOption::Qualification,
            &request.qualification,
            kinds,
            &mut found,
        ),
    };
    (checked, found)
}

/// One `rank` Question per ranking Target over all participants, then one
/// `value` Question per qualification Target and participant.
pub(crate) fn build_questions(data: &CompetitionData, request: &CheckedRequest) -> Vec<Question> {
    let mut out = Vec::new();
    for target in &request.ranking {
        out.push(Question::Rank {
            target: target.clone(),
            participants: None,
        });
    }
    for target in &request.qualification {
        for participant in data.participants().keys() {
            out.push(Question::Value {
                target: target.clone(),
                participant: participant.clone(),
            });
        }
    }
    out
}
