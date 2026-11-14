//! Shared generator and checker of the contradictory-flag property tests
//! (Property 5): inserts flag and lifecycle keys into one object of a compact
//! JSON text and states the diagnostics that must result.

#![allow(dead_code, clippy::expect_used, clippy::indexing_slicing)]

use proptest::prelude::*;
use rt_foundation::{Code, Diagnostic, ExitCode, Location, Severity};
use rt_json::{FLAG_KEYS, LIFECYCLE_KEYS};

/// Insert positions of one JSON object in compact text.
pub struct Object {
    /// Offsets at which `"key":value,` may be inserted before member `i`.
    pub before: Vec<usize>,
    /// Offset of the closing brace.
    pub close: usize,
    pub empty: bool,
}

/// Finds every object of compact JSON text and its insert offsets.
pub fn objects(text: &str) -> Vec<Object> {
    let mut done: Vec<Object> = Vec::new();
    // Stack of open containers: Some(index into `open`) for objects, None for
    // arrays.
    let mut stack: Vec<Option<usize>> = Vec::new();
    // Each entry is (before offsets, open offset).
    let mut open: Vec<(Vec<usize>, usize)> = Vec::new();
    let (mut in_str, mut esc) = (false, false);
    for (i, b) in text.bytes().enumerate() {
        if in_str {
            match (esc, b) {
                (true, _) => esc = false,
                (false, b'\\') => esc = true,
                (false, b'"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => {
                open.push((vec![i + 1], i));
                stack.push(Some(open.len() - 1));
            }
            b'[' => stack.push(None),
            b',' => {
                if let Some(Some(k)) = stack.last() {
                    open[*k].0.push(i + 1);
                }
            }
            b'}' | b']' => {
                if let Some(Some(k)) = stack.pop() {
                    let (before, o) = open[k].clone();
                    done.push(Object {
                        before,
                        close: i,
                        empty: i == o + 1,
                    });
                }
            }
            _ => {}
        }
    }
    done
}

#[derive(Debug, Clone)]
pub struct Insertion {
    pub key: &'static str,
    pub value: &'static str,
    /// Slot 0..=members: before member `slot`, or the end when
    /// `slot == members`.
    pub slot: usize,
}

pub struct Edited {
    pub text: String,
    /// Keys in input order.
    pub order: Vec<&'static str>,
}

pub fn apply(text: &str, object: &Object, ins: &[Insertion]) -> Edited {
    let members = if object.empty { 0 } else { object.before.len() };
    let mut sorted: Vec<(usize, usize)> = ins
        .iter()
        .enumerate()
        .map(|(i, x)| (x.slot.min(members), i))
        .collect();
    sorted.sort_unstable();
    let order = sorted.iter().map(|(_, i)| ins[*i].key).collect();
    let item = |i: usize| format!("\"{}\":{}", ins[i].key, ins[i].value);
    // Build edits by slot, applied from the last offset to the first.
    let mut edits: Vec<(usize, String)> = Vec::new();
    let slots: std::collections::BTreeSet<usize> = sorted.iter().map(|s| s.0).collect();
    for slot in slots {
        let items: Vec<String> = sorted
            .iter()
            .filter(|s| s.0 == slot)
            .map(|s| item(s.1))
            .collect();
        if object.empty {
            edits.push((object.close, items.join(",")));
        } else if slot == members {
            edits.push((object.close, format!(",{}", items.join(","))));
        } else {
            edits.push((object.before[slot], format!("{},", items.join(","))));
        }
    }
    let mut out = text.to_owned();
    for (offset, piece) in edits.into_iter().rev() {
        out.insert_str(offset, &piece);
    }
    Edited { text: out, order }
}

pub fn insertions() -> impl Strategy<Value = Vec<Insertion>> {
    let keys: Vec<&'static str> = FLAG_KEYS
        .iter()
        .chain(LIFECYCLE_KEYS.iter())
        .copied()
        .collect();
    let values = prop_oneof![
        Just("true"),
        Just("false"),
        Just("null"),
        Just("0"),
        Just("\"passed\""),
        Just("\"Approved\""),
        Just("[]"),
        Just("{}"),
    ];
    proptest::sample::subsequence(keys, 1..=10).prop_flat_map(move |chosen| {
        let n = chosen.len();
        (
            Just(chosen),
            proptest::collection::vec(values.clone(), n),
            proptest::collection::vec(0usize..64, n),
        )
            .prop_map(|(chosen, values, slots)| {
                chosen
                    .into_iter()
                    .zip(values)
                    .zip(slots)
                    .map(|((key, value), slot)| Insertion { key, value, slot })
                    .collect()
            })
    })
}

/// The diagnostics the flag keys of `edited` must produce: one code per
/// category present, with its offending keys in input order.
pub fn expected(edited: &Edited) -> Vec<(Code, Vec<&'static str>)> {
    let in_order = |set: &[&str]| -> Vec<&'static str> {
        edited
            .order
            .iter()
            .copied()
            .filter(|k| set.contains(k))
            .collect()
    };
    let flags = in_order(&FLAG_KEYS);
    let lifecycle = in_order(&LIFECYCLE_KEYS);
    let mut expected: Vec<(Code, Vec<&'static str>)> = Vec::new();
    if !flags.is_empty() {
        let both = flags.contains(&"has_errors") && flags.contains(&"no_errors");
        let code = if both {
            Code::DiagnosticStateBothFlags
        } else {
            Code::DiagnosticStateContradiction
        };
        expected.push((code, flags));
    }
    if !lifecycle.is_empty() {
        expected.push((Code::LifecycleFlagContradiction, lifecycle));
    }
    expected
}

/// Asserts the exact diagnostics for the keys inserted into one object.
pub fn check(
    errs: &[Diagnostic],
    edited: &Edited,
    line_text: &str,
    expect_line: Option<u32>,
) -> Result<(), TestCaseError> {
    let expected = expected(edited);
    prop_assert_eq!(errs.len(), expected.len(), "{:?}", errs);
    for (code, keys) in expected {
        let found: Vec<&Diagnostic> = errs.iter().filter(|d| d.code() == code).collect();
        prop_assert_eq!(found.len(), 1, "code {:?}", code);
        let d = found[0];
        prop_assert_eq!(d.severity(), Severity::Error);
        prop_assert_eq!(code.exit_code(), ExitCode::InvalidInput);
        for k in &keys {
            prop_assert!(d.message().contains(k), "{} missing {}", d.message(), k);
        }
        prop_assert_eq!(d.related().len(), keys.len() - 1);
        match d.primary() {
            Location::Input(l) => {
                let field = l.field.clone().unwrap_or_default();
                prop_assert!(field.ends_with(keys[0]), "{field} vs {}", keys[0]);
                let at = line_text.find(&format!("\"{}\":", keys[0])).expect("key");
                prop_assert_eq!(l.column, Some(at as u32 + 1));
                if let Some(line) = expect_line {
                    prop_assert_eq!(l.line, Some(line));
                }
            }
            other => prop_assert!(false, "not an input location: {:?}", other),
        }
    }
    Ok(())
}
