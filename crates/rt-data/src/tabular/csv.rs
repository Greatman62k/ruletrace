//! The CSV parser of the Application_Input_Format (design §8.3).

use super::report::Reporter;
use rt_foundation::{Code, DiagnosticState, InputId, MAX_COMPETITION_DATA_BYTES};
use std::iter::Peekable;
use std::str::Chars;

/// The largest number of data rows of one file (`RT-LIM-0004`).
pub const MAX_DATA_ROWS: usize = 100_000;

const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// The largest accepted file size in bytes (`RT-LIM-0001`). A file of exactly
/// this size is accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteLimit(u64);

impl ByteLimit {
    /// The 32 MiB limit of Competition_Data and application input files.
    pub const DEFAULT: ByteLimit = ByteLimit(MAX_COMPETITION_DATA_BYTES);

    /// A limit of `bytes`.
    #[must_use]
    pub fn new(bytes: u64) -> Self {
        Self(bytes)
    }

    /// The limit in bytes.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

impl Default for ByteLimit {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One cell with the position of its first character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The cell text, never trimmed. A quoted cell holds the text between its
    /// quotes with `""` read as one quote.
    pub text: String,
    /// 1-based line of the first character of the cell (the opening quote of
    /// a quoted cell).
    pub line: u32,
    /// 1-based column in Unicode scalar values of that character, counted
    /// after a leading byte order mark.
    pub column: u32,
}

/// A parsed file: the header row and the data rows, each with as many cells
/// as the header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    /// The file the table was read from, backslashes normalized to `/`.
    pub input: InputId,
    /// The header cells.
    pub header: Vec<Cell>,
    /// The data rows in file order.
    pub rows: Vec<Vec<Cell>>,
}

/// The dialect: UTF-8, one optional leading byte order mark, LF or CRLF line
/// ends, `,` separators, `"` quotes with `""` as an escaped quote (a quoted
/// cell may hold separators and line ends), the first row as the header, and
/// cells that are never trimmed. A blank line, a line whose first unquoted
/// character is `#`, a NUL character, a header line that ends in a separator
/// (an empty column name) and a row whose cell count differs from the header
/// are errors.
///
/// A data row that ends in a separator has an empty final cell, so `1,` and
/// `1,""` are the same two cells. The row is then checked against the header
/// like any other: a row whose empty final cell is one cell too many is
/// reported at that cell, and a row one cell short is reported at its end.
///
/// # Errors
/// `RT-LIM-0001` for a file over `size`, `RT-LIM-0004` for more than 100,000
/// data rows, and `RT-APP-0001` for every other fault, one per fault.
pub fn parse_csv(bytes: &[u8], input: &InputId, size: ByteLimit) -> Result<Table, DiagnosticState> {
    let input = normalized(input);
    let mut rep = Reporter::new(&input);
    if u64::try_from(bytes.len()).map_or(true, |n| n > size.get()) {
        let at = rep.file();
        rep.push(
            Code::LimitFileTooLarge,
            at,
            format!(
                "the file has {} bytes, the limit is {}",
                bytes.len(),
                size.get()
            ),
        );
        return Err(rep.into_state());
    }
    let body = bytes.strip_prefix(&BOM[..]).unwrap_or(bytes);
    let text = match std::str::from_utf8(body) {
        Ok(text) => text,
        Err(e) => {
            let prefix = body.get(..e.valid_up_to()).unwrap_or_default();
            let (line, column) = position_after(&String::from_utf8_lossy(prefix));
            let at = rep.at(line, column, None);
            rep.push(
                Code::AppInputMalformed,
                at,
                "the file is not valid UTF-8".to_owned(),
            );
            return Err(rep.into_state());
        }
    };
    let (header, rows) = read_rows(text, &mut rep);
    match header {
        Some(header) if rep.is_clean() => Ok(Table {
            input: input.clone(),
            header,
            rows,
        }),
        _ => Err(rep.into_state()),
    }
}

fn normalized(input: &InputId) -> InputId {
    InputId::new(input.as_str().replace('\\', "/")).unwrap_or_else(|_| input.clone())
}

/// The 1-based position of the character that follows `prefix`.
fn position_after(prefix: &str) -> (u32, u32) {
    let mut lexer = Lexer::new(prefix);
    while lexer.bump().is_some() {}
    lexer.pos()
}

struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
    line: u32,
    column: u32,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            chars: text.chars().peekable(),
            line: 1,
            column: 1,
        }
    }

    fn pos(&self) -> (u32, u32) {
        (self.line, self.column)
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        if c == '\n' {
            self.line = self.line.saturating_add(1);
            self.column = 1;
        } else {
            self.column = self.column.saturating_add(1);
        }
        Some(c)
    }

    /// True at `\n` or at `\r\n`. A `\r` not followed by `\n` is not a line
    /// end.
    fn at_line_end(&mut self) -> bool {
        match self.peek() {
            Some('\n') => true,
            Some('\r') => {
                let mut ahead = self.chars.clone();
                ahead.next();
                ahead.next() == Some('\n')
            }
            _ => false,
        }
    }

    fn consume_line_end(&mut self) {
        if self.peek() == Some('\r') {
            self.bump();
        }
        if self.peek() == Some('\n') {
            self.bump();
        }
    }

    fn skip_line(&mut self) {
        while let Some(c) = self.bump() {
            if c == '\n' {
                break;
            }
        }
    }
}

/// One lexed row. `ok` is false when a fault was reported for it.
struct Row {
    cells: Vec<Cell>,
    ok: bool,
    end: (u32, u32),
    /// Position of the separator that ends the row, when the row ends in
    /// one. The empty cell after it is the last of `cells`.
    trailing: Option<(u32, u32)>,
}

fn read_rows(text: &str, rep: &mut Reporter<'_>) -> (Option<Vec<Cell>>, Vec<Vec<Cell>>) {
    let mut lexer = Lexer::new(text);
    let mut header: Option<Vec<Cell>> = None;
    let mut header_seen = false;
    let mut rows = Vec::new();
    let mut data_rows = 0usize;
    while lexer.peek().is_some() {
        let (line, column) = lexer.pos();
        if lexer.at_line_end() {
            let at = rep.at(line, column, None);
            rep.push(Code::AppInputMalformed, at, "blank line".to_owned());
            lexer.consume_line_end();
            continue;
        }
        if lexer.peek() == Some('#') {
            let at = rep.at(line, column, None);
            let message = "comment line (quote the first cell to start a cell with #)";
            rep.push(Code::AppInputMalformed, at, message.to_owned());
            lexer.skip_line();
            continue;
        }
        if header_seen {
            if data_rows >= MAX_DATA_ROWS {
                let at = rep.at(line, column, None);
                rep.push(
                    Code::LimitCountExceeded,
                    at,
                    format!("more than {MAX_DATA_ROWS} data rows"),
                );
                break;
            }
            data_rows += 1;
        }
        let row = lex_row(&mut lexer, rep);
        if !header_seen {
            header_seen = true;
            if let Some((sl, sc)) = row.trailing {
                // An empty column name cannot be mapped to a field.
                let at = rep.at(sl, sc, None);
                let message = "the header ends in a separator (an empty column name)";
                rep.push(Code::AppInputMalformed, at, message.to_owned());
            } else if row.ok {
                header = Some(row.cells);
            }
            continue;
        }
        if !row.ok {
            continue;
        }
        let Some(head) = header.as_ref() else {
            continue;
        };
        if row.cells.len() == head.len() {
            rows.push(row.cells);
        } else if let Some(extra) = row.cells.get(head.len()) {
            let at = rep.at(extra.line, extra.column, None);
            let message = format!(
                "the row has {} cells, the header has {}",
                row.cells.len(),
                head.len()
            );
            rep.push(Code::AppInputMalformed, at, message);
        } else {
            let field = head.get(row.cells.len()).map(|c| c.text.as_str());
            let at = rep.at(row.end.0, row.end.1, field.filter(|f| !f.is_empty()));
            let message = format!(
                "the row has {} cells, the header has {}",
                row.cells.len(),
                head.len()
            );
            rep.push(Code::AppInputMalformed, at, message);
        }
    }
    if !header_seen {
        let at = rep.at(1, 1, None);
        rep.push(
            Code::AppInputMalformed,
            at,
            "the file has no header row".to_owned(),
        );
    }
    (header, rows)
}

fn lex_row(lexer: &mut Lexer<'_>, rep: &mut Reporter<'_>) -> Row {
    let mut cells = Vec::new();
    let mut ok = true;
    loop {
        let (line, column) = lexer.pos();
        let lexed = if lexer.peek() == Some('"') {
            lex_quoted(lexer, rep, &mut ok)
        } else {
            lex_plain(lexer, rep, &mut ok)
        };
        let Some(text) = lexed else {
            return Row {
                cells,
                ok: false,
                end: lexer.pos(),
                trailing: None,
            };
        };
        cells.push(Cell { text, line, column });
        match lexer.peek() {
            Some(',') => {
                let delimiter = lexer.pos();
                lexer.bump();
                if lexer.peek().is_none() || lexer.at_line_end() {
                    // The delimiter closes the row: an empty final cell.
                    let (line, column) = lexer.pos();
                    cells.push(Cell {
                        text: String::new(),
                        line,
                        column,
                    });
                    lexer.consume_line_end();
                    return Row {
                        cells,
                        ok,
                        end: (line, column),
                        trailing: Some(delimiter),
                    };
                }
            }
            _ => {
                let end = lexer.pos();
                lexer.consume_line_end();
                return Row {
                    cells,
                    ok,
                    end,
                    trailing: None,
                };
            }
        }
    }
}

fn nul(lexer: &Lexer<'_>, rep: &mut Reporter<'_>, ok: &mut bool) {
    let (line, column) = lexer.pos();
    let at = rep.at(line, column, None);
    rep.push(
        Code::AppInputMalformed,
        at,
        "NUL character in the file".to_owned(),
    );
    *ok = false;
}

/// An unquoted cell. `None` after a fault that ends the row (the rest of the
/// line is skipped).
fn lex_plain(lexer: &mut Lexer<'_>, rep: &mut Reporter<'_>, ok: &mut bool) -> Option<String> {
    let mut text = String::new();
    loop {
        match lexer.peek() {
            None | Some(',') => return Some(text),
            Some('\r') | Some('\n') if lexer.at_line_end() => return Some(text),
            Some('"') => {
                let (line, column) = lexer.pos();
                let at = rep.at(line, column, None);
                rep.push(
                    Code::AppInputMalformed,
                    at,
                    "quote inside an unquoted cell".to_owned(),
                );
                lexer.skip_line();
                return None;
            }
            Some('\r') => {
                let (line, column) = lexer.pos();
                let at = rep.at(line, column, None);
                rep.push(
                    Code::AppInputMalformed,
                    at,
                    "carriage return without a line feed".to_owned(),
                );
                lexer.skip_line();
                return None;
            }
            Some(c) => {
                if c == '\0' {
                    nul(lexer, rep, ok);
                }
                lexer.bump();
                text.push(c);
            }
        }
    }
}

/// A quoted cell. `None` after a fault that ends the row.
fn lex_quoted(lexer: &mut Lexer<'_>, rep: &mut Reporter<'_>, ok: &mut bool) -> Option<String> {
    let (line, column) = lexer.pos();
    lexer.bump();
    let mut text = String::new();
    loop {
        match lexer.peek() {
            None => {
                let at = rep.at(line, column, None);
                rep.push(
                    Code::AppInputMalformed,
                    at,
                    "quoted cell is not closed".to_owned(),
                );
                return None;
            }
            Some('"') => {
                lexer.bump();
                if lexer.peek() == Some('"') {
                    lexer.bump();
                    text.push('"');
                    continue;
                }
                break;
            }
            Some(c) => {
                if c == '\0' {
                    nul(lexer, rep, ok);
                }
                lexer.bump();
                text.push(c);
            }
        }
    }
    if lexer.peek().is_none() || lexer.peek() == Some(',') || lexer.at_line_end() {
        return Some(text);
    }
    let (l, c) = lexer.pos();
    let at = rep.at(l, c, None);
    let message = if lexer.peek() == Some('\r') {
        "carriage return without a line feed"
    } else {
        "text after the closing quote of a cell"
    };
    rep.push(Code::AppInputMalformed, at, message.to_owned());
    lexer.skip_line();
    None
}
