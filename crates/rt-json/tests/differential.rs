//! Differential test of the RT-JSON parser against `serde_json` as a dev-only
//! oracle (design §4.1.1, risk R-D3).
//!
//! Valid and invalid documents are generated. For every document, RT-JSON and
//! `serde_json` must agree on accept or reject, and on the decoded value when
//! both accept. The dialect is stricter than RFC 8259 in exactly one way that
//! `serde_json` does not share, duplicate object keys, so a rejection whose
//! message says "duplicate object key" is the only permitted disagreement (and
//! then `serde_json` must accept). The oracle also rejects floats outside the
//! `f64` range ("number out of range"), which RT-JSON keeps as a lexeme; those
//! documents are skipped.
//!
//! Feature: ruletrace-compiler, R-D3: RT-JSON agrees with serde_json on
//! generated documents.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

#[path = "../../../tools/proptest_floor.rs"]
mod proptest_floor;
use proptest::prelude::*;
use proptest_floor::property_tests;
use rt_foundation::{Code, InputId};
use rt_json::{parse, write_compact, write_pretty, Kind, ParseLimits, Position, Value};
use serde_json::Value as Oracle;

const CASES: u32 = 512;

fn run(bytes: &[u8]) -> Result<Value<Position>, Box<rt_foundation::Diagnostic>> {
    parse(
        &InputId::new("gen.json").unwrap(),
        bytes,
        ParseLimits::new(1 << 20, 64),
    )
}

/// Structural equality of an RT-JSON value and an oracle value.
fn same(ours: &Value<Position>, theirs: &Oracle) -> Result<(), String> {
    match (ours.kind(), theirs) {
        (Kind::Null, Oracle::Null) => Ok(()),
        (Kind::Bool(a), Oracle::Bool(b)) if a == b => Ok(()),
        (Kind::Integer(n), Oracle::Number(m)) if m.as_u64() == Some(*n) => Ok(()),
        (Kind::Number(lexeme), Oracle::Number(m)) => {
            // Only digit strings can be compared exactly without trusting float
            // parsing.
            if lexeme.bytes().all(|b| b.is_ascii_digit()) && m.as_u64().is_some() {
                if m.as_u64() == lexeme.parse::<u64>().ok() {
                    Ok(())
                } else {
                    Err(format!("integer {lexeme} vs {m}"))
                }
            } else {
                Ok(())
            }
        }
        (Kind::String(a), Oracle::String(b)) if a == b => Ok(()),
        (Kind::Array(a), Oracle::Array(b)) if a.len() == b.len() => {
            a.iter().zip(b).try_for_each(|(x, y)| same(x, y))
        }
        (Kind::Object(a), Oracle::Object(b)) if a.len() == b.len() => a.iter().try_for_each(|m| {
            let other = b
                .get(&m.key)
                .ok_or_else(|| format!("key {:?} missing", m.key))?;
            same(&m.value, other)
        }),
        (kind, theirs) => Err(format!("{kind:?} vs {theirs:?}")),
    }
}

/// Compares the two parsers on one input. Returns true when the input was
/// compared.
fn agree(bytes: &[u8]) -> Result<bool, TestCaseError> {
    let oracle = serde_json::from_slice::<Oracle>(bytes);
    let ours = run(bytes);
    match (ours, oracle) {
        (Ok(a), Ok(b)) => {
            same(&a, &b).map_err(|e| TestCaseError::fail(format!("{e} for {bytes:?}")))?;
            Ok(true)
        }
        (Err(d), Ok(_)) => {
            prop_assert!(
                d.code() == Code::IrSyntaxError && d.message().contains("duplicate object key"),
                "RT-JSON rejected what serde_json accepts: {} for {:?}",
                d.message(),
                String::from_utf8_lossy(bytes)
            );
            Ok(true)
        }
        (Ok(_), Err(e)) => {
            if e.to_string().contains("number out of range") {
                return Ok(false);
            }
            Err(TestCaseError::fail(format!(
                "RT-JSON accepted what serde_json rejects ({e}): {:?}",
                String::from_utf8_lossy(bytes)
            )))
        }
        (Err(d), Err(_)) => {
            prop_assert!(
                matches!(d.code(), Code::IrSyntaxError),
                "unexpected code {:?}",
                d.code()
            );
            Ok(true)
        }
    }
}

fn ws() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        4 => Just(""),
        2 => Just(" "),
        1 => Just("\n"),
        1 => Just("\t"),
        1 => Just("\r\n  "),
    ]
}

fn string_fragment() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        4 => Just("a"),
        2 => Just("Z9 "),
        1 => Just("é"),
        1 => Just("😀"),
        1 => Just("\u{7f}"),
        1 => Just("\u{10FFFF}"),
        1 => Just("\\\""),
        1 => Just("\\\\"),
        1 => Just("\\/"),
        1 => Just("\\b\\f\\n\\r\\t"),
        1 => Just("\\u0000"),
        1 => Just("\\u001F"),
        1 => Just("\\u00e9"),
        1 => Just("\\uD834\\uDD1E"),
        1 => Just("\\ud800\\udc00"),
        1 => Just("//"),
    ]
}

fn json_string() -> impl Strategy<Value = String> {
    prop::collection::vec(string_fragment(), 0..6).prop_map(|f| format!("\"{}\"", f.concat()))
}

fn number() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => (0u64..100_000).prop_map(|n| n.to_string()),
        1 => Just("0".to_string()),
        1 => Just("9007199254740991".to_string()),
        1 => Just("9007199254740992".to_string()),
        1 => Just("18446744073709551615".to_string()),
        1 => Just("18446744073709551616".to_string()),
        2 => ("-?", "(0|[1-9][0-9]{0,6})", "(\\.[0-9]{1,6})?", "([eE][+-]?[0-9]{1,2})?")
            .prop_map(|(a, b, c, d)| format!("{a}{b}{c}{d}")),
    ]
}

/// A valid RT-JSON document with random whitespace and unique keys.
fn valid_doc() -> impl Strategy<Value = String> {
    let leaf = prop_oneof![
        Just("null".to_string()),
        Just("true".to_string()),
        Just("false".to_string()),
        number(),
        json_string(),
    ];
    let value = leaf.prop_recursive(5, 48, 5, |inner| {
        prop_oneof![
            prop::collection::vec((ws(), inner.clone(), ws()), 0..5).prop_map(|items| {
                let body: Vec<String> = items
                    .into_iter()
                    .map(|(a, v, b)| format!("{a}{v}{b}"))
                    .collect();
                format!("[{}]", body.join(","))
            }),
            prop::collection::vec((ws(), string_fragment(), ws(), inner, ws()), 0..5).prop_map(
                |members| {
                    let body: Vec<String> = members
                        .into_iter()
                        .enumerate()
                        // The index keeps keys unique whatever the fragment is.
                        .map(|(i, (a, frag, b, v, c))| format!("{a}\"k{i}{frag}\"{b}:{c}{v}"))
                        .collect();
                    format!("{{{}}}", body.join(","))
                }
            ),
        ]
    });
    (ws(), value, ws()).prop_map(|(a, v, b)| format!("{a}{v}{b}"))
}

#[derive(Clone, Debug)]
enum Mutation {
    Delete(usize),
    Insert(usize, u8),
    Replace(usize, u8),
    Truncate(usize),
    Splice(usize, &'static [u8]),
}

fn mutation() -> impl Strategy<Value = Mutation> {
    let byte = prop_oneof![
        4 => prop::sample::select(b"{}[]\",:\\ 'tfnu+-.eE0189/*#\n\t\r".to_vec()),
        1 => Just(0x00u8),
        1 => Just(0x1Fu8),
        1 => Just(0x7Fu8),
        1 => Just(0x80u8),
        1 => Just(0xC3u8),
        1 => Just(0xFFu8),
    ];
    let chunks: Vec<&'static [u8]> = vec![
        b"\\ud800",
        b"\\udc00",
        b"\\ud800\\ud800",
        b"\\u12",
        b"\\x",
        b",,",
        b"\xEF\xBB\xBF",
        b"//c\n",
        b"/*c*/",
        b"01",
        b"-",
        b"1.",
        b"1e",
        b"\xED\xA0\x80",
        b"\xC0\x80",
        b"\"\"",
        b"NaN",
        b"nul",
    ];
    prop_oneof![
        3 => any::<prop::sample::Index>().prop_map(|i| Mutation::Delete(i.index(usize::MAX))),
        3 => (any::<prop::sample::Index>(), byte.clone())
            .prop_map(|(i, b)| Mutation::Insert(i.index(usize::MAX), b)),
        3 => (any::<prop::sample::Index>(), byte)
            .prop_map(|(i, b)| Mutation::Replace(i.index(usize::MAX), b)),
        1 => any::<prop::sample::Index>().prop_map(|i| Mutation::Truncate(i.index(usize::MAX))),
        2 => (any::<prop::sample::Index>(), prop::sample::select(chunks))
            .prop_map(|(i, c)| Mutation::Splice(i.index(usize::MAX), c)),
    ]
}

fn mutate(mut bytes: Vec<u8>, m: &Mutation) -> Vec<u8> {
    let n = bytes.len() + 1;
    match m {
        Mutation::Delete(i) if !bytes.is_empty() => {
            bytes.remove(i % bytes.len());
        }
        Mutation::Insert(i, b) => bytes.insert(i % n, *b),
        Mutation::Replace(i, b) if !bytes.is_empty() => {
            let at = i % bytes.len();
            bytes[at] = *b;
        }
        Mutation::Truncate(i) => bytes.truncate(i % n),
        Mutation::Splice(i, chunk) => {
            let at = i % n;
            bytes.splice(at..at, chunk.iter().copied());
        }
        _ => {}
    }
    bytes
}

property_tests! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    /// Every generated valid document is accepted by both parsers and decodes
    /// identically.
    #[test]
    fn valid_documents_are_accepted_by_both(doc in valid_doc()) {
        let oracle = serde_json::from_str::<Oracle>(&doc).map_err(|e| {
            TestCaseError::fail(format!("generator produced a document the oracle rejects: {e}: {doc:?}"))
        })?;
        let ours = run(doc.as_bytes()).map_err(|d| {
            TestCaseError::fail(format!("RT-JSON rejected a valid document: {}: {doc:?}", d.message()))
        })?;
        same(&ours, &oracle).map_err(TestCaseError::fail)?;
    }

    /// One to three byte-level mutations of a valid document: both parsers
    /// agree on accept or reject, and on the value when both accept.
    #[test]
    fn mutated_documents_get_the_same_verdict(
        doc in valid_doc(),
        edits in prop::collection::vec(mutation(), 1..4),
    ) {
        let bytes = edits.iter().fold(doc.into_bytes(), mutate);
        agree(&bytes)?;
    }

    /// Arbitrary bytes drawn from the structural alphabet: the same verdict.
    #[test]
    fn structural_noise_gets_the_same_verdict(
        bytes in prop::collection::vec(
            prop::sample::select(b"{}[]\":,\\ \n01-+.eEatrulfns/\xC3\xA9".to_vec()),
            0..24,
        )
    ) {
        agree(&bytes)?;
    }

    /// A repeated key is the one place RT-JSON is stricter: it must reject, at
    /// the repeat.
    #[test]
    fn repeated_keys_are_rejected(
        key in prop::sample::select(vec!["a", "k", "\\u0061", "é", "\\u00e9"]),
        first in number(),
        second in json_string(),
        pad in ws(),
    ) {
        // "a" and "\u0061" are the same key, and so are "é" and "\u00e9".
        let canon = |k: &'static str| match k { "\\u0061" => "a", "\\u00e9" => "é", other => other };
        let spelled = [key, canon(key)];
        let doc = format!("{{{pad}\"{}\":{first},\"x\":null,\"{}\":{second}}}", spelled[0], spelled[1]);
        let d = run(doc.as_bytes()).unwrap_err();
        prop_assert_eq!(d.code(), Code::IrSyntaxError);
        prop_assert!(d.message().contains("duplicate object key"));
        prop_assert!(serde_json::from_str::<Oracle>(&doc).is_ok());
    }

    /// Writing a parsed document and parsing it again changes nothing, in
    /// either form, and the oracle reads the writer output as the same value as
    /// the original text.
    #[test]
    fn writers_round_trip_and_the_oracle_agrees(doc in valid_doc()) {
        let parsed = run(doc.as_bytes()).unwrap();
        let compact = write_compact(&parsed);
        let pretty = write_pretty(&parsed);
        let again = run(compact.as_bytes()).unwrap();
        prop_assert_eq!(write_compact(&again), compact.clone());
        let from_pretty = run(pretty.as_bytes()).unwrap();
        prop_assert_eq!(write_compact(&from_pretty), compact.clone());
        prop_assert!(pretty.ends_with('\n'));
        let original = serde_json::from_str::<Oracle>(&doc).unwrap();
        prop_assert_eq!(serde_json::from_str::<Oracle>(&compact).unwrap(), original.clone());
        prop_assert_eq!(serde_json::from_str::<Oracle>(&pretty).unwrap(), original);
    }
}
