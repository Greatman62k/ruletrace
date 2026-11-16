//! Every endpoint that configuration validation accepts must build an
//! adapter, and every endpoint it refuses must be RT-AI-0004 (wave 3
//! issue 7, wave 4 blocking 2). Expectations are written by hand from the
//! loopback definition of Req 28.6: `localhost` in any case, 127.0.0.0/8
//! and the IPv6 address ::1 in any spelling that `std` parses to it.

use crate::tests_e2e_models::run_models;
use crate::tests_e2e_responses::{errors, exit, Rig, DEFAULT};

const ACCEPTED: [&str; 12] = [
    "http://[::1]:9",
    "http://[0:0:0:0:0:0:0:1]:9",
    "http://[0::1]:9",
    "http://[::0:1]:9",
    "http://[0000:0000:0000:0000:0000:0000:0000:0001]:9",
    "http://[::1]:9/v1x",
    "http://127.0.0.2:9",
    "http://127.255.255.254:9",
    "http://localhost:9",
    "http://LOCALHOST:9",
    "http://LocalHost:9",
    "http://[0::0:1]:9",
];

const REFUSED: [&str; 12] = [
    "http://[::ffff:127.0.0.1]:9",
    "http://[::]:9",
    "http://127.1:9",
    "http://localhost.:9",
    "http://127.0.0.01:9",
    "http://127.00.0.1:9",
    "http://[::2]:9",
    "http://[0:0:0:0:0:0:0:2]:9",
    "http://0.0.0.0:9",
    "http://192.0.2.1:9",
    "http://[2001:db8::1]:9",
    "http://localhost.evil.example:9",
];

#[test]
fn every_accepted_spelling_builds_an_adapter_and_fails_only_at_the_wire() {
    for endpoint in ACCEPTED {
        let rig = Rig::new("agree-ok", endpoint, &DEFAULT);
        let ran = run_models(&rig, &[]);
        let found = errors(&ran);
        assert_eq!(found, ["RT-AI-0109"], "{endpoint}: {}", ran.stderr);
        assert!(!ran.stderr.contains("RT-CLI-0006"), "{endpoint}");
        assert_eq!(exit(ran.code), 5, "{endpoint}: {}", ran.stderr);
    }
}

#[test]
fn every_refused_spelling_is_rt_ai_0004_with_exit_2() {
    for endpoint in REFUSED {
        let rig = Rig::new("agree-refused", endpoint, &DEFAULT);
        let ran = run_models(&rig, &[]);
        assert_eq!(errors(&ran), ["RT-AI-0004"], "{endpoint}: {}", ran.stderr);
        assert_eq!(exit(ran.code), 2, "{endpoint}: {}", ran.stderr);
    }
}

// Validation and the adapter must agree on 43 spellings. The expected
// verdicts are hand-written, so the test does not derive them from either
// implementation.
#[test]
fn validation_and_the_adapter_endpoint_agree_on_every_spelling() {
    let table: [(&str, bool); 43] = [
        ("http://localhost", true),
        ("http://LOCALHOST", true),
        ("http://localhost:9", true),
        ("http://localhost.", false),
        ("http://localhost.localdomain", false),
        ("http://127.0.0.1", true),
        ("http://127.0.0.2:9", true),
        ("http://127.255.255.255", true),
        ("http://127.1", false),
        ("http://127.0.1", false),
        ("http://127.0.0.01", false),
        ("http://127.00.0.1", false),
        ("http://0x7f.0.0.1", false),
        ("http://2130706433", false),
        ("http://128.0.0.1", false),
        ("http://126.255.255.255", false),
        ("http://0.0.0.0", false),
        ("http://192.0.2.1", false),
        ("http://[::1]", true),
        ("http://[0:0:0:0:0:0:0:1]", true),
        ("http://[0::1]", true),
        ("http://[::0:1]", true),
        ("http://[0000:0000:0000:0000:0000:0000:0000:0001]", true),
        ("http://[::0001]", true),
        ("http://[::ffff:127.0.0.1]", false),
        ("http://[::127.0.0.1]", false),
        ("http://[::]", false),
        ("http://[::2]", false),
        ("http://[1::1]", false),
        ("http://[fe80::1]", false),
        ("http://[2001:db8::1]", false),
        ("http://[::1%25lo]", false),
        ("http://[::1", false),
        ("http://::1", false),
        ("http://127.0.0.1\u{200b}", false),
        ("http://127.0.0.1/\u{202e}", false),
        ("http://local\u{feff}host", false),
        ("http://127.0.0.1/a b", false),
        ("http://127.0.0.1\\x", false),
        ("http://127.0.0.1//", true),
        ("http://127.0.0.1:9#f", false),
        ("http://LOCALHOST##", false),
        ("https://127.0.0.1:001271", false),
    ];
    for (url, loopback) in table {
        let tool = crate::endpoint::check(url, None).is_ok();
        let adapter = rt_ai_responses::Endpoint::parse(url).is_ok();
        assert_eq!(tool, loopback, "tool validation of {url}");
        assert_eq!(adapter, loopback, "adapter endpoint of {url}");
    }
}
