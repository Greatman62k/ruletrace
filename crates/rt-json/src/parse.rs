//! The strict, positioned, iterative RT-JSON parser (design §4.1.1).

use crate::value::{Kind, Member, Position, Value, MAX_SAFE_INTEGER};
use rt_foundation::{Code, Diagnostic, InputId, InputLocation, Location};
use std::collections::BTreeSet;

/// The effective limits of one parse (design §4.1.5). A value equal to a limit
/// is accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseLimits {
    max_bytes: u64,
    max_depth: u32,
}

impl ParseLimits {
    /// Limits for one parse. Offsets are 32-bit, so a byte limit above
    /// `u32::MAX` is applied as `u32::MAX`.
    #[must_use]
    pub const fn new(max_bytes: u64, max_depth: u32) -> Self {
        Self {
            max_bytes,
            max_depth,
        }
    }

    /// The byte limit as given.
    #[must_use]
    pub const fn max_bytes(self) -> u64 {
        self.max_bytes
    }

    /// The container depth limit.
    #[must_use]
    pub const fn max_depth(self) -> u32 {
        self.max_depth
    }
}

/// Parses one RT-JSON document.
///
/// Checks, in this order: size against the limit (before any content is read),
/// byte order mark, UTF-8, then the grammar. Parsing stops at the first error,
/// so the returned diagnostic is the one with the lowest offset. Container
/// depth is checked when a container opens, before any value is built, and the
/// outermost container has depth 1.
///
/// # Errors
/// - `RT-LIM-0001` when the input is larger than the limit (names the file,
///   limit and size).
/// - `RT-LIM-0002` when nesting is deeper than the limit (names the file,
///   limit, depth reached and byte offset, with the line and column of the
///   container that exceeded it).
/// - `RT-IR-0001` for any other violation of the dialect. The message says
///   which.
///
/// Every diagnostic has an input location with line and column, except the size
/// error, which has none.
pub fn parse(
    input: &InputId,
    bytes: &[u8],
    limits: ParseLimits,
) -> Result<Value<Position>, Box<Diagnostic>> {
    let (tree, duplicate) = parse_inner(input, bytes, limits, false)?;
    match duplicate {
        Some(d) => Err(d),
        None => Ok(tree),
    }
}

/// The result of [`parse_keeping_duplicates`].
#[derive(Debug)]
pub struct Lenient {
    /// The whole document, with every repeated key kept as its own member.
    pub tree: Value<Position>,
    /// The diagnostic that [`parse`] would have returned for the first
    /// repeated key, if the document has one.
    pub first_duplicate: Option<Box<Diagnostic>>,
}

/// Parses like [`parse`] but keeps repeated object keys instead of failing.
///
/// For callers that must inspect a document that [`parse`] rejects. Every
/// other check, order of checks and limit is the same as in [`parse`]. The
/// tree may hold objects with repeated keys and must not be used as a
/// decoded value.
///
/// # Errors
/// The errors of [`parse`] other than the repeated-key error.
pub fn parse_keeping_duplicates(
    input: &InputId,
    bytes: &[u8],
    limits: ParseLimits,
) -> Result<Lenient, Box<Diagnostic>> {
    let (tree, first_duplicate) = parse_inner(input, bytes, limits, true)?;
    Ok(Lenient {
        tree,
        first_duplicate,
    })
}

type Parsed = (Value<Position>, Option<Box<Diagnostic>>);

fn parse_inner(
    input: &InputId,
    bytes: &[u8],
    limits: ParseLimits,
    keep_duplicates: bool,
) -> Result<Parsed, Box<Diagnostic>> {
    let actual = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let effective = limits.max_bytes.min(u64::from(u32::MAX));
    if actual > effective {
        return Err(Box::new(Diagnostic::new(
            Code::LimitFileTooLarge,
            format!(
                "input file '{input}' is {actual} bytes, larger than the effective limit of \
                 {effective} bytes"
            ),
            Location::Input(InputLocation {
                input: input.clone(),
                line: None,
                column: None,
                field: None,
            }),
        )));
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let at = Position {
            byte: 0,
            line: 1,
            column: 1,
        };
        return Err(syntax(input, at, "a byte order mark is not allowed"));
    }
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => {
            let valid = error.valid_up_to();
            let prefix = bytes
                .get(..valid)
                .and_then(|p| std::str::from_utf8(p).ok())
                .unwrap_or("");
            let at = position_in(prefix);
            return Err(syntax(input, at, "input is not valid UTF-8"));
        }
    };
    let mut parser = Parser {
        input,
        text,
        bytes,
        i: 0,
        line: 1,
        col: 1,
        max_depth: limits.max_depth,
        keep_duplicates,
        duplicate: None,
    };
    let tree = parser.document()?;
    Ok((tree, parser.duplicate))
}

/// Position of the end of `prefix`, which is where the next byte would be.
fn position_in(prefix: &str) -> Position {
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
    Position {
        byte: u32::try_from(prefix.len()).unwrap_or(u32::MAX),
        line,
        column,
    }
}

fn location(input: &InputId, at: Position) -> Location {
    Location::Input(InputLocation {
        input: input.clone(),
        line: Some(at.line),
        column: Some(at.column),
        field: None,
    })
}

fn syntax(input: &InputId, at: Position, what: &str) -> Box<Diagnostic> {
    Box::new(Diagnostic::new(
        Code::IrSyntaxError,
        format!("syntax error at byte {}: {what}", at.byte),
        location(input, at),
    ))
}

enum Body {
    Array(Vec<Value<Position>>),
    Object {
        members: Vec<Member<Position>>,
        keys: BTreeSet<String>,
        pending: Option<(String, Position)>,
    },
}

struct Frame {
    open: Position,
    body: Body,
}

enum Step {
    /// The next thing in the input is a value.
    Value,
    /// A value was completed.
    Done(Value<Position>),
}

struct Parser<'a> {
    input: &'a InputId,
    text: &'a str,
    bytes: &'a [u8],
    i: usize,
    line: u32,
    col: u32,
    max_depth: u32,
    keep_duplicates: bool,
    duplicate: Option<Box<Diagnostic>>,
}

impl Parser<'_> {
    fn pos(&self) -> Position {
        Position {
            byte: u32::try_from(self.i).unwrap_or(u32::MAX),
            line: self.line,
            column: self.col,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.i).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.i.saturating_add(offset)).copied()
    }

    /// Consumes one byte. The column advances once per scalar value, on its
    /// first byte.
    fn bump(&mut self) {
        if let Some(b) = self.peek() {
            self.i = self.i.saturating_add(1);
            if b == b'\n' {
                self.line = self.line.saturating_add(1);
                self.col = 1;
            } else if b & 0xC0 != 0x80 {
                self.col = self.col.saturating_add(1);
            }
        }
    }

    fn err(&self, what: &str) -> Box<Diagnostic> {
        syntax(self.input, self.pos(), what)
    }

    fn unexpected(&self, expected: &str) -> Box<Diagnostic> {
        match self.text.get(self.i..).and_then(|rest| rest.chars().next()) {
            None => self.err(&format!("unexpected end of input, expected {expected}")),
            Some(c) => self.err(&format!(
                "unexpected character U+{:04X}, expected {expected}",
                u32::from(c)
            )),
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.bump();
        }
    }

    fn document(&mut self) -> Result<Value<Position>, Box<Diagnostic>> {
        let mut stack: Vec<Frame> = Vec::new();
        let mut step = Step::Value;
        loop {
            step = match step {
                Step::Value => {
                    self.skip_ws();
                    self.value_start(&mut stack)?
                }
                Step::Done(value) => {
                    if stack.is_empty() {
                        self.skip_ws();
                        if self.peek().is_some() {
                            return Err(self.err("unexpected data after the top-level value"));
                        }
                        return Ok(value);
                    }
                    self.attach(&mut stack, value)?
                }
            };
        }
    }

    /// Adds a completed value to the open container, then reads the separator
    /// or the closer.
    fn attach(
        &mut self,
        stack: &mut Vec<Frame>,
        value: Value<Position>,
    ) -> Result<Step, Box<Diagnostic>> {
        let Some(frame) = stack.last_mut() else {
            return Err(self.err("internal parser state error"));
        };
        let closer = match &mut frame.body {
            Body::Array(items) => {
                items.push(value);
                b']'
            }
            Body::Object {
                members, pending, ..
            } => {
                let Some((key, key_pos)) = pending.take() else {
                    return Err(self.err("internal parser state error"));
                };
                members.push(Member {
                    key,
                    key_pos,
                    value,
                });
                b'}'
            }
        };
        self.skip_ws();
        match self.peek() {
            Some(b',') => {
                self.bump();
                if let Body::Object { keys, pending, .. } = &mut frame.body {
                    *pending = Some(self.read_key(keys)?);
                }
                Ok(Step::Value)
            }
            Some(c) if c == closer => {
                self.bump();
                let Some(frame) = stack.pop() else {
                    return Err(self.err("internal parser state error"));
                };
                let kind = match frame.body {
                    Body::Array(items) => Kind::Array(items),
                    Body::Object { members, .. } => Kind::Object(members),
                };
                Ok(Step::Done(Value::new(frame.open, kind)))
            }
            _ => Err(self.unexpected(if closer == b']' {
                "',' or ']'"
            } else {
                "',' or '}'"
            })),
        }
    }

    fn value_start(&mut self, stack: &mut Vec<Frame>) -> Result<Step, Box<Diagnostic>> {
        let at = self.pos();
        match self.peek() {
            Some(b'{' | b'[') => self.open(stack, at),
            Some(b'"') => {
                let s = self.string()?;
                Ok(Step::Done(Value::new(at, Kind::String(s))))
            }
            Some(b't') => self.literal("true", at, Kind::Bool(true)),
            Some(b'f') => self.literal("false", at, Kind::Bool(false)),
            Some(b'n') => self.literal("null", at, Kind::Null),
            Some(b'-' | b'0'..=b'9') => self.number(at),
            _ => Err(self.unexpected("a value")),
        }
    }

    fn open(&mut self, stack: &mut Vec<Frame>, at: Position) -> Result<Step, Box<Diagnostic>> {
        let is_object = self.peek() == Some(b'{');
        let depth = u32::try_from(stack.len())
            .unwrap_or(u32::MAX)
            .saturating_add(1);
        if depth > self.max_depth {
            return Err(Box::new(Diagnostic::new(
                Code::LimitNestingTooDeep,
                format!(
                    "document '{}' nests containers {depth} deep at byte {}, deeper than the \
                     effective limit of {}",
                    self.input, at.byte, self.max_depth
                ),
                location(self.input, at),
            )));
        }
        self.bump();
        self.skip_ws();
        let closer = if is_object { b'}' } else { b']' };
        if self.peek() == Some(closer) {
            self.bump();
            let kind = if is_object {
                Kind::Object(Vec::new())
            } else {
                Kind::Array(Vec::new())
            };
            return Ok(Step::Done(Value::new(at, kind)));
        }
        let body = if is_object {
            let mut keys = BTreeSet::new();
            let pending = Some(self.read_key(&mut keys)?);
            Body::Object {
                members: Vec::new(),
                keys,
                pending,
            }
        } else {
            Body::Array(Vec::new())
        };
        stack.push(Frame { open: at, body });
        Ok(Step::Value)
    }

    /// Reads `"key" :` and returns the key and its position. Rejects a repeated
    /// key at the position of the repeat.
    fn read_key(
        &mut self,
        keys: &mut BTreeSet<String>,
    ) -> Result<(String, Position), Box<Diagnostic>> {
        self.skip_ws();
        if self.peek() != Some(b'"') {
            return Err(self.unexpected("a string key"));
        }
        let key_pos = self.pos();
        let key = self.string()?;
        if !keys.insert(key.clone()) {
            let error = syntax(self.input, key_pos, "duplicate object key");
            if !self.keep_duplicates {
                return Err(error);
            }
            if self.duplicate.is_none() {
                self.duplicate = Some(error);
            }
        }
        self.skip_ws();
        if self.peek() != Some(b':') {
            return Err(self.unexpected("':'"));
        }
        self.bump();
        Ok((key, key_pos))
    }

    fn literal(
        &mut self,
        word: &str,
        at: Position,
        kind: Kind<Position>,
    ) -> Result<Step, Box<Diagnostic>> {
        let end = self.i.saturating_add(word.len());
        if self.bytes.get(self.i..end) != Some(word.as_bytes()) {
            return Err(self.err(&format!("invalid literal, expected '{word}'")));
        }
        for _ in 0..word.len() {
            self.bump();
        }
        Ok(Step::Done(Value::new(at, kind)))
    }

    fn digits(&mut self) -> Result<(), Box<Diagnostic>> {
        if !matches!(self.peek(), Some(b'0'..=b'9')) {
            return Err(self.unexpected("a digit"));
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.bump();
        }
        Ok(())
    }

    fn number(&mut self, at: Position) -> Result<Step, Box<Diagnostic>> {
        let start = self.i;
        let negative = self.peek() == Some(b'-');
        if negative {
            self.bump();
        }
        match self.peek() {
            Some(b'0') => self.bump(),
            Some(b'1'..=b'9') => self.digits()?,
            _ => return Err(self.unexpected("a digit")),
        }
        let mut plain = !negative;
        if self.peek() == Some(b'.') {
            self.bump();
            self.digits()?;
            plain = false;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.bump();
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.bump();
            }
            self.digits()?;
            plain = false;
        }
        let lexeme = self.text.get(start..self.i).unwrap_or("");
        if plain {
            if let Ok(n) = lexeme.parse::<u64>() {
                if n <= MAX_SAFE_INTEGER {
                    return Ok(Step::Done(Value::new(at, Kind::Integer(n))));
                }
            }
        }
        Ok(Step::Done(Value::new(at, Kind::Number(lexeme.to_string()))))
    }

    fn hex4(&mut self) -> Result<u32, Box<Diagnostic>> {
        let mut value = 0u32;
        for _ in 0..4 {
            let digit = self
                .peek()
                .and_then(|b| char::from(b).to_digit(16))
                .ok_or_else(|| self.unexpected("four hexadecimal digits after '\\u'"))?;
            value = value * 16 + digit;
            self.bump();
        }
        Ok(value)
    }

    fn string(&mut self) -> Result<String, Box<Diagnostic>> {
        self.bump();
        let mut out = String::new();
        let mut run = self.i;
        loop {
            let Some(b) = self.peek() else {
                return Err(self.err("unterminated string"));
            };
            match b {
                b'"' => {
                    out.push_str(self.text.get(run..self.i).unwrap_or(""));
                    self.bump();
                    return Ok(out);
                }
                0..=0x1F => return Err(self.err("unescaped control character in string")),
                b'\\' => {
                    out.push_str(self.text.get(run..self.i).unwrap_or(""));
                    let escape_at = self.pos();
                    self.bump();
                    self.escape(&mut out, escape_at)?;
                    run = self.i;
                }
                _ => self.bump(),
            }
        }
    }

    fn escape(&mut self, out: &mut String, escape_at: Position) -> Result<(), Box<Diagnostic>> {
        let simple = match self.peek() {
            Some(b'"') => Some('"'),
            Some(b'\\') => Some('\\'),
            Some(b'/') => Some('/'),
            Some(b'b') => Some('\u{8}'),
            Some(b'f') => Some('\u{c}'),
            Some(b'n') => Some('\n'),
            Some(b'r') => Some('\r'),
            Some(b't') => Some('\t'),
            Some(b'u') => None,
            _ => return Err(self.unexpected("a valid escape character")),
        };
        self.bump();
        if let Some(c) = simple {
            out.push(c);
            return Ok(());
        }
        let first = self.hex4()?;
        let code = match first {
            0xD800..=0xDBFF => {
                if self.peek() != Some(b'\\') || self.peek_at(1) != Some(b'u') {
                    return Err(syntax(
                        self.input,
                        escape_at,
                        "unpaired high surrogate in string escape",
                    ));
                }
                self.bump();
                self.bump();
                let second = self.hex4()?;
                if !(0xDC00..=0xDFFF).contains(&second) {
                    return Err(syntax(
                        self.input,
                        escape_at,
                        "unpaired high surrogate in string escape",
                    ));
                }
                0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
            }
            0xDC00..=0xDFFF => {
                return Err(syntax(
                    self.input,
                    escape_at,
                    "unpaired low surrogate in string escape",
                ))
            }
            other => other,
        };
        match char::from_u32(code) {
            Some(c) => {
                out.push(c);
                Ok(())
            }
            None => Err(syntax(self.input, escape_at, "invalid escape code point")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> InputId {
        InputId::new("t.json").unwrap()
    }

    fn lim() -> ParseLimits {
        ParseLimits::new(1 << 20, 64)
    }

    fn ok(text: &str) -> Value<Position> {
        parse(&id(), text.as_bytes(), lim()).unwrap()
    }

    fn fail(text: &[u8]) -> Diagnostic {
        *parse(&id(), text, lim()).unwrap_err()
    }

    fn at(d: &Diagnostic) -> (u32, u32) {
        match d.primary() {
            Location::Input(l) => (l.line.unwrap(), l.column.unwrap()),
            other => panic!("unexpected location {other:?}"),
        }
    }

    #[test]
    fn rfc_examples_parse_with_positions() {
        let v = ok("{\n  \"a\": [1, true, null, \"é\"],\n  \"b\": {}\n}");
        assert_eq!(
            *v.pos(),
            Position {
                byte: 0,
                line: 1,
                column: 1
            }
        );
        let a = v.get("a").unwrap();
        assert_eq!(
            a.key_pos,
            Position {
                byte: 4,
                line: 2,
                column: 3
            }
        );
        let items = a.value.as_array().unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].as_u64(), Some(1));
        assert_eq!(
            *items[3].pos(),
            Position {
                byte: 25,
                line: 2,
                column: 24
            }
        );
        assert_eq!(items[3].as_str(), Some("é"));
        assert!(matches!(v.get("b").unwrap().value.kind(), Kind::Object(m) if m.is_empty()));
        for scalar in ["0", "-1", "1.5", "1e3", "\"x\"", "true", " null\n"] {
            ok(scalar);
        }
    }

    #[test]
    fn numbers_are_integer_or_lexeme() {
        assert_eq!(ok("9007199254740991").as_u64(), Some(MAX_SAFE_INTEGER));
        for lexeme in ["9007199254740992", "-0", "-3", "0.6", "1E+2", "2.50"] {
            assert_eq!(
                ok(lexeme).kind(),
                &Kind::Number(lexeme.to_string()),
                "{lexeme}"
            );
        }
        assert_eq!(at(&fail(b"01")), (1, 2));
        assert_eq!(at(&fail(b"1.")), (1, 3));
        assert_eq!(at(&fail(b"+1")), (1, 1));
    }

    #[test]
    fn string_escapes_and_surrogates() {
        let v = ok(r#""\"\\\/\b\f\n\r\t\u0041\ud83d\ude00""#);
        assert_eq!(v.as_str(), Some("\"\\/\u{8}\u{c}\n\r\tA😀"));
        assert_eq!(at(&fail(br#""\ud83d""#)), (1, 2));
        assert_eq!(at(&fail(br#""\ude00""#)), (1, 2));
        assert_eq!(at(&fail(br#""\ud83d\u0041""#)), (1, 2));
        assert_eq!(at(&fail(br#""\x""#)), (1, 3));
        assert_eq!(at(&fail(b"\"a\x01\"")), (1, 3));
        assert_eq!(at(&fail(b"\"abc")), (1, 5));
    }

    #[test]
    fn dialect_restrictions() {
        assert_eq!(at(&fail(b"\xEF\xBB\xBF{}")), (1, 1));
        let bad = fail(b"{\n\"a\": \"\xFF\"}");
        assert_eq!((bad.code(), at(&bad)), (Code::IrSyntaxError, (2, 7)));
        assert!(bad.message().contains("byte 8"));
        assert_eq!(at(&fail(b"{} // c")), (1, 4));
        assert_eq!(at(&fail(b"{\"a\":1,}")), (1, 8));
        assert_eq!(at(&fail(b"[1,]")), (1, 4));
        assert_eq!(at(&fail(b"[1 2]")), (1, 4));
        assert_eq!(at(&fail(b"{} {}")), (1, 4));
        assert_eq!(at(&fail(b"")), (1, 1));
        assert_eq!(at(&fail(b"nul")), (1, 1));
        assert_eq!(at(&fail(b"truex")), (1, 5));
        assert_eq!(at(&fail(b"{'a':1}")), (1, 2));
    }

    #[test]
    fn duplicate_key_is_reported_at_the_repeat() {
        let d = fail(b"{\"a\":1,\n \"b\":2, \"a\":3}");
        assert_eq!(d.code(), Code::IrSyntaxError);
        assert_eq!(at(&d), (2, 9));
        // The same key in different objects is fine.
        ok("{\"a\":{\"a\":1},\"b\":{\"a\":2}}");
    }

    #[test]
    fn keeping_duplicates_returns_every_member_and_the_first_repeat() {
        let text = b"{\"a\":1,\n \"b\":2, \"a\":3, \"a\":4}";
        let lenient = parse_keeping_duplicates(&id(), text, lim()).unwrap();
        let keys: Vec<&str> = lenient
            .tree
            .as_object()
            .unwrap()
            .iter()
            .map(|m| m.key.as_str())
            .collect();
        assert_eq!(keys, ["a", "b", "a", "a"]);
        let first = lenient.first_duplicate.unwrap();
        assert_eq!(first.code(), Code::IrSyntaxError);
        assert_eq!(at(&first), (2, 9));
        assert_eq!(*first, fail(text));
    }

    #[test]
    fn keeping_duplicates_reports_nothing_for_a_clean_document() {
        let lenient = parse_keeping_duplicates(&id(), b"{\"a\":{\"a\":1}}", lim()).unwrap();
        assert!(lenient.first_duplicate.is_none());
    }

    #[test]
    fn keeping_duplicates_still_fails_on_a_later_syntax_error() {
        let d = *parse_keeping_duplicates(&id(), b"{\"a\":1,\"a\":2,}", lim()).unwrap_err();
        assert_eq!(d.code(), Code::IrSyntaxError);
        assert_eq!(at(&d), (1, 14));
        assert!(!d.message().contains("duplicate"));
    }

    #[test]
    fn keeping_duplicates_applies_the_depth_limit() {
        let l = ParseLimits::new(1 << 20, 2);
        let d = *parse_keeping_duplicates(&id(), b"{\"a\":1,\"a\":[[1]]}", l).unwrap_err();
        assert_eq!(d.code(), Code::LimitNestingTooDeep);
    }

    #[test]
    fn columns_count_scalar_values() {
        let d = fail("[\"éé😀\" x]".as_bytes());
        assert_eq!(at(&d), (1, 8));
    }

    #[test]
    fn size_limit_is_checked_first_and_inclusive() {
        let text = b"[0,0,0]";
        let at_limit = ParseLimits::new(7, 64);
        assert!(parse(&id(), text, at_limit).is_ok());
        let d = *parse(&id(), text, ParseLimits::new(6, 64)).unwrap_err();
        assert_eq!(d.code(), Code::LimitFileTooLarge);
        assert!(d.message().contains("t.json") && d.message().contains('6'));
        assert!(d.message().contains('7'));
        // Oversized and malformed: the size error wins.
        let d = *parse(&id(), b"\xFF\xFF\xFF", ParseLimits::new(2, 64)).unwrap_err();
        assert_eq!(d.code(), Code::LimitFileTooLarge);
    }

    #[test]
    fn depth_limit_is_inclusive_and_positioned() {
        let nest = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
        let l = ParseLimits::new(1 << 20, 3);
        assert!(parse(&id(), nest(3).as_bytes(), l).is_ok());
        let d = *parse(&id(), nest(4).as_bytes(), l).unwrap_err();
        assert_eq!(d.code(), Code::LimitNestingTooDeep);
        assert_eq!(at(&d), (1, 4));
        assert!(d.message().contains("t.json") && d.message().contains("limit of 3"));
        assert!(d.message().contains("4 deep") && d.message().contains("byte 3"));
        // Objects count too, and depth is checked before a later syntax error.
        let d = *parse(&id(), b"{\"a\":{\"b\":{\"c\":{ ???", l).unwrap_err();
        assert_eq!(d.code(), Code::LimitNestingTooDeep);
    }

    #[test]
    fn very_deep_documents_parse_and_drop_without_overflow() {
        let n = 200_000;
        let text = format!("{}{}", "[".repeat(n), "]".repeat(n));
        let v = parse(&id(), text.as_bytes(), ParseLimits::new(1 << 24, 1_000_000)).unwrap();
        drop(v);
    }
}
