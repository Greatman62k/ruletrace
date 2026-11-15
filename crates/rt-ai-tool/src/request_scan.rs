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
use rt_json::{write_compact, Value};

fn hit(redactor: &Redactor, text: &str) -> bool {
    redactor.finds(text) || redactor.finds_fragment(text)
}

// Where the matched text comes from, which decides the advice of the message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    // A clause, the feedback or the revised content: input the user supplied
    // for this run, which needs no advice.
    Input,
    // A declaration of the Rulebook.
    Declarations,
    // The `--revise-rule` value.
    ReviseRule,
    // The `--revise-revision` value.
    ReviseRevision,
    // Text that RuleTrace writes: the fixed head, keys and the vocabulary.
    Fixed,
}

// The members are examined in this order, so that a value that the user gave
// for the revision, which the declarations repeat, is named by its own
// member. Any other member follows.
const MEMBER_ORDER: [&str; 4] = ["clauses", "revise", "declarations", "vocabulary"];

fn member_rank(name: &str) -> usize {
    MEMBER_ORDER
        .iter()
        .position(|n| *n == name)
        .unwrap_or(MEMBER_ORDER.len())
}

// Whether a string value of the declarations, or a rule identifier of
// `highest_revision`, holds the match. A match that only crosses keys or
// punctuation of the fixed layout is not user text.
fn declarations_hold_user_text(redactor: &Redactor, declarations: &Value<()>) -> bool {
    fn walk(redactor: &Redactor, value: &Value<()>) -> bool {
        if let Some(text) = value.as_str() {
            return hit(redactor, text);
        }
        if let Some(items) = value.as_array() {
            return items.iter().any(|item| walk(redactor, item));
        }
        value
            .as_object()
            .is_some_and(|members| members.iter().any(|m| walk(redactor, &m.value)))
    }
    let rule_ids = declarations
        .get("highest_revision")
        .and_then(|m| m.value.as_object())
        .is_some_and(|members| members.iter().any(|m| hit(redactor, &m.key)));
    rule_ids || walk(redactor, declarations)
}

// The member of the request that holds the match, as a path of names that
// RuleTrace writes and of indexes that it counts, and where the match comes
// from. The path never holds text of the request.
pub(crate) fn member_path(
    redactor: &Redactor,
    request: &ProviderRequest,
    base: &str,
) -> (String, Source) {
    let mut members: Vec<_> = request.context.iter().collect();
    members.sort_by_key(|(name, _)| member_rank(name));
    for (name, value) in members {
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
        let source = match inner.as_deref() {
            Some(p) if p.starts_with("clauses[") => Source::Input,
            Some("revise.feedback" | "revise.content") => Source::Input,
            Some("revise.rule") => Source::ReviseRule,
            Some("revise.revision") => Source::ReviseRevision,
            _ if name == "declarations" && declarations_hold_user_text(redactor, value) => {
                Source::Declarations
            }
            _ => Source::Fixed,
        };
        let path = format!("{base}.{}", inner.unwrap_or_else(|| name.clone()));
        return (path, source);
    }
    (base.to_owned(), Source::Fixed)
}

pub(crate) fn advice(source: Source) -> &'static str {
    match source {
        Source::Input => "",
        Source::Declarations => {
            "; the credential or part of it occurs in the Rulebook declarations"
        }
        Source::ReviseRule => {
            "; the credential or part of it occurs in the revision target given \
             with --revise-rule"
        }
        Source::ReviseRevision => {
            "; the credential or part of it occurs in the revision target given \
             with --revise-revision"
        }
        Source::Fixed => {
            "; the credential or part of it occurs in fixed request text, so it \
             is likely too short or too common"
        }
    }
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
        let (path, source) = member_path(redactor, planned.request(), &base);
        let field = [path, base].into_iter().find(|p| !hit(redactor, p));
        let advice = advice(source);
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
