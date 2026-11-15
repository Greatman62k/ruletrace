//! Property 30 for `ruletrace-ai`: the exit code follows the earliest
//! failing stage of Req 16.17 (design 6.1).
//!
//! **Validates: Requirements 16.13, 16.14, 16.16, 16.17, 16.24, 27.10**
//!
//! Each case injects any subset of the failure causes of the nine failing
//! stages at once, through the real `run`, over files on disk. The expected
//! code and Diagnostic are the ones of the earliest stage present, written
//! down by hand in [`expected`]. A Warning never changes the code, and no
//! code outside 0, 2, 3, 5, 6 and 70 occurs.

use crate::tests::MapEnv;
use crate::tests_cli_rig::{run_with, Fixture, Ran};
use crate::tests_memfs::{Faults, MemFs};
use crate::tests_provenance::RULES_HASH;
use crate::tests_rig::{evidence, payload};
use crate::{StdFs, StoreFs};
use proptest::prelude::*;
use std::io::ErrorKind;
use std::sync::atomic::{AtomicUsize, Ordering};

static CASE: AtomicUsize = AtomicUsize::new(0);

const ALLOWED: [u8; 6] = [0, 2, 3, 5, 6, 70];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Response {
    Good,
    Malformed,
    Schema,
    Forbidden,
    Provenance,
}

#[derive(Clone, Debug)]
struct Case {
    arguments: bool,
    configuration: bool,
    consent: bool,
    input: bool,
    log: bool,
    provider: bool,
    response: Response,
    store: bool,
    warning: bool,
}

fn response() -> impl Strategy<Value = Response> {
    prop_oneof![
        Just(Response::Good),
        Just(Response::Malformed),
        Just(Response::Schema),
        Just(Response::Forbidden),
        Just(Response::Provenance),
    ]
}

fn cases() -> impl Strategy<Value = Case> {
    (
        (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()),
        (any::<bool>(), any::<bool>(), response(), any::<bool>()),
        any::<bool>(),
    )
        .prop_map(|((a, c, k, i), (l, p, r, s), w)| Case {
            arguments: a,
            configuration: c,
            consent: k,
            input: i,
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
    if case.consent {
        return (2, Some("RT-AI-0008"));
    }
    if case.input {
        return (2, Some("RT-CLI-0003"));
    }
    if case.log {
        return (70, Some("RT-AI-0010"));
    }
    if case.provider {
        return (5, Some("RT-AI-0105"));
    }
    match case.response {
        Response::Malformed => return (6, Some("RT-AI-0201")),
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
        Response::Good => payload(0),
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

fn run_case(case: &Case) -> Ran {
    let fixture = Fixture::new(&format!("p30-{}", CASE.fetch_add(1, Ordering::SeqCst)));
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
        use std::os::unix::fs::PermissionsExt;
        let open = std::fs::Permissions::from_mode(0o644);
        std::fs::set_permissions(fixture.path("ai.toml"), open).unwrap();
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
    let fs: &dyn StoreFs = if case.store { &memory } else { &StdFs };
    run_with(&args, &MapEnv::new(&[]), fs)
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
