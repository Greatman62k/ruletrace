//! The canonical RT-JSON writers (design §4.1.1).
//!
//! Both forms use the same string escaping and write object members in the order the value
//! stores them, so the fixed per-object key order is the order in which the caller builds the
//! value. Writing is iterative and does not depend on nesting depth.

use crate::value::{Kind, Value};
use std::fmt::Write as _;

/// Compact form: no whitespace. This is the form that is hashed.
#[must_use]
pub fn write_compact<P>(value: &Value<P>) -> String {
    write(value, false)
}

/// Pretty form: two-space indent, one member or element per line, trailing line feed.
#[must_use]
pub fn write_pretty<P>(value: &Value<P>) -> String {
    let mut out = write(value, true);
    out.push('\n');
    out
}

enum Task<'a, P> {
    Value(&'a Value<P>, usize),
    Text(&'static str),
    Key(&'a str),
    Line(usize),
}

fn write<P>(root: &Value<P>, pretty: bool) -> String {
    let colon = if pretty { ": " } else { ":" };
    let mut out = String::new();
    let mut stack = vec![Task::Value(root, 0)];
    while let Some(task) = stack.pop() {
        match task {
            Task::Text(text) => out.push_str(text),
            Task::Key(key) => push_string(&mut out, key),
            Task::Line(level) => {
                if pretty {
                    out.push('\n');
                    for _ in 0..level {
                        out.push_str("  ");
                    }
                }
            }
            Task::Value(value, level) => match value.kind() {
                Kind::Null => out.push_str("null"),
                Kind::Bool(true) => out.push_str("true"),
                Kind::Bool(false) => out.push_str("false"),
                Kind::Integer(n) => {
                    // Writing to a String cannot fail.
                    let _ = write!(out, "{n}");
                }
                Kind::Number(text) => out.push_str(text),
                Kind::String(text) => push_string(&mut out, text),
                Kind::Array(items) if items.is_empty() => out.push_str("[]"),
                Kind::Object(members) if members.is_empty() => out.push_str("{}"),
                Kind::Array(items) => {
                    out.push('[');
                    stack.push(Task::Text("]"));
                    stack.push(Task::Line(level));
                    for (index, item) in items.iter().enumerate().rev() {
                        stack.push(Task::Value(item, level + 1));
                        stack.push(Task::Line(level + 1));
                        if index > 0 {
                            stack.push(Task::Text(","));
                        }
                    }
                }
                Kind::Object(members) => {
                    out.push('{');
                    stack.push(Task::Text("}"));
                    stack.push(Task::Line(level));
                    for (index, member) in members.iter().enumerate().rev() {
                        stack.push(Task::Value(&member.value, level + 1));
                        stack.push(Task::Text(colon));
                        stack.push(Task::Key(&member.key));
                        stack.push(Task::Line(level + 1));
                        if index > 0 {
                            stack.push(Task::Text(","));
                        }
                    }
                }
            },
        }
    }
    out
}

/// `\"`, `\\`, `\n`, `\r`, `\t`, `\u00xx` (lowercase hex) for every other Unicode `Cc`
/// character, all other scalar values literally.
fn push_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, ParseLimits};
    use rt_foundation::InputId;

    fn obj(members: Vec<(&str, Value<()>)>) -> Value<()> {
        Value::object(
            members
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
        .unwrap()
    }

    fn sample() -> Value<()> {
        obj(vec![
            ("z", Value::integer(1)),
            (
                "a",
                Value::array(vec![
                    Value::null(),
                    Value::boolean(true),
                    Value::string("x"),
                ]),
            ),
            ("e", obj(vec![])),
            ("f", Value::array(vec![])),
            ("s", Value::string("q\"\\\n\r\t\u{1}\u{7f}\u{85}é")),
        ])
    }

    #[test]
    fn compact_keeps_member_order_and_escapes() {
        assert_eq!(
            write_compact(&sample()),
            "{\"z\":1,\"a\":[null,true,\"x\"],\"e\":{},\"f\":[],\
             \"s\":\"q\\\"\\\\\\n\\r\\t\\u0001\\u007f\\u0085é\"}"
        );
    }

    #[test]
    fn pretty_uses_two_spaces_and_a_trailing_line_feed() {
        let v = obj(vec![
            (
                "a",
                Value::array(vec![Value::integer(1), obj(vec![("b", Value::null())])]),
            ),
            ("c", Value::array(vec![])),
        ]);
        assert_eq!(
            write_pretty(&v),
            "{\n  \"a\": [\n    1,\n    {\n      \"b\": null\n    }\n  ],\n  \"c\": []\n}\n"
        );
    }

    #[test]
    fn output_parses_back_to_the_same_canonical_text() {
        let id = InputId::new("t").unwrap();
        let l = ParseLimits::new(1 << 20, 64);
        let compact = write_compact(&sample());
        let parsed = parse(&id, compact.as_bytes(), l).unwrap();
        assert_eq!(write_compact(&parsed), compact);
        let pretty = write_pretty(&parsed);
        assert!(pretty.ends_with("}\n"));
        let again = parse(&id, pretty.as_bytes(), l).unwrap();
        assert_eq!(write_compact(&again), compact);
        assert_eq!(write_pretty(&again), pretty);
    }

    #[test]
    fn number_lexemes_are_kept_verbatim() {
        let id = InputId::new("t").unwrap();
        let v = parse(
            &id,
            b"[-0, 1.50, 1e3, 9007199254740992]",
            ParseLimits::new(100, 4),
        )
        .unwrap();
        assert_eq!(write_compact(&v), "[-0,1.50,1e3,9007199254740992]");
    }

    #[test]
    fn built_objects_reject_duplicate_keys() {
        let r = Value::object(vec![
            ("a".into(), Value::null()),
            ("a".into(), Value::null()),
        ]);
        assert_eq!(r.unwrap_err().0, "a");
    }

    #[test]
    fn deep_values_write_without_recursion() {
        let n = 100_000;
        let mut v = Value::array(vec![]);
        for _ in 0..n {
            v = Value::array(vec![v]);
        }
        let text = write_compact(&v);
        assert_eq!(text.len(), 2 * (n + 1));
    }
}
