//! The `vocabulary` member of a Provider_Request (design 3.7.2, 8.9).

use rt_json::Value;

/// The UD-32 option that decides whether `rank_reference` is a supported
/// operation or a reserved unsupported name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ud32Option {
    /// `rank_reference` is a supported operation.
    RankReference,
    /// `rank_reference` is reserved as unsupported.
    ThresholdOnly,
}

impl Ud32Option {
    /// The option the user confirmed in `tools/decisions/UD-32.md`.
    pub const CONFIRMED: Self = Self::RankReference;
}

pub(crate) const RULE_KINDS: [&str; 5] = [
    "eligibility",
    "derived",
    "weighted_score",
    "ranking",
    "default",
];

const OPERATIONS: [&str; 21] = [
    "add", "sub", "mul", "div", "eq", "ne", "lt", "le", "gt", "ge", "and", "or", "not", "in",
    "field", "ref", "sum", "min", "max", "count", "round",
];

const TYPES: [&str; 5] = ["number", "boolean", "text", "date", "enum"];

pub(crate) const ROUNDING_MODES: [&str; 3] = ["half_even", "half_away_from_zero", "toward_zero"];

// Capabilities that Req 14.5 excludes, in ascending name order.
const UNSUPPORTED: [(&str, &str); 5] = [
    ("bracket_generation", "bracket generation"),
    ("prize_allocation", "prize allocation"),
    ("progression", "tournament progression"),
    ("seeding", "seeding"),
    ("slot_allocation", "slot-based qualification"),
];

const RANK_REFERENCE: (&str, &str) = ("rank_reference", "rank reference");

fn strings(items: &[&str]) -> Value<()> {
    Value::array(items.iter().map(|s| Value::string(*s)).collect())
}

fn unsupported_entry(name: &str, capability: &str) -> Option<Value<()>> {
    Value::object(vec![
        ("name".to_owned(), Value::string(name)),
        ("capability".to_owned(), Value::string(capability)),
    ])
    .ok()
}

/// The vocabulary object for `option`, or `None` when it cannot be built.
pub(crate) fn vocabulary(option: Ud32Option) -> Option<Value<()>> {
    let mut operations: Vec<&str> = OPERATIONS.to_vec();
    let mut unsupported: Vec<(&str, &str)> = UNSUPPORTED.to_vec();
    match option {
        Ud32Option::RankReference => operations.push(RANK_REFERENCE.0),
        Ud32Option::ThresholdOnly => {
            unsupported.push(RANK_REFERENCE);
            unsupported.sort_unstable();
        }
    }
    let unsupported = unsupported
        .iter()
        .map(|(name, capability)| unsupported_entry(name, capability))
        .collect::<Option<Vec<_>>>()?;
    Value::object(vec![
        ("rule_kinds".to_owned(), strings(&RULE_KINDS)),
        ("operations".to_owned(), strings(&operations)),
        ("types".to_owned(), strings(&TYPES)),
        ("rounding_modes".to_owned(), strings(&ROUNDING_MODES)),
        ("unsupported".to_owned(), Value::array(unsupported)),
    ])
    .ok()
}
