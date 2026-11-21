//! The code that UD-44 adds (task 13.3 scoped exception). The expected row is
//! written from `tools/decisions/UD-44.md` and design §9.4, not read from the
//! catalogue.

use rt_foundation::{Code, ExitCode, Severity};

#[test]
fn rt_cli_0007_is_the_lifeline_closed_error_with_exit_70() {
    let code = Code::from_id("RT-CLI-0007").expect("RT-CLI-0007 is in the catalogue");
    assert_eq!(code.id(), "RT-CLI-0007");
    assert_eq!(code.mnemonic(), "CLI_LIFELINE_CLOSED");
    assert_eq!(Code::from_mnemonic("CLI_LIFELINE_CLOSED"), Some(code));
    assert_eq!(code.severity(), Severity::Error);
    assert_eq!(code.exit_code(), ExitCode::Internal);
    assert_eq!(code.exit_code().value(), 70);
    assert_eq!(Code::ALL.iter().filter(|c| **c == code).count(), 1);
}

#[test]
fn the_codes_before_it_keep_their_rows() {
    for (id, mnemonic, exit) in [
        ("RT-CLI-0005", "CLI_OUTPUT_WRITE_FAILED", 70),
        ("RT-CLI-0006", "INTERNAL_ERROR", 70),
    ] {
        let code = Code::from_id(id).expect(id);
        assert_eq!(code.mnemonic(), mnemonic);
        assert_eq!(code.severity(), Severity::Error);
        assert_eq!(code.exit_code().value(), exit);
    }
}

// The `executable` column and the meaning are not exposed by `Code`, so the row
// is compared as written in UD-44 and design §9.4.
#[test]
fn the_catalogue_row_carries_the_executables_and_meaning_of_ud_44() {
    let catalogue = include_str!("../../../catalogue/diagnostics.v1.json");
    let row = catalogue
        .lines()
        .find(|l| l.contains("\"RT-CLI-0007\""))
        .expect("RT-CLI-0007 row");
    assert!(row.contains("\"executable\": \"rt, ai\""), "{row}");
    assert!(row.contains("\"exit\": 70"), "{row}");
    assert!(row.contains("--lifeline-stdin"), "{row}");
    assert!(row.contains("end of file"), "{row}");
}
