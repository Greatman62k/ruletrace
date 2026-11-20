//! The two codes of the RN-26 amendment (user decision 2 of 2026-11-17).
//! Identifiers, mnemonics, severity and exit code are written from that
//! decision, not read from the catalogue: both are Warnings with exit
//! code 0, like `RT-EXE-0011`, because a published difference never
//! changes the evaluation exit code.

use rt_foundation::{Code, Diagnostic, ExitCode, InputId, InputLocation, Location, Severity};

fn location(input: InputId) -> Location {
    Location::Input(InputLocation {
        input,
        line: None,
        column: None,
        field: Some("entries[p2/standing]".to_owned()),
    })
}

fn check(code: Code, id: &str, mnemonic: &str, input: InputId) {
    assert_eq!(code.id(), id);
    assert_eq!(code.mnemonic(), mnemonic);
    assert_eq!(Code::from_mnemonic(mnemonic), Some(code));
    assert_eq!(code.severity(), Severity::Warning);
    assert_eq!(code.exit_code(), ExitCode::Success);
    assert_eq!(code.exit_code().value(), 0);
    let d = Diagnostic::new(code, "m".to_owned(), location(input));
    assert_eq!(d.code(), code);
    assert_eq!(d.severity(), Severity::Warning);
    assert!(!d.severity().is_blocking());
    assert_eq!(
        Code::ALL.iter().filter(|c| c.id() == id).count(),
        1,
        "{id} must appear once"
    );
}

#[test]
fn rt_exe_0012_is_the_split_tie_group_warning_with_exit_0() {
    let code = Code::from_id("RT-EXE-0012").unwrap();
    check(
        code,
        "RT-EXE-0012",
        "PUBLISHED_TIE_GROUP_SPLIT",
        InputId::new("p.json").unwrap(),
    );
}

#[test]
fn rt_exe_0013_is_the_published_tie_not_computed_warning_with_exit_0() {
    let code = Code::from_id("RT-EXE-0013").unwrap();
    check(
        code,
        "RT-EXE-0013",
        "PUBLISHED_TIE_NOT_COMPUTED",
        InputId::new("p.json").unwrap(),
    );
}

#[test]
fn rt_exe_0011_keeps_its_mnemonic_severity_and_exit_code() {
    let code = Code::from_id("RT-EXE-0011").unwrap();
    check(
        code,
        "RT-EXE-0011",
        "PUBLISHED_RESULT_DIFFERS",
        InputId::new("p.json").unwrap(),
    );
}

#[test]
fn the_two_new_codes_are_distinct_from_each_other_and_from_rt_exe_0011() {
    let ids = ["RT-EXE-0011", "RT-EXE-0012", "RT-EXE-0013"];
    let codes: Vec<Code> = ids.iter().map(|i| Code::from_id(i).unwrap()).collect();
    assert_ne!(codes[0], codes[1]);
    assert_ne!(codes[0], codes[2]);
    assert_ne!(codes[1], codes[2]);
}
