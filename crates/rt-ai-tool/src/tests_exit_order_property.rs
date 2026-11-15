//! Property 30 for `ruletrace-ai`: the exit code follows the earliest
//! failing stage of Req 16.17 (design 6.1).
//!
//! **Validates: Requirements 16.13, 16.14, 16.16, 16.17, 16.24, 27.10**
//!
//! Each case injects any subset of the failure causes of the failing
//! stages at once, through the real `run_with`, over files on disk. The
//! expected code and Diagnostic are the ones of the earliest stage present,
//! written down by hand in [`expected`] from the table of Req 16.17 and the
//! check order of Req 28.17. A Warning never changes the code, and no code
//! outside 0, 2, 3, 5, 6 and 70 occurs.

use crate::tests::MapEnv;
use crate::tests_cli_rig::{Fixture, Ran};
use crate::tests_gate_rig::{run_providers, Steps};
use crate::tests_memfs::{Faults, MemFs};
use crate::tests_provenance::RULES_HASH;
use crate::tests_rig::{evidence, payload};
use crate::{BuiltInProviders, Providers, StdFs, StoreFs};
use proptest::prelude::*;
use rt_ai_adapter::{Completion, ProviderFailure};
use rt_ai_mock::{ScriptResult, Step};
use std::io::ErrorKind;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

static CASE: AtomicUsize = AtomicUsize::new(0);

const ALLOWED: [u8; 6] = [0, 2, 3, 5, 6, 70];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Response {
    Good,
    TooLarge,
    Malformed,
    Incomplete,
    Schema,
    Forbidden,
    Provenance,
}

// The provider kind of the configuration. A hosted provider needs a
// credential. This build links no hosted integration, so `Unlinked` is
// the real `BuiltInProviders` and `Linked` stands for a build that has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hosting {
    Mock,
    Linked,
    Unlinked,
}

#[derive(Clone, Debug)]
struct Case {
    arguments: bool,
    configuration: bool,
    hosting: Hosting,
    consent: bool,
    credential_missing: bool,
    input: bool,
    store_invalid: bool,
    log: bool,
    provider: bool,
    response: Response,
    store: bool,
    warning: bool,
}

fn response() -> impl Strategy<Value = Response> {
    prop_oneof![
        Just(Response::Good),
        Just(Response::TooLarge),
        Just(Response::Malformed),
        Just(Response::Incomplete),
        Just(Response::Schema),
        Just(Response::Forbidden),
        Just(Response::Provenance),
    ]
}

fn hosting() -> impl Strategy<Value = Hosting> {
    prop_oneof![
        Just(Hosting::Mock),
        Just(Hosting::Linked),
        Just(Hosting::Unlinked),
    ]
}

fn cases() -> impl Strategy<Value = Case> {
    (
        (any::<bool>(), any::<bool>(), hosting(), any::<bool>()),
        (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()),
        (any::<bool>(), response(), any::<bool>(), any::<bool>()),
    )
        .prop_map(|((a, c, h, k), (m, i, v, l), (p, r, s, w))| Case {
            arguments: a,
            configuration: c,
            hosting: h,
            consent: k,
            credential_missing: m,
            input: i,
            store_invalid: v,
            log: l,
            provider: p,
            response: r,
            store: s,
            warning: w,
        })
}

// The exit code and the one error code of the earliest failing stage,
// in the order of the table of Req 16.17.
fn expected(case: &Case) -> (u8, Option<&'static str>) {
    if case.arguments {
        return (2, Some("RT-CLI-0004"));
    }
    if case.configuration {
        return (2, Some("RT-AI-0002"));
    }
    if case.hosting == Hosting::Unlinked {
        return (2, Some("RT-AI-0006"));
    }
    if case.consent {
        return (2, Some("RT-AI-0008"));
    }
    if case.hosting == Hosting::Linked && case.credential_missing {
        return (2, Some("RT-AI-0007"));
    }
    if case.input {
        return (2, Some("RT-CLI-0003"));
    }
    if case.store_invalid {
        return (2, Some("RT-AI-0302"));
    }
    if case.log {
        return (70, Some("RT-AI-0010"));
    }
    if case.provider {
        return (5, Some("RT-AI-0105"));
    }
    match case.response {
        Response::TooLarge => return (6, Some("RT-AI-0203")),
        Response::Malformed => return (6, Some("RT-AI-0201")),
        Response::Incomplete => return (6, Some("RT-AI-0205")),
        Response::Schema => return (6, Some("RT-AI-0202")),
        Response::Forbidden => return (6, Some("RT-AI-0204")),
        Response::Provenance => return (3, Some("RT-PRV-0102")),
        Response::Good => {}
    }
    if case.store {
        return (70, Some("RT-AI-0303"));
    }
    (0, None)
}

fn body(response: Response) -> String {
    match response {
        Response::Good | Response::Incomplete => payload(0),
        Response::TooLarge => payload(0).replace("clause 0", &"x".repeat(2000)),
        Response::Malformed => "{ not json".to_owned(),
        Response::Schema => r#"{"schema_version":"1","proposals":"x"}"#.to_owned(),
        Response::Forbidden => {
            r#"{"schema_version":"1","proposals":[],"approval_status":"approved"}"#.to_owned()
        }
        Response::Provenance => {
            let good = evidence(0);
            payload(0).replace(&good, &good.replace(RULES_HASH, &"0".repeat(64)))
        }
    }
}

fn steps_for(case: &Case) -> Vec<Step> {
    if case.provider {
        let failure = ProviderFailure::Other { http_status: None };
        return vec![Step {
            elapsed: Duration::ZERO,
            result: ScriptResult::Failure(failure),
        }];
    }
    let completion = match case.response {
        Response::Incomplete => Completion::Incomplete,
        _ => Completion::Complete,
    };
    vec![Step {
        elapsed: Duration::from_millis(100),
        result: ScriptResult::Response {
            completion,
            payload: body(case.response).into_bytes(),
        },
    }]
}

fn configure(fixture: &Fixture, case: &Case) {
    let extra = match case.response {
        Response::TooLarge => "max_response_bytes = 1024\n",
        _ => "",
    };
    let text = match case.hosting {
        Hosting::Mock => fixture.mock_config(extra),
        Hosting::Linked | Hosting::Unlinked => fixture.real_config(extra),
    };
    fixture.write_config(&text);
}

fn run_case(case: &Case) -> Ran {
    let fixture = Fixture::new(&format!("p30-{}", CASE.fetch_add(1, Ordering::SeqCst)));
    configure(&fixture, case);
    if case.provider {
        fixture.write_script_for(&"1".repeat(64), &body(case.response));
    } else {
        fixture.write_script(&body(case.response));
    }
    if !case.log {
        std::fs::create_dir_all(fixture.path("work")).unwrap();
    }
    if case.configuration {
        fixture.write_config("schema_version = 1\nenabled =\n");
    }
    if case.warning {
        open_configuration(&fixture);
    }
    if case.input {
        std::fs::remove_file(fixture.path("candidates.json")).unwrap();
    }
    let mut extra: Vec<&str> = Vec::new();
    if !case.consent {
        extra.extend(["--consent", "rules.md"]);
    }
    if case.arguments {
        extra.push("--bogus");
    }
    let args = fixture.args(&extra);
    let memory = MemFs::new();
    if case.store {
        memory.with_faults(Faults {
            create_temp: Some(ErrorKind::PermissionDenied),
            ..Faults::default()
        });
    }
    if case.store_invalid {
        let batches = fixture.path("store").join("batches");
        let file = batches.join(format!("{}.json", "a".repeat(64)));
        std::fs::create_dir_all(&batches).unwrap();
        std::fs::write(&file, "{").unwrap();
        memory.plant(&file.to_string_lossy(), b"{");
    }
    let fs: &dyn StoreFs = if case.store { &memory } else { &StdFs };
    let linked;
    let providers: &dyn Providers = if case.hosting == Hosting::Unlinked {
        &BuiltInProviders
    } else if case.hosting == Hosting::Linked || case.response == Response::Incomplete {
        linked = Steps::new(steps_for(case)).0;
        &linked
    } else {
        &BuiltInProviders
    };
    let hosted = case.hosting != Hosting::Mock && !case.credential_missing;
    let env = if hosted {
        MapEnv::new(&[("PROVIDER_CREDENTIAL", "credential-value-0123456789")])
    } else {
        MapEnv::new(&[])
    };
    run_providers(&args, &env, providers, fs)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn the_exit_code_is_the_one_of_the_earliest_failing_stage(case in cases()) {
        let ran = run_case(&case);
        let (code, error) = expected(&case);
        prop_assert!(ALLOWED.contains(&ran.code.value()), "{:?}: {}", case, ran.code.value());
        prop_assert_eq!(ran.code.value(), code, "{:?}\n{}", case, ran.stderr);
        let errors: Vec<String> = ran
            .shapes()
            .into_iter()
            .filter(|(severity, _)| severity == "error")
            .map(|(_, id)| id)
            .collect();
        let want: Vec<String> = error.into_iter().map(str::to_owned).collect();
        prop_assert_eq!(errors, want, "{:?}\n{}", case, ran.stderr);
    }
}

// Without permission bits no Warning arises, and the expected exit code
// does not depend on one.
fn open_configuration(fixture: &Fixture) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let open = std::fs::Permissions::from_mode(0o644);
        std::fs::set_permissions(fixture.path("ai.toml"), open).unwrap();
    }
    #[cfg(not(unix))]
    let _ = fixture;
}
