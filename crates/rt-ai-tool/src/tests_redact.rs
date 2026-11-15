//! Fragment matching of the `Redactor` (Req 28.9). The credentials are
//! written out literally and every expected value is counted by hand, so no
//! expectation comes from the production window logic.

use crate::credential::Credential;
use crate::redact::{Redactor, FRAGMENT_CHARS};

fn redactor(value: &str) -> Redactor {
    Redactor::new(&Credential::new(value.to_owned()).unwrap())
}

#[test]
fn the_fragment_length_is_eight_characters() {
    assert_eq!(FRAGMENT_CHARS, 8);
}

#[test]
fn a_window_of_7_characters_misses_at_start_middle_and_end() {
    let r = redactor("ABCDEFGHIJKLMNOP");
    assert!(!r.finds_fragment("..ABCDEFG.."));
    assert!(!r.finds_fragment("..HIJKLMN.."));
    assert!(!r.finds_fragment("..JKLMNOP.."));
}

#[test]
fn a_window_of_8_characters_hits_at_start_middle_and_end() {
    let r = redactor("ABCDEFGHIJKLMNOP");
    assert!(r.finds_fragment("..ABCDEFGH.."));
    assert!(r.finds_fragment("..FGHIJKLM.."));
    assert!(r.finds_fragment("..IJKLMNOP.."));
}

#[test]
fn a_text_of_exactly_one_window_is_matched_and_a_shorter_text_is_not() {
    let r = redactor("ABCDEFGHIJKLMNOP");
    assert!(r.finds_fragment("EFGHIJKL"));
    assert!(!r.finds_fragment("EFGHIJK"));
    assert!(!r.finds_fragment(""));
}

#[test]
fn a_window_with_one_changed_character_misses() {
    let r = redactor("ABCDEFGHIJKLMNOP");
    assert!(!r.finds_fragment("..ABCDEFxH.."));
    assert!(!r.finds_fragment("..abcdefgh.."));
}

#[test]
fn a_credential_of_7_characters_is_matched_only_whole() {
    let r = redactor("abc1234");
    assert!(r.finds("x abc1234 y"));
    assert!(!r.finds_fragment("x abc1234 y"));
    assert!(!r.finds_fragment("abc1234"));
}

#[test]
fn a_credential_of_8_characters_has_exactly_one_window() {
    let r = redactor("abcd1234");
    assert!(r.finds_fragment("..abcd1234.."));
    assert!(!r.finds_fragment("..abcd123.."));
    assert!(!r.finds_fragment("..bcd1234.."));
}

#[test]
fn windows_are_counted_in_characters_not_bytes() {
    let r = redactor("δζξΩεηθλμν");
    assert!(r.finds_fragment("..δζξΩεηθλ.."));
    assert!(r.finds_fragment("..ζξΩεηθλμ.."));
    assert!(r.finds_fragment("..ξΩεηθλμν.."));
    assert!(!r.finds_fragment("..δζξΩεηθ.."));
    assert!(!r.finds_fragment("..δζξΩ.."));
}

#[test]
fn overlapping_windows_of_a_repeating_credential_are_all_found() {
    let r = redactor("abcabcabcabc");
    assert!(r.finds_fragment("..bcabcabc.."));
    assert!(r.finds_fragment("..cabcabca.."));
    assert!(!r.finds_fragment("..cbacbacb.."));
    assert!(!r.finds_fragment("..abcabca.."));
}

#[test]
fn a_run_that_starts_inside_the_credential_and_ends_outside_misses() {
    let r = redactor("0123456789");
    assert!(r.finds_fragment("xx23456789yy"));
    assert!(!r.finds_fragment("xx3456789yy"));
}

// The plain form has `"`. The JSON form has the two characters `\"`.
#[test]
fn a_window_that_only_the_json_escaped_form_has_is_found() {
    let r = redactor(r#"key"abcdef"x"#);
    assert!(r.finds_fragment(r#"..abcdef\".."#));
    assert!(r.finds_fragment(r#"..y\"abcde.."#));
    assert!(!r.finds_fragment(r#"..abcdef\x.."#));
    assert!(!r.finds_fragment(r#"..bcdef\".."#));
}

#[test]
fn the_plain_form_of_a_quoted_credential_is_found_too() {
    let r = redactor(r#"key"abcdef"x"#);
    assert!(r.finds_fragment(r#"..ey"abcde.."#));
}

// ESC is U+001B. JSON writes it `\u001b` and the control form `\u{1b}`.
#[test]
fn a_truncated_json_escape_of_a_control_character_is_found() {
    let r = redactor("tok\u{1b}abcdef\u{1b}x");
    assert!(r.finds_fragment(r"..u001babc.."));
    assert!(r.finds_fragment(r"..\u001bab.."));
    assert!(!r.finds_fragment(r"..u001ba.."));
}

#[test]
fn a_truncated_control_escape_of_a_control_character_is_found() {
    let r = redactor("tok\u{1b}abcdef\u{1b}x");
    assert!(r.finds_fragment(r"..u{1b}abc.."));
    assert!(r.finds_fragment(r"..\u{1b}ab.."));
    assert!(!r.finds_fragment(r"..u{1b}ab.."));
}

#[test]
fn a_credential_without_fragments_never_matches_a_fragment() {
    let none = Redactor::no_credential();
    assert!(!none.finds_fragment("ABCDEFGHIJKLMNOP"));
    assert!(!none.finds("ABCDEFGHIJKLMNOP"));
}
