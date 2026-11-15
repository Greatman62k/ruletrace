//! Typed access to the top-level fields of the configuration table. Every
//! failure is a `RT-AI-0002` Diagnostic at the offending key.

use crate::locate::Locator;
use rt_foundation::{Code, Diagnostic, Location};
use toml_edit::{Item, Key, TableLike};

/// A value together with the location of its key.
#[derive(Clone, Debug)]
pub(crate) struct Found<T> {
    pub(crate) value: T,
    pub(crate) at: Location,
}

/// Field access over one parsed table.
pub(crate) struct Reader<'a> {
    pub(crate) table: &'a dyn TableLike,
    pub(crate) locator: &'a Locator<'a>,
    /// Field path of `table` with a trailing dot, empty at the top level.
    pub(crate) prefix: &'a str,
}

impl Reader<'_> {
    /// The field path of `name` in this table, as shown in locations.
    pub(crate) fn path(&self, name: &str) -> String {
        format!("{}{name}", self.prefix)
    }

    pub(crate) fn key_location(&self, key: &Key, name: &str) -> Location {
        self.locator.at(key.span(), Some(&self.path(name)))
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
            None => Err(self.invalid(
                at,
                format!("field `{}` must be {expected}", self.path(name)),
            )),
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
            self.locator.at(None, Some(&self.path(name))),
            format!("required field `{}` is missing", self.path(name)),
        ));
    }
}

impl<'a> Reader<'a> {
    /// A nested table (standard or inline) as a reader whose locations use
    /// the dotted path `prefix`. `Ok(None)` when the key is absent.
    pub(crate) fn subtable(
        &self,
        name: &str,
        prefix: &'a str,
    ) -> Result<Option<(Location, Reader<'a>)>, Diagnostic> {
        let table: &'a dyn TableLike = self.table;
        let Some((key, item)) = table.get_key_value(name) else {
            return Ok(None);
        };
        let at = self.key_location(key, name);
        match item.as_table_like() {
            Some(inner) => Ok(Some((
                at,
                Reader {
                    table: inner,
                    locator: self.locator,
                    prefix,
                },
            ))),
            None => Err(self.invalid(at, format!("field `{}` must be a table", self.path(name)))),
        }
    }
}
