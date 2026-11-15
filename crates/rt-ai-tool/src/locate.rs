//! Conversion of TOML byte offsets into Input_Locations.

use rt_foundation::{InputId, InputLocation, Location};
use std::ops::Range;

/// Builds locations inside one configuration text.
pub(crate) struct Locator<'a> {
    input: &'a InputId,
    text: &'a str,
}

impl<'a> Locator<'a> {
    pub(crate) fn new(input: &'a InputId, text: &'a str) -> Self {
        Self { input, text }
    }

    fn position(&self, offset: usize) -> Option<(u32, u32)> {
        let prefix = self.text.get(..offset)?;
        let mut line = 1u32;
        let mut column = 1u32;
        for c in prefix.chars() {
            if c == '\n' {
                line = line.saturating_add(1);
                column = 1;
            } else {
                column = column.saturating_add(1);
            }
        }
        Some((line, column))
    }

    /// Location at the start of `span`. Without a span, or with an offset
    /// that is not inside the text, line and column stay unknown.
    pub(crate) fn at(&self, span: Option<Range<usize>>, field: Option<&str>) -> Location {
        let position = span.and_then(|s| self.position(s.start));
        Location::Input(InputLocation {
            input: self.input.clone(),
            line: position.map(|p| p.0),
            column: position.map(|p| p.1),
            field: field.map(str::to_owned),
        })
    }
}
