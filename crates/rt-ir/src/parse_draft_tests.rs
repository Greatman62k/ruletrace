//! Tests of `parse_draft`, the decoder of a stored rule draft (design §3.8.2).

use super::*;
use rt_foundation::{Code, Diagnostic, InputId, Location, RevisionNumber};

/// A draft: a rule object without `revision` and `origin`. Its deepest
/// container is the `left` field, at depth 3.
const DRAFT: &str = r#"{"id":"elig.new","kind":"eligibility","target":"eligible2","type":"boolean","expr":{"node":"cmp","t":"boolean","op":"ge","left":{"node":"field","t":"number","name":"age"},"right":{"node":"num","t":"number","value":"16"}},"markers":[],"spans":[{"document":"rules.md","start":9,"end":59,"line":3,"column":1}]}"#;

fn input() -> InputId {
    InputId::new("proposal-store").unwrap()
}

fn origin() -> Origin {
    Origin::Manual {
        author: Some(Identity::new("assigned-author").unwrap()),
    }
}

fn parse(text: &str, revision: u32, max_depth: u32) -> Result<RuleRevision, Vec<Diagnostic>> {
    parse_draft(
        text.as_bytes(),
        &input(),
        RevisionNumber::new(revision).unwrap(),
        origin(),
        1 << 20,
        max_depth,
    )
    .map(|s| s.into_parts().0)
    .map_err(|r| r.into_diagnostics().as_slice().to_vec())
}

fn fail(text: &str) -> Vec<Diagnostic> {
    match parse(text, 7, 64) {
        Ok(_) => panic!("expected a rejection of {text}"),
        Err(d) => d,
    }
}

fn input_position(d: &Diagnostic) -> (Option<u32>, Option<u32>) {
    match d.primary() {
        Location::Input(l) => (l.line, l.column),
        other => panic!("no input location: {other:?}"),
    }
}

#[test]
fn a_draft_decodes_with_the_supplied_revision_and_origin() {
    let rev = parse(DRAFT, 7, 64).unwrap();
    assert_eq!(rev.key.id.as_str(), "elig.new");
    assert_eq!(rev.key.revision.get(), 7);
    assert_eq!(rev.origin, origin());
    assert_eq!(rev.content.target.as_str(), "eligible2");
    assert_eq!(rev.content.spans.len(), 1);
    assert_eq!(rev.content.spans[0].document.as_str(), "rules.md");
    assert_eq!(
        (rev.content.spans[0].start, rev.content.spans[0].end),
        (9, 59)
    );
    // The same bytes decode to the same content under another number.
    let other = parse(DRAFT, 8, 64).unwrap();
    assert_eq!(other.key.revision.get(), 8);
    assert_eq!(other.content, rev.content);
}

#[test]
fn a_draft_naming_revision_or_origin_is_an_unknown_field_at_the_key() {
    for (member, key) in [
        (r#""revision":9,"#, "revision"),
        (r#""origin":{"kind":"manual","author":null},"#, "origin"),
    ] {
        let text = DRAFT.replacen(r#""kind""#, &format!(r#"{member}"kind""#), 1);
        let d = fail(&text);
        assert_eq!(d.len(), 1, "{key}: {d:?}");
        assert_eq!(d[0].code(), Code::IrUnknownField, "{key}");
        assert!(d[0].message().contains(&format!("'{key}'")), "{key}");
        // The key starts right after the opening quote position of `"key"`.
        let at = text.find(&format!(r#""{key}""#)).unwrap();
        assert_eq!(input_position(&d[0]), (Some(1), Some(at as u32 + 1)));
    }
}

#[test]
fn a_truncated_draft_is_a_syntax_error_at_the_end_of_the_input() {
    let text = &DRAFT[..DRAFT.len() - 20];
    let d = fail(text);
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].code(), Code::IrSyntaxError);
    let (line, column) = input_position(&d[0]);
    assert_eq!(line, Some(1));
    let column = column.unwrap() as usize;
    // One past the last byte of a single-line input, counting from 1.
    assert_eq!(column, text.len() + 1);
}

#[test]
fn the_depth_cap_applies_exactly_and_to_overridden_limits() {
    assert!(parse(DRAFT, 7, 3).is_ok());
    let d = match parse(DRAFT, 7, 2) {
        Ok(_) => panic!("depth 3 must exceed a limit of 2"),
        Err(d) => d,
    };
    assert_eq!(d[0].code(), Code::LimitNestingTooDeep);
    let deep = format!(
        r#"{{"id":"elig.new","x":{}{}}}"#,
        "[".repeat(50_000),
        "]".repeat(50_000)
    );
    let r = parse_draft(
        deep.as_bytes(),
        &input(),
        RevisionNumber::new(1).unwrap(),
        origin(),
        u64::MAX,
        u32::MAX,
    )
    .unwrap_err();
    assert_eq!(
        r.diagnostics().as_slice()[0].code(),
        Code::LimitNestingTooDeep
    );
}

#[test]
fn an_oversized_draft_is_refused_by_the_byte_limit() {
    let exact = u64::try_from(DRAFT.len()).unwrap();
    let ok = parse_draft(
        DRAFT.as_bytes(),
        &input(),
        RevisionNumber::new(1).unwrap(),
        origin(),
        exact,
        64,
    );
    assert!(ok.is_ok());
    let small = parse_draft(
        DRAFT.as_bytes(),
        &input(),
        RevisionNumber::new(1).unwrap(),
        origin(),
        exact - 1,
        64,
    )
    .unwrap_err();
    assert_eq!(
        small.diagnostics().as_slice()[0].code(),
        Code::LimitFileTooLarge
    );
}
