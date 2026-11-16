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

const ALLOWED: [Allowed; 6] = [
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
        file: "crates/rt-tests-ai/tests/live_ci_config.rs",
        host: "api.openai.com",
        reason: "expected text of a --dry-run announcement and of the CI step's endpoint; nothing is sent",
    },
    Allowed {
        file: "crates/rt-demo/src/tests.rs",
        host: "x",
        reason: "a document address in a manifest, not an endpoint",
    },
];

/// Names that give a test source a way to reach a provider.
const REACHING: [&str; 10] = [
    "BuiltInProviders",
    "responses_adapter",
    "ResponsesAdapter",
    "run_providers",
    "CARGO_BIN_EXE",
    "Command::new",
    "list_models(",
    "rt_ai_tool::run",
    "crate::run(",
    "run_with(",
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

/// Hosts of every `https://` and `http://` literal in `text`, without port,
/// path, brackets or format placeholders. A bracketed host is an IPv6
/// literal and is read up to the closing bracket.
fn url_hosts(text: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    for scheme in ["https://", "http://"] {
        for part in text.split(scheme).skip(1) {
            let host = match part.strip_prefix('[') {
                Some(inner) => inner.split_once(']').map_or("", |(host, _)| host),
                None => {
                    let end = part
                        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-')))
                        .unwrap_or(part.len());
                    &part[..end]
                }
            };
            if !host.is_empty() {
                hosts.push(host.to_ascii_lowercase());
            }
        }
    }
    hosts
}

/// `localhost`, 127.0.0.0/8 and `::1`. Every other literal, including a
/// documentation or private IP address, counts as a real host.
fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|a| a.is_loopback())
        || host
            .parse::<std::net::Ipv6Addr>()
            .is_ok_and(|a| a.is_loopback())
}

/// The unlisted non-loopback hosts that `text` of the file `relative`
/// names.
fn findings_in(relative: &str, text: &str) -> Vec<String> {
    url_hosts(text)
        .into_iter()
        .filter(|host| !is_loopback(host))
        .filter(|host| !ALLOWED.iter().any(|a| a.file == relative && a.host == host))
        .map(|host| format!("{relative}: {host}"))
        .collect()
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
            .replace('\\', "/");
        // The scan itself holds the literal "https://" only as a delimiter.
        if relative.ends_with("network_guard.rs") {
            continue;
        }
        let text = std::fs::read_to_string(file).unwrap();
        if !reaches_a_provider(file, &text) {
            continue;
        }
        findings.extend(findings_in(&relative, &text));
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
            url_hosts(&text).iter().any(|h| h == allowed.host),
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
                .replace('\\', "/")
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
        url_hosts(text),
        ["api.openai.com", "127.0.0.1", "localhost"]
    );
    assert!(!is_loopback("api.openai.com"));
    assert!(is_loopback("127.0.0.1") && is_loopback("localhost"));
    assert!(!is_loopback("127.0.0.1.evil.example"));
    assert!(!is_loopback("localhost.evil.example"));
}

// Negative fixture: the probes of the wave 4 review must be found, and the
// loopback spellings next to them must not be.
#[test]
fn the_scan_finds_ip_literals_that_are_not_loopback() {
    let probes = [
        ("https://[2001:db8::1]", "2001:db8::1"),
        ("https://[2001:db8::1]:8443/v1", "2001:db8::1"),
        ("http://192.0.2.1", "192.0.2.1"),
        ("https://203.0.113.9:443", "203.0.113.9"),
        ("http://10.0.0.1/x", "10.0.0.1"),
        ("http://[::ffff:127.0.0.1]", "::ffff:127.0.0.1"),
        ("http://[::]", "::"),
        ("http://0.0.0.0", "0.0.0.0"),
    ];
    for (literal, host) in probes {
        let text = format!(r#"let url = "{literal}";"#);
        assert_eq!(
            findings_in("probe.rs", &text),
            [format!("probe.rs: {host}")],
            "{literal}"
        );
    }
    for literal in [
        "https://[::1]:9",
        "http://[0:0:0:0:0:0:0:1]",
        "http://127.0.0.2",
        "https://LOCALHOST",
    ] {
        let text = format!(r#"let url = "{literal}";"#);
        assert_eq!(
            findings_in("probe.rs", &text),
            Vec::<String>::new(),
            "{literal}"
        );
    }
}
