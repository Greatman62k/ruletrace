//! Properties of the version 2 configuration checks (task 10.13).
//!
//! The expectations come from how each input is built, never from the code
//! under test: a host is generated as "official", "loopback" or "other by
//! construction", and a fault set is generated as twelve independent
//! switches whose earliest switch decides the result (Req 28.17 as amended
//! by Req 35).

use crate::tests::{at, one, run, MapEnv, CANARY};
use crate::tests_config_v2::{meta_env, v2, V2};
use crate::{load, Consent};
use proptest::prelude::*;
use rt_foundation::{CliLocation, Code, DocumentId, ExitCode, InputId, Location, Severity};
use std::collections::BTreeSet;

const OFFICIAL: &str = "api.meta.ai";
const LOOPBACKS: [&str; 6] = [
    "127.0.0.1",
    "127.5.6.7",
    "127.255.255.254",
    "localhost",
    "LOCALHOST",
    "[::1]",
];

#[derive(Clone, Debug)]
enum Host {
    Official {
        mask: Vec<bool>,
        port: Option<u16>,
        path: usize,
    },
    Loopback {
        which: usize,
        port: Option<u16>,
        tls: bool,
    },
    Other(String),
}

fn cased(mask: &[bool]) -> String {
    OFFICIAL
        .chars()
        .zip(mask.iter().cycle())
        .map(|(c, up)| if *up { c.to_ascii_uppercase() } else { c })
        .collect()
}

// Hosts that differ from the official host and from every loopback form by
// construction.
fn other_host() -> impl Strategy<Value = String> {
    (
        0usize..8,
        "[a-z][a-z0-9]{0,9}",
        0usize..OFFICIAL.len(),
        (1u8..127, 0u8..=255, 0u8..=255, 1u8..=254),
    )
        .prop_map(|(shape, label, index, (a, b, c, d))| match shape {
            0 => format!("{label}.example.test"),
            1 => format!("{OFFICIAL}.{label}"),
            2 => format!("{label}.{OFFICIAL}"),
            3 => format!("{OFFICIAL}."),
            4 => {
                // A different character at one position. Digits never equal
                // a letter or a dot of the official host.
                let mut chars: Vec<char> = OFFICIAL.chars().collect();
                chars[index] = '7';
                chars.into_iter().collect()
            }
            5 => {
                let mut chars: Vec<char> = OFFICIAL.chars().collect();
                chars.remove(index);
                chars.into_iter().collect()
            }
            6 => format!("{a}.{b}.{c}.{d}"),
            _ => format!("api-{label}.ai"),
        })
}

fn arb_host() -> impl Strategy<Value = Host> {
    prop_oneof![
        (
            proptest::collection::vec(any::<bool>(), OFFICIAL.len()),
            proptest::option::of(prop_oneof![Just(443u16), Just(8443u16), Just(1u16)]),
            0usize..3,
        )
            .prop_map(|(mask, port, path)| Host::Official { mask, port, path }),
        (
            0usize..LOOPBACKS.len(),
            proptest::option::of(1u16..=65535),
            any::<bool>(),
        )
            .prop_map(|(which, port, tls)| Host::Loopback { which, port, tls }),
        other_host().prop_map(Host::Other),
    ]
}

fn url(host: &Host) -> String {
    let port = |p: &Option<u16>| p.map_or(String::new(), |p| format!(":{p}"));
    match host {
        Host::Official {
            mask,
            port: p,
            path,
        } => {
            let path = ["", "/", "/v1"][*path];
            format!("https://{}{}{path}", cased(mask), port(p))
        }
        Host::Loopback {
            which,
            port: p,
            tls,
        } => {
            let scheme = if *tls { "https" } else { "http" };
            format!("{scheme}://{}{}", LOOPBACKS[*which], port(p))
        }
        Host::Other(h) => format!("https://{h}"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    // The credential can reach only the official host or a loopback
    // address: any generated host that is neither is refused with
    // RT-AI-0013 at the endpoint field, and the other two are accepted.
    #[test]
    fn the_host_restriction_accepts_only_the_official_host_and_loopback(host in arb_host()) {
        let text = v2("https://api.meta.ai", &url(&host));
        let result = run(&text, &meta_env());
        match host {
            Host::Other(_) => {
                let d = one(result, Code::AiConfigEndpointHostNotPermitted);
                prop_assert_eq!(
                    at(&d),
                    (Some(12), Some(1), Some("provider.endpoint".to_owned()))
                );
            }
            Host::Official { .. } => {
                let ok = result.map_err(|e| TestCaseError::fail(format!("{e:?}\n{text}")))?;
                prop_assert_eq!(ok.config.endpoint.unwrap().host, OFFICIAL);
            }
            Host::Loopback { .. } => {
                prop_assert!(result.is_ok(), "{:?}\n{}", result.err(), text);
            }
        }
    }
}

// Stage names, in the order of Req 28.17 as amended by Req 35.
const STAGES: [&str; 12] = [
    "syntax",
    "secret",
    "fields",
    "limits",
    "scheme",
    "host",
    "disabled",
    "pair",
    "guard",
    "capability",
    "consent",
    "credential",
];

fn command_line() -> Location {
    Location::Cli(CliLocation::Command("ruletrace-ai".to_owned()))
}

fn expectation(stage: usize, text_lines: usize) -> (Code, Location) {
    let input = |line: Option<u32>, column: Option<u32>, field: Option<&str>| {
        Location::Input(rt_foundation::InputLocation {
            input: InputId::new("ai.toml").unwrap(),
            line,
            column,
            field: field.map(str::to_owned),
        })
    };
    let line = |n: u32| Some(n);
    match stage {
        0 => (
            Code::AiConfigInvalid,
            input(line(u32::try_from(text_lines + 1).unwrap()), Some(5), None),
        ),
        1 => (
            Code::AiConfigInlineSecret,
            input(line(4), Some(1), Some("ai_identities")),
        ),
        2 => (
            Code::AiConfigInvalid,
            input(line(5), Some(1), Some("transmission_log")),
        ),
        3 => (
            Code::AiConfigLimitInvalid,
            input(line(19), Some(1), Some("limits.max_retries")),
        ),
        4 => (
            Code::AiConfigEndpointInsecure,
            input(line(12), Some(1), Some("provider.endpoint")),
        ),
        5 => (
            Code::AiConfigEndpointHostNotPermitted,
            input(line(12), Some(1), Some("provider.endpoint")),
        ),
        6 => (Code::AiDisabled, input(line(2), Some(1), Some("enabled"))),
        7 => (
            Code::AiProviderUnknown,
            input(line(10), Some(1), Some("provider.protocol")),
        ),
        8 => (
            Code::AiModelTrainingModeRefused,
            input(line(11), Some(1), Some("provider.model")),
        ),
        9 => (
            Code::AiCapabilityUnsupported,
            input(line(14), Some(1), Some("provider.reasoning_effort")),
        ),
        10 => (Code::AiConsentMissing, command_line()),
        _ => (
            Code::AiCredentialMissing,
            input(line(13), Some(1), Some("provider.credential_env_var")),
        ),
    }
}

fn faulty_text(on: &[bool]) -> String {
    let mut text = V2.to_owned();
    let swap = |text: &mut String, old: &str, new: &str| {
        assert!(text.contains(old), "{old}");
        *text = text.replace(old, new);
    };
    if on[1] {
        swap(&mut text, "\"muse-spark-1.3\"]", "\"sk-zzz\"]");
    }
    if on[2] {
        swap(
            &mut text,
            "transmission_log = \"tx.jsonl\"",
            "transmission_log = 5",
        );
    }
    if on[3] {
        swap(&mut text, "max_retries = 0", "max_retries = 3");
    }
    // The scheme fault names a host that the host fault would also refuse,
    // so the scheme fault must be the one reported when both are on.
    if on[4] {
        swap(&mut text, "https://api.meta.ai", "http://evil.example");
    } else if on[5] {
        swap(&mut text, "https://api.meta.ai", "https://evil.example");
    }
    if on[6] {
        swap(&mut text, "enabled = true", "enabled = false");
    }
    if on[7] {
        swap(
            &mut text,
            "protocol = \"openai-responses\"",
            "protocol = \"mock-replay\"",
        );
    }
    if on[8] {
        swap(
            &mut text,
            "model = \"muse-spark-1.3\"",
            "model = \"muse-spark-1.3-contributor\"",
        );
    }
    if on[9] {
        text.push_str("\n[capabilities]\nreasoning_effort = \"no\"\n");
    }
    if on[0] {
        let lines = text.lines().count();
        text.push_str("x = = 1\n");
        assert_eq!(lines, text.lines().count() - 1);
    }
    text
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    // With any subset of the twelve faults, only the earliest failing
    // check is reported, with its exact code and place, and the
    // environment is read only when every earlier check passed.
    #[test]
    fn only_the_earliest_failing_check_of_the_amended_order_is_reported(
        on in proptest::collection::vec(any::<bool>(), STAGES.len())
    ) {
        let text = faulty_text(&on);
        let env = if on[11] { MapEnv::new(&[("OTHER", CANARY)]) } else { meta_env() };
        let consent = if on[10] {
            Consent::absent(command_line())
        } else {
            Consent::named(BTreeSet::new(), command_line())
        };
        let docs: BTreeSet<DocumentId> = BTreeSet::new();
        let result = load(&InputId::new("ai.toml").unwrap(), &text, &consent, &docs, &env);
        match on.iter().position(|f| *f) {
            None => {
                let ok = result.map_err(|e| TestCaseError::fail(format!("{e:?}\n{text}")))?;
                let credential = ok.credential.unwrap();
                prop_assert_eq!(credential.expose(), CANARY);
                prop_assert_eq!(env.reads(), vec!["MODEL_API_KEY".to_owned()]);
            }
            Some(stage) => {
                let found = result.err().ok_or_else(|| TestCaseError::fail(format!("loaded: {text}")))?;
                let lines = text.lines().count() - usize::from(on[0]);
                let (code, place) = expectation(stage, lines);
                prop_assert_eq!(found.len(), 1, "{} {:?}\n{}", STAGES[stage], found, text);
                let d = &found[0];
                prop_assert_eq!(d.code(), code, "{}\n{}", STAGES[stage], text);
                prop_assert_eq!(d.severity(), Severity::Error);
                prop_assert_eq!(d.code().exit_code(), ExitCode::InvalidInput);
                prop_assert_eq!(d.primary(), &place, "{}\n{}", STAGES[stage], text);
                let shown = format!("{found:?}");
                prop_assert!(!shown.contains("sk-zzz") && !shown.contains(CANARY));
                let expected_reads = if stage == 11 { vec!["MODEL_API_KEY".to_owned()] } else { vec![] };
                prop_assert_eq!(env.reads(), expected_reads, "{}", STAGES[stage]);
            }
        }
    }
}
