//! Values, Outcomes and the reasons an Outcome is Indeterminate.

use rt_foundation::Code;
use rt_numeric::ExactNumber;

/// A Rank_Position and, for a Tie_Group, its members.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankPlace {
    position: u32,
    group: Vec<String>,
}

impl RankPlace {
    /// A Rank_Position (1 or more). `group` lists the members of the Tie_Group,
    /// empty when the participant is alone at the position. Members are stored
    /// in ascending byte order without repeats, which is a serialization order
    /// only and carries no ranking meaning (Req 12.2). Position 0 gives `None`.
    #[must_use]
    pub fn new(position: u32, mut group: Vec<String>) -> Option<Self> {
        if position == 0 {
            return None;
        }
        group.sort();
        group.dedup();
        Some(Self { position, group })
    }

    /// The position (1 plus the number of participants strictly ahead).
    #[must_use]
    pub fn position(&self) -> u32 {
        self.position
    }

    /// Members of the Tie_Group in ascending order; empty when the position is
    /// not shared.
    #[must_use]
    pub fn group(&self) -> &[String] {
        &self.group
    }
}

/// One entry of Standings: a position and the participants that hold it.
///
/// One member means the participant is alone at the position. Several members
/// are one Tie_Group. Members are stored in ascending byte order without
/// repeats, which is a serialization order only and carries no ranking meaning
/// (Req 12.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    position: u32,
    members: Vec<String>,
}

impl Place {
    /// A place. Position 0 or no member gives `None`.
    #[must_use]
    pub fn new(position: u32, mut members: Vec<String>) -> Option<Self> {
        members.sort();
        members.dedup();
        (position != 0 && !members.is_empty()).then_some(Self { position, members })
    }

    /// The Rank_Position (1 plus the number of participants strictly ahead).
    #[must_use]
    pub fn position(&self) -> u32 {
        self.position
    }

    /// The participants at the position, ascending. More than one means a
    /// Tie_Group.
    #[must_use]
    pub fn members(&self) -> &[String] {
        &self.members
    }

    /// Whether the place is a Tie_Group.
    #[must_use]
    pub fn is_tie_group(&self) -> bool {
        self.members.len() > 1
    }
}

/// A value read, computed or decided. Numbers are exact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceValue {
    /// An exact number.
    Number(ExactNumber),
    /// A Boolean.
    Bool(bool),
    /// Text.
    Text(String),
    /// A calendar date in its canonical text.
    Date(String),
    /// A member of a declared enumeration.
    Enum(String),
    /// A Rank_Position.
    Rank(RankPlace),
    /// The Standings of a ranking question: every participant exactly once, in
    /// position order.
    Standings(Vec<Place>),
}

/// Why an Outcome is Indeterminate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// A required input is a Missing_Value.
    MissingInput {
        /// Participant (record) identifier.
        participant: String,
        /// Field name.
        field: String,
    },
    /// `min` or `max` of an empty collection.
    EmptyAggregate {
        /// Collection or field name.
        field: String,
    },
    /// A blocking Diagnostic recorded in the trace (fault, execution conflict,
    /// missing rank value).
    Diagnosed(Code),
}

impl Reason {
    /// Total order used for serialization: kind, then text parts, all bytewise.
    pub(crate) fn sort_key(&self) -> (u8, String, String) {
        match self {
            Reason::MissingInput { participant, field } => (0, participant.clone(), field.clone()),
            Reason::EmptyAggregate { field } => (1, field.clone(), String::new()),
            Reason::Diagnosed(code) => (2, code.id().to_owned(), String::new()),
        }
    }
}

/// The result for one question: a value, or Indeterminate with the reasons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A definite value.
    Decided(TraceValue),
    /// No definite value can be given. Use [`Outcome::indeterminate`] to
    /// normalise the reasons.
    Indeterminate(Vec<Reason>),
}

impl Outcome {
    /// An Indeterminate Outcome with reasons in canonical order and without
    /// repeats.
    #[must_use]
    pub fn indeterminate(reasons: Vec<Reason>) -> Self {
        Outcome::Indeterminate(normalise(reasons))
    }

    /// The same Outcome with Indeterminate reasons in canonical order and
    /// without repeats.
    #[must_use]
    pub fn normalised(&self) -> Outcome {
        match self {
            Outcome::Decided(v) => Outcome::Decided(v.clone()),
            Outcome::Indeterminate(r) => Outcome::indeterminate(r.clone()),
        }
    }
}

pub(crate) fn normalise(mut reasons: Vec<Reason>) -> Vec<Reason> {
    reasons.sort_by_key(Reason::sort_key);
    reasons.dedup();
    reasons
}
