//! Endpoint validation versus the adapter, on generated adversarial text
//! (wave 5 issues 1 to 3). Whatever the tool's validation accepts must
//! build an adapter endpoint, and `http` must never reach a host that is
//! not loopback. The loopback spellings are written by hand below, so the
//! oracle does not depend on either parser.

use proptest::sample::select;
use proptest::strategy::Strategy;
use proptest::test_runner::{Config, RngAlgorithm, TestError, TestRng, TestRunner};

const CASES: u32 = 512;
const SEED: [u8; 32] = *b"rt-ai-tool endpoint agreement 01";

const SCHEMES: [&str; 8] = ["http", "https", "HTTP", "Https", "HtTp", "ftp", "ws", ""];
const SEPARATORS: [&str; 4] = ["://", "://", ":/", ":///"];
const LOOPBACK_HOSTS: [&str; 9] = [
    "127.0.0.1",
    "127.255.255.254",
    "localhost",
    "LocalHost",
    "LOCALHOST",
    "[::1]",
    "[0:0:0:0:0:0:0:1]",
    "[0::1]",
    "[::0001]",
];
const OTHER_HOSTS: [&str; 18] = [
    "api.openai.com",
    "API.OpenAI.com",
    "api.openai.com.",
    "evil.example",
    "localhost.",
    "localhost.evil.example",
    "127.0.0.1.",
    "127.0.0.01",
    "127.1",
    "0x7f.0.0.1",
    "2130706433",
    "[::ffff:127.0.0.1]",
    "[::2]",
    "[::1",
    "::1",
    "",
    "192.0.2.1",
    "127\u{3002}0\u{3002}0\u{3002}1",
];
const USERINFO: [&str; 5] = ["", "", "", "u@", "u:p@"];
const PORTS: [&str; 12] = [
    "", "", ":9", ":65535", ":001271", ":00080", ":09", ":0", ":99999", ":+9", ":9:9", ":",
];
const PATHS: [&str; 9] = [
    "",
    "/",
    "//",
    "/v1",
    "/v1//",
    "/%2e%2e",
    "/a b",
    "/\u{200b}",
    "/a\\b",
];
const QUERIES: [&str; 4] = ["", "", "", "?q=1"];
const FRAGMENTS: [&str; 6] = ["", "", "", "#f", "##", "#"];
const NOISE: [&str; 9] = ["", "", "", "", " ", "\t", "\u{0}", "\n", "\u{feff}"];

#[derive(Clone, Debug)]
struct Parts {
    scheme: &'static str,
    host: &'static str,
    text: String,
}

fn parts() -> impl Strategy<Value = Parts> {
    let host = (0..2usize).prop_flat_map(|kind| {
        if kind == 0 {
            select(LOOPBACK_HOSTS.to_vec()).boxed()
        } else {
            select(OTHER_HOSTS.to_vec()).boxed()
        }
    });
    (
        (select(NOISE.to_vec()), select(SCHEMES.to_vec())),
        (select(SEPARATORS.to_vec()), select(USERINFO.to_vec()), host),
        (select(PORTS.to_vec()), select(PATHS.to_vec())),
        (select(QUERIES.to_vec()), select(FRAGMENTS.to_vec())),
        select(NOISE.to_vec()),
    )
        .prop_map(
            |((lead, scheme), (sep, user, host), (port, path), (query, fragment), tail)| Parts {
                scheme,
                host,
                text: format!("{lead}{scheme}{sep}{user}{host}{port}{path}{query}{fragment}{tail}"),
            },
        )
}

fn is_loopback_spelling(host: &str) -> bool {
    LOOPBACK_HOSTS.contains(&host)
}

/// The tool's complete endpoint verdict: the userinfo and query check of
/// Req 28.18, then the transport and host checks.
fn tool_accepts(raw: &str, official: Option<&str>) -> bool {
    !crate::endpoint::has_userinfo_or_query(raw) && crate::endpoint::check(raw, official).is_ok()
}

fn violation(
    case: &Parts,
    official: Option<&str>,
    accepts: &dyn Fn(&str, Option<&str>) -> bool,
) -> Result<(), String> {
    if !accepts(&case.text, official) {
        return Ok(());
    }
    if case.scheme.eq_ignore_ascii_case("http") && !is_loopback_spelling(case.host) {
        return Err(format!("http accepted for a remote host: {case:?}"));
    }
    if rt_ai_responses::Endpoint::parse(&case.text).is_err() {
        return Err(format!(
            "accepted by the tool, refused by the adapter: {case:?}"
        ));
    }
    Ok(())
}

fn run(accepts: &'static dyn Fn(&str, Option<&str>) -> bool) -> Result<(), String> {
    let config = Config {
        cases: CASES,
        failure_persistence: None,
        ..Config::default()
    };
    let rng = TestRng::from_seed(RngAlgorithm::ChaCha, &SEED);
    let mut runner = TestRunner::new_with_rng(config, rng);
    let strategy = (parts(), select(vec![None, Some("api.openai.com")]));
    let outcome = runner.run(&strategy, |(case, official)| {
        violation(&case, official, accepts).map_err(proptest::test_runner::TestCaseError::fail)
    });
    match outcome {
        Ok(()) => Ok(()),
        Err(TestError::Fail(reason, minimal)) => {
            Err(format!("seed {SEED:?}: {reason}; minimal case {minimal:?}"))
        }
        Err(other) => Err(format!("{other}")),
    }
}

#[test]
fn whatever_the_tool_accepts_builds_an_adapter_and_http_stays_on_loopback() {
    if let Err(message) = run(&tool_accepts) {
        panic!("{message}");
    }
}

// Mutant: validation that stops after the host check, as before the fix.
#[test]
fn the_property_detects_a_validator_that_ignores_the_adapter_rules() {
    fn lax(raw: &str, _official: Option<&str>) -> bool {
        crate::endpoint::parse(raw).is_some() && !crate::endpoint::has_userinfo_or_query(raw)
    }
    let message = run(&lax).expect_err("the lax validator must be caught");
    assert!(
        message.contains("accepted by the tool, refused by the adapter")
            || message.contains("http accepted for a remote host"),
        "{message}"
    );
}

// Mutant: http accepted for every host.
#[test]
fn the_property_detects_clear_text_to_a_remote_host() {
    fn open(raw: &str, _official: Option<&str>) -> bool {
        !crate::endpoint::has_userinfo_or_query(raw)
            && crate::endpoint::parse(raw).is_some_and(|p| p.scheme == "http" && !p.has_fragment)
    }
    let message = run(&open).expect_err("clear text to a remote host must be caught");
    assert!(
        message.contains("http accepted for a remote host"),
        "{message}"
    );
}

#[test]
fn the_generator_covers_every_class_of_input_it_claims() {
    let mut runner = TestRunner::new_with_rng(
        Config {
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::from_seed(RngAlgorithm::ChaCha, &SEED),
    );
    let strategy = parts();
    let mut texts = Vec::new();
    for _ in 0..CASES {
        let tree = strategy.new_tree(&mut runner).expect("a value");
        texts.push(proptest::strategy::ValueTree::current(&tree).text);
    }
    let any = |f: &dyn Fn(&str) -> bool| texts.iter().any(|t| f(t));
    assert!(any(&|t| t.contains('#')));
    assert!(any(&|t| t.contains(":001271")));
    assert!(any(&|t| t.contains('\u{200b}') || t.contains('\u{0}')));
    assert!(any(&|t| t.contains('@')));
    assert!(any(&|t| t.starts_with("ftp")));
    assert!(any(&|t| tool_accepts(t, None)));
    assert!(any(&|t| !tool_accepts(t, None)));
}
