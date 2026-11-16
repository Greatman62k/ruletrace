//! No test may reach a real host (safety rule after a test once made a real
//! HTTPS call with a fake credential).
//!
//! Every test source that can build a hosted adapter, call one or run the
//! binary (it is an integration test or names one of `REACHING`) may name only loopback hosts in an
//! `https://` literal, except for the listed literals. The other test
//! sources are pure configuration or parsing tests that never reach an
//! adapter; the runtime guard covers a mistake there. Each listed literal never reaches an adapter: it is a template
//! text that the test replaces with a loopback stub endpoint, or a value that
//! configuration validation refuses before any adapter is built. The unit
//! tests of `rt-ai-tool` back this scan with a runtime guard in
//! `hosted::responses_adapter` (see `tests_network_guard.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

/// A host literal that a test source may contain, with the reason.
struct Allowed {
    file: &'static str,
    host: &'static str,
    reason: &'static str,
}

const ALLOWED: [Allowed; 5] = [
    Allowed {
        file: "crates/rt-ai-tool/src/tests_e2e_responses.rs",
        host: "api.meta.ai",
        reason: "template text replaced by the loopback stub endpoint",
    },
    Allowed {
        file: "crates/rt-ai-tool/src/tests_e2e_responses.rs",
        host: "evil.example",
        reason: "refused by RT-AI-0013 before an adapter exists; asserts zero connections",
    },
    Allowed {
        file: "crates/rt-ai-tool/src/tests_cli_rig.rs",
        host: "api.openai.com",
        reason: "default fixture configuration; tests that send replace it or use a script",
    },
    Allowed {
        file: "crates/rt-ai-tool/src/tests_cli_order.rs",
        host: "api",
        reason: "prefix of a replacement that turns the default endpoint into http; providers are test doubles",
    },
    Allowed {
        file: "crates/rt-demo/src/tests.rs",
        host: "x",
        reason: "a document address in a manifest, not an endpoint",
    },
];

/// Names that give a test source a way to reach a provider.
const REACHING: [&str; 7] = [
    "BuiltInProviders",
    "responses_adapter",
    "ResponsesAdapter",
    "run_providers",
    "CARGO_BIN_EXE",
    "Command::new",
    "list_models(",
];

/// Integration test files build real adapters by construction; unit test
/// files are scanned when they name one of `REACHING`.
fn reaches_a_provider(file: &Path, text: &str) -> bool {
    file.components().any(|c| c.as_os_str() == "tests")
        || REACHING.iter().any(|name| text.contains(name))
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.map(|e| e.unwrap().path()) {
        if entry.is_dir() {
            rust_files(&entry, out);
        } else if entry.extension().is_some_and(|e| e == "rs") {
            out.push(entry);
        }
    }
}

/// Hosts of every `https://` literal in `text`, without port, path or
/// format placeholders.
fn https_hosts(text: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    for part in text.split("https://").skip(1) {
        let end = part
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-')))
            .unwrap_or(part.len());
        if end > 0 {
            hosts.push(part[..end].to_ascii_lowercase());
        }
    }
    hosts
}

fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|a| a.is_loopback())
}

fn scanned_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in [
        "crates/rt-ai-tool/src",
        "crates/rt-ai-tool/tests",
        "crates/rt-ai-responses/src",
        "crates/rt-ai-responses/tests",
        "crates/rt-demo/src",
        "crates/rt-tests-ai",
    ] {
        rust_files(&root.join(dir), &mut files);
    }
    files.retain(|f| {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        let in_tests_dir = f.components().any(|c| c.as_os_str() == "tests");
        name.starts_with("tests") || in_tests_dir
    });
    files.sort();
    files
}

#[test]
fn test_sources_name_only_loopback_hosts_apart_from_the_listed_literals() {
    let root = root();
    let files = scanned_files(&root);
    assert!(
        files.len() > 40,
        "the scan found only {} files",
        files.len()
    );
    let mut findings = Vec::new();
    for file in &files {
        let relative = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        // The scan itself holds the literal "https://" only as a delimiter.
        if relative.ends_with("network_guard.rs") {
            continue;
        }
        let text = std::fs::read_to_string(file).unwrap();
        if !reaches_a_provider(file, &text) {
            continue;
        }
        for host in https_hosts(&text) {
            let listed = ALLOWED.iter().any(|a| a.file == relative && a.host == host);
            if !is_loopback(&host) && !listed {
                findings.push(format!("{relative}: https://{host}"));
            }
        }
    }
    findings.sort();
    findings.dedup();
    assert_eq!(
        findings,
        Vec::<String>::new(),
        "unlisted non-loopback hosts"
    );
}

#[test]
fn every_listed_literal_still_occurs_and_has_a_reason() {
    let root = root();
    for allowed in &ALLOWED {
        assert!(!allowed.reason.is_empty());
        let text = std::fs::read_to_string(root.join(allowed.file)).unwrap();
        assert!(
            https_hosts(&text).iter().any(|h| h == allowed.host),
            "stale allowance {} {}",
            allowed.file,
            allowed.host
        );
    }
}

#[test]
fn the_scan_covers_the_files_that_reach_a_provider() {
    let root = root();
    let reaching: Vec<String> = scanned_files(&root)
        .iter()
        .filter(|f| reaches_a_provider(f, &std::fs::read_to_string(f).unwrap()))
        .map(|f| {
            f.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    for expected in [
        "crates/rt-ai-tool/src/tests_e2e_responses.rs",
        "crates/rt-ai-tool/src/tests_e2e_models.rs",
        "crates/rt-ai-tool/src/tests_exit_order_property.rs",
        "crates/rt-ai-responses/tests/stub_http.rs",
        "crates/rt-ai-responses/tests/tls_verification.rs",
    ] {
        assert!(
            reaching.iter().any(|f| f == expected),
            "{expected} not scanned"
        );
    }
}

#[test]
fn the_scan_detects_a_real_host_and_ignores_loopback() {
    let text = r#"a "https://api.openai.com/v1" b "https://127.0.0.1:{}" c "https://LOCALHOST""#;
    assert_eq!(
        https_hosts(text),
        ["api.openai.com", "127.0.0.1", "localhost"]
    );
    assert!(!is_loopback("api.openai.com"));
    assert!(is_loopback("127.0.0.1") && is_loopback("localhost"));
    assert!(!is_loopback("127.0.0.1.evil.example"));
    assert!(!is_loopback("localhost.evil.example"));
}
