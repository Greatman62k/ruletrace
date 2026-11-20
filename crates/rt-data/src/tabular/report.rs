//! Diagnostic construction with Input_Locations.

use super::csv::Cell;
use rt_foundation::{
    escape_controls, Code, Diagnostic, DiagnosticState, InputId, InputLocation, Location,
};

/// Longest cell text echoed into a message, in scalar values.
const SHOWN_SCALARS: usize = 40;

/// Collects the diagnostics of one file.
pub(crate) struct Reporter<'a> {
    input: &'a InputId,
    diags: Vec<Diagnostic>,
}

impl<'a> Reporter<'a> {
    pub(crate) fn new(input: &'a InputId) -> Self {
        Self {
            input,
            diags: Vec::new(),
        }
    }

    pub(crate) fn at(&self, line: u32, column: u32, field: Option<&str>) -> Location {
        Location::Input(InputLocation {
            input: self.input.clone(),
            line: Some(line),
            column: Some(column),
            field: field.map(str::to_owned),
        })
    }

    pub(crate) fn cell(&self, cell: &Cell, field: &str) -> Location {
        self.at(cell.line, cell.column, Some(field))
    }

    pub(crate) fn file(&self) -> Location {
        Location::Input(InputLocation {
            input: self.input.clone(),
            line: None,
            column: None,
            field: None,
        })
    }

    pub(crate) fn push(&mut self, code: Code, at: Location, message: String) {
        self.diags.push(Diagnostic::new(code, message, at));
    }

    pub(crate) fn push_related(
        &mut self,
        code: Code,
        at: Location,
        message: String,
        related: Location,
    ) {
        self.diags
            .push(Diagnostic::new(code, message, at).with_related(vec![related]));
    }

    pub(crate) fn is_clean(&self) -> bool {
        self.diags.is_empty()
    }

    /// `Ok(value)` when nothing was reported, otherwise the sorted state.
    pub(crate) fn finish<T>(self, value: T) -> Result<T, DiagnosticState> {
        if self.diags.is_empty() {
            Ok(value)
        } else {
            Err(self.into_state())
        }
    }

    pub(crate) fn into_state(self) -> DiagnosticState {
        let mut state = DiagnosticState::empty();
        for d in self.diags {
            state.push(d);
        }
        state.sorted()
    }
}

/// Cell text for a message: control characters escaped, long text cut.
pub(crate) fn shown(text: &str) -> String {
    let mut cut: String = text.chars().take(SHOWN_SCALARS).collect();
    if text.chars().nth(SHOWN_SCALARS).is_some() {
        cut.push('…');
    }
    escape_controls(&cut)
}
