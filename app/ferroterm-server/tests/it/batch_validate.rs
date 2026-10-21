// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `$batch-validate-code`: many validations against one value set, in one
//! request.
//!
//! No `OperationDefinition` declares the operation, in any core package or in
//! the terminology ecosystem IG, so the contract is the IG's own test cases
//! (`batch/batch-validate`) plus the ecosystem's `$validate-code` semantics
//! (#265).

use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::Server;
use ferroterm_testkit::fhir::{ANIMALS, VS_PETS};

/// The `validation` entries of an answer, in order.
fn validations(body: &Value) -> Vec<&Value> {
    body["parameter"]
        .as_array()
        .expect("parameters")
        .iter()
        .filter(|p| p["name"] == "validation")
        .map(|p| &p["resource"])
        .collect()
}

/// One validation's output by name.
fn output<'a>(validation: &'a Value, name: &str) -> Option<&'a Value> {
    validation["parameter"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
}

fn coding(code: &str) -> Value {
    json!({"name": "validation", "resource": {"resourceType": "Parameters", "parameter": [
        {"name": "coding", "valueCoding": {"system": ANIMALS, "code": code}}
    ]}})
}

#[tokio::test]
async fn every_validation_answers_in_its_own_slot_in_order() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .post(
            "/r4b/ValueSet/$batch-validate-code",
            &json!({"resourceType": "Parameters", "parameter": [
                {"name": "url", "valueUri": VS_PETS},
                coding("kitten"),
                coding("dog"),
                coding("nothing-like-it")
            ]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let answers = validations(&body);
    assert_eq!(answers.len(), 3, "one answer per validation: {body}");
    assert_eq!(
        output(answers[0], "result").expect("result")["valueBoolean"],
        true
    );
    assert_eq!(
        output(answers[0], "code").expect("code")["valueCode"],
        "kitten",
        "the answers stay in the order the request stated them"
    );
    assert_eq!(
        output(answers[1], "result").expect("result")["valueBoolean"],
        false,
        "the dog is not a pet in this value set: {body}"
    );
    assert_eq!(
        output(answers[2], "result").expect("result")["valueBoolean"],
        false,
        "a code no system defines: {body}"
    );
}

#[tokio::test]
async fn a_validation_states_its_own_inputs_over_the_shared_ones() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .post(
            "/r4b/ValueSet/$batch-validate-code",
            &json!({"resourceType": "Parameters", "parameter": [
                {"name": "url", "valueUri": VS_PETS},
                {"name": "lenient-display-validation", "valueBoolean": true},
                {"name": "validation", "resource": {"resourceType": "Parameters", "parameter": [
                    {"name": "coding", "valueCoding": {"system": ANIMALS, "code": "kitten", "display": "Wrong"}}
                ]}},
                {"name": "validation", "resource": {"resourceType": "Parameters", "parameter": [
                    {"name": "coding", "valueCoding": {"system": ANIMALS, "code": "kitten", "display": "Wrong"}},
                    {"name": "lenient-display-validation", "valueBoolean": false}
                ]}}
            ]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let answers = validations(&body);
    assert_eq!(
        output(answers[0], "result").expect("result")["valueBoolean"],
        true,
        "the request's lenient display validation applies: {body}"
    );
    assert_eq!(
        output(answers[1], "result").expect("result")["valueBoolean"],
        false,
        "the validation's own value wins for that validation alone: {body}"
    );
}

#[tokio::test]
async fn a_validation_the_server_cannot_run_answers_an_outcome_beside_the_others() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .post(
            "/r4b/ValueSet/$batch-validate-code",
            &json!({"resourceType": "Parameters", "parameter": [
                {"name": "url", "valueUri": VS_PETS},
                {"name": "validation", "resource": {"resourceType": "Parameters", "parameter": [
                    {"name": "codingX", "valueCoding": {"system": ANIMALS, "code": "kitten"}}
                ]}},
                coding("kitten")
            ]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "the batch itself succeeds: {body}");
    let answers = validations(&body);
    assert_eq!(answers.len(), 2);
    assert_eq!(
        answers[0]["resourceType"], "OperationOutcome",
        "the refusal lands in its own slot: {body}"
    );
    assert_eq!(
        output(answers[1], "result").expect("result")["valueBoolean"],
        true,
        "the other validations still answer: {body}"
    );
}

#[tokio::test]
async fn the_operation_is_offered_on_post_alone() {
    let server = Server::start_with_resources();
    let request = http::Request::get(format!("/r4b/ValueSet/$batch-validate-code?url={VS_PETS}"))
        .body(axum::body::Body::empty())
        .expect("request");
    let response = server.send(request).await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
