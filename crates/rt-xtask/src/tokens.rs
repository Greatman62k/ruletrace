//! A small Rust tokenizer for the source scans (design §3.3). It is a heuristic
//! reader, not a parser: it removes comments, keeps string contents as one
//! token, and keeps line numbers. It uses no regex crate.

use std::collections::BTreeSet;

/// Kind of a token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Identifier or keyword.
    Ident(String),
    /// Lifetime such as `'a` (text without the quote).
    Lifetime(String),
    /// Punctuation. `::`, `->` and `=>` are single tokens, everything else is
    /// one character.
    Punct(String),
    /// Content of a string literal (escapes are not decoded).
    Str(String),
    /// Number or character literal.
    Lit(String),
}

/// One token with its 1-based line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// The kind.
    pub kind: Kind,
    /// Line of the first character.
    pub line: usize,
}

impl Token {
    /// The identifier text, if this is an identifier.
    #[must_use]
    pub fn ident(&self) -> Option<&str> {
        match &self.kind {
            Kind::Ident(s) => Some(s),
            _ => None,
        }
    }

    /// Whether this is the identifier `s`.
    #[must_use]
    pub fn is_ident(&self, s: &str) -> bool {
        self.ident() == Some(s)
    }

    /// Whether this is the punctuation `s`.
    #[must_use]
    pub fn is_punct(&self, s: &str) -> bool {
        matches!(&self.kind, Kind::Punct(p) if p == s)
    }

    /// Text used when tokens are joined into a type name.
    fn text(&self) -> &str {
        match &self.kind {
            Kind::Ident(s) | Kind::Punct(s) | Kind::Lit(s) | Kind::Str(s) => s,
            Kind::Lifetime(_) => "",
        }
    }

    fn word_like(&self) -> bool {
        matches!(self.kind, Kind::Ident(_) | Kind::Lit(_))
    }
}

/// Joins tokens into one normalized text: lifetimes are dropped and a space is
/// kept only between two word-like tokens.
#[must_use]
pub fn normalize(tokens: &[Token]) -> String {
    let mut out = String::new();
    let mut prev_word = false;
    for t in tokens {
        if matches!(t.kind, Kind::Lifetime(_)) {
            continue;
        }
        if prev_word && t.word_like() {
            out.push(' ');
        }
        out.push_str(t.text());
        prev_word = t.word_like();
    }
    out
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Splits Rust source text into tokens. Never fails: unterminated literals end
/// at end of input.
#[must_use]
pub fn tokenize(src: &str) -> Vec<Token> {
    let cs: Vec<char> = src.chars().collect();
    let at = |i: usize| cs.get(i).copied();
    let raw_start = |j: usize| -> Option<usize> {
        // Returns the number of `#` when a raw string starts at `j` (at the
        // `r`).
        if at(j) != Some('r') {
            return None;
        }
        let mut k = j + 1;
        while at(k) == Some('#') {
            k += 1;
        }
        (at(k) == Some('"')).then_some(k - j - 1)
    };
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    while let Some(c) = at(i) {
        let start_line = line;
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '/' && at(i + 1) == Some('/') {
            while at(i).is_some_and(|d| d != '\n') {
                i += 1;
            }
        } else if c == '/' && at(i + 1) == Some('*') {
            let mut depth = 1usize;
            i += 2;
            while depth > 0 {
                match (at(i), at(i + 1)) {
                    (None, _) => break,
                    (Some('/'), Some('*')) => {
                        depth += 1;
                        i += 2;
                    }
                    (Some('*'), Some('/')) => {
                        depth -= 1;
                        i += 2;
                    }
                    (Some('\n'), _) => {
                        line += 1;
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
        } else if c == 'b' && (matches!(at(i + 1), Some('"' | '\'')) || raw_start(i + 1).is_some())
        {
            i += 1;
        } else if let Some(hashes) = raw_start(i) {
            let mut j = i + 1 + hashes + 1;
            let mut text = String::new();
            loop {
                match at(j) {
                    None => break,
                    Some('"') if (1..=hashes).all(|n| at(j + n) == Some('#')) => {
                        j += 1 + hashes;
                        break;
                    }
                    Some(d) => {
                        if d == '\n' {
                            line += 1;
                        }
                        text.push(d);
                        j += 1;
                    }
                }
            }
            i = j;
            out.push(Token {
                kind: Kind::Str(text),
                line: start_line,
            });
        } else if c == '"' {
            let mut j = i + 1;
            let mut text = String::new();
            while let Some(d) = at(j) {
                if d == '"' {
                    j += 1;
                    break;
                }
                if d == '\\' {
                    text.push(d);
                    j += 1;
                    if let Some(e) = at(j) {
                        if e == '\n' {
                            line += 1;
                        }
                        text.push(e);
                        j += 1;
                    }
                    continue;
                }
                if d == '\n' {
                    line += 1;
                }
                text.push(d);
                j += 1;
            }
            i = j;
            out.push(Token {
                kind: Kind::Str(text),
                line: start_line,
            });
        } else if c == '\'' {
            if at(i + 1) == Some('\\') {
                let mut j = i + 3;
                while at(j).is_some_and(|d| d != '\'') {
                    j += 1;
                }
                i = j + 1;
                out.push(Token {
                    kind: Kind::Lit("'c'".to_string()),
                    line: start_line,
                });
            } else if at(i + 2) == Some('\'') {
                i += 3;
                out.push(Token {
                    kind: Kind::Lit("'c'".to_string()),
                    line: start_line,
                });
            } else {
                let mut j = i + 1;
                let mut text = String::new();
                while let Some(d) = at(j).filter(|d| is_ident_char(*d)) {
                    text.push(d);
                    j += 1;
                }
                i = j.max(i + 1);
                out.push(Token {
                    kind: Kind::Lifetime(text),
                    line: start_line,
                });
            }
        } else if c.is_ascii_digit() {
            let mut j = i;
            let mut text = String::new();
            while let Some(d) = at(j) {
                let fraction = d == '.' && at(j + 1).is_some_and(|n| n.is_ascii_digit());
                if is_ident_char(d) || fraction {
                    text.push(d);
                    j += 1;
                } else {
                    break;
                }
            }
            i = j;
            let suffix = ["f32", "f64"]
                .into_iter()
                .find(|s| text.ends_with(s) && !text.starts_with("0x"));
            out.push(Token {
                kind: Kind::Lit(text),
                line: start_line,
            });
            if let Some(s) = suffix {
                out.push(Token {
                    kind: Kind::Ident(s.to_string()),
                    line: start_line,
                });
            }
        } else if is_ident_start(c) {
            let mut j = i;
            let mut text = String::new();
            while let Some(d) = at(j).filter(|d| is_ident_char(*d)) {
                text.push(d);
                j += 1;
            }
            i = j;
            out.push(Token {
                kind: Kind::Ident(text),
                line: start_line,
            });
        } else {
            let two = match (c, at(i + 1)) {
                (':', Some(':')) => Some("::"),
                ('-', Some('>')) => Some("->"),
                ('=', Some('>')) => Some("=>"),
                _ => None,
            };
            let text = match two {
                Some(t) => {
                    i += 2;
                    t.to_string()
                }
                None => {
                    i += 1;
                    c.to_string()
                }
            };
            out.push(Token {
                kind: Kind::Punct(text),
                line: start_line,
            });
        }
    }
    out
}

/// Index just after the group that opens at `open` (a `(`, `[` or `{`), or the
/// end of input.
#[must_use]
pub fn skip_group(tokens: &[Token], open: usize) -> usize {
    let mut depth = 0usize;
    let mut i = open;
    while let Some(t) = tokens.get(i) {
        if t.is_punct("(") || t.is_punct("[") || t.is_punct("{") {
            depth += 1;
        } else if t.is_punct(")") || t.is_punct("]") || t.is_punct("}") {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return i + 1;
            }
        }
        i += 1;
    }
    tokens.len()
}

/// Index just after the item that starts at `start`: through its first
/// top-level `{...}` block or its first top-level `;`.
fn skip_item(tokens: &[Token], start: usize) -> usize {
    let mut i = start;
    while let Some(t) = tokens.get(i) {
        if t.is_punct("{") {
            return skip_group(tokens, i);
        }
        if t.is_punct("(") || t.is_punct("[") {
            i = skip_group(tokens, i);
            continue;
        }
        if t.is_punct(";") {
            return i + 1;
        }
        i += 1;
    }
    tokens.len()
}

/// Whether `tokens[i..]` starts an attribute `#[name ...]` that marks test code
/// (`#[cfg(test)]` or `#[test]`). Returns the index after the attribute.
fn test_attr(tokens: &[Token], i: usize) -> Option<usize> {
    let is = |k: usize, f: &dyn Fn(&Token) -> bool| tokens.get(i + k).is_some_and(f);
    if !(is(0, &|t| t.is_punct("#")) && is(1, &|t| t.is_punct("["))) {
        return None;
    }
    let cfg_test = is(2, &|t| t.is_ident("cfg"))
        && is(3, &|t| t.is_punct("("))
        && is(4, &|t| t.is_ident("test"))
        && is(5, &|t| t.is_punct(")"))
        && is(6, &|t| t.is_punct("]"));
    let plain_test = is(2, &|t| t.is_ident("test")) && is(3, &|t| t.is_punct("]"));
    (cfg_test || plain_test).then(|| skip_group(tokens, i + 1))
}

/// Tokens of `src` without test-only items (`#[cfg(test)]` and `#[test]`), and
/// the names of out-of-line test modules (`#[cfg(test)] mod name;`). A file
/// that starts with `#![cfg(test)]` yields no tokens.
#[must_use]
pub fn production_tokens(src: &str) -> (Vec<Token>, BTreeSet<String>) {
    let tokens = tokenize(src);
    let mut kept = Vec::with_capacity(tokens.len());
    let mut test_mods = BTreeSet::new();
    let inner_cfg_test = tokens.windows(7).next().is_some_and(|w| {
        w.first().is_some_and(|t| t.is_punct("#"))
            && w.get(1).is_some_and(|t| t.is_punct("!"))
            && w.get(2).is_some_and(|t| t.is_punct("["))
            && w.get(3).is_some_and(|t| t.is_ident("cfg"))
            && w.get(4).is_some_and(|t| t.is_punct("("))
            && w.get(5).is_some_and(|t| t.is_ident("test"))
    });
    if inner_cfg_test {
        return (kept, test_mods);
    }
    let mut i = 0usize;
    while i < tokens.len() {
        if let Some(mut after) = test_attr(&tokens, i) {
            // Further attributes on the same item.
            while tokens.get(after).is_some_and(|t| t.is_punct("#"))
                && tokens.get(after + 1).is_some_and(|t| t.is_punct("["))
            {
                after = skip_group(&tokens, after + 1);
            }
            let mut k = after;
            if tokens.get(k).is_some_and(|t| t.is_ident("pub")) {
                k += 1;
            }
            if tokens.get(k).is_some_and(|t| t.is_ident("mod"))
                && tokens.get(k + 2).is_some_and(|t| t.is_punct(";"))
            {
                if let Some(name) = tokens.get(k + 1).and_then(Token::ident) {
                    test_mods.insert(name.to_string());
                }
            }
            i = skip_item(&tokens, after);
            continue;
        }
        if let Some(t) = tokens.get(i) {
            kept.push(t.clone());
        }
        i += 1;
    }
    (kept, test_mods)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idents(src: &str) -> Vec<String> {
        tokenize(src)
            .iter()
            .filter_map(|t| t.ident().map(str::to_string))
            .collect()
    }

    #[test]
    fn comments_and_strings_hide_identifiers() {
        let src = "// f64\n/* HashMap /* nested */ f32 */ let s = \"f64 \\\" HashMap\"; r#\"f64\"#; 'a'; x";
        assert_eq!(idents(src), vec!["let", "s", "x"]);
    }

    #[test]
    fn lines_are_counted_through_comments_and_strings() {
        let toks = tokenize("a\n/* \n */ b \"x\ny\" c");
        let lines: Vec<usize> = toks.iter().map(|t| t.line).collect();
        assert_eq!(lines, vec![1, 3, 3, 4]);
    }

    #[test]
    fn float_suffix_is_an_identifier_and_hex_is_not() {
        assert!(idents("let x = 1.0f64;").contains(&"f64".to_string()));
        assert!(!idents("let x = 0x1f64;").contains(&"f64".to_string()));
    }

    #[test]
    fn lifetimes_are_not_char_literals() {
        assert_eq!(idents("fn f<'a>(x: &'a str)"), vec!["fn", "f", "x", "str"]);
    }

    #[test]
    fn test_items_are_removed() {
        let src = "fn a() {}\n#[cfg(test)]\nmod tests { use std::collections::HashMap; }\n#[cfg(test)]\nmod more;\nfn b() {}";
        let (toks, mods) = production_tokens(src);
        let names: Vec<&str> = toks.iter().filter_map(Token::ident).collect();
        assert_eq!(names, vec!["fn", "a", "fn", "b"]);
        assert!(mods.contains("more"));
    }

    #[test]
    fn normalize_drops_lifetimes() {
        let toks = tokenize("&'a mut Vec<Diagnostic>");
        assert_eq!(normalize(&toks), "&mut Vec<Diagnostic>");
    }
}
