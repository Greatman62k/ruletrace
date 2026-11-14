//! Input positions kept on IR nodes for diagnostics.

use rt_json::Position;

/// Where a node started in the parsed Rule_IR input, or nothing for a node
/// built in memory.
///
/// Positions never take part in equality, ordering or hashing: two Rulebooks
/// that differ only in layout are equal, which is what the round-trip property
/// of Req 3.10 and 4.6 compares.
#[derive(Clone, Copy, Debug, Default)]
pub struct Loc(Option<Position>);

impl Loc {
    /// No position (a node built in memory).
    pub const NONE: Loc = Loc(None);

    /// A position from the parser.
    #[must_use]
    pub fn at(position: Position) -> Self {
        Self(Some(position))
    }

    /// The position, when the node was parsed.
    #[must_use]
    pub fn position(&self) -> Option<Position> {
        self.0
    }
}

impl PartialEq for Loc {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for Loc {}

impl std::hash::Hash for Loc {
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {}
}
