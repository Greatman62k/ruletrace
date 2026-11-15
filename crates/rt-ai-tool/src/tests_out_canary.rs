//! Canary tests of [`Out`] itself (Req 28.9, 28.15, 23.22). The text that
//! reaches `Out` may copy untrusted input that a cut or a truncation left
//! with only part of the credential, so every credential is checked by
//! every run of eight consecutive characters and not only as a whole. The
//! credentials include one longer than 60 characters, a dotted one and a
//! bracketed one. The oracle is `tests_redact_fragments::leaked`.

use crate::tests_redact_fragments::{canaries, leaked, redactor};
use crate::{Out, Redactor};
use rt_foundation::{CliLocation, Code, Diagnostic, DocumentId, InputId, InputLocation, Location};

fn piece(credential: &str, from: usize, len: usize) -> String {
    credential.chars().skip(from).take(len).collect()
}

// Whole, a prefix cut mid-way, an inner cut, and a suffix, each at least
// eight characters long.
fn copies(credential: &str) -> Vec<String> {
    let n = credential.chars().count();
    vec![
        credential.to_owned(),
        piece(credential, 0, n - 3),
        piece(credential, 3, 12),
        piece(credential, n - 10, 10),
    ]
}

fn streams(redactor: Redactor, text: &str) -> (String, String) {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let mut out = Out::new(&mut stdout, &mut stderr, redactor);
    out.stdout(text).unwrap();
    out.stderr(text).unwrap();
    drop(out);
    (
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

fn input_at(id: &str, field: &str) -> Location {
    Location::Input(InputLocation {
        input: InputId::new(id).unwrap(),
        line: Some(1),
        column: Some(2),
        field: Some(field.to_owned()),
    })
}

fn rendered(redactor: Redactor, d: &Diagnostic) -> String {
    let mut stderr = Vec::new();
    let mut out = Out::new(Vec::new(), &mut stderr, redactor);
    out.diagnostic(d).unwrap();
    drop(out);
    String::from_utf8(stderr).unwrap()
}

#[test]
fn the_long_credential_is_longer_than_sixty_characters() {
    assert!(canaries().iter().any(|c| c.chars().count() > 60));
    assert!(canaries().iter().any(|c| c.contains('.')));
    assert!(canaries().iter().any(|c| c.contains('[')));
}

#[test]
fn stdout_and_stderr_hold_no_run_of_eight_credential_characters() {
    for c in canaries() {
        for copy in copies(c) {
            let text = format!("seen {copy} here\n");
            let (stdout, stderr) = streams(redactor(c), &text);
            assert_eq!(leaked(&stdout, c), None, "{c} / {copy}: {stdout}");
            assert_eq!(leaked(&stderr, c), None, "{c} / {copy}: {stderr}");
            assert!(stdout.starts_with("seen "), "{stdout}");
        }
    }
}

#[test]
fn a_diagnostic_message_and_every_location_hold_no_run_of_eight_characters() {
    for c in canaries() {
        for copy in copies(c) {
            let at_input = Diagnostic::new(
                Code::AiRequestInvalid,
                format!("bad {copy}"),
                input_at(&format!("file-{copy}"), &copy),
            );
            let at_argument = Diagnostic::new(
                Code::CliArgumentInvalid,
                "x".to_owned(),
                Location::Cli(CliLocation::Argument {
                    position: 3,
                    name: Some(copy.clone()),
                }),
            );
            let at_command = Diagnostic::new(
                Code::CliUnknownCommand,
                "x".to_owned(),
                Location::Cli(CliLocation::Command(copy.clone())),
            );
            let mut all = vec![at_input, at_argument, at_command];
            if let Ok(document) = DocumentId::new(copy.as_str()) {
                all.push(Diagnostic::new(
                    Code::AiRequestInvalid,
                    "x".to_owned(),
                    Location::Document(document),
                ));
            }
            for d in all {
                let text = rendered(redactor(c), &d);
                assert_eq!(leaked(&text, c), None, "{c} / {copy}: {text}");
                assert!(text.contains(d.code().id()), "{text}");
            }
        }
    }
}

#[test]
fn the_oracle_sees_a_leak_when_nothing_is_redacted() {
    for c in canaries() {
        for copy in copies(c) {
            let (stdout, _) = streams(Redactor::no_credential(), &copy);
            assert!(leaked(&stdout, c).is_some(), "{c} / {copy}");
        }
    }
}

#[test]
fn text_without_credential_characters_passes_through_unchanged() {
    for c in canaries() {
        let text = "run summary: complete\nprocessed:\n  rules.md 0 11\n";
        let (stdout, stderr) = streams(redactor(c), text);
        assert_eq!(stdout, text);
        assert_eq!(stderr, text);
    }
}
