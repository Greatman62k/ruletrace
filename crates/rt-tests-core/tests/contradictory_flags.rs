//! Property 5 (Rulebook and ledger part, task 4.12): contradictory flag input is rejected.
//!
//! Any non-empty set of the flag keys (`has_errors`, `no_errors`, `validation_status`,
//! `validation_result`, `passed`) and lifecycle keys (`approval_status`, `is_approved`,
//! `is_rejected`, `is_proposed`, `approval_record`) is inserted, each at its own position and
//! with its own value, into one object of a valid Rulebook or of a valid ledger line. The
//! parser must return one Error per category present (`RT-DGN-0002` when both `has_errors` and
//! `no_errors` are present, else `RT-DGN-0001`, and `RT-DGN-0003` for lifecycle keys), each
//! naming every offending field, with the first offending key in input order as the primary
//! location and the others as related locations. No value is returned, so no later stage can
//! run. The catalogue maps all three codes to exit code 2 (`InvalidInput`); the CLI has no
//! validate command yet, so the exit code is asserted at the catalogue level.
//!
//! Later tasks (7.8, 9.12) extend the generator to the other input kinds.
//!
//! **Validates: Requirements 4.10, 22.6, 22.7, 22.9, 22.10, 22.11, 22.12, 22.13, 22.16, 22.17, 22.18, 20.25**

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use proptest::prelude::*;
use rt_foundation::{Code, Diagnostic, ExitCode, InputId, Location, Severity};
use rt_hash::sha256;
use rt_json::{ParseLimits, FLAG_KEYS, LIFECYCLE_KEYS};
use rt_ledger::{parse_ledger, HEADER_LINE, ZERO_HASH};

const DOC_HASH: &str = "eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a";

fn rulebook_text() -> String {
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"spring-open","version":"2026.1","documents":[{{"id":"rules.md","sha256":"{DOC_HASH}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}},{{"name":"licence","type":"boolean"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{{"id":"elig.basic","revision":1,"origin":{{"kind":"manual","author":null}},"kind":"eligibility","target":"eligible","type":"boolean","expr":{{"node":"and","t":"boolean","args":[{{"node":"cmp","t":"boolean","op":"ge","left":{{"node":"field","t":"number","name":"age"}},"right":{{"node":"num","t":"number","value":"16"}}}},{{"node":"field","t":"boolean","name":"licence"}}]}},"markers":[],"spans":[{{"document":"rules.md","start":21,"end":91,"line":3,"column":1}}]}}],"precedence":[]}}"#
    )
}

fn hex(seed: &str) -> String {
    sha256(seed.as_bytes()).as_str().to_owned()
}

/// A valid ledger of `n` records, sealed with an independently computed hash chain.
fn ledger_lines(n: usize) -> Vec<String> {
    let mut lines = vec![HEADER_LINE.to_owned()];
    let mut prev = ZERO_HASH.to_owned();
    for i in 1..=n {
        let head = format!(
            r#"{{"seq":{i},"kind":"approval","rule":"r{i}","revision":{i},"content_hash":"{}","ir_version":"1","binding":[{{"document":"d.md","sha256":"{}"}}],"reviewer":"Alice","time":"2026-11-14T09:00:00Z","reason":null,"prev":"{prev}""#,
            hex(&format!("c{i}")),
            hex("doc")
        );
        let hash = hex(&format!("{head}}}"));
        lines.push(format!(r#"{head},"hash":"{hash}"}}"#));
        prev = hash;
    }
    lines
}

/// Insert positions of one JSON object in compact text.
struct Object {
    /// Offsets at which `"key":value,` may be inserted before member `i`.
    before: Vec<usize>,
    /// Offset of the closing brace.
    close: usize,
    empty: bool,
}

/// Finds every object of compact JSON text and its insert offsets.
fn objects(text: &str) -> Vec<Object> {
    let mut done: Vec<Object> = Vec::new();
    // Stack of open containers: Some(index into `open`) for objects, None for arrays.
    let mut stack: Vec<Option<usize>> = Vec::new();
    let mut open: Vec<(Vec<usize>, usize)> = Vec::new(); // (before offsets, open offset)
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
struct Insertion {
    key: &'static str,
    value: &'static str,
    /// Slot 0..=members: before member `slot`, or the end when `slot == members`.
    slot: usize,
}

struct Edited {
    text: String,
    /// Keys in input order.
    order: Vec<&'static str>,
}

fn apply(text: &str, object: &Object, ins: &[Insertion]) -> Edited {
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

fn insertions() -> impl Strategy<Value = Vec<Insertion>> {
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

/// Asserts the exact diagnostics for the keys inserted into one object.
fn check(
    errs: &[Diagnostic],
    edited: &Edited,
    line_text: &str,
    expect_line: Option<u32>,
) -> Result<(), TestCaseError> {
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn rulebook_with_flag_or_lifecycle_keys_is_rejected(
        ins in insertions(),
        which in any::<usize>(),
    ) {
        let base = rulebook_text();
        let objs = objects(&base);
        prop_assert!(objs.len() > 5);
        let object = &objs[which % objs.len()];
        let edited = apply(&base, object, &ins);
        let input = InputId::new("rulebook.json").expect("input id");
        let result = rt_ir::parse_rulebook(edited.text.as_bytes(), &input, 8 << 20, 64);
        match result {
            Ok(_) => prop_assert!(false, "Rulebook accepted: {}", edited.text),
            Err(rejected) => {
                let diags = rejected.into_diagnostics();
                check(diags.as_slice(), &edited, &edited.text, Some(1))?;
            }
        }
    }

    #[test]
    fn ledger_with_flag_or_lifecycle_keys_is_rejected(
        ins in insertions(),
        n in 1usize..4,
        line_pick in any::<usize>(),
        which in any::<usize>(),
    ) {
        let mut lines = ledger_lines(n);
        let target = line_pick % lines.len();
        let objs = objects(&lines[target]);
        let object = &objs[which % objs.len()];
        let edited = apply(&lines[target], object, &ins);
        lines[target] = edited.text.clone();
        let mut bytes = lines.join("\n").into_bytes();
        bytes.push(b'\n');
        let input = InputId::new("approvals.ledger").expect("input id");
        match parse_ledger(&input, &bytes, ParseLimits::new(1 << 20, 64)) {
            Ok(_) => prop_assert!(false, "ledger accepted"),
            Err(diags) => check(&diags, &edited, &edited.text, Some(target as u32 + 1))?,
        }
    }
}

#[test]
fn unmodified_inputs_are_accepted() {
    let input = InputId::new("rulebook.json").expect("input id");
    assert!(rt_ir::parse_rulebook(rulebook_text().as_bytes(), &input, 8 << 20, 64).is_ok());
    let mut bytes = ledger_lines(3).join("\n").into_bytes();
    bytes.push(b'\n');
    let input = InputId::new("approvals.ledger").expect("input id");
    assert!(parse_ledger(&input, &bytes, ParseLimits::new(1 << 20, 64)).is_ok());
}
