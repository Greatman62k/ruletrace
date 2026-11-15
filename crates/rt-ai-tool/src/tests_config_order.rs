//! Property 38: configuration checks are ordered and total (Req 28.1, 28.3
//! to 28.7, 28.17, 28.18, 23.19).
//!
//! Each case chooses faults for the nine checks of Req 28.17 and builds the
//! TOML text, the consent and the environment from those choices. The
//! expected code and position come from the same choices, never from the
//! code under test. The expected failure is the earliest check with a fault.

use crate::{load, Consent, EnvReader, ProviderKind};
use proptest::prelude::*;
use rt_foundation::{
    CliLocation, Code, DocumentId, ExitCode, InputId, InputLocation, Location, Severity,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

const STAGES: usize = 9;
const NONE_FAILS: usize = STAGES;
const SECRET_NAMES: [&str; 6] = [
    "api_key",
    "Authorization",
    "my_TOKEN",
    "client_secret",
    "PassWord",
    "monkey",
];
const SECRET_PREFIXES: [&str; 5] = ["sk-", "sk_", "Bearer", "ghp_", "AKIA"];
const GOOD_ENDPOINTS: [&str; 9] = [
    "https://api.example.org",
    "HTTPS://API.EXAMPLE.ORG:8443/v1/responses",
    "http://localhost:8080/v1",
    "http://127.0.0.1",
    "http://127.200.3.4:1",
    "http://[::1]:9/x",
    "https://0.0.0.0",
    "https://[::]:443/",
    "Http://LocalHost",
];
const INSECURE_ENDPOINTS: [&str; 9] = [
    "http://example.org",
    "http://0.0.0.0:8080",
    "http://[::]/x",
    "http://[::]",
    "http://10.0.0.1",
    "http://localhost.evil.example",
    "HTTP://example.org",
    "http://127.0.0.1.evil.example",
    "ftp://example.org",
];
const MALFORMED_ENDPOINTS: [&str; 3] = ["example.org/path", "https://", "https:///x"];
const MISSING_FIELDS: [&str; 6] = [
    "provider",
    "model",
    "allow_document_text",
    "ai_identities",
    "transmission_log",
    "schema_version",
];

struct MapEnv {
    values: BTreeMap<String, String>,
    reads: RefCell<Vec<String>>,
}

impl EnvReader for MapEnv {
    fn get(&self, name: &str) -> Option<String> {
        self.reads.borrow_mut().push(name.to_owned());
        self.values.get(name).cloned()
    }
}

/// A line of the generated file, named before the final layout is known.
#[derive(Clone, Debug)]
enum Pos {
    Key(&'static str),
    Top(usize),
    Tail(usize),
}

#[derive(Clone, Debug)]
enum Where {
    At {
        pos: Option<Pos>,
        column: Option<u32>,
        field: Option<String>,
    },
    Exact(Location),
}

#[derive(Clone, Debug)]
struct Expect {
    code: Code,
    at: Where,
    needs_enabled: bool,
}

fn at(pos: Pos, column: u32, field: Option<&str>) -> Where {
    Where::At {
        pos: Some(pos),
        column: Some(column),
        field: field.map(str::to_owned),
    }
}

fn without_line(field: Option<&str>) -> Where {
    Where::At {
        pos: None,
        column: None,
        field: field.map(str::to_owned),
    }
}

#[derive(Clone, Debug)]
struct Case {
    mock: bool,
    first: usize,
    present: Vec<bool>,
    sel: Vec<usize>,
    sub: Vec<usize>,
    marker: String,
    cred_suffix: String,
    run_docs: BTreeSet<String>,
    cred_value: String,
    limits: Vec<usize>,
    good_endpoint: usize,
}

fn arb_case() -> impl Strategy<Value = Case> {
    (
        any::<bool>(),
        0..=NONE_FAILS,
        proptest::collection::vec(any::<bool>(), STAGES),
        proptest::collection::vec(0usize..64, STAGES),
        proptest::collection::vec(0usize..64, STAGES),
        "[a-z0-9]{6}",
        "[A-Z0-9_]{1,10}",
        proptest::collection::btree_set("[a-d]{1,2}", 0..4),
        "[a-f0-9]{8}",
        proptest::collection::vec(0usize..64, 8),
        0usize..64,
    )
        .prop_map(
            |(mock, first, present, sel, sub, marker, cred, docs, hex, limits, good)| Case {
                mock,
                // Mock-replay has no endpoint and no credential to fault.
                first: if mock && (first == 4 || first == 8) {
                    first + 1
                } else {
                    first
                },
                present,
                sel,
                sub,
                marker: format!("zq{marker}"),
                cred_suffix: cred,
                run_docs: docs,
                cred_value: format!("cv-{hex}"),
                limits,
                good_endpoint: good,
            },
        )
}

struct Build {
    base: Vec<(&'static str, String)>,
    top: Vec<String>,
    tail: Vec<String>,
    expect: Vec<Option<Expect>>,
    forbidden: Vec<String>,
    consent_fault: Option<usize>,
    env_fault: Option<usize>,
}

impl Build {
    fn put(&mut self, key: &'static str, line: String) {
        let slot = self.base.iter_mut().find(|(k, _)| *k == key).unwrap();
        slot.1 = line;
    }

    fn push_top(&mut self, line: String) -> Pos {
        self.top.push(line);
        Pos::Top(self.top.len() - 1)
    }

    fn push_tail(&mut self, lines: &[String]) -> Pos {
        let first = self.tail.len();
        self.tail.extend(lines.iter().cloned());
        Pos::Tail(first)
    }

    fn line_of(&self, pos: &Pos) -> u32 {
        let index = match pos {
            Pos::Key(key) => self.base.iter().position(|(k, _)| k == key).unwrap(),
            Pos::Top(i) => self.base.len() + i,
            Pos::Tail(i) => self.base.len() + self.top.len() + i,
        };
        u32::try_from(index + 1).unwrap()
    }

    fn text(&self) -> String {
        let lines = self
            .base
            .iter()
            .map(|(_, l)| l)
            .chain(&self.top)
            .chain(&self.tail);
        lines.fold(String::new(), |mut acc, l| {
            acc.push_str(l);
            acc.push('\n');
            acc
        })
    }

    fn set(&mut self, stage: usize, code: Code, at: Where) {
        self.expect[stage] = Some(Expect {
            code,
            at,
            needs_enabled: false,
        });
    }
}

fn new_build(case: &Case) -> Build {
    let mut base: Vec<(&'static str, String)> = vec![
        ("schema_version", "schema_version = 1".to_owned()),
        ("enabled", "enabled = true".to_owned()),
    ];
    if case.mock {
        base.push(("provider", "provider = \"mock-replay\"".to_owned()));
        base.push(("model", "model = \"model-1\"".to_owned()));
        base.push(("script", "script = \"recorded.json\"".to_owned()));
    } else {
        base.push(("provider", "provider = \"openai-responses\"".to_owned()));
        base.push(("model", "model = \"model-1\"".to_owned()));
        let good = GOOD_ENDPOINTS[case.good_endpoint % GOOD_ENDPOINTS.len()];
        base.push(("endpoint", format!("endpoint = \"{good}\"")));
        let name = format!("CRED_{}", case.cred_suffix);
        base.push((
            "credential_env_var",
            format!("credential_env_var = \"{name}\""),
        ));
    }
    base.push((
        "allow_document_text",
        "allow_document_text = true".to_owned(),
    ));
    base.push(("ai_identities", "ai_identities = [\"a\", \"b\"]".to_owned()));
    base.push((
        "transmission_log",
        "transmission_log = \"tx.jsonl\"".to_owned(),
    ));
    Build {
        base,
        top: Vec::new(),
        tail: Vec::new(),
        expect: vec![None; STAGES],
        forbidden: Vec::new(),
        consent_fault: None,
        env_fault: None,
    }
}

// Limit values are chosen from the edges of the ranges of Req 29.1 and
// Req 28.2. At most one is out of range, so one Diagnostic is expected.
fn emit_limits(b: &mut Build, case: &Case, faulty: bool) {
    let ls = &case.limits;
    let deadlines = [None, Some(1), Some(2), Some(60), Some(299), Some(300)];
    let mut deadline = deadlines[ls[0] % deadlines.len()];
    let mut retries = [None, Some(0), Some(1), Some(2)][ls[2] % 4];
    let mut clauses = [None, Some(1), Some(2), Some(99), Some(100)][ls[3] % 5];
    let mut bytes = [
        None,
        Some(1024),
        Some(1025),
        Some(16_777_215),
        Some(16_777_216),
    ][ls[4] % 5];
    let bad_field = case.sel[3] % 5;
    let bad_pick = case.sub[3];
    let mut bad_deadline_ceiling = deadline.unwrap_or(60);
    if faulty && bad_field == 0 {
        deadline = Some([0, 301, -1, i64::MAX][bad_pick % 4]);
        bad_deadline_ceiling = 60;
    }
    let ceiling = bad_deadline_ceiling;
    let candidates: Vec<i64> = [1, 20, 119, 120]
        .into_iter()
        .filter(|a| *a <= ceiling)
        .collect();
    let absent_ok = usize::from(ceiling >= 20);
    let k = ls[1] % (candidates.len() + absent_ok);
    let mut attempt = candidates.get(k).copied();
    if faulty {
        match bad_field {
            1 => {
                let relative = [1, 2, 60, 119][ls[0] % 4];
                let pick = [0, 121, -1, i64::MAX, relative + 1][bad_pick % 5];
                if bad_pick % 5 == 4 {
                    deadline = Some(relative);
                }
                attempt = Some(pick);
            }
            2 => retries = Some([3, -1, 256, i64::MAX][bad_pick % 4]),
            3 => clauses = Some([0, 101, -1, i64::MAX][bad_pick % 4]),
            4 => bytes = Some([1023, 16_777_217, 0, -1][bad_pick % 4]),
            _ => {}
        }
    }
    let fields: [(&str, Option<i64>); 5] = [
        ("total_deadline_secs", deadline),
        ("attempt_timeout_secs", attempt),
        ("max_retries", retries),
        ("max_clauses_per_request", clauses),
        ("max_response_bytes", bytes),
    ];
    for (index, (name, value)) in fields.into_iter().enumerate() {
        let Some(value) = value else { continue };
        let pos = b.push_top(format!("{name} = {value}"));
        if faulty && index == bad_field {
            b.set(3, Code::AiConfigLimitInvalid, at(pos, 1, Some(name)));
        }
    }
}

fn apply_syntax(b: &mut Build, v: usize) {
    let (line, column) = [("x = = 1", 5), ("= 1", 1), ("twice = 2", 1)][v % 3];
    // The third form repeats a key written by this fault only, so no other
    // fault can remove the first occurrence.
    if v % 3 == 2 {
        b.push_top("twice = 1".to_owned());
    }
    let pos = b.push_top(line.to_owned());
    b.set(0, Code::AiConfigInvalid, at(pos, column, None));
}

fn apply_secret(b: &mut Build, case: &Case, v: usize, sub: usize) {
    let m = &case.marker;
    let prefix = SECRET_PREFIXES[sub % SECRET_PREFIXES.len()];
    let name = SECRET_NAMES[sub % SECRET_NAMES.len()];
    let kinds = if case.mock { 12 } else { 14 };
    let secret = Code::AiConfigInlineSecret;
    let expected = match v % kinds {
        0 => at(b.push_top(format!("{name}_{m} = 1")), 1, None),
        1 => at(
            b.push_top(format!("extra = {{ password_{m} = \"p\" }}")),
            1,
            None,
        ),
        2 => {
            let lines = ["[extra]".to_owned(), format!("api_key_{m} = 1")];
            at(b.push_tail(&lines), 2, None)
        }
        3 => {
            let lines = ["[[extra]]".to_owned(), format!("authorization_{m} = \"p\"")];
            at(b.push_tail(&lines), 3, None)
        }
        4 => {
            b.put("model", format!("model = \"{prefix}{m}\""));
            at(Pos::Key("model"), 1, Some("model"))
        }
        5 => {
            b.put(
                "ai_identities",
                format!("ai_identities = [\"ok\", \"{prefix}{m}\"]"),
            );
            at(Pos::Key("ai_identities"), 1, Some("ai_identities"))
        }
        6 => {
            let line = format!("extra = {{ inner = \"{prefix}{m}\" }}");
            at(b.push_top(line), 1, Some("extra"))
        }
        7 => {
            let lines = ["[extra]".to_owned(), format!("inner = \"{prefix}{m}\"")];
            at(b.push_tail(&lines), 2, Some("extra"))
        }
        8 => {
            let lines = ["[[extra]]".to_owned(), format!("inner = \"{prefix}{m}\"")];
            at(b.push_tail(&lines), 3, Some("extra"))
        }
        9 => at(b.push_top(format!("{prefix}{m} = 1")), 1, None),
        10 => at(b.push_top(format!("extra.token_{m} = 1")), 1, None),
        11 => {
            let line = format!("extra = [{{ inner = \"{prefix}{m}\" }}]");
            at(b.push_top(line), 1, Some("extra"))
        }
        12 => {
            let bad = ["low-{m}", "9ABC", "", "A B {m}", "a1"][sub % 5].replace("{m}", m);
            b.put(
                "credential_env_var",
                format!("credential_env_var = \"{bad}\""),
            );
            at(
                Pos::Key("credential_env_var"),
                1,
                Some("credential_env_var"),
            )
        }
        _ => {
            let forms = [
                format!("https://u:{m}@api.example.org"),
                format!("https://{m}@api.example.org/x"),
                format!("https://api.example.org/p?k={m}"),
                format!("http://localhost?x={m}"),
                "https://api.example.org?".to_owned(),
            ];
            b.put("endpoint", format!("endpoint = \"{}\"", forms[sub % 5]));
            at(Pos::Key("endpoint"), 1, Some("endpoint"))
        }
    };
    b.set(1, secret, expected);
    b.forbidden.push(m.clone());
}

fn apply_fields(b: &mut Build, v: usize) {
    let invalid = Code::AiConfigInvalid;
    let wrong_type: [(&'static str, &str); 9] = [
        ("model", "model = 5"),
        ("enabled", "enabled = \"yes\""),
        ("allow_document_text", "allow_document_text = \"true\""),
        ("ai_identities", "ai_identities = [1, 2]"),
        ("ai_identities", "ai_identities = \"a\""),
        ("provider", "provider = true"),
        ("model", "model = \"\""),
        ("schema_version", "schema_version = 2"),
        ("transmission_log", "transmission_log = \"\""),
    ];
    match v % 16 {
        0 => {
            let pos = b.push_top("bogus_field = 1".to_owned());
            b.set(2, invalid, at(pos, 1, Some("bogus_field")));
        }
        n @ 1..=9 => {
            let (key, line) = wrong_type[n - 1];
            b.put(key, line.to_owned());
            b.set(2, invalid, at(Pos::Key(key), 1, Some(key)));
        }
        n => {
            let key = MISSING_FIELDS[n - 10];
            b.put(key, "# removed".to_owned());
            b.set(2, invalid, without_line(Some(key)));
            if let Some(e) = b.expect[2].as_mut() {
                e.needs_enabled = true;
            }
        }
    }
}

fn apply_endpoint(b: &mut Build, v: usize) {
    let insecure = INSECURE_ENDPOINTS.len();
    let (url, code) = if v % 12 < insecure {
        (INSECURE_ENDPOINTS[v % 12], Code::AiConfigEndpointInsecure)
    } else {
        (
            MALFORMED_ENDPOINTS[v % 12 - insecure],
            Code::AiConfigInvalid,
        )
    };
    b.put("endpoint", format!("endpoint = \"{url}\""));
    b.set(4, code, at(Pos::Key("endpoint"), 1, Some("endpoint")));
}

fn command_line() -> Location {
    Location::Cli(CliLocation::Command("ruletrace-ai".to_owned()))
}

fn consent_argument() -> Location {
    Location::Cli(CliLocation::Argument {
        position: 3,
        name: Some("--consent".to_owned()),
    })
}

fn apply_stage(b: &mut Build, case: &Case, stage: usize) {
    let (v, sub) = (case.sel[stage], case.sub[stage]);
    match stage {
        0 => apply_syntax(b, v),
        1 => apply_secret(b, case, v, sub),
        2 => apply_fields(b, v),
        4 => apply_endpoint(b, v),
        5 if v % 2 == 0 => {
            b.put("enabled", "enabled = false".to_owned());
            b.set(
                5,
                Code::AiDisabled,
                at(Pos::Key("enabled"), 1, Some("enabled")),
            );
        }
        5 => {
            b.put("enabled", "# omitted".to_owned());
            b.set(5, Code::AiDisabled, without_line(None));
        }
        6 => {
            let kind = ["other-kind", "anthropic-messages", "mock"][v % 3];
            b.put("provider", format!("provider = \"{kind}\""));
            b.set(
                6,
                Code::AiProviderUnknown,
                at(Pos::Key("provider"), 1, Some("provider")),
            );
        }
        7 => {
            b.consent_fault = Some(v % 5);
            let location = if v % 5 == 0 {
                command_line()
            } else {
                consent_argument()
            };
            if v % 5 == 4 {
                b.put(
                    "allow_document_text",
                    "allow_document_text = false".to_owned(),
                );
            }
            b.set(7, Code::AiConsentMissing, Where::Exact(location));
        }
        _ => {
            b.env_fault = Some(v % 3);
            b.set(
                8,
                Code::AiCredentialMissing,
                at(
                    Pos::Key("credential_env_var"),
                    1,
                    Some("credential_env_var"),
                ),
            );
        }
    }
}

fn ids(names: &BTreeSet<String>) -> BTreeSet<DocumentId> {
    names
        .iter()
        .map(|n| DocumentId::new(n.clone()).unwrap())
        .collect()
}

fn consent_for(case: &Case, fault: Option<usize>) -> Consent {
    let exact = ids(&case.run_docs);
    match fault {
        Some(0) => Consent::absent(command_line()),
        Some(1) => {
            let mut more = case.run_docs.clone();
            more.insert("zz9".to_owned());
            Consent::named(ids(&more), consent_argument())
        }
        Some(2) => {
            let mut fewer = case.run_docs.clone();
            if fewer.pop_first().is_none() {
                fewer.insert("zz9".to_owned());
            }
            Consent::named(ids(&fewer), consent_argument())
        }
        Some(3) => Consent::named(ids(&["zz8".to_owned()].into()), consent_argument()),
        _ => Consent::named(exact, consent_argument()),
    }
}

fn env_for(case: &Case, fault: Option<usize>) -> MapEnv {
    let name = format!("CRED_{}", case.cred_suffix);
    let mut values: BTreeMap<String, String> = [
        ("OPENAI_API_KEY", "decoy-1"),
        ("HOME", "/home/decoy"),
        ("PROVIDER_CREDENTIAL", "decoy-2"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    match fault {
        None => {
            values.insert(name, case.cred_value.clone());
        }
        Some(0) => {}
        Some(1) => {
            values.insert(name, String::new());
        }
        _ => {
            values.insert(format!("{name}X"), case.cred_value.clone());
        }
    }
    MapEnv {
        values,
        reads: RefCell::new(Vec::new()),
    }
}

fn input() -> InputId {
    InputId::new("ai.toml").unwrap()
}

fn resolve(b: &Build, at: &Where) -> Location {
    match at {
        Where::Exact(l) => l.clone(),
        Where::At { pos, column, field } => Location::Input(InputLocation {
            input: input(),
            line: pos.as_ref().map(|p| b.line_of(p)),
            column: *column,
            field: field.clone(),
        }),
    }
}

// Missing required fields only fail while the file is enabled, so a
// disabled-state fault hides them (Req 28.2, 28.17).
fn first_failure(b: &Build, present: &[bool]) -> Option<(usize, Expect)> {
    let disabled = present[5];
    (0..STAGES).find_map(|s| {
        let e = b.expect[s].as_ref().filter(|_| present[s])?;
        (!(e.needs_enabled && disabled)).then(|| (s, e.clone()))
    })
}

fn masked_presence(case: &Case) -> Vec<bool> {
    let mut present: Vec<bool> = (0..STAGES)
        .map(|s| match s.cmp(&case.first) {
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => true,
            std::cmp::Ordering::Greater => case.present[s],
        })
        .collect();
    if case.mock {
        present[4] = false;
        present[8] = false;
    }
    present
}

fn check_case(case: &Case) -> Result<(), TestCaseError> {
    let mut present = masked_presence(case);
    let mut b = new_build(case);
    // A missing-field fault is only the earliest failure when enabled.
    let missing_first = case.first == 2 && case.sel[2] % 16 >= 10;
    if missing_first {
        present[5] = false;
    }
    emit_limits(&mut b, case, present[3]);
    for stage in (0..STAGES).rev().filter(|s| *s != 3 && present[*s]) {
        apply_stage(&mut b, case, stage);
    }
    let text = b.text();
    let consent = consent_for(case, b.consent_fault);
    let env = env_for(case, b.env_fault);
    let run_docs = ids(&case.run_docs);
    let result = load(&input(), &text, &consent, &run_docs, &env);
    let name = format!("CRED_{}", case.cred_suffix);
    let expected = first_failure(&b, &present);
    let reads = env.reads.borrow().clone();
    match (result, expected) {
        (Ok(ok), None) => {
            let shown = format!("{ok:?}");
            prop_assert!(!shown.contains(&case.cred_value), "{shown}");
            if case.mock {
                prop_assert!(ok.credential.is_none());
                prop_assert_eq!(ok.config.provider, ProviderKind::MockReplay);
                prop_assert!(reads.is_empty(), "{:?}", reads);
            } else {
                prop_assert_eq!(ok.config.provider, ProviderKind::OpenAiResponses);
                let cred = ok.credential.unwrap();
                prop_assert_eq!(cred.expose(), case.cred_value.as_str());
                prop_assert_eq!(reads, vec![name]);
            }
        }
        (Err(found), Some((stage, want))) => {
            prop_assert_eq!(found.len(), 1, "{:?}\n{}", found, text);
            let d = &found[0];
            prop_assert_eq!(d.code(), want.code, "{:?}\n{}", found, text);
            prop_assert_eq!(d.severity(), Severity::Error);
            prop_assert_eq!(d.code().exit_code(), ExitCode::InvalidInput);
            prop_assert_eq!(d.primary(), &resolve(&b, &want.at), "{}", text);
            let shown = format!("{found:?} {}", d.message());
            for secret in b.forbidden.iter().chain([&case.cred_value]) {
                prop_assert!(!shown.contains(secret.as_str()), "{shown}");
            }
            if stage == 8 {
                prop_assert_eq!(reads, vec![name]);
            } else {
                prop_assert!(reads.is_empty(), "stage {stage} read {reads:?}");
            }
        }
        (other, expected) => {
            prop_assert!(false, "{other:?} versus {expected:?}\n{text}");
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    // Only the earliest failing check of Req 28.17 is reported, with its
    // code and position, no secret is echoed, and the environment is read
    // only for the named variable after every earlier check passed.
    #[test]
    fn only_the_earliest_failing_check_is_reported(case in arb_case()) {
        check_case(&case)?;
    }
}
