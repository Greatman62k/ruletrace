//! Unit tests of the RT-JSON dialect: RFC 8259 examples, each restriction, the
//! position of the first error, and limits at and one above the bound.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use rt_foundation::{Code, Diagnostic, InputId, Location};
use rt_json::MAX_SAFE_INTEGER;
use rt_json::{parse, write_compact, write_pretty, Kind, ParseLimits, Position, Value};

fn id() -> InputId {
    InputId::new("t.json").unwrap()
}

fn lim() -> ParseLimits {
    ParseLimits::new(1 << 20, 64)
}

fn ok(text: &str) -> Value<Position> {
    parse(&id(), text.as_bytes(), lim()).unwrap()
}

fn fail(bytes: &[u8]) -> Diagnostic {
    *parse(&id(), bytes, lim()).unwrap_err()
}

fn at(d: &Diagnostic) -> (u32, u32) {
    match d.primary() {
        Location::Input(l) => (l.line.unwrap(), l.column.unwrap()),
        other => panic!("unexpected location {other:?}"),
    }
}

/// Asserts a syntax error (RT-IR-0001) at the given line and column.
fn syntax_at(bytes: &[u8], line: u32, column: u32) {
    let d = fail(bytes);
    assert_eq!(d.code(), Code::IrSyntaxError, "{bytes:?}");
    assert_eq!(at(&d), (line, column), "{:?}: {}", bytes, d.message());
}

#[test]
fn rfc8259_section_13_examples_parse() {
    let image = r#"{
        "Image": {
            "Width":  800,
            "Height": 600,
            "Title":  "View from 15th Floor",
            "Thumbnail": {
                "Url":    "http://www.example.com/image/481989943",
                "Height": 125,
                "Width":  100
            },
            "Animated" : false,
            "IDs": [116, 943, 234, 38793]
        }
    }"#;
    let v = ok(image);
    let img = &v.get("Image").unwrap().value;
    assert_eq!(img.get("Width").unwrap().value.as_u64(), Some(800));
    assert_eq!(
        img.get("Thumbnail")
            .unwrap()
            .value
            .get("Url")
            .unwrap()
            .value
            .as_str(),
        Some("http://www.example.com/image/481989943")
    );
    assert_eq!(img.get("IDs").unwrap().value.as_array().unwrap().len(), 4);

    let array = r#"[
        {"precision": "zip", "Latitude": 37.7668, "Longitude": -122.3959, "Country": "US"},
        {"precision": "zip", "Latitude": 37.371991, "Longitude": -122.026020, "Country": "US"}
    ]"#;
    let v = ok(array);
    let first = &v.as_array().unwrap()[0];
    // Fractions stay lexemes; the parser never builds a float.
    assert_eq!(
        first.get("Latitude").unwrap().value.kind(),
        &Kind::Number("37.7668".into())
    );
    assert_eq!(
        first.get("Longitude").unwrap().value.kind(),
        &Kind::Number("-122.3959".into())
    );

    // RFC 8259 allows any value at the top level.
    assert_eq!(ok("\"Hello world!\"").as_str(), Some("Hello world!"));
    assert_eq!(ok("42").as_u64(), Some(42));
    assert_eq!(ok("true").kind(), &Kind::Bool(true));
    assert_eq!(ok("false").kind(), &Kind::Bool(false));
    assert_eq!(ok("null").kind(), &Kind::Null);
}

#[test]
fn rfc8259_string_examples() {
    // Section 7: U+1D11E as a surrogate pair, and as a literal scalar.
    assert_eq!(ok(r#""\uD834\uDD1E""#).as_str(), Some("\u{1D11E}"));
    assert_eq!(ok("\"\u{1D11E}\"").as_str(), Some("\u{1D11E}"));
    // Section 8.3 / 9: escapes are case-insensitive in hex digits.
    assert_eq!(ok(r#""\u00e9\u00E9""#).as_str(), Some("éé"));
    // An escaped NUL is a legal string; an unescaped one is not.
    assert_eq!(ok(r#""\u0000""#).as_str(), Some("\0"));
    syntax_at(b"\"\x00\"", 1, 2);
    // Section 6: number grammar.
    for good in ["0", "-0", "0.0", "-1.5e+10", "1E-2", "10", "1e0"] {
        ok(good);
    }
    for bad in [
        "-", "1e", "1e+", ".5", "5.", "0x10", "1_0", "--1", "Infinity", "NaN", "00",
    ] {
        assert!(parse(&id(), bad.as_bytes(), lim()).is_err(), "{bad}");
    }
}

#[test]
fn whitespace_is_exactly_the_four_rfc_characters() {
    ok(" \t\r\n[ \t\r\n1 \t\r\n] \t\r\n");
    syntax_at(b"[\x0B1]", 1, 2);
    syntax_at(b"[\x0C1]", 1, 2);
    syntax_at("[\u{A0}1]".as_bytes(), 1, 2);
    syntax_at("[\u{FEFF}1]".as_bytes(), 1, 2);
}

#[test]
fn restriction_utf8_only() {
    // Invalid byte in a string: position is the byte (line, column counted in
    // scalars).
    let d = fail(b"[\"a\xFF\"]");
    assert_eq!((d.code(), at(&d)), (Code::IrSyntaxError, (1, 4)));
    assert!(d.message().contains("byte 3") && d.message().contains("UTF-8"));
    // Truncated multibyte sequence at the end.
    syntax_at(b"\"\xC3", 1, 2);
    // Overlong and surrogate encodings are not UTF-8.
    syntax_at(b"[\"\xC0\x80\"]", 1, 3);
    syntax_at(b"[\"\xED\xA0\x80\"]", 1, 3);
    // UTF-16 input is not UTF-8.
    syntax_at(b"\xFF\xFE[\x00]\x00", 1, 1);
}

#[test]
fn restriction_no_bom() {
    let d = fail(b"\xEF\xBB\xBF[]");
    assert_eq!((d.code(), at(&d)), (Code::IrSyntaxError, (1, 1)));
    assert!(d.message().contains("byte order mark"));
    // A BOM anywhere else is just an unexpected character.
    syntax_at("[]\u{FEFF}".as_bytes(), 1, 3);
}

#[test]
fn restriction_no_comments() {
    syntax_at(b"// c\n[]", 1, 1);
    syntax_at(b"/* c */[]", 1, 1);
    syntax_at(b"[1, // c\n2]", 1, 5);
    syntax_at(b"[] # c", 1, 4);
    // Comment markers inside a string are data.
    assert_eq!(
        ok("\"// not a comment\"").as_str(),
        Some("// not a comment")
    );
}

#[test]
fn restriction_no_duplicate_keys_reported_at_the_repeat() {
    let d = fail(b"{\"a\":1,\n  \"a\":2}");
    assert_eq!((d.code(), at(&d)), (Code::IrSyntaxError, (2, 3)));
    assert!(d.message().contains("duplicate"));
    // Escapes are resolved before comparison.
    let d = fail(br#"{"a":1,"\u0061":2}"#);
    assert_eq!((d.code(), at(&d)), (Code::IrSyntaxError, (1, 8)));
    assert!(d.message().contains("duplicate"));
    // Nested objects have their own key sets; siblings in arrays too.
    ok(r#"{"a":{"a":{"a":1}},"b":[{"a":1},{"a":2}]}"#);
    // A repeat inside a nested object is found there.
    syntax_at(b"{\"x\":{\"k\":1,\"k\":2}}", 1, 13);
}

#[test]
fn restriction_integers_up_to_the_safe_bound_else_lexeme() {
    assert_eq!(ok("9007199254740991").as_u64(), Some(MAX_SAFE_INTEGER));
    assert_eq!(
        ok("9007199254740992").kind(),
        &Kind::Number("9007199254740992".into())
    );
    assert_eq!(
        ok("18446744073709551616").kind(),
        &Kind::Number("18446744073709551616".into())
    );
    assert_eq!(ok("0").as_u64(), Some(0));
    for lexeme in ["-0", "-1", "1.0", "1e2", "0.60", "1E+2"] {
        assert_eq!(ok(lexeme).kind(), &Kind::Number(lexeme.into()), "{lexeme}");
    }
    // Leading zeros are not JSON; the error is at the second digit.
    syntax_at(b"[01]", 1, 3);
    syntax_at(b"[-01]", 1, 4);
}

#[test]
fn restriction_strings_escapes_surrogates_and_controls() {
    // Every control character below U+0020 must be escaped.
    for c in 0u8..0x20 {
        syntax_at(&[b'"', b'a', c, b'"'], 1, 3);
    }
    // DEL and U+0080.. are allowed literally.
    assert_eq!(ok("\"\u{7F}\u{80}\"").as_str(), Some("\u{7F}\u{80}"));
    // Invalid escapes: position is the character after the backslash.
    syntax_at(br#""\a""#, 1, 3);
    syntax_at(br#""\u12""#, 1, 6);
    syntax_at(br#""\u12G4""#, 1, 6);
    syntax_at(br#""\U0041""#, 1, 3);
    syntax_at(b"\"\\", 1, 3);
    // Unpaired surrogates: position is the backslash of the escape that starts
    // the problem.
    syntax_at(br#""a\ud800""#, 1, 3);
    syntax_at(br#""a\udc00""#, 1, 3);
    syntax_at(br#""a\ud800x""#, 1, 3);
    syntax_at(br#""a\ud800\ud800""#, 1, 3);
    syntax_at(br#""a\udc00\ud800""#, 1, 3);
    // A valid pair at the edges of the range.
    assert_eq!(ok(r#""\ud800\udc00""#).as_str(), Some("\u{10000}"));
    assert_eq!(ok(r#""\udbff\udfff""#).as_str(), Some("\u{10FFFF}"));
    // Single quotes are not string delimiters.
    syntax_at(b"'a'", 1, 1);
}

#[test]
fn restriction_literals_and_structure() {
    for bad in ["True", "FALSE", "Null", "nul", "tru", "fals", "undefined"] {
        assert!(parse(&id(), bad.as_bytes(), lim()).is_err(), "{bad}");
    }
    syntax_at(b"truex", 1, 5);
    syntax_at(b"[1,]", 1, 4);
    syntax_at(b"{\"a\":1,}", 1, 8);
    syntax_at(b"[,1]", 1, 2);
    syntax_at(b"{\"a\" 1}", 1, 6);
    syntax_at(b"{\"a\":}", 1, 6);
    syntax_at(b"{1:2}", 1, 2);
    syntax_at(b"{\"a\":1 \"b\":2}", 1, 8);
    syntax_at(b"[1 2]", 1, 4);
    syntax_at(b"[1", 1, 3);
    syntax_at(b"{\"a\":1", 1, 7);
    syntax_at(b"[] []", 1, 4);
    syntax_at(b"", 1, 1);
    syntax_at(b"  \n ", 2, 2);
}

#[test]
fn first_error_position_uses_lines_and_scalar_columns() {
    // Only LF ends a line: CR alone and CRLF count as one column and one line
    // respectively.
    syntax_at(b"[\r1\r,]", 1, 6);
    syntax_at(b"[\r\n1\r\n,]", 3, 2);
    // Columns count scalar values, not bytes.
    syntax_at("[\"é😀\" 1]".as_bytes(), 1, 7);
    // The error is the first one: later problems are not reported.
    let d = fail(b"[1,\n  x,\n  y]");
    assert_eq!(at(&d), (2, 3));
    // Value and key positions are reported for a good parse, in byte, line,
    // column.
    let v = ok("{\n\t\"k\":\n\t [ \"😀\" , 7 ]}");
    let k = v.get("k").unwrap();
    assert_eq!(
        k.key_pos,
        Position {
            byte: 3,
            line: 2,
            column: 2
        }
    );
    let items = k.value.as_array().unwrap();
    assert_eq!(
        *k.value.pos(),
        Position {
            byte: 10,
            line: 3,
            column: 3
        }
    );
    assert_eq!(
        *items[1].pos(),
        Position {
            byte: 21,
            line: 3,
            column: 11
        }
    );
}

#[test]
fn size_limit_at_and_one_above() {
    let text = b"[1, 2]";
    let n = u64::try_from(text.len()).unwrap();
    assert!(parse(&id(), text, ParseLimits::new(n, 8)).is_ok());
    let d = *parse(&id(), text, ParseLimits::new(n - 1, 8)).unwrap_err();
    assert_eq!(d.code(), Code::LimitFileTooLarge);
    assert!(d.message().contains("t.json"));
    assert!(d.message().contains(&format!("{n} bytes")));
    assert!(d.message().contains(&format!("{} bytes", n - 1)));
    // The size error has no line and column.
    match d.primary() {
        Location::Input(l) => assert_eq!((l.line, l.column), (None, None)),
        other => panic!("unexpected location {other:?}"),
    }
    // Empty input against a zero limit passes the size check and fails the
    // grammar.
    let d = *parse(&id(), b"", ParseLimits::new(0, 8)).unwrap_err();
    assert_eq!(d.code(), Code::IrSyntaxError);
    // The size check precedes the BOM and UTF-8 checks.
    let d = *parse(&id(), b"\xEF\xBB\xBF\xFF", ParseLimits::new(3, 8)).unwrap_err();
    assert_eq!(d.code(), Code::LimitFileTooLarge);
}

#[test]
fn depth_limit_at_and_one_above_for_arrays_and_objects() {
    for depth in [1usize, 2, 7, 64] {
        let l = ParseLimits::new(1 << 20, u32::try_from(depth).unwrap());
        let arrays = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
        let objects = format!("{}1{}", "{\"k\":".repeat(depth), "}".repeat(depth));
        assert!(parse(&id(), arrays.as_bytes(), l).is_ok(), "{depth}");
        assert!(parse(&id(), objects.as_bytes(), l).is_ok(), "{depth}");
        let over_arrays = format!("[{arrays}]");
        let d = *parse(&id(), over_arrays.as_bytes(), l).unwrap_err();
        assert_eq!(d.code(), Code::LimitNestingTooDeep, "{depth}");
        // The container that exceeded the limit is the one after `depth`
        // openers.
        assert_eq!(at(&d), (1, u32::try_from(depth).unwrap() + 1));
        assert!(d.message().contains(&format!("limit of {depth}")));
        assert!(d.message().contains(&format!("{} deep", depth + 1)));
        let over_objects = format!("{{\"k\":{objects}}}");
        let d = *parse(&id(), over_objects.as_bytes(), l).unwrap_err();
        assert_eq!(d.code(), Code::LimitNestingTooDeep, "{depth}");
    }
    // Depth counts open containers, not total containers.
    let l = ParseLimits::new(1 << 20, 2);
    assert!(parse(&id(), b"[[],[],[[]][]", l).is_err());
    assert!(parse(&id(), b"[[],[],[]]", l).is_ok());
    // A scalar document has depth zero.
    assert!(parse(&id(), b"1", ParseLimits::new(8, 0)).is_ok());
    let d = *parse(&id(), b"[]", ParseLimits::new(8, 0)).unwrap_err();
    assert_eq!(d.code(), Code::LimitNestingTooDeep);
}

#[test]
fn writers_are_canonical_and_reparse() {
    let text = "{\n \"b\" : [ 1 , \"a\\u0001\\u007f\\/é\" , -0.50 , null ] ,\n \"a\" : { } }";
    let v = ok(text);
    // Member order is document order, not sorted.
    assert_eq!(
        write_compact(&v),
        "{\"b\":[1,\"a\\u0001\\u007f/é\",-0.50,null],\"a\":{}}"
    );
    assert_eq!(
        write_pretty(&v),
        "{\n  \"b\": [\n    1,\n    \"a\\u0001\\u007f/é\",\n    -0.50,\n    null\n  ],\n  \"a\": {}\n}\n"
    );
    for form in [write_compact(&v), write_pretty(&v)] {
        let again = ok(&form);
        assert_eq!(write_compact(&again), write_compact(&v));
    }
    // Every Unicode Cc character is escaped, including DEL and the C1 range.
    assert_eq!(
        write_compact(&Value::string("\u{7f}\u{85}")),
        "\"\\u007f\\u0085\""
    );
    // Escapes: only the short forms for quote, backslash, LF, CR, TAB; \u00XX
    // for other controls.
    let s = Value::string("\"\\\n\r\t\u{8}\u{c}\u{1f}");
    assert_eq!(
        write_compact(&s),
        "\"\\\"\\\\\\n\\r\\t\\u0008\\u000c\\u001f\""
    );
}
