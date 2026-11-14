//! The shared flag-key and lifecycle-key check of decoders (Req 4.10, 22.9,
//! 22.10, 22.17).

use crate::value::{Kind, Position, Value};
use rt_foundation::{escape_controls, Code, Diagnostic, InputId, InputLocation, Location};

/// Keys that supply a diagnostic-state flag or a validation status
/// (`RT-DGN-0001`/`0002`).
pub const FLAG_KEYS: [&str; 5] = [
    "has_errors",
    "no_errors",
    "validation_status",
    "validation_result",
    "passed",
];

/// Keys that supply a lifecycle flag or a stored Approval_Status
/// (`RT-DGN-0003`).
pub const LIFECYCLE_KEYS: [&str; 5] = [
    "approval_status",
    "is_approved",
    "is_rejected",
    "is_proposed",
    "approval_record",
];

/// Checks every object of a parsed document for flag and lifecycle keys,
/// whatever the value is and whether or not it agrees with a derived value.
///
/// Each object that has such keys gives, in addition to nothing else:
/// - one `RT-DGN-0002` when it has both `has_errors` and `no_errors`, naming
///   every flag key of the object; otherwise one `RT-DGN-0001` naming every
///   flag key of the object;
/// - one `RT-DGN-0003` naming every lifecycle key of the object.
///
/// The primary location is the first offending key in input order (line, column
/// and a field path such as `rules[0].has_errors`). Further offending keys are
/// related locations in input order. The result is sorted. Callers run this
/// before any unknown-name check, return no value on `Err`, and run no later
/// stage.
///
/// # Errors
/// All diagnostics found, sorted. `Ok` means the document has none of these
/// keys.
pub fn check_flag_keys(input: &InputId, document: &Value<Position>) -> Result<(), Vec<Diagnostic>> {
    let mut found = Vec::new();
    let mut stack: Vec<(&Value<Position>, String)> = vec![(document, String::new())];
    while let Some((value, path)) = stack.pop() {
        match value.kind() {
            Kind::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    stack.push((item, format!("{path}[{index}]")));
                }
            }
            Kind::Object(members) => {
                let mut flags = Vec::new();
                let mut lifecycle = Vec::new();
                for member in members {
                    let child = if path.is_empty() {
                        escape_controls(&member.key)
                    } else {
                        format!("{path}.{}", escape_controls(&member.key))
                    };
                    if FLAG_KEYS.contains(&member.key.as_str()) {
                        flags.push((member.key.as_str(), member.key_pos, child.clone()));
                    } else if LIFECYCLE_KEYS.contains(&member.key.as_str()) {
                        lifecycle.push((member.key.as_str(), member.key_pos, child.clone()));
                    }
                    stack.push((&member.value, child));
                }
                if !flags.is_empty() {
                    let both = flags.iter().any(|f| f.0 == "has_errors")
                        && flags.iter().any(|f| f.0 == "no_errors");
                    let (code, what) = if both {
                        (
                            Code::DiagnosticStateBothFlags,
                            "both has_errors and no_errors",
                        )
                    } else {
                        (
                            Code::DiagnosticStateContradiction,
                            "a diagnostic-state flag",
                        )
                    };
                    found.push(build(input, code, what, &flags));
                }
                if !lifecycle.is_empty() {
                    let what = "a lifecycle flag or stored approval status";
                    found.push(build(
                        input,
                        Code::LifecycleFlagContradiction,
                        what,
                        &lifecycle,
                    ));
                }
            }
            _ => {}
        }
    }
    if found.is_empty() {
        Ok(())
    } else {
        found.sort();
        Err(found)
    }
}

fn build(
    input: &InputId,
    code: Code,
    what: &str,
    fields: &[(&str, Position, String)],
) -> Diagnostic {
    let names: Vec<&str> = fields.iter().map(|f| f.0).collect();
    let loc = |f: &(&str, Position, String)| {
        Location::Input(InputLocation {
            input: input.clone(),
            line: Some(f.1.line),
            column: Some(f.1.column),
            field: Some(f.2.clone()),
        })
    };
    let mut locations = fields.iter().map(loc);
    // Callers pass a non-empty slice; the fallback keeps this function total.
    let primary = locations
        .next()
        .unwrap_or(Location::None(Code::InternalError));
    Diagnostic::new(
        code,
        format!(
            "input supplies {what}, which is derived and never accepted: {}",
            names.join(", ")
        ),
        primary,
    )
    .with_related(locations.collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, ParseLimits};

    fn run(text: &str) -> Result<(), Vec<Diagnostic>> {
        let id = InputId::new("r.json").unwrap();
        let doc = parse(&id, text.as_bytes(), ParseLimits::new(1 << 20, 64)).unwrap();
        check_flag_keys(&id, &doc)
    }

    fn field_of(d: &Diagnostic) -> (Option<u32>, Option<u32>, Option<String>) {
        match d.primary() {
            Location::Input(l) => (l.line, l.column, l.field.clone()),
            _ => panic!("no input location"),
        }
    }

    #[test]
    fn clean_documents_pass() {
        assert!(run(r#"{"rules":[{"id":"a"}],"x":{"y":[]}}"#).is_ok());
    }

    #[test]
    fn every_flag_key_gives_dgn_0001_whether_or_not_it_agrees() {
        for key in FLAG_KEYS.iter().filter(|k| **k != "no_errors") {
            for value in ["true", "false", "\"passed\""] {
                let errs = run(&format!("{{\"{key}\": {value}}}")).unwrap_err();
                assert_eq!(errs.len(), 1);
                assert_eq!(errs[0].code(), Code::DiagnosticStateContradiction);
                assert!(errs[0].message().contains(key));
                assert_eq!(
                    field_of(&errs[0]),
                    (Some(1), Some(2), Some((*key).to_string()))
                );
            }
        }
    }

    #[test]
    fn both_flags_give_one_dgn_0002_naming_both() {
        let errs = run("{\n \"a\":1,\n \"no_errors\":true, \"has_errors\":false}").unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].code(), Code::DiagnosticStateBothFlags);
        assert!(errs[0].message().contains("no_errors, has_errors"));
        assert_eq!(
            field_of(&errs[0]),
            (Some(3), Some(2), Some("no_errors".into()))
        );
        assert_eq!(errs[0].related().len(), 1);
    }

    #[test]
    fn lifecycle_keys_give_dgn_0003_naming_every_field() {
        let errs =
            run(r#"{"rules":[{"is_approved":false,"approval_status":"Approved"}]}"#).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].code(), Code::LifecycleFlagContradiction);
        assert!(errs[0].message().contains("is_approved, approval_status"));
        assert_eq!(
            field_of(&errs[0]).2.as_deref(),
            Some("rules[0].is_approved")
        );
        for key in LIFECYCLE_KEYS {
            let errs = run(&format!("{{\"{key}\": null}}")).unwrap_err();
            assert_eq!(errs[0].code(), Code::LifecycleFlagContradiction);
        }
    }

    #[test]
    fn flag_and_lifecycle_in_one_object_give_two_diagnostics() {
        let errs = run(r#"{"has_errors":false,"is_proposed":true}"#).unwrap_err();
        let codes: Vec<_> = errs.iter().map(Diagnostic::code).collect();
        assert_eq!(errs.len(), 2);
        assert!(codes.contains(&Code::DiagnosticStateContradiction));
        assert!(codes.contains(&Code::LifecycleFlagContradiction));
    }
}
