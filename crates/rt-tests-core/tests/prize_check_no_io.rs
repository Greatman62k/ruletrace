//! The `prize-check` library performs no file, network, clock, random,
//! environment or subprocess access, has no binary target, depends only on the
//! crates of its line in `tools/allowed-edges.v1`, and reaches the engine
//! through exactly one call (Req 31.2, 31.3, 31.11, 24.12).
//!
//! The scan reads the crate's sources with a small lexer that drops comments
//! and the contents of string and character literals, so a forbidden name in
//! prose does not count and one in code does. This is the minimum check of
//! task 9.5. The full `check-app-layers` scan belongs to task 9.8.
//!
//! **Validates: Requirements 31.2, 31.3, 31.11**

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};

/// Identifiers that name file, network, clock, random, environment, thread or
/// subprocess access.
const FORBIDDEN: [&str; 17] = [
    "fs",
    "env",
    "process",
    "net",
    "time",
    "thread",
    "Instant",
    "SystemTime",
    "File",
    "OpenOptions",
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "Command",
    "rand",
    "getrandom",
    "option_env",
];

/// Compile-time file inclusion is allowed only in test files.
const COMPILE_TIME_READS: [&str; 3] = ["include_str", "include_bytes", "include"];

const ALLOWED_DEPENDENCIES: [&str; 7] = [
    "rt-foundation",
    "rt-numeric",
    "rt-data",
    "rt-compiled",
    "rt-engine",
    "rt-explain",
    "rt-srcmodel",
];

fn crate_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../prize-check")
}

/// The identifiers of `source` outside comments and literals, in order.
fn identifiers(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let at = |k: usize| chars.get(k).copied().unwrap_or('\0');
    while i < chars.len() {
        let c = at(i);
        if c == '/' && at(i + 1) == '/' {
            while i < chars.len() && at(i) != '\n' {
                i += 1;
            }
        } else if c == '/' && at(i + 1) == '*' {
            let mut depth = 1;
            i += 2;
            while i < chars.len() && depth > 0 {
                if at(i) == '/' && at(i + 1) == '*' {
                    depth += 1;
                    i += 2;
                } else if at(i) == '*' && at(i + 1) == '/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else if c == '"' {
            i += 1;
            while i < chars.len() && at(i) != '"' {
                i += if at(i) == '\\' { 2 } else { 1 };
            }
            i += 1;
        } else if (c == 'r' || (c == 'b' && at(i + 1) == 'r'))
            && raw_string_hashes(&chars, i).is_some()
        {
            let hashes = raw_string_hashes(&chars, i).unwrap();
            let start = i + usize::from(c == 'b') + 1 + hashes + 1;
            i = start;
            while i < chars.len() {
                if at(i) == '"' && (1..=hashes).all(|k| at(i + k) == '#') {
                    i += 1 + hashes;
                    break;
                }
                i += 1;
            }
        } else if c == '\'' {
            if at(i + 1) == '\\' {
                i += 2;
                while i < chars.len() && at(i) != '\'' {
                    i += 1;
                }
                i += 1;
            } else if at(i + 2) == '\'' {
                i += 3;
            } else {
                i += 1;
            }
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while at(i).is_ascii_alphanumeric() || at(i) == '_' {
                i += 1;
            }
            out.push(chars[start..i].iter().collect());
        } else {
            i += 1;
        }
    }
    out
}

/// For a raw string starting at `i` (`r"`, `r#"`, `br"`), the number of `#`.
fn raw_string_hashes(chars: &[char], i: usize) -> Option<usize> {
    let at = |k: usize| chars.get(k).copied().unwrap_or('\0');
    let mut k = i + usize::from(at(i) == 'b') + 1;
    let mut hashes = 0;
    while at(k) == '#' {
        hashes += 1;
        k += 1;
    }
    (at(k) == '"').then_some(hashes)
}

fn violations(file: &str, source: &str, is_test_file: bool) -> Vec<String> {
    let mut found = Vec::new();
    for ident in identifiers(source) {
        let compile_time = COMPILE_TIME_READS.contains(&ident.as_str());
        if FORBIDDEN.contains(&ident.as_str()) || (compile_time && !is_test_file) {
            found.push(format!("{file}: `{ident}`"));
        }
    }
    found
}

fn sources() -> Vec<(String, String)> {
    let dir = crate_dir().join("src");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read_to_string(&path).unwrap())
        })
        .collect();
    out.sort();
    out
}

#[test]
fn no_source_file_names_file_network_clock_random_environment_or_process_access() {
    let files = sources();
    assert!(files.len() >= 8, "the scan must see the crate's sources");
    let mut found = Vec::new();
    for (name, text) in &files {
        found.extend(violations(name, text, name.starts_with("tests_")));
    }
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_scanner_flags_code_and_ignores_comments_and_literals() {
    // One negative fixture per forbidden family, each in code.
    for code in [
        "fn f() { let _ = std::fs::read(\"x\"); }",
        "use std::env;",
        "use std::{io, process};",
        "fn f() { std::net::TcpStream::connect(\"h\"); }",
        "fn f() { let _ = std::time::Instant::now(); }",
        "fn f() { std::thread::spawn(|| ()); }",
        "fn f() { let _ = std::process::Command::new(\"x\"); }",
        "fn f() { let _ = option_env!(\"X\"); }",
        "const T: &str = include_str!(\"x\");",
    ] {
        assert!(
            !violations("fixture.rs", code, false).is_empty(),
            "not flagged: {code}"
        );
    }
    // The same names in prose, strings and characters are not code.
    for prose in [
        "// std::fs::read is not used here\nfn f() {}",
        "/* std::env /* nested std::process */ */ fn f() {}",
        "/// Calls std::net.\nfn f() {}",
        "const S: &str = \"std::fs::read\";",
        "const S: &str = r#\"std::env \"quoted\" \"#;",
        "fn f<'a>(x: &'a str) -> char { '\\n' }",
    ] {
        assert_eq!(
            violations("fixture.rs", prose, false),
            Vec::<String>::new(),
            "flagged: {prose}"
        );
    }
    // Compile-time inclusion is allowed in test files only.
    let include = "const T: &str = include_str!(\"x\");";
    assert!(violations("tests_x.rs", include, true).is_empty());
}

#[test]
fn the_package_is_a_library_with_only_the_dependencies_of_its_allowed_edges_line() {
    let dir = crate_dir();
    for absent in ["src/main.rs", "src/io.rs", "src/bin", "build.rs"] {
        assert!(!dir.join(absent).exists(), "{absent} must not exist");
    }
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    let tables: Vec<&str> = manifest
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('['))
        .collect();
    assert_eq!(tables, vec!["[package]", "[lints]", "[dependencies]"]);
    let dependencies: Vec<&str> = manifest
        .split("[dependencies]")
        .nth(1)
        .unwrap()
        .lines()
        .filter_map(|l| l.split('=').next())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();
    for name in &dependencies {
        assert!(ALLOWED_DEPENDENCIES.contains(name), "{name} is not allowed");
    }
    // The line of the allowed-edges file names the same set (or a superset).
    let edges = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/allowed-edges.v1"),
    )
    .unwrap();
    let line = edges
        .lines()
        .find(|l| l.starts_with("crate prize-check "))
        .unwrap();
    for name in &dependencies {
        assert!(line.contains(name), "{name} is not on the edges line");
    }
}

#[test]
fn the_engine_is_called_in_exactly_one_place() {
    let mut calls = Vec::new();
    for (name, text) in sources() {
        let idents = identifiers(&text);
        for window in idents.windows(2) {
            if window[0] == "rt_engine" && window[1] == "evaluate" {
                calls.push(name.clone());
            }
        }
    }
    assert_eq!(calls, vec!["evaluate.rs".to_owned()]);
}
