// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `$validate-code` on a `Coding` or `CodeableConcept` that states no code
//! or no system.
//!
//! `Coding.system` and `Coding.code` are both 0..1
//! (<https://hl7.org/fhir/R4B/datatypes.html#Coding>), so such an input is a
//! valid instance, and the operation answers whether it is a valid code
//! (`result`, <https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>)
//! instead of refusing the request: `result` false, with an `invalid-data`
//! warning at the coding itself.

use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::{Server, parameter, parameters};
use ferroterm_testkit::fhir::{ANIMALS, VS_PETS};

/// The issues of a validation answer as (severity, expression) pairs.
fn issues(body: &Value) -> Vec<(String, String)> {
    parameter(body, "issues")
        .and_then(|issues| issues.pointer("/resource/issue"))
        .and_then(Value::as_array)
        .map(|issues| {
            issues
                .iter()
                .map(|issue| {
                    (
                        text(issue.pointer("/severity")),
                        text(issue.pointer("/expression/0")),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A string member, or the empty string when there is none.
fn text(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn result(body: &Value) -> bool {
    parameter(body, "result").expect("a result")["valueBoolean"] == true
}

#[tokio::test]
async fn a_coding_without_a_code_is_not_a_valid_code_of_a_value_set() {
    let server = Server::start_with_resources();
    for coding in [json!({"system": ANIMALS}), json!({"display": "Dog"})] {
        let (status, body) = server
            .post(
                "/r4b/ValueSet/$validate-code",
                &parameters(&[
                    ("url", json!({"valueUri": VS_PETS})),
                    ("coding", json!({"valueCoding": coding})),
                ]),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(!result(&body), "{body}");
        assert_eq!(
            issues(&body),
            [(String::from("warning"), String::from("Coding"))],
            "one warning at the coding: {body}"
        );
    }
}

#[tokio::test]
async fn a_codeable_concept_without_a_code_to_judge_fails_against_the_value_set() {
    let server = Server::start_with_resources();
    let no_code = json!({"coding": [{"system": ANIMALS}]});
    let text_only = json!({"text": "Dog"});
    for (concept, expected) in [
        (
            no_code,
            vec![
                (String::from("error"), String::new()),
                (
                    String::from("warning"),
                    String::from("CodeableConcept.coding[0]"),
                ),
            ],
        ),
        (text_only, vec![(String::from("error"), String::new())]),
    ] {
        let (status, body) = server
            .post(
                "/r4b/ValueSet/$validate-code",
                &parameters(&[
                    ("url", json!({"valueUri": VS_PETS})),
                    ("codeableConcept", json!({"valueCodeableConcept": concept})),
                ]),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(!result(&body), "{body}");
        assert_eq!(issues(&body), expected, "{body}");
    }
}

#[tokio::test]
async fn a_coding_the_code_system_cannot_check_is_not_a_valid_code() {
    let server = Server::start();
    for (input, expression) in [
        (
            ("coding", json!({"valueCoding": {"system": ANIMALS}})),
            "Coding",
        ),
        (
            ("coding", json!({"valueCoding": {"code": "dog"}})),
            "Coding",
        ),
        (
            (
                "codeableConcept",
                json!({"valueCodeableConcept": {"coding": [{"code": "dog"}]}}),
            ),
            "CodeableConcept.coding[0]",
        ),
    ] {
        let (status, body) = server
            .post("/r4b/CodeSystem/$validate-code", &parameters(&[input]))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(!result(&body), "{body}");
        assert_eq!(
            issues(&body),
            [(String::from("warning"), String::from(expression))],
            "{body}"
        );
    }
}

#[tokio::test]
async fn naming_nothing_to_validate_is_still_a_request_error() {
    let server = Server::start();
    let (status, _) = server
        .post(
            "/r4b/CodeSystem/$validate-code",
            &parameters(&[("url", json!({"valueUri": ANIMALS}))]),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
