//! Argument parser tests (design 3.9.2; Req 17.7, 27.2, 27.11). Expected
//! codes and locations are written by hand from the requirements.

use crate::{parse, ProposeArgs};
use rt_foundation::{CliLocation, Code, Diagnostic, DocumentId, ExitCode, Location};
use std::collections::BTreeSet;
use std::ffi::OsString;

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

const GOOD: [&str; 13] = [
    "propose",
    "--config",
    "ai.toml",
    "--rulebook",
    "rb.json",
    "--source-model",
    "m.json",
    "--candidates",
    "c.json",
    "--store",
    "store",
    "--created-at",
    "2026-11-15T10:20:30Z",
];

fn good_with(extra: &[&str]) -> Vec<OsString> {
    let mut args = os(&GOOD);
    args.extend(extra.iter().map(OsString::from));
    args
}

fn parsed(extra: &[&str]) -> ProposeArgs {
    parse(&good_with(extra)).unwrap()
}

fn problems(args: &[OsString]) -> Vec<Diagnostic> {
    parse(args).unwrap_err()
}

fn arg_at(position: u32, name: Option<&str>) -> Location {
    Location::Cli(CliLocation::Argument {
        position,
        name: name.map(str::to_owned),
    })
}

fn command_at() -> Location {
    Location::Cli(CliLocation::Command("propose".to_owned()))
}

fn ids(names: &[&str]) -> BTreeSet<DocumentId> {
    names.iter().map(|n| DocumentId::new(*n).unwrap()).collect()
}

#[test]
fn a_complete_command_line_is_parsed_into_its_fields() {
    let args = parsed(&["--dry-run", "--consent", "a.md", "--consent", "b.md"]);
    assert_eq!(args.config.to_str(), Some("ai.toml"));
    assert_eq!(args.rulebook.to_str(), Some("rb.json"));
    assert_eq!(args.source_models.len(), 1);
    assert_eq!(args.candidates.to_str(), Some("c.json"));
    assert_eq!(args.store.to_str(), Some("store"));
    assert_eq!(args.created_at, "2026-11-15T10:20:30Z");
    assert!(args.dry_run);
    assert!(args.revise.is_none());
    assert!(args.consent.check(true, &ids(&["a.md", "b.md"])).is_none());
}

#[test]
fn source_models_are_kept_in_the_order_given() {
    let mut raw = os(&GOOD);
    raw.extend(os(&["--source-model", "second.json"]));
    let args = parse(&raw).unwrap();
    let names: Vec<_> = args.source_models.iter().map(|p| p.to_str()).collect();
    assert_eq!(names, [Some("m.json"), Some("second.json")]);
}

#[test]
fn options_may_appear_in_any_order() {
    let reordered = os(&[
        "propose",
        "--created-at",
        "2026-11-15T10:20:30Z",
        "--store",
        "store",
        "--candidates",
        "c.json",
        "--source-model",
        "m.json",
        "--rulebook",
        "rb.json",
        "--config",
        "ai.toml",
    ]);
    assert!(parse(&reordered).is_ok());
}

#[test]
fn no_consent_argument_is_absent_consent_and_not_a_parse_error() {
    let args = parsed(&[]);
    let d = args.consent.check(true, &ids(&["a.md"])).unwrap();
    assert_eq!(d.code(), Code::AiConsentMissing);
    assert_eq!(d.primary(), &command_at());
}

#[test]
fn the_consent_location_is_the_first_consent_argument() {
    let args = parsed(&["--consent", "a.md"]);
    let d = args.consent.check(true, &ids(&["a.md", "b.md"])).unwrap();
    assert_eq!(d.code(), Code::AiConsentMissing);
    assert_eq!(d.primary(), &arg_at(14, Some("--consent")));
}

#[test]
fn a_missing_configuration_is_ai_disabled_with_exit_2() {
    let mut raw = os(&GOOD);
    raw.drain(1..3);
    let found = problems(&raw);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code(), Code::AiDisabled);
    assert_eq!(found[0].primary(), &command_at());
    assert_eq!(found[0].code().exit_code(), ExitCode::InvalidInput);
}

#[test]
fn each_missing_required_option_is_reported_once_at_the_command() {
    let found = problems(&os(&["propose", "--config", "ai.toml"]));
    let messages: Vec<_> = found.iter().map(|d| d.message().to_owned()).collect();
    assert_eq!(
        messages,
        [
            "required option '--rulebook' is missing",
            "required option '--source-model' is missing",
            "required option '--candidates' is missing",
            "required option '--store' is missing",
            "required option '--created-at' is missing",
        ]
    );
    for d in &found {
        assert_eq!(d.code(), Code::CliMissingArgument);
        assert_eq!(d.primary(), &command_at());
        assert_eq!(d.code().exit_code(), ExitCode::InvalidInput);
    }
}

#[test]
fn no_command_and_an_unknown_command_are_exit_2_diagnostics() {
    let none = problems(&[]);
    assert_eq!(none[0].code(), Code::CliMissingArgument);
    assert_eq!(none[0].primary(), &arg_at(0, None));
    let unknown = problems(&os(&["review", "list"]));
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].code(), Code::CliUnknownCommand);
    assert_eq!(
        unknown[0].primary(),
        &Location::Cli(CliLocation::Command("review".to_owned()))
    );
    assert_eq!(unknown[0].code().exit_code(), ExitCode::InvalidInput);
}

#[test]
fn an_unknown_option_and_a_stray_argument_name_their_position() {
    let found = problems(&good_with(&["--bogus", "stray"]));
    let at: Vec<_> = found
        .iter()
        .map(|d| (d.code(), d.primary().clone()))
        .collect();
    assert_eq!(
        at,
        [
            (Code::CliArgumentInvalid, arg_at(13, Some("--bogus"))),
            (Code::CliArgumentInvalid, arg_at(14, Some("stray"))),
        ]
    );
}

#[test]
fn a_repeated_single_option_is_refused_and_a_repeatable_one_is_not() {
    let found = problems(&good_with(&["--store", "other"]));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code(), Code::CliArgumentInvalid);
    assert_eq!(found[0].primary(), &arg_at(13, Some("--store")));
    assert!(parse(&good_with(&["--consent", "a.md", "--consent", "b.md"])).is_ok());
}

#[test]
fn a_value_that_looks_like_an_option_is_not_taken_as_a_value() {
    let mut raw = os(&GOOD);
    raw.truncate(9);
    raw.extend(os(&[
        "--store",
        "--dry-run",
        "--created-at",
        "2026-11-15T10:20:30Z",
    ]));
    let found = problems(&raw);
    let seen: Vec<_> = found
        .iter()
        .map(|d| (d.code(), d.primary().clone(), d.message().to_owned()))
        .collect();
    assert_eq!(
        seen,
        [
            (
                Code::CliArgumentInvalid,
                arg_at(9, Some("--store")),
                "option '--store' needs a value".to_owned()
            ),
            (
                Code::CliMissingArgument,
                command_at(),
                "required option '--store' is missing".to_owned()
            ),
        ]
    );
}

#[test]
fn the_created_at_time_must_have_the_documented_shape() {
    for bad in [
        "2026-11-15",
        "2026-11-15T10:20:30",
        "2026-11-15 10:20:30Z",
        "2026-11-15T10:20:30+00:00",
        "2026-11-15T10:20:3Z",
        "2026-11-15T10:20:30ZZ",
        "٢٠٢٦-11-15T10:20:30Z",
    ] {
        let mut raw = os(&GOOD);
        raw.pop();
        raw.push(bad.into());
        let found = problems(&raw);
        assert_eq!(found.len(), 1, "{bad}");
        assert_eq!(found[0].code(), Code::CliArgumentInvalid, "{bad}");
        assert_eq!(
            found[0].primary(),
            &arg_at(12, Some("--created-at")),
            "{bad}"
        );
    }
}

#[test]
fn a_revision_request_needs_all_three_options() {
    let found = problems(&good_with(&["--revise-rule", "r.1"]));
    let messages: Vec<_> = found.iter().map(|d| d.message().to_owned()).collect();
    assert_eq!(
        messages,
        [
            "required option '--revise-revision' is missing",
            "required option '--feedback-file' is missing",
        ]
    );
    let args = parsed(&[
        "--revise-rule",
        "r.1",
        "--revise-revision",
        "3",
        "--feedback-file",
        "f.txt",
    ]);
    let revise = args.revise.unwrap();
    assert_eq!(revise.rule.as_str(), "r.1");
    assert_eq!(revise.revision.get(), 3);
    assert_eq!(revise.feedback.to_str(), Some("f.txt"));
}

#[test]
fn a_revision_number_must_be_a_positive_decimal_integer() {
    for bad in ["0", "-1", "1.5", "x", "4294967296"] {
        let found = problems(&good_with(&[
            "--revise-rule",
            "r.1",
            "--revise-revision",
            bad,
            "--feedback-file",
            "f.txt",
        ]));
        assert_eq!(found.len(), 1, "{bad}");
        assert_eq!(
            found[0].primary(),
            &arg_at(16, Some("--revise-revision")),
            "{bad}"
        );
    }
}

// A lone surrogate is the Windows counterpart of the byte 0xff.
fn not_utf8_argument() -> OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(vec![0x66, 0xff])
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[0x66, 0xd800])
    }
}

#[test]
fn an_argument_that_is_not_utf8_is_refused_with_its_position() {
    let mut raw = os(&["propose"]);
    raw.push(not_utf8_argument());
    let found = problems(&raw);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code(), Code::CliArgumentInvalid);
    assert_eq!(found[0].primary(), &arg_at(1, None));
}

#[test]
fn every_parse_failure_has_exit_code_2() {
    let cases: Vec<Vec<OsString>> = vec![
        vec![],
        os(&["x"]),
        os(&["propose"]),
        good_with(&["--bogus"]),
        good_with(&["--store", "again"]),
        good_with(&["--consent", ""]),
    ];
    for case in cases {
        for d in problems(&case) {
            assert_eq!(d.code().exit_code(), ExitCode::InvalidInput, "{d:?}");
        }
    }
}
