//! Which member and which kind of source the credential scan names for a
//! match. Requests are built by hand, so that a member that the planner
//! cannot make the first match is still reached. Expected paths and advice
//! are written out by hand.

use crate::request_scan::{advice, member_path, Source};
use crate::{Credential, Redactor};
use rt_ai_adapter::{ProviderRequest, RequestLimits};
use rt_json::Value;

fn object(members: Vec<(&str, Value<()>)>) -> Value<()> {
    Value::object(
        members
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    )
    .unwrap()
}

fn declarations(input: &str, rule: &str, revision: u32) -> Value<()> {
    object(vec![
        ("ir_version", Value::string("1")),
        (
            "inputs",
            Value::array(vec![object(vec![
                ("name", Value::string(input)),
                ("type", Value::string("number")),
            ])]),
        ),
        (
            "highest_revision",
            object(vec![(rule, Value::integer(revision))]),
        ),
    ])
}

fn request(input: &str, rule: &str, revision: u32, feedback: &str) -> ProviderRequest {
    ProviderRequest {
        task: "propose".to_owned(),
        model: "m1".to_owned(),
        limits: RequestLimits {
            deadline_s: 60,
            attempt_s: 20,
            retries: 2,
            response_bytes: 1_048_576,
        },
        context: vec![
            ("clauses".to_owned(), Value::array(Vec::new())),
            (
                "declarations".to_owned(),
                declarations(input, rule, revision),
            ),
            (
                "vocabulary".to_owned(),
                object(vec![("types", Value::array(vec![Value::string("number")]))]),
            ),
            (
                "revise".to_owned(),
                object(vec![
                    ("rule", Value::string(rule)),
                    ("revision", Value::integer(revision)),
                    ("content", Value::string("{}")),
                    ("feedback", Value::string(feedback)),
                ]),
            ),
        ],
    }
}

fn scan(credential: &str, request: &ProviderRequest) -> (String, Source) {
    let credential = Credential::new(credential.to_owned()).unwrap();
    member_path(&Redactor::new(&credential), request, "requests[0]")
}

#[test]
fn a_match_in_the_revision_number_names_the_revise_revision_option() {
    let request = request("age", "elig.basic", 12_345_678, "shorter");
    assert_eq!(
        scan("12345678", &request),
        (
            "requests[0].revise.revision".to_owned(),
            Source::ReviseRevision
        )
    );
    assert_eq!(
        advice(Source::ReviseRevision),
        "; the credential or part of it occurs in the revision target given with \
         --revise-revision"
    );
}

#[test]
fn a_match_in_the_revision_rule_names_the_revise_rule_option() {
    let request = request("age", "elig.basic", 2, "shorter");
    assert_eq!(
        scan("elig.basic", &request),
        ("requests[0].revise.rule".to_owned(), Source::ReviseRule)
    );
    assert_eq!(
        advice(Source::ReviseRule),
        "; the credential or part of it occurs in the revision target given with \
         --revise-rule"
    );
}

#[test]
fn a_match_in_a_declared_name_or_rule_identifier_is_a_declarations_match() {
    let mut no_revise = request("participant_age", "other.rule", 1, "x");
    no_revise.context.pop();
    for credential in ["participant_age", "other.rule"] {
        assert_eq!(
            scan(credential, &no_revise),
            ("requests[0].declarations".to_owned(), Source::Declarations),
            "{credential}"
        );
    }
    assert_eq!(
        advice(Source::Declarations),
        "; the credential or part of it occurs in the Rulebook declarations"
    );
}

#[test]
fn a_match_in_a_key_or_the_vocabulary_or_the_head_is_fixed_text() {
    let request = request("age", "elig.basic", 2, "shorter");
    for (credential, path) in [
        ("ir_version", "requests[0].declarations"),
        ("deadline_s", "requests[0]"),
    ] {
        assert_eq!(
            scan(credential, &request),
            (path.to_owned(), Source::Fixed),
            "{credential}"
        );
    }
    assert_eq!(
        advice(Source::Fixed),
        "; the credential or part of it occurs in fixed request text, so it is \
         likely too short or too common"
    );
}

#[test]
fn a_match_in_the_feedback_is_input_and_has_no_advice() {
    let request = request("age", "elig.basic", 2, "use zebra_stripes please");
    assert_eq!(
        scan("zebra_stripes", &request),
        ("requests[0].revise.feedback".to_owned(), Source::Input)
    );
    assert_eq!(advice(Source::Input), "");
}

fn with_input_type(mut request: ProviderRequest, ty: &str) -> ProviderRequest {
    let declared = object(vec![
        ("ir_version", Value::string("1")),
        (
            "inputs",
            Value::array(vec![object(vec![
                ("name", Value::string("age")),
                ("type", Value::string(ty)),
            ])]),
        ),
        (
            "highest_revision",
            object(vec![("r.one", Value::integer(1))]),
        ),
    ]);
    for (name, value) in &mut request.context {
        if name == "declarations" {
            *value = declared.clone();
        }
    }
    request
}

#[test]
fn a_match_in_the_model_name_names_the_model_and_has_its_own_advice() {
    let mut request = request("age", "elig.basic", 2, "shorter");
    request.model = "acme-reasoner-2025-01".to_owned();
    assert_eq!(
        scan("acme-reasoner-2025-01", &request),
        ("requests[0].model".to_owned(), Source::Model)
    );
    assert_eq!(
        scan("sk-acme-reasoner-2025-01-tail", &request),
        ("requests[0].model".to_owned(), Source::Model)
    );
    assert_eq!(
        advice(Source::Model),
        "; the credential or part of it occurs in the model name set in the AI \
         configuration"
    );
}

#[test]
fn a_match_in_a_member_is_named_before_the_model() {
    let mut request = request("age", "elig.basic", 2, "use zebra_stripes please");
    request.model = "zebra_stripes".to_owned();
    assert_eq!(
        scan("zebra_stripes", &request),
        ("requests[0].revise.feedback".to_owned(), Source::Input)
    );
}

#[test]
fn a_type_token_is_fixed_text_and_only_the_enum_name_is_user_text() {
    let bare = with_input_type(request("age", "elig.basic", 2, "x"), "boolean");
    assert_eq!(
        scan("boolean", &bare),
        ("requests[0].declarations".to_owned(), Source::Fixed)
    );
    let named = with_input_type(request("age", "elig.basic", 2, "x"), "enum:Division");
    for (credential, expected) in [
        ("Division", Source::Declarations),
        ("enum:Division", Source::Declarations),
        ("enum", Source::Fixed),
    ] {
        assert_eq!(
            scan(credential, &named),
            ("requests[0].declarations".to_owned(), expected),
            "{credential}"
        );
    }
}
