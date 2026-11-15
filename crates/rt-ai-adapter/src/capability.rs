//! Capabilities of a provider and the pure negotiation against what a run
//! requires (UD-37, Req 35.2).

/// A three-valued answer. Unknown is not yes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tri {
    /// Not established.
    Unknown,
    /// Supported.
    Yes,
    /// Not supported.
    No,
}

/// What a provider is declared to support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Strict JSON Schema structured output.
    pub structured_output: Tri,
    /// A reasoning effort control.
    pub reasoning_effort: Tri,
    /// Token usage in responses. Informational: a run cannot require it.
    pub usage_reporting: Tri,
}

impl Capabilities {
    /// Nothing is established, so every requirement fails closed.
    pub const UNKNOWN: Self = Self {
        structured_output: Tri::Unknown,
        reasoning_effort: Tri::Unknown,
        usage_reporting: Tri::Unknown,
    };
}

/// The request side of structured output: the schema that a provider is
/// asked to follow. It is a pure function of the vocabulary and is not part
/// of the canonical request bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredOutput {
    /// The schema name that the provider echoes.
    pub name: String,
    /// The canonical schema bytes.
    pub schema: Vec<u8>,
}

/// What a run requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Required {
    /// Strict JSON Schema structured output.
    pub structured_output: bool,
    /// A reasoning effort control.
    pub reasoning_effort: bool,
}

/// A requirement that the available capabilities do not meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unmet {
    /// The capability name.
    pub capability: &'static str,
    /// Its value: `No` or `Unknown`.
    pub value: Tri,
}

/// Checks `required` against `available`.
///
/// Pure: no clock, file, environment or network. A required capability is
/// met only by `Yes`, so `Unknown` fails closed.
///
/// # Errors
/// Every unmet requirement, in ascending byte order of the capability name.
pub fn negotiate(required: &Required, available: &Capabilities) -> Result<(), Vec<Unmet>> {
    let checks = [
        (
            required.reasoning_effort,
            "reasoning_effort",
            available.reasoning_effort,
        ),
        (
            required.structured_output,
            "structured_output",
            available.structured_output,
        ),
    ];
    let mut unmet: Vec<Unmet> = checks
        .into_iter()
        .filter(|(needed, _, value)| *needed && *value != Tri::Yes)
        .map(|(_, capability, value)| Unmet { capability, value })
        .collect();
    unmet.sort_by_key(|u| u.capability);
    if unmet.is_empty() {
        Ok(())
    } else {
        Err(unmet)
    }
}
