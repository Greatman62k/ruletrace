//! Property 5 (Rulebook and ledger part): contradictory flag input is rejected.
//!
//! Any non-empty set of the flag keys (`has_errors`, `no_errors`,
//! `validation_status`, `validation_result`, `passed`) and lifecycle keys
//! (`approval_status`, `is_approved`, `is_rejected`, `is_proposed`,
//! `approval_record`) is inserted, each at its own position and with its own
//! value, into one object of a valid Rulebook or of a valid ledger line. The
//! parser must return one Error per category present (`RT-DGN-0002` when both
//! `has_errors` and `no_errors` are present, else `RT-DGN-0001`, and
//! `RT-DGN-0003` for lifecycle keys), each naming every offending field, with
//! the first offending key in input order as the primary location and the
//! others as related locations. No value is returned, so no later stage can
//! run. The catalogue maps all three codes to exit code 2 (`InvalidInput`); the
//! exit code is asserted at the catalogue level.
//!
//! **Validates: Requirements 4.10, 22.6, 22.7, 22.9, 22.10, 22.11, 22.12,
//! 22.13, 22.16, 22.17, 22.18, 20.25**

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use proptest::prelude::*;
use rt_foundation::InputId;
use rt_hash::sha256;
use rt_json::ParseLimits;
use rt_ledger::{parse_ledger, HEADER_LINE, ZERO_HASH};

#[path = "support/flags.rs"]
mod flags;

use flags::{apply, check, insertions, objects};

const DOC_HASH: &str = "eb4d68f2de94881529749748a8de99f39505ae89a2ae4fa9d88a6d7b07de788a";

fn rulebook_text() -> String {
    format!(
        r#"{{"format":"ruletrace-rulebook","ir_version":"1","name":"spring-open","version":"2026.1","documents":[{{"id":"rules.md","sha256":"{DOC_HASH}"}}],"types":{{"inputs":[{{"name":"age","type":"number"}},{{"name":"licence","type":"boolean"}}],"enums":[],"sets":[],"collections":[]}},"rules":[{{"id":"elig.basic","revision":1,"origin":{{"kind":"manual","author":null}},"kind":"eligibility","target":"eligible","type":"boolean","expr":{{"node":"and","t":"boolean","args":[{{"node":"cmp","t":"boolean","op":"ge","left":{{"node":"field","t":"number","name":"age"}},"right":{{"node":"num","t":"number","value":"16"}}}},{{"node":"field","t":"boolean","name":"licence"}}]}},"markers":[],"spans":[{{"document":"rules.md","start":21,"end":91,"line":3,"column":1}}]}}],"precedence":[]}}"#
    )
}

fn hex(seed: &str) -> String {
    sha256(seed.as_bytes()).as_str().to_owned()
}

/// A valid ledger of `n` records, sealed with an independently computed hash
/// chain.
fn ledger_lines(n: usize) -> Vec<String> {
    let mut lines = vec![HEADER_LINE.to_owned()];
    let mut prev = ZERO_HASH.to_owned();
    for i in 1..=n {
        let head = format!(
            r#"{{"seq":{i},"kind":"approval","rule":"r{i}","revision":{i},"content_hash":"{}","ir_version":"1","binding":[{{"document":"d.md","sha256":"{}"}}],"reviewer":"Alice","time":"2026-11-14T09:00:00Z","reason":null,"prev":"{prev}""#,
            hex(&format!("c{i}")),
            hex("doc")
        );
        let hash = hex(&format!("{head}}}"));
        lines.push(format!(r#"{head},"hash":"{hash}"}}"#));
        prev = hash;
    }
    lines
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn rulebook_with_flag_or_lifecycle_keys_is_rejected(
        ins in insertions(),
        which in any::<usize>(),
    ) {
        let base = rulebook_text();
        let objs = objects(&base);
        prop_assert!(objs.len() > 5);
        let object = &objs[which % objs.len()];
        let edited = apply(&base, object, &ins);
        let input = InputId::new("rulebook.json").expect("input id");
        let result = rt_ir::parse_rulebook(edited.text.as_bytes(), &input, 8 << 20, 64);
        match result {
            Ok(_) => prop_assert!(false, "Rulebook accepted: {}", edited.text),
            Err(rejected) => {
                let diags = rejected.into_diagnostics();
                check(diags.as_slice(), &edited, &edited.text, Some(1))?;
            }
        }
    }

    #[test]
    fn ledger_with_flag_or_lifecycle_keys_is_rejected(
        ins in insertions(),
        n in 1usize..4,
        line_pick in any::<usize>(),
        which in any::<usize>(),
    ) {
        let mut lines = ledger_lines(n);
        let target = line_pick % lines.len();
        let objs = objects(&lines[target]);
        let object = &objs[which % objs.len()];
        let edited = apply(&lines[target], object, &ins);
        lines[target] = edited.text.clone();
        let mut bytes = lines.join("\n").into_bytes();
        bytes.push(b'\n');
        let input = InputId::new("approvals.ledger").expect("input id");
        match parse_ledger(&input, &bytes, ParseLimits::new(1 << 20, 64)) {
            Ok(_) => prop_assert!(false, "ledger accepted"),
            Err(diags) => check(&diags, &edited, &edited.text, Some(target as u32 + 1))?,
        }
    }
}

#[test]
fn unmodified_inputs_are_accepted() {
    let input = InputId::new("rulebook.json").expect("input id");
    assert!(rt_ir::parse_rulebook(rulebook_text().as_bytes(), &input, 8 << 20, 64).is_ok());
    let mut bytes = ledger_lines(3).join("\n").into_bytes();
    bytes.push(b'\n');
    let input = InputId::new("approvals.ledger").expect("input id");
    assert!(parse_ledger(&input, &bytes, ParseLimits::new(1 << 20, 64)).is_ok());
}
