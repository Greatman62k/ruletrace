//! Guards the copied glue code of the two Workflow_Modules until it has one
//! home (review 9.6 F1, RN-52): the copies must stay identical, so a change
//! to one copy fails here until the other is changed the same way.
//!
//! Also checks that every `tests_*` module of both crates is compiled only
//! under `cfg(test)`, which the engine-call scans rely on (review 9.6 F5).

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};

const COPIED: [&str; 2] = ["mapping.rs", "classify.rs"];

fn src(krate: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(krate)
        .join("src")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_copied_files_are_byte_identical_in_both_workflow_modules() {
    for name in COPIED {
        let prize = read(&src("prize-check").join(name));
        let bracket = read(&src("bracket-check").join(name));
        assert!(
            prize == bracket,
            "{name} differs between prize-check and bracket-check; change both or give the code one home (RN-52)"
        );
    }
}

/// The `mod tests_*;` declarations of `lib.rs`, each with whether the
/// preceding non-blank line is `#[cfg(test)]`.
fn test_modules(lib: &str) -> Vec<(String, bool)> {
    let lines: Vec<&str> = lib.lines().map(str::trim).collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(rest) = line.strip_prefix("mod tests") else {
            continue;
        };
        let name = format!("tests{}", rest.trim_end_matches(';'));
        let guarded = lines[..i]
            .iter()
            .rev()
            .find(|l| !l.is_empty())
            .is_some_and(|l| *l == "#[cfg(test)]");
        out.push((name, guarded));
    }
    out
}

#[test]
fn every_test_module_of_both_crates_is_compiled_only_under_cfg_test() {
    for krate in ["prize-check", "bracket-check"] {
        let lib = read(&src(krate).join("lib.rs"));
        let modules = test_modules(&lib);
        assert!(!modules.is_empty(), "{krate}: no test module found");
        for (name, guarded) in modules {
            assert!(guarded, "{krate}: module {name} is not under #[cfg(test)]");
        }
        for entry in std::fs::read_dir(src(krate)).unwrap() {
            let file = entry.unwrap().file_name().to_string_lossy().into_owned();
            if let Some(stem) = file
                .strip_prefix("tests_")
                .and_then(|f| f.strip_suffix(".rs"))
            {
                assert!(
                    lib.contains(&format!("mod tests_{stem};")),
                    "{krate}: {file} is not declared in lib.rs"
                );
            }
        }
    }
}

#[test]
fn the_cfg_test_check_detects_an_unguarded_module() {
    let lib = "#[cfg(test)]\nmod tests_a;\n\nmod tests_b;\n";
    assert_eq!(
        test_modules(lib),
        vec![("tests_a".to_owned(), true), ("tests_b".to_owned(), false)]
    );
}
