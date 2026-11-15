//! The single output type of the tool. Everything written to standard
//! output or standard error passes through the [`Redactor`], which also
//! removes cut and truncated copies of the credential.

use crate::redact::Redactor;
use rt_foundation::{escape_controls, CliLocation, Diagnostic, Location, Severity};
use std::io::{self, Write};

/// Redacting writers for standard output and standard error.
pub struct Out<O: Write, E: Write> {
    stdout: O,
    stderr: E,
    redactor: Redactor,
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Ambiguity => "ambiguity",
        Severity::Unsupported => "unsupported",
    }
}

fn location(location: &Location, r: &Redactor) -> String {
    match location {
        Location::Source(s) => format!("source {}", clean(r, s.doc().as_str())),
        Location::Document(d) => format!("document '{}'", clean(r, d.as_str())),
        Location::Input(i) => {
            let mut text = format!("input '{}'", clean(r, i.input.as_str()));
            if let Some(line) = i.line {
                text.push_str(&format!(" line {line}"));
            }
            if let Some(column) = i.column {
                text.push_str(&format!(" column {column}"));
            }
            if let Some(field) = &i.field {
                text.push_str(&format!(" field {}", clean(r, field)));
            }
            text
        }
        Location::Cli(CliLocation::Argument { position, name }) => match name {
            Some(name) => format!("argument {position} ({})", clean(r, name)),
            None => format!("argument {position}"),
        },
        Location::Cli(CliLocation::Command(name)) => format!("command '{}'", clean(r, name)),
        Location::None(reason) => format!("no location ({})", reason.id()),
    }
}

/// Redacts first, then escapes: an escaped credential would no longer match.
/// Fragments are removed too, because a message can copy untrusted text that
/// a cut left with only part of the credential.
fn clean(r: &Redactor, text: &str) -> String {
    escape_controls(&r.redact_fragments(text))
}

/// One Diagnostic as a line group, every line ending in a line feed. No
/// credential is known here, so nothing is redacted.
#[must_use]
pub fn render_diagnostic(d: &Diagnostic) -> String {
    render(d, &Redactor::no_credential())
}

fn render(d: &Diagnostic, r: &Redactor) -> String {
    let mut text = format!(
        "{} {}: {}\n  at {}\n",
        severity_name(d.severity()),
        d.code().id(),
        clean(r, d.message()).replace('\n', "\\u{a}"),
        location(d.primary(), r)
    );
    for related in d.related() {
        text.push_str(&format!("  related {}\n", location(related, r)));
    }
    text
}

impl<O: Write, E: Write> Out<O, E> {
    /// Output that redacts with `redactor`.
    pub fn new(stdout: O, stderr: E, redactor: Redactor) -> Self {
        Self {
            stdout,
            stderr,
            redactor,
        }
    }

    /// Output for a run that has no credential. Use [`Out::set_redactor`]
    /// as soon as one has been read.
    pub fn without_credential(stdout: O, stderr: E) -> Self {
        Self::new(stdout, stderr, Redactor::no_credential())
    }

    /// Replaces the redactor. Called as soon as a credential has been read.
    pub fn set_redactor(&mut self, redactor: Redactor) {
        self.redactor = redactor;
    }

    /// Writes redacted text to standard output. Runs of eight or more
    /// credential characters are replaced, not only the whole value.
    ///
    /// # Errors
    /// The error of the underlying writer.
    pub fn stdout(&mut self, text: &str) -> io::Result<()> {
        let text = self.redactor.redact_fragments(text);
        self.stdout.write_all(text.as_bytes())
    }

    /// Writes redacted text to standard error.
    ///
    /// # Errors
    /// The error of the underlying writer.
    pub fn stderr(&mut self, text: &str) -> io::Result<()> {
        let text = self.redactor.redact_fragments(text);
        self.stderr.write_all(text.as_bytes())
    }

    /// Writes a Diagnostic to standard error. The credential is removed
    /// before control characters are escaped.
    ///
    /// # Errors
    /// The error of the underlying writer.
    pub fn diagnostic(&mut self, d: &Diagnostic) -> io::Result<()> {
        let text = render(d, &self.redactor);
        self.stderr.write_all(text.as_bytes())
    }
}
