//! `check-app-layers` (design §3.3 item 6, §8.10, Req 31.10, 31.11, 24.12,
//! 15.8, 15.10, 20.17).
//!
//! The application layers (class app, and class lab once `rt-lab` exists) map
//! input, build Questions and present results. They must hold no business
//! logic. The scan reads their production tokens (test items are dropped, as
//! in `scan-sources`) and fails with the crate, file and line. It is a
//! heuristic over tokens and cannot see types. The byte-identity and
//! metamorphic tests are the evidence that the layers add no logic. The scan
//! is the early alarm.
//!
//! # Rules
//!
//! | Rule | Fails on |
//! | --- | --- |
//! | `app-arith` | binary `+ - * / %` and their assignment forms |
//! | `app-compare` | `<` and `>` as comparison, and `==` or `!=` when an operand names an Exact_Number carrier |
//! | `app-order` | sort, binary search, minimum, maximum, ranking, reversing and aggregate functions, rounding and checked-arithmetic methods, `Ordering`, `BinaryHeap`, `Reverse` |
//! | `app-numeric` | any path into `rt_numeric` other than the rendering function |
//! | `app-tiebreak` | an identifier or literal that names tie-breaking |
//! | `app-rounding` | a rounding mode name as identifier or literal |
//! | `app-literal` | a number other than 0 and 1, and a string literal not listed in `tools/app-literals.v1` |
//! | `app-field-name` | a string literal equal to a name that a committed rulebook declares |
//! | `app-io` | file, network, clock, random, environment, standard stream, thread or subprocess use |
//! | `app-external` and the edge rules | a dependency outside the allowed list, direct or transitive |
//!
//! # How the heuristics decide
//!
//! An operator is binary arithmetic when the token before it is value-like (an
//! identifier that is not a keyword, a number, `)`, `]` or `?`). A prefix `-`,
//! a dereference `*`, a glob `*`, `->`, `=>`, and a `+` in a trait bound
//! (the statement holds `impl`, `dyn` or `where`, or is an item declaration)
//! are therefore not arithmetic. `<` and `>` are generic brackets when the
//! token before `<` is not value-like, when it names a type (starts with an
//! upper-case letter and is not an all-capitals constant), or when the
//! statement is an item declaration. A generic `>` closes a tracked `<`.
//!
//! `==` and `!=` between identifiers, enum values and records carry no order
//! and no arithmetic, so they are allowed. Example: the check that an
//! Outcome equals the Outcome of its trace compares two typed records
//! and decides nothing about a number. Equality is flagged only when, within
//! its operand tokens (up to a `;`, brace, comma, `=>` or unmatched bracket),
//! the identifiers `ExactNumber`, `Number`, `Rational`, `Decimal`,
//! `to_canonical`, `parse_canonical`, `parse_literal` or `rt_numeric` appear,
//! because then the operands carry an Exact_Number or its text.
//!
//! `BTreeMap` and `BTreeSet` are allowed. They order identifiers and give the
//! participants and Targets their canonical order, which is the only order the
//! layers use (design §8.5: the members of a Tie_Group are written in
//! ascending identifier for serialization, and the order has no ranking
//! meaning). Any call that sorts, reverses, or selects a minimum or maximum is
//! flagged even over identifiers, because the layers need none: the order
//! comes from the ordered collections and from the engine's Outcomes.
//!
//! # String literals
//!
//! A string literal in production code is one of: a name of a specific
//! competition (a name that a committed rulebook declares), a tie-breaking or
//! rounding-mode name, a number, or text of the layer itself (CSV role names,
//! option names and message texts). The first three always fail, even when
//! listed. The fourth passes only when `tools/app-literals.v1` lists that
//! exact text, with a reason, for the crate. The list may not hold a name
//! that a committed rulebook declares. The names come from every
//! `ruletrace-rulebook` file under `tests/fixtures` and `demo`: the `name`
//! and `target` of every node and the `id` of every rule.

use crate::edges::{Class, EdgeList};
use crate::tokens::{production_tokens, skip_group, Kind, Token};
use rt_json::{parse, ParseLimits, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Path of the literal list, relative to the repository root.
pub const LITERALS_PATH: &str = "tools/app-literals.v1";
const LITERALS_HEADER: &str = "# format: ruletrace-app-literals v1";

/// Which rule a hit belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AppRule {
    /// Binary arithmetic operator.
    Arith,
    /// Comparison.
    Compare,
    /// Sorting, ordering, selection, aggregation, rounding.
    Order,
    /// Use of `rt_numeric` beyond rendering.
    Numeric,
    /// Tie-breaking name.
    TieBreak,
    /// Rounding mode name.
    Rounding,
    /// Number or unlisted string literal.
    Literal,
    /// Name declared by a rulebook.
    FieldName,
    /// File, network, clock, random, environment, thread or subprocess.
    Io,
}

impl AppRule {
    /// The name printed in front of a finding.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Arith => "app-arith",
            Self::Compare => "app-compare",
            Self::Order => "app-order",
            Self::Numeric => "app-numeric",
            Self::TieBreak => "app-tiebreak",
            Self::Rounding => "app-rounding",
            Self::Literal => "app-literal",
            Self::FieldName => "app-field-name",
            Self::Io => "app-io",
        }
    }
}

/// One flagged token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    /// The rule.
    pub rule: AppRule,
    /// 1-based line.
    pub line: usize,
    /// What was found.
    pub detail: String,
}

/// One entry of `tools/app-literals.v1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralEntry {
    /// Crate name, or `*` for every checked crate.
    pub crate_name: String,
    /// The text between the quotes, as written in the source.
    pub text: String,
    /// Why the literal is layer text and not competition logic.
    pub reason: String,
}

/// The listed string literals.
pub type Literals = Vec<LiteralEntry>;

/// Parses `tools/app-literals.v1`. An entry is
/// `literal <crate|*> "<text>" reason <reason>`.
///
/// # Errors
/// A message naming the line when the header is missing or an entry is
/// malformed or has no reason.
pub fn parse_literals(text: &str) -> Result<Literals, String> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    match lines.next() {
        Some((_, l)) if l.trim() == LITERALS_HEADER => {}
        _ => return Err(format!("first line must be `{LITERALS_HEADER}`")),
    }
    let mut out = Vec::new();
    for (n, raw) in lines {
        let line = raw.trim();
        if line.starts_with('#') {
            continue;
        }
        let at = n + 1;
        let Some(rest) = line.strip_prefix("literal ") else {
            return Err(format!("line {at}: unknown entry"));
        };
        let Some((crate_name, quoted)) = rest.trim_start().split_once(' ') else {
            return Err(format!("line {at}: expected a crate and a quoted text"));
        };
        let Some(body) = quoted.trim_start().strip_prefix('"') else {
            return Err(format!("line {at}: the text must start with a quote"));
        };
        let Some((text, reason)) = body.split_once("\" reason ") else {
            return Err(format!(
                "line {at}: an entry needs `\" reason <text>` after the text"
            ));
        };
        if reason.trim().is_empty() {
            return Err(format!("line {at}: the reason must not be empty"));
        }
        out.push(LiteralEntry {
            crate_name: crate_name.to_string(),
            text: text.to_string(),
            reason: reason.trim().to_string(),
        });
    }
    Ok(out)
}

/// Checks the list: each entry names a checked crate or `*`, and none is a
/// name that a rulebook declares.
///
/// # Errors
/// One line per entry that is not allowed.
pub fn validate_literals(
    entries: &[LiteralEntry],
    list: &EdgeList,
    names: &BTreeSet<String>,
) -> Result<(), String> {
    let mut bad = Vec::new();
    for e in entries {
        let class = list.crates.get(&e.crate_name).map(|c| c.class);
        if e.crate_name != "*" && !matches!(class, Some(Class::App | Class::Lab)) {
            bad.push(format!(
                "[app-literals] entry \"{}\" names '{}', which is not an application or Laboratory crate",
                e.text, e.crate_name
            ));
        }
        if names.contains(&e.text) {
            bad.push(format!(
                "[app-literals] entry \"{}\" is a name declared by a committed rulebook and cannot be listed",
                e.text
            ));
        }
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(bad.join("\n"))
    }
}

fn collect_names(value: &Value<rt_json::Position>, in_rules: bool, out: &mut BTreeSet<String>) {
    if let Some(members) = value.as_object() {
        for m in members {
            let key = m.key.as_str();
            let named = matches!(key, "name" | "target") || (in_rules && key == "id");
            if let (true, Some(text)) = (named, m.value.as_str()) {
                if text.chars().count() > 1 {
                    out.insert(text.to_string());
                }
            }
            collect_names(&m.value, in_rules, out);
        }
    } else if let Some(items) = value.as_array() {
        for item in items {
            collect_names(item, in_rules, out);
        }
    }
}

/// The names that the rulebook text declares: the `name` and `target` of every
/// node of `types` and `rules`, and the `id` of every rule.
///
/// # Errors
/// A message when the text is not RT-JSON.
pub fn rulebook_names(text: &str) -> Result<BTreeSet<String>, String> {
    let input = rt_foundation::InputId::new("rulebook").map_err(|e| e.to_string())?;
    let doc = parse(
        &input,
        text.as_bytes(),
        ParseLimits::new(64 * 1024 * 1024, 64),
    )
    .map_err(|d| d.message().to_string())?;
    let mut out = BTreeSet::new();
    for (key, in_rules) in [("types", false), ("rules", true)] {
        if let Some(member) = doc.get(key) {
            collect_names(&member.value, in_rules, &mut out);
        }
    }
    Ok(out)
}

fn walk_json(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    let Ok(read) = fs::read_dir(dir) else {
        return Ok(());
    };
    let mut paths: Vec<_> = read.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            walk_json(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
    Ok(())
}

/// Reads the declared names of every committed rulebook under
/// `tests/fixtures` and `demo`. Returns the names and the number of
/// rulebooks read.
///
/// # Errors
/// A message when a rulebook file does not parse.
pub fn load_names(root: &Path) -> Result<(BTreeSet<String>, usize), String> {
    let mut files = Vec::new();
    for dir in ["tests/fixtures", "demo"] {
        walk_json(&root.join(dir), &mut files)?;
    }
    let mut names = BTreeSet::new();
    let mut count = 0usize;
    for path in files {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if !text.contains("\"ruletrace-rulebook\"") {
            continue;
        }
        let found =
            rulebook_names(&text).map_err(|m| format!("[app-names] {}: {m}", path.display()))?;
        names.extend(found);
        count += 1;
    }
    Ok((names, count))
}

const NUMERIC_MARKERS: &[&str] = &[
    "ExactNumber",
    "Number",
    "Rational",
    "Decimal",
    "to_canonical",
    "parse_canonical",
    "parse_literal",
    "rt_numeric",
];

/// The only item of `rt_numeric` an application layer may name: it renders a
/// value that the engine computed.
const RT_NUMERIC_ALLOWED: &[&str] = &["to_canonical"];

/// Method and function names that sort, select, aggregate, round, or
/// compute with checked arithmetic. A name is flagged where it is called.
const ORDER_CALLS: &[&str] = &[
    "sort",
    "sort_by",
    "sort_by_key",
    "sort_by_cached_key",
    "sort_unstable",
    "sort_unstable_by",
    "sort_unstable_by_key",
    "select_nth_unstable",
    "is_sorted",
    "is_sorted_by",
    "is_sorted_by_key",
    "binary_search",
    "binary_search_by",
    "binary_search_by_key",
    "partition_point",
    "min",
    "max",
    "min_by",
    "max_by",
    "min_by_key",
    "max_by_key",
    "clamp",
    "cmp",
    "partial_cmp",
    "total_cmp",
    "then_with",
    "lt",
    "le",
    "gt",
    "ge",
    "is_lt",
    "is_le",
    "is_gt",
    "is_ge",
    "reverse",
    "rev",
    "sum",
    "product",
    "fold",
    "try_fold",
    "reduce",
    "floor",
    "ceil",
    "trunc",
    "abs",
    "signum",
    "pow",
    "powi",
    "powf",
    "sqrt",
    "rem_euclid",
    "div_euclid",
    "abs_diff",
    "add",
    "sub",
    "mul",
    "div",
    "neg",
    "rem",
    "add_assign",
    "sub_assign",
    "mul_assign",
    "div_assign",
    "is_zero",
    "last",
    "next_back",
    "nth_back",
    "first_key_value",
    "last_key_value",
    "pop_first",
    "pop_last",
];
const ORDER_CALL_PREFIXES: &[&str] = &[
    "checked_",
    "saturating_",
    "wrapping_",
    "overflowing_",
    "round",
];
/// Identifiers flagged wherever they appear.
const ORDER_IDENTS: &[&str] = &["Ordering", "BinaryHeap", "Reverse"];

const ROUNDING_NAMES: &[&str] = &[
    "RoundingMode",
    "HalfEven",
    "HalfAwayFromZero",
    "TowardZero",
    "half_even",
    "half_away_from_zero",
    "toward_zero",
];

const STD_IO_MODULES: &[&str] = &["fs", "env", "net", "process", "io", "time", "thread"];
const IO_IDENTS: &[&str] = &[
    "Instant",
    "SystemTime",
    "UNIX_EPOCH",
    "File",
    "OpenOptions",
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "rand",
    "getrandom",
];
/// Methods of `Path` and `PathBuf` that read the file system. A name is
/// flagged where it is called as a method.
const IO_METHODS: &[&str] = &[
    "exists",
    "try_exists",
    "is_file",
    "is_dir",
    "is_symlink",
    "metadata",
    "symlink_metadata",
    "read_dir",
    "read_link",
    "canonicalize",
];
/// Macros that read the environment or the file system at compile time, or
/// write a standard stream.
const IO_MACROS: &[&str] = &[
    "env",
    "option_env",
    "include",
    "include_str",
    "include_bytes",
    "print",
    "println",
    "eprint",
    "eprintln",
    "dbg",
];

/// Keywords after which the next operator is a prefix, not a binary one.
const PREFIX_KEYWORDS: &[&str] = &[
    "return", "in", "if", "else", "match", "while", "for", "let", "mut", "ref", "move", "as",
    "break", "continue", "loop", "yield", "where", "impl", "dyn", "unsafe", "const", "static",
    "pub", "fn", "use", "mod", "type", "struct", "enum", "trait",
];
const ITEM_KEYWORDS: &[&str] = &[
    "fn", "struct", "enum", "trait", "type", "impl", "union", "use", "mod",
];

fn value_like(token: Option<&Token>) -> bool {
    match token.map(|t| &t.kind) {
        Some(Kind::Ident(name)) => !PREFIX_KEYWORDS.contains(&name.as_str()),
        Some(Kind::Lit(_)) => true,
        Some(Kind::Punct(p)) => matches!(p.as_str(), ")" | "]" | "?"),
        _ => false,
    }
}

/// Index of the first token of the statement or item that holds `tokens[i]`.
fn statement_starts(tokens: &[Token]) -> Vec<usize> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut start = 0usize;
    for (i, t) in tokens.iter().enumerate() {
        out.push(start);
        if t.is_punct(";") || t.is_punct("{") || t.is_punct("}") {
            start = i + 1;
        }
    }
    out
}

/// Whether the statement from `start` through `at` is an item declaration
/// (`fn`, `struct`, `impl`, ...) or holds `impl`, `dyn` or `where`.
fn declaration_or_bound(tokens: &[Token], start: usize, at: usize, bounds: bool) -> bool {
    let slice = tokens.get(start..at).unwrap_or(&[]);
    if bounds
        && slice
            .iter()
            .any(|t| t.is_ident("impl") || t.is_ident("dyn") || t.is_ident("where"))
    {
        return true;
    }
    let mut k = 0usize;
    while let Some(t) = slice.get(k) {
        if t.is_punct("#") && slice.get(k + 1).is_some_and(|n| n.is_punct("[")) {
            k = skip_group(slice, k + 1);
        } else if t.is_ident("pub") {
            k += 1;
            if slice.get(k).is_some_and(|n| n.is_punct("(")) {
                k = skip_group(slice, k);
            }
        } else if matches!(
            t.ident(),
            Some("unsafe" | "async" | "const" | "extern" | "default")
        ) || matches!(t.kind, Kind::Str(_))
        {
            k += 1;
        } else {
            return t.ident().is_some_and(|n| ITEM_KEYWORDS.contains(&n));
        }
    }
    false
}

fn type_like(name: &str) -> bool {
    let mut chars = name.chars();
    let upper = chars.next().is_some_and(char::is_uppercase);
    let constant = name.chars().count() > 1 && !name.chars().any(char::is_lowercase);
    upper && !constant
}

fn operand_window_names_a_number(tokens: &[Token], op: usize, rightward: bool) -> bool {
    let mut depth = 0i32;
    let mut k = if rightward { op + 2 } else { op };
    for _ in 0..24 {
        let token = if rightward {
            tokens.get(k)
        } else {
            k = match k.checked_sub(1) {
                Some(v) => v,
                None => return false,
            };
            tokens.get(k)
        };
        let Some(t) = token else { return false };
        let open = t.is_punct("(") || t.is_punct("[");
        let close = t.is_punct(")") || t.is_punct("]");
        let (enter, leave) = if rightward {
            (open, close)
        } else {
            (close, open)
        };
        if enter {
            depth += 1;
        } else if leave {
            depth -= 1;
            if depth < 0 {
                return false;
            }
        }
        let doubled = |c: &str| t.is_punct(c) && tokens.get(k + 1).is_some_and(|n| n.is_punct(c));
        if t.is_punct(";")
            || t.is_punct("{")
            || t.is_punct("}")
            || t.is_punct(",")
            || t.is_punct("=>")
            || doubled("&")
            || doubled("|")
        {
            return false;
        }
        if t.ident().is_some_and(|n| NUMERIC_MARKERS.contains(&n)) {
            return true;
        }
        if rightward {
            k += 1;
        }
    }
    false
}

fn normalized(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn names_tie_breaking(text: &str) -> bool {
    normalized(text).contains("tiebreak")
}

fn names_rounding_mode(text: &str) -> bool {
    ROUNDING_NAMES.iter().any(|n| text.contains(n))
}

const INTEGER_SUFFIXES: &[&str] = &[
    "usize", "isize", "u128", "i128", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8",
];

/// Whether a number literal is an index or count (0 or 1), or part of a tuple
/// index.
fn number_allowed(text: &str, after_dot: bool) -> bool {
    let digits: String = text.chars().filter(|c| *c != '_').collect();
    if after_dot && digits.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return true;
    }
    let without_suffix = INTEGER_SUFFIXES
        .iter()
        .find_map(|s| digits.strip_suffix(s))
        .unwrap_or(&digits);
    without_suffix == "0" || without_suffix == "1"
}

/// Scans the production tokens of one file. `names` are the names that
/// rulebooks declare, and `listed` the string literals the crate may use.
#[must_use]
pub fn scan_tokens(
    tokens: &[Token],
    names: &BTreeSet<String>,
    listed: &BTreeSet<&str>,
) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut push = |rule: AppRule, line: usize, detail: String| {
        hits.push(Hit { rule, line, detail });
    };
    let starts = statement_starts(tokens);
    let mut angle = 0usize;
    for (i, t) in tokens.iter().enumerate() {
        let prev = i.checked_sub(1).and_then(|p| tokens.get(p));
        let next = |k: usize| tokens.get(i + k);
        let start = starts.get(i).copied().unwrap_or(0);
        if t.is_punct(";") || t.is_punct("{") || t.is_punct("}") {
            angle = 0;
        }
        match &t.kind {
            Kind::Punct(p) => {
                let binary = value_like(prev);
                match p.as_str() {
                    "/" | "%" => push(AppRule::Arith, t.line, format!("operator `{p}`")),
                    "+" if binary && !declaration_or_bound(tokens, start, i, true) => {
                        push(AppRule::Arith, t.line, "operator `+`".to_string());
                    }
                    "-" | "*" if binary => {
                        push(AppRule::Arith, t.line, format!("operator `{p}`"));
                    }
                    "<" => {
                        let generic = !binary
                            || declaration_or_bound(tokens, start, i, false)
                            || prev.and_then(Token::ident).is_some_and(type_like);
                        if generic {
                            angle += 1;
                        } else {
                            push(AppRule::Compare, t.line, "comparison `<`".to_string());
                        }
                    }
                    ">" => {
                        if angle > 0 {
                            angle -= 1;
                        } else if binary {
                            push(AppRule::Compare, t.line, "comparison `>`".to_string());
                        }
                    }
                    "=" | "!" if next(1).is_some_and(|n| n.is_punct("=")) => {
                        let is_equality = if p == "=" {
                            !prev.is_some_and(|b| {
                                b.is_punct("=")
                                    || b.is_punct("!")
                                    || b.is_punct("<")
                                    || b.is_punct(">")
                            })
                        } else {
                            true
                        };
                        if is_equality
                            && (operand_window_names_a_number(tokens, i, false)
                                || operand_window_names_a_number(tokens, i, true))
                        {
                            let op = if p == "=" { "==" } else { "!=" };
                            push(
                                AppRule::Compare,
                                t.line,
                                format!("equality `{op}` of Exact_Number operands"),
                            );
                        }
                    }
                    _ => {}
                }
            }
            Kind::Lit(text) if text.starts_with(|c: char| c.is_ascii_digit()) => {
                let after_dot = prev.is_some_and(|b| b.is_punct("."));
                if !number_allowed(text, after_dot) {
                    push(
                        AppRule::Literal,
                        t.line,
                        format!("number literal `{text}` (only 0 and 1 are indices or counts)"),
                    );
                }
            }
            Kind::Str(text) => {
                let detail = |what: &str| format!("string literal \"{text}\" {what}");
                if names.contains(text) {
                    push(
                        AppRule::FieldName,
                        t.line,
                        detail("is a name declared by a committed rulebook"),
                    );
                } else if names_tie_breaking(text) {
                    push(AppRule::TieBreak, t.line, detail("names tie-breaking"));
                } else if names_rounding_mode(text) {
                    push(AppRule::Rounding, t.line, detail("names a rounding mode"));
                } else if !listed.contains(text.as_str()) {
                    push(
                        AppRule::Literal,
                        t.line,
                        detail(&format!("is not listed in {LITERALS_PATH}")),
                    );
                }
            }
            Kind::Ident(name) => {
                let name = name.as_str();
                let call = next(1).is_some_and(|n| n.is_punct("("))
                    || (next(1).is_some_and(|n| n.is_punct("::"))
                        && next(2).is_some_and(|n| n.is_punct("<")));
                if ORDER_IDENTS.contains(&name)
                    || (call
                        && (ORDER_CALLS.contains(&name)
                            || ORDER_CALL_PREFIXES.iter().any(|p| name.starts_with(p))))
                {
                    push(AppRule::Order, t.line, format!("`{name}`"));
                }
                if names_tie_breaking(name) {
                    push(AppRule::TieBreak, t.line, format!("`{name}`"));
                }
                if ROUNDING_NAMES.contains(&name) {
                    push(AppRule::Rounding, t.line, format!("`{name}`"));
                }
                if name == "rt_numeric" {
                    numeric_path(tokens, i, &mut push);
                }
                io_use(tokens, i, name, &mut push);
            }
            _ => {}
        }
    }
    hits
}

/// Flags the items that follow `rt_numeric ::` unless each is rendering.
fn numeric_path(tokens: &[Token], i: usize, push: &mut impl FnMut(AppRule, usize, String)) {
    let line = tokens.get(i).map_or(0, |t| t.line);
    let flag = |push: &mut dyn FnMut(AppRule, usize, String), what: &str| {
        push(AppRule::Numeric, line, format!("`rt_numeric::{what}`"));
    };
    if !tokens.get(i + 1).is_some_and(|n| n.is_punct("::")) {
        flag(push, "(module)");
        return;
    }
    match tokens.get(i + 2) {
        Some(n) if n.is_punct("{") => {
            let end = skip_group(tokens, i + 2);
            for g in tokens.get(i + 3..end.saturating_sub(1)).unwrap_or(&[]) {
                match &g.kind {
                    Kind::Ident(id) if !RT_NUMERIC_ALLOWED.contains(&id.as_str()) => {
                        flag(push, id);
                    }
                    Kind::Punct(p) if p == "*" => flag(push, "*"),
                    _ => {}
                }
            }
        }
        Some(n) => match n.ident() {
            Some(id) if RT_NUMERIC_ALLOWED.contains(&id) => {}
            Some(id) => flag(push, id),
            None => flag(push, "*"),
        },
        None => flag(push, "(module)"),
    }
}

/// Flags file, network, clock, random, environment, stream, thread and
/// subprocess names.
fn io_use(tokens: &[Token], i: usize, name: &str, push: &mut impl FnMut(AppRule, usize, String)) {
    let line = tokens.get(i).map_or(0, |t| t.line);
    let mut flag = |what: &str| push(AppRule::Io, line, format!("`{what}`"));
    let after_path = tokens.get(i + 1).is_some_and(|n| n.is_punct("::"));
    if matches!(name, "std" | "core") && after_path {
        match tokens.get(i + 2) {
            Some(n) if n.is_punct("{") => {
                let end = skip_group(tokens, i + 2);
                for g in tokens.get(i + 3..end).unwrap_or(&[]) {
                    if let Some(id) = g.ident().filter(|id| STD_IO_MODULES.contains(id)) {
                        flag(&format!("std::{id}"));
                    }
                }
            }
            Some(n) => {
                if let Some(id) = n.ident().filter(|id| STD_IO_MODULES.contains(id)) {
                    flag(&format!("std::{id}"));
                }
            }
            None => {}
        }
    }
    if IO_IDENTS.contains(&name) {
        flag(name);
    }
    let method_call = i
        .checked_sub(1)
        .and_then(|p| tokens.get(p))
        .is_some_and(|p| p.is_punct("."))
        && tokens.get(i + 1).is_some_and(|n| n.is_punct("("));
    if method_call && IO_METHODS.contains(&name) {
        flag(name);
    }
    let prev_path = i
        .checked_sub(1)
        .and_then(|p| tokens.get(p))
        .is_some_and(|p| p.is_punct("::"));
    let before = i.checked_sub(2).and_then(|p| tokens.get(p));
    let builder = name == "Command"
        && ((prev_path && before.is_some_and(|b| b.is_ident("process")))
            || (!prev_path && after_path));
    if builder {
        flag("Command");
    }
    if IO_MACROS.contains(&name) && tokens.get(i + 1).is_some_and(|n| n.is_punct("!")) {
        flag(&format!("{name}!"));
    }
}

/// Scans the files of one crate. `files` are `(path relative to the crate
/// directory, text)`. Returns one line per finding.
#[must_use]
pub fn scan_crate(
    krate: &str,
    files: &[(String, String)],
    literals: &[LiteralEntry],
    names: &BTreeSet<String>,
) -> Vec<String> {
    let listed: BTreeSet<&str> = literals
        .iter()
        .filter(|e| e.crate_name == "*" || e.crate_name == krate)
        .map(|e| e.text.as_str())
        .collect();
    let parsed: Vec<(&String, Vec<Token>, BTreeSet<String>)> = files
        .iter()
        .map(|(path, text)| {
            let (tokens, mods) = production_tokens(text);
            (path, tokens, mods)
        })
        .collect();
    let test_mods: BTreeSet<&String> = parsed.iter().flat_map(|(_, _, m)| m.iter()).collect();
    let mut findings = Vec::new();
    for (path, tokens, _) in &parsed {
        let stem = Path::new(path.as_str())
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if test_mods.iter().any(|m| m.as_str() == stem) {
            continue;
        }
        let mut hits = scan_tokens(tokens, names, &listed);
        hits.sort_by(|a, b| (a.line, a.rule, &a.detail).cmp(&(b.line, b.rule, &b.detail)));
        hits.dedup();
        for hit in hits {
            findings.push(format!(
                "[{}] {krate}: {path}:{}: {}",
                hit.rule.name(),
                hit.line,
                hit.detail
            ));
        }
    }
    findings
}

/// Names of the application-layer crates that have a source directory.
pub fn layers(root: &Path, list: &EdgeList) -> Vec<String> {
    list.crates
        .iter()
        .filter(|(_, e)| matches!(e.class, Class::App | Class::Lab))
        .map(|(n, _)| n.clone())
        .filter(|n| root.join("crates").join(n).join("src").is_dir())
        .collect()
}

/// Result of the source part of the check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scanned {
    /// Crates scanned.
    pub crates: usize,
    /// Files scanned.
    pub files: usize,
    /// Rulebooks that gave the declared names.
    pub rulebooks: usize,
}

/// Runs the source rules on the application layers of the repository at
/// `root`.
///
/// # Errors
/// One line per finding, or the load failure.
pub fn run_sources(root: &Path) -> Result<Scanned, String> {
    let list = crate::scan::load_edges(root)?;
    let text = fs::read_to_string(root.join(LITERALS_PATH))
        .map_err(|e| format!("[app-literals] cannot read {LITERALS_PATH}: {e}"))?;
    let literals =
        parse_literals(&text).map_err(|m| format!("[app-literals] {LITERALS_PATH}: {m}"))?;
    let (names, rulebooks) = load_names(root)?;
    if rulebooks == 0 {
        return Err(
            "[app-names] no committed rulebook under tests/fixtures or demo gives field names"
                .to_string(),
        );
    }
    validate_literals(&literals, &list, &names)?;
    let crates = layers(root, &list);
    let mut findings = Vec::new();
    let mut file_count = 0usize;
    for name in &crates {
        let dir = root.join("crates").join(name);
        let mut files = crate::scan::read_crate_files(&dir)?;
        if let Ok(build) = fs::read_to_string(dir.join("build.rs")) {
            files.push(("build.rs".to_string(), build));
        }
        file_count += files.len();
        findings.extend(scan_crate(name, &files, &literals, &names));
    }
    if findings.is_empty() {
        Ok(Scanned {
            crates: crates.len(),
            files: file_count,
            rulebooks,
        })
    } else {
        Err(findings.join("\n"))
    }
}

/// Runs the edge rules of the application layers given the bytes of
/// `cargo metadata`. Returns the number of layers checked.
///
/// # Errors
/// One line per finding, or the load failure.
pub fn run_edges(root: &Path, metadata_bytes: &[u8]) -> Result<usize, String> {
    let list = crate::scan::load_edges(root)?;
    let md = crate::metadata::parse_metadata(metadata_bytes)
        .map_err(|m| format!("[metadata-malformed] {m}"))?;
    let findings = crate::graph::check_app_edges(&list, &md);
    if findings.is_empty() {
        Ok(list
            .crates
            .iter()
            .filter(|(n, e)| {
                matches!(e.class, Class::App | Class::Lab) && md.members.contains_key(*n)
            })
            .count())
    } else {
        Err(findings
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

#[cfg(test)]
#[path = "app_layers_tests.rs"]
mod tests;
