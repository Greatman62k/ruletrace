//! Typed access to the top-level fields of the configuration table. Every
//! failure is a `RT-AI-0002` Diagnostic at the offending key.

use crate::locate::Locator;
use rt_foundation::{Code, Diagnostic, Location};
use toml_edit::{Item, Key, Table};

/// A value together with the location of its key.
#[derive(Clone, Debug)]
pub(crate) struct Found<T> {
    pub(crate) value: T,
    pub(crate) at: Location,
}

/// Field access over one parsed table.
pub(crate) struct Reader<'a> {
    pub(crate) table: &'a Table,
    pub(crate) locator: &'a Locator<'a>,
}

impl Reader<'_> {
    pub(crate) fn key_location(&self, key: &Key, name: &str) -> Location {
        self.locator.at(key.span(), Some(name))
    }

    fn entry(&self, name: &str) -> Option<(Location, &Item)> {
        let (key, item) = self.table.get_key_value(name)?;
        Some((self.key_location(key, name), item))
    }

    pub(crate) fn invalid(&self, at: Location, message: String) -> Diagnostic {
        Diagnostic::new(Code::AiConfigInvalid, message, at)
    }

    fn typed<T>(
        &self,
        name: &str,
        expected: &str,
        convert: impl FnOnce(&Item) -> Option<T>,
    ) -> Result<Option<Found<T>>, Diagnostic> {
        let Some((at, item)) = self.entry(name) else {
            return Ok(None);
        };
        match convert(item) {
            Some(value) => Ok(Some(Found { value, at })),
            None => Err(self.invalid(at, format!("field `{name}` must be {expected}"))),
        }
    }

    pub(crate) fn string(&self, name: &str) -> Result<Option<Found<String>>, Diagnostic> {
        self.typed(name, "a string", |i| i.as_str().map(str::to_owned))
    }

    pub(crate) fn boolean(&self, name: &str) -> Result<Option<Found<bool>>, Diagnostic> {
        self.typed(name, "true or false", Item::as_bool)
    }

    pub(crate) fn integer(&self, name: &str) -> Result<Option<Found<i64>>, Diagnostic> {
        self.typed(name, "an integer", Item::as_integer)
    }

    pub(crate) fn string_list(&self, name: &str) -> Result<Option<Found<Vec<String>>>, Diagnostic> {
        self.typed(name, "an array of strings", |item| {
            let array = item.as_array()?;
            array
                .iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
    }

    /// Reports a required field that is not in the file. A field that is
    /// present but unreadable already has its own finding. A missing field
    /// has no key, so only its name is known.
    pub(crate) fn require(&self, name: &str, readable: bool, out: &mut Vec<Diagnostic>) {
        if readable || self.table.contains_key(name) {
            return;
        }
        out.push(self.invalid(
            self.locator.at(None, Some(name)),
            format!("required field `{name}` is missing"),
        ));
    }
}
