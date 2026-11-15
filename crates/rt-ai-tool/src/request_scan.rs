//! The credential check of a planned run (Req 28.14, 23.33 step 4).
//!
//! A Provider_Request carries no credential. The check runs on the
//! canonical bytes of every planned request, so a credential that sits in a
//! clause text, in the feedback or in the revised rule content stops the
//! run before the Transmission_Log, the store and the provider are touched.

use crate::plan::RunPlan;
use crate::redact::Redactor;
use rt_ai_adapter::ProviderRequest;
use rt_foundation::{Code, Diagnostic, InputLocation, Location};
use rt_json::write_compact;

fn hit(redactor: &Redactor, text: &str) -> bool {
    redactor.finds(text) || redactor.finds_fragment(text)
}

// The member of the request that holds the match, as a path of names that
// RuleTrace writes and of indexes that it counts, and whether the member
// is one that carries a clause, the feedback or revised content. The path
// never holds text of the request.
fn member_path(redactor: &Redactor, request: &ProviderRequest, base: &str) -> (String, bool) {
    for (name, value) in &request.context {
        if !hit(redactor, &write_compact(value)) {
            continue;
        }
        let inner = match name.as_str() {
            "clauses" => value.as_array().and_then(|items| {
                let k = items
                    .iter()
                    .position(|v| hit(redactor, &write_compact(v)))?;
                Some(format!("clauses[{k}]"))
            }),
            "revise" => value.as_object().and_then(|members| {
                let m = members
                    .iter()
                    .find(|m| hit(redactor, &write_compact(&m.value)))?;
                Some(format!("revise.{}", m.key))
            }),
            _ => None,
        };
        let from_input = inner.as_deref().is_some_and(|p| {
            p.starts_with("clauses[") || p == "revise.feedback" || p == "revise.content"
        });
        let path = format!("{base}.{}", inner.unwrap_or_else(|| name.clone()));
        return (path, from_input);
    }
    (base.to_owned(), false)
}

fn located(at: &Location, field: Option<String>) -> Location {
    match at {
        Location::Input(input) => Location::Input(InputLocation {
            field,
            ..input.clone()
        }),
        other => other.clone(),
    }
}

/// Refuses a plan when any request holds the credential or a run of eight
/// characters of it. Nothing has been sent, logged or stored at this point.
///
/// The Diagnostic names the request and the member that holds the match,
/// never the matched text. A credential that matches RuleTrace's own fixed
/// request text refuses every run, which is the stated trade-off of the
/// fragment check.
///
/// # Errors
/// `RT-AI-0308` for the first request, in plan order, that holds a match.
pub(crate) fn refuse_credential(
    plan: &RunPlan,
    redactor: &Redactor,
    at: &Location,
) -> Result<(), Diagnostic> {
    let total = plan.requests().len();
    for (index, planned) in plan.requests().iter().enumerate() {
        let bytes = String::from_utf8_lossy(planned.bytes());
        if !hit(redactor, &bytes) {
            continue;
        }
        let base = format!("requests[{index}]");
        let (path, from_input) = member_path(redactor, planned.request(), &base);
        let field = [path, base].into_iter().find(|p| !hit(redactor, p));
        let advice = if from_input {
            ""
        } else {
            "; the credential or part of it occurs in fixed request text, so it \
             is likely too short or too common"
        };
        return Err(Diagnostic::new(
            Code::AiRequestContainsCredential,
            format!(
                "request {} of {total} contains the credential value or a run of at \
                 least 8 consecutive characters of it{advice}; nothing was sent or \
                 written",
                index + 1
            ),
            located(at, field),
        ));
    }
    Ok(())
}
