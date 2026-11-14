//! The golden-file comparison harness. Bytes are compared exactly: one byte, one line ending or
//! one trailing space is a failure. A failure names the golden file, the platform and the first
//! differing offset.

use std::path::{Path, PathBuf};

/// The `tests/fixtures` directory of the workspace.
pub fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// The platform name: the CI matrix label when set, otherwise the operating system and architecture.
pub fn platform() -> String {
    match std::env::var("RULETRACE_CI_PLATFORM") {
        Ok(p) if !p.is_empty() => p,
        _ => format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
    }
}

/// Compares `actual` with the bytes of the golden file `golden`.
pub fn compare_bytes(golden: &Path, actual: &[u8]) -> Result<(), String> {
    let expected = std::fs::read(golden).map_err(|e| {
        format!(
            "golden file {} (platform {}) cannot be read: {e}",
            golden.display(),
            platform()
        )
    })?;
    if expected == actual {
        return Ok(());
    }
    let offset = expected
        .iter()
        .zip(actual)
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| expected.len().min(actual.len()));
    Err(format!(
        "golden file {} differs on platform {}: first difference at byte offset {offset} (expected {} bytes, actual {} bytes)",
        golden.display(),
        platform(),
        expected.len(),
        actual.len()
    ))
}

/// Every file below `dir`, in sorted order.
pub fn files_below(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut entries: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => return out,
    };
    entries.sort();
    for p in entries {
        if p.is_dir() {
            out.extend(files_below(&p));
        } else {
            out.push(p);
        }
    }
    out
}

/// The first credential-looking string of `text`, if any.
pub fn credential_like(text: &str) -> Option<String> {
    if let Some(i) = text.find("PRIVATE KEY-----") {
        return Some(text[i..i + "PRIVATE KEY-----".len()].to_owned());
    }
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .find(|t| {
            (t.starts_with("sk-") && t.len() >= 20)
                || (t.starts_with("ghp_") && t.len() >= 20)
                || (t.starts_with("github_pat_") && t.len() >= 30)
                || (t.starts_with("AKIA")
                    && t.len() == 20
                    && t.chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
        })
        .map(str::to_owned)
}

#[test]
fn golden_harness_names_file_platform_and_offset() {
    let dir = std::env::temp_dir().join(format!("rt-golden-harness-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("expected.txt");
    std::fs::write(&file, b"abc\n").unwrap();
    assert!(compare_bytes(&file, b"abc\n").is_ok());
    let msg = compare_bytes(&file, b"abd\n").unwrap_err();
    assert!(msg.contains("expected.txt") && msg.contains(&platform()));
    assert!(msg.contains("byte offset 2"), "{msg}");
    assert!(compare_bytes(&file, b"abc\r\n")
        .unwrap_err()
        .contains("byte offset 3"));
    assert!(compare_bytes(&file, b"abc\n ")
        .unwrap_err()
        .contains("byte offset 4"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn golden_credential_scanner_detects_credential_looking_strings() {
    assert!(credential_like("key sk-abcdefghijklmnopqrstuvwx").is_some());
    assert!(credential_like("AKIAABCDEFGHIJKLMNOP").is_some());
    assert!(credential_like("-----BEGIN RSA PRIVATE KEY-----").is_some());
    assert!(credential_like("# Spring Open Rules\n1. A competitor is eligible").is_none());
}

#[test]
fn golden_fixtures_hold_no_credentials_and_record_provenance() {
    let root = fixtures_root();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "no fixtures under {}", root.display());
    for dir in dirs {
        assert!(
            dir.join("PROVENANCE.md").is_file(),
            "fixture {} records no source and permission (PROVENANCE.md)",
            dir.display()
        );
        for f in files_below(&dir) {
            if let Ok(text) = std::fs::read_to_string(&f) {
                assert_eq!(
                    credential_like(&text),
                    None,
                    "credential-looking string in {} (platform {})",
                    f.display(),
                    platform()
                );
            }
        }
    }
}
