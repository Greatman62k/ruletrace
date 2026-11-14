//! Block structure of a document (design §3.5, Markdown mapping).
//!
//! The parser is line based. It reads the decoded text once, from the top, and
//! never looks back except at the table delimiter row one line ahead. Every
//! offset it produces is a byte offset into the text it was given.
//!
//! Conventions fixed here where the design leaves room:
//! - An element span starts at the first byte of its first line that is not a
//!   space or tab, so leading indentation is never part of a span. It ends at
//!   the end of its last line, before the line terminator. Trailing spaces stay
//!   in the span (bytes are not normalized).
//! - Headings, fences, list markers and tables are recognised at any
//!   indentation.
//! - An indentation width counts a tab as four and any other byte as one.
//! - A list item takes the non-blank lines that follow its first line, up to a
//!   blank line or the next block start. A later item is nested when it is
//!   indented deeper than the open item.
//! - A table row has the table as its parent. Every other block has the nearest
//!   open list item that is indented less than the block, otherwise the nearest
//!   enclosing heading.

use crate::label::leading_label;
use rt_srcmodel::ElementKind;

/// The text is longer than a 32-bit offset can number. The size limit makes
/// this unreachable.
#[derive(Debug)]
pub(crate) struct Overflow;

/// An element before its label is read.
#[derive(Debug)]
pub(crate) struct Draft {
    pub(crate) kind: ElementKind,
    pub(crate) level: Option<u8>,
    /// Offset where a numbering label may start, for kinds that carry one.
    pub(crate) label_from: Option<usize>,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) line: u32,
    pub(crate) column: u32,
    pub(crate) parent: Option<u32>,
}

impl Draft {
    /// The numbering label of the element, read from its text.
    pub(crate) fn label<'t>(&self, text: &'t str) -> Option<&'t str> {
        let from = self.label_from?;
        leading_label(text.get(from..self.end)?)
    }
}

#[derive(Clone, Copy)]
struct Line {
    /// Offset of the first byte of the line.
    start: usize,
    /// Offset where structure detection starts: after a BOM on the first line.
    begin: usize,
    /// Offset of the line terminator, or the end of the text.
    end: usize,
}

fn split_lines(text: &str) -> Vec<Line> {
    let bytes = text.as_bytes();
    let bom = if text.starts_with('\u{feff}') { 3 } else { 0 };
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while let Some(&b) = bytes.get(i) {
        let next = match b {
            b'\n' => i + 1,
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => i + 2,
            b'\r' => i + 1,
            _ => {
                i += 1;
                continue;
            }
        };
        lines.push(Line {
            start,
            begin: start,
            end: i,
        });
        start = next;
        i = next;
    }
    if start < bytes.len() {
        lines.push(Line {
            start,
            begin: start,
            end: bytes.len(),
        });
    }
    if let Some(first) = lines.first_mut() {
        first.begin = bom.min(first.end);
    }
    lines
}

fn is_blank_byte(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// Leading space and tab bytes, and their width with a tab counted as four.
fn indent(body: &str) -> (usize, usize) {
    let mut bytes = 0;
    let mut width = 0;
    for b in body.bytes() {
        match b {
            b' ' => width += 1,
            b'\t' => width += 4,
            _ => break,
        }
        bytes += 1;
    }
    (bytes, width)
}

/// Number of `#` of an ATX heading marker, when `rest` starts one.
fn heading_level(rest: &str) -> Option<u8> {
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    match rest.as_bytes().get(hashes) {
        None | Some(b' ' | b'\t') => u8::try_from(hashes).ok(),
        Some(_) => None,
    }
}

/// The fence character and length when `rest` opens a fenced code block.
fn fence_open(rest: &str) -> Option<(u8, usize)> {
    let c = *rest.as_bytes().first()?;
    if c != b'`' && c != b'~' {
        return None;
    }
    let run = rest.bytes().take_while(|&b| b == c).count();
    if run < 3 {
        return None;
    }
    let info = rest.get(run..)?;
    (c != b'`' || !info.contains('`')).then_some((c, run))
}

/// Whether `rest` closes a fence opened with `c` and length `len`.
fn fence_close(rest: &str, c: u8, len: usize) -> bool {
    let run = rest.bytes().take_while(|&b| b == c).count();
    run >= len && rest.bytes().skip(run).all(is_blank_byte)
}

/// The byte length of a list marker and the spaces after it, when `rest` starts
/// a list item.
fn list_marker(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let marker = match *bytes.first()? {
        b'-' | b'*' | b'+' => 1,
        b'0'..=b'9' => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            if digits > 9 || !matches!(bytes.get(digits), Some(b'.' | b')')) {
                return None;
            }
            digits + 1
        }
        _ => return None,
    };
    match bytes.get(marker) {
        None => Some(marker),
        Some(b' ' | b'\t') => {
            let spaces = bytes
                .get(marker..)?
                .iter()
                .take_while(|&&b| is_blank_byte(b))
                .count();
            Some(marker + spaces)
        }
        Some(_) => None,
    }
}

/// The cells of a pipe-table row, split on `|` that a backslash does not
/// escape.
fn split_cells(row: &str) -> Vec<&str> {
    let t = row.trim_matches([' ', '\t']);
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = match t.strip_suffix('|') {
        Some(inner) if !inner.ends_with('\\') => inner,
        _ => t,
    };
    let mut cells = Vec::new();
    let mut from = 0;
    let mut escaped = false;
    for (i, c) in t.char_indices() {
        if c == '|' && !escaped {
            cells.push(t.get(from..i).unwrap_or(""));
            from = i + 1;
        }
        escaped = c == '\\' && !escaped;
    }
    cells.push(t.get(from..).unwrap_or(""));
    cells
}

fn is_delimiter_cell(cell: &str) -> bool {
    let c = cell.trim_matches([' ', '\t']);
    let c = c.strip_prefix(':').unwrap_or(c);
    let c = c.strip_suffix(':').unwrap_or(c);
    !c.is_empty() && c.bytes().all(|b| b == b'-')
}

struct Parser<'a> {
    text: &'a str,
    lines: Vec<Line>,
    markdown: bool,
    drafts: Vec<Draft>,
    headings: Vec<(u8, u32)>,
    lists: Vec<(usize, u32)>,
}

impl<'a> Parser<'a> {
    fn body(&self, index: usize) -> &'a str {
        self.lines
            .get(index)
            .and_then(|l| self.text.get(l.begin..l.end))
            .unwrap_or("")
    }

    fn is_blank(&self, index: usize) -> bool {
        self.body(index).bytes().all(is_blank_byte)
    }

    /// Offset of the first non-blank byte of the line, and the text from there.
    fn rest(&self, index: usize) -> (usize, &'a str) {
        let body = self.body(index);
        let (bytes, _) = indent(body);
        let begin = self.lines.get(index).map_or(0, |l| l.begin);
        (begin + bytes, body.get(bytes..).unwrap_or(""))
    }

    fn width(&self, index: usize) -> usize {
        indent(self.body(index)).1
    }

    fn line_end(&self, index: usize) -> usize {
        self.lines.get(index).map_or(0, |l| l.end)
    }

    fn push(
        &mut self,
        kind: ElementKind,
        level: Option<u8>,
        label_from: Option<usize>,
        span: (usize, usize),
        line_index: usize,
        parent: Option<u32>,
    ) -> Result<u32, Overflow> {
        let id = u32::try_from(self.drafts.len()).map_err(|_| Overflow)?;
        let line_start = self.lines.get(line_index).map_or(0, |l| l.start);
        let before = self
            .text
            .get(line_start..span.0)
            .map_or(0, |s| s.chars().count());
        let column = u32::try_from(before)
            .ok()
            .and_then(|c| c.checked_add(1))
            .ok_or(Overflow)?;
        let line = u32::try_from(line_index + 1).map_err(|_| Overflow)?;
        self.drafts.push(Draft {
            kind,
            level,
            label_from,
            start: span.0,
            end: span.1,
            line,
            column,
            parent,
        });
        Ok(id)
    }

    /// Closes the lists that a block of the given width does not belong to, and
    /// returns its parent.
    fn block_parent(&mut self, width: usize) -> Option<u32> {
        while self.lists.last().is_some_and(|&(w, _)| w >= width) {
            self.lists.pop();
        }
        self.lists
            .last()
            .map(|&(_, id)| id)
            .or_else(|| self.headings.last().map(|&(_, id)| id))
    }

    fn table_starts(&self, index: usize, rest: &str) -> bool {
        if !rest.contains('|') || index + 1 >= self.lines.len() || self.is_blank(index + 1) {
            return false;
        }
        let header = split_cells(rest);
        let delimiter = split_cells(self.body(index + 1));
        header.len() == delimiter.len() && delimiter.iter().all(|c| is_delimiter_cell(c))
    }

    fn starts_block(&self, index: usize) -> bool {
        let (_, rest) = self.rest(index);
        (self.markdown
            && (heading_level(rest).is_some()
                || fence_open(rest).is_some()
                || self.table_starts(index, rest)))
            || list_marker(rest).is_some()
    }

    fn run(&mut self) -> Result<(), Overflow> {
        let mut open: Option<usize> = None;
        let mut i = 0;
        while i < self.lines.len() {
            if self.is_blank(i) {
                open = None;
                i += 1;
                continue;
            }
            let (first, rest) = self.rest(i);
            let end = self.line_end(i);
            if self.markdown {
                if let Some(level) = heading_level(rest) {
                    open = None;
                    self.heading(i, first, end, level, rest)?;
                    i += 1;
                    continue;
                }
                if let Some((c, len)) = fence_open(rest) {
                    open = None;
                    i = self.fence(i, first, c, len)?;
                    continue;
                }
            }
            if let Some(marker) = list_marker(rest) {
                self.list_item(i, first, end, marker)?;
                open = self.drafts.len().checked_sub(1);
                i += 1;
                continue;
            }
            if self.markdown && self.table_starts(i, rest) {
                open = None;
                i = self.table(i, first)?;
                continue;
            }
            match open.and_then(|index| self.drafts.get_mut(index)) {
                Some(draft) => draft.end = end,
                None => {
                    let parent = self.block_parent(self.width(i));
                    self.push(
                        ElementKind::Paragraph,
                        None,
                        Some(first),
                        (first, end),
                        i,
                        parent,
                    )?;
                    open = self.drafts.len().checked_sub(1);
                }
            }
            i += 1;
        }
        Ok(())
    }

    fn heading(
        &mut self,
        i: usize,
        first: usize,
        end: usize,
        level: u8,
        rest: &str,
    ) -> Result<(), Overflow> {
        while self.headings.last().is_some_and(|&(l, _)| l >= level) {
            self.headings.pop();
        }
        let parent = self.headings.last().map(|&(_, id)| id);
        let after_hashes = rest.get(usize::from(level)..).unwrap_or("");
        let spaces = after_hashes
            .bytes()
            .take_while(|&b| is_blank_byte(b))
            .count();
        let text_from = first + usize::from(level) + spaces;
        let id = self.push(
            ElementKind::Heading,
            Some(level),
            Some(text_from),
            (first, end),
            i,
            parent,
        )?;
        self.headings.push((level, id));
        self.lists.clear();
        Ok(())
    }

    /// Reads a fenced code block that opens on line `i`. Returns the next line
    /// to read.
    fn fence(&mut self, i: usize, first: usize, c: u8, len: usize) -> Result<usize, Overflow> {
        // An unclosed fence ends at its last non-blank line, so that a span
        // never ends in a line terminator.
        let mut last = i;
        let mut j = i + 1;
        while j < self.lines.len() {
            if fence_close(self.rest(j).1, c, len) {
                last = j;
                break;
            }
            if !self.is_blank(j) {
                last = j;
            }
            j += 1;
        }
        let parent = self.block_parent(self.width(i));
        let end = self.line_end(last);
        self.push(ElementKind::CodeBlock, None, None, (first, end), i, parent)?;
        Ok(last + 1)
    }

    fn list_item(
        &mut self,
        i: usize,
        first: usize,
        end: usize,
        marker: usize,
    ) -> Result<(), Overflow> {
        let width = self.width(i);
        let parent = self.block_parent(width);
        let content = first + marker;
        let label_from = (content < end).then_some(content);
        let id = self.push(
            ElementKind::ListItem,
            None,
            label_from,
            (first, end),
            i,
            parent,
        )?;
        self.lists.push((width, id));
        Ok(())
    }

    /// Reads a pipe table whose header row is line `i`. Returns the next line
    /// to read.
    fn table(&mut self, i: usize, first: usize) -> Result<usize, Overflow> {
        let mut last = i + 1;
        while last + 1 < self.lines.len()
            && !self.is_blank(last + 1)
            && !self.starts_block(last + 1)
        {
            last += 1;
        }
        let parent = self.block_parent(self.width(i));
        let table = self.push(
            ElementKind::Table,
            None,
            None,
            (first, self.line_end(last)),
            i,
            parent,
        )?;
        for row in (i..=last).filter(|&row| row != i + 1) {
            let (start, _) = self.rest(row);
            let end = self.line_end(row);
            self.push(
                ElementKind::TableRow,
                None,
                None,
                (start, end),
                row,
                Some(table),
            )?;
        }
        Ok(last + 1)
    }
}

/// Parses `text` into elements in ascending start order.
pub(crate) fn parse(text: &str, markdown: bool) -> Result<Vec<Draft>, Overflow> {
    let mut parser = Parser {
        text,
        lines: split_lines(text),
        markdown,
        drafts: Vec::new(),
        headings: Vec::new(),
        lists: Vec::new(),
    };
    parser.run()?;
    Ok(parser.drafts)
}
