//! The five Diagnostic codes that UD-37 reserves (task 10.10, Req 35.3,
//! 35.6, 35.8, 35.18, 35.19). The expected identifiers, mnemonics,
//! severities and exit codes are written from `tools/decisions/UD-37.md`
//! section 5, not read from the catalogue.

use rt_foundation::{Code, Diagnostic, ExitCode, InputId, InputLocation, Location, Severity};

const RESERVED: [(&str, &str, ExitCode); 5] = [
    (
        "RT-AI-0011",
        "AI_CAPABILITY_UNSUPPORTED",
        ExitCode::InvalidInput,
    ),
    (
        "RT-AI-0012",
        "AI_MODEL_TRAINING_MODE_REFUSED",
        ExitCode::InvalidInput,
    ),
    (
        "RT-AI-0013",
        "AI_CONFIG_ENDPOINT_HOST_NOT_PERMITTED",
        ExitCode::InvalidInput,
    ),
    (
        "RT-AI-0109",
        "AI_MODEL_DISCOVERY_FAILED",
        ExitCode::Provider,
    ),
    (
        "RT-AI-0110",
        "AI_MODEL_NOT_IN_CATALOGUE",
        ExitCode::Provider,
    ),
];

#[test]
fn each_reserved_code_is_in_the_catalogue_with_its_severity_and_exit_code() {
    for (id, mnemonic, exit) in RESERVED {
        let code = Code::from_id(id).unwrap_or_else(|| panic!("{id} is not in the catalogue"));
        assert_eq!(code.id(), id);
        assert_eq!(code.mnemonic(), mnemonic, "{id}");
        assert_eq!(code.severity(), Severity::Error, "{id}");
        assert_eq!(code.exit_code(), exit, "{id}");
        let place = Location::Input(InputLocation {
            input: InputId::new("ai.toml").unwrap(),
            line: None,
            column: None,
            field: None,
        });
        let d = Diagnostic::new(code, "m".to_owned(), place);
        assert_eq!(d.code(), code);
        assert_eq!(d.severity(), Severity::Error, "{id}");
        assert!(d.severity().is_blocking(), "{id}");
    }
}

#[test]
fn the_catalogue_holds_exactly_the_147_earlier_codes_and_the_five_new_ones() {
    // RT-EXE-0012 and RT-EXE-0013 (RN-26, decision of 2026-11-17) and
    // RT-CLI-0007 (UD-44, decision of 2026-11-20) follow.
    assert_eq!(Code::ALL.len(), 147 + RESERVED.len() + 2 + 1);
    for (id, _, _) in RESERVED {
        assert_eq!(
            Code::ALL.iter().filter(|c| c.id() == id).count(),
            1,
            "{id} must appear once"
        );
    }
}

#[test]
fn the_neighbouring_codes_keep_their_rows() {
    for (id, severity, exit) in [
        ("RT-AI-0010", Severity::Error, ExitCode::Internal),
        ("RT-AI-0108", Severity::Error, ExitCode::Provider),
        ("RT-AI-0101", Severity::Warning, ExitCode::Success),
        ("RT-AI-0309", Severity::Warning, ExitCode::Success),
    ] {
        let code = Code::from_id(id).unwrap();
        assert_eq!(
            (code.severity(), code.exit_code()),
            (severity, exit),
            "{id}"
        );
    }
}
