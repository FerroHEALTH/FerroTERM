// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `POST [base]` with a `batch` Bundle
//! (<https://hl7.org/fhir/R4B/http.html#transaction>).

use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::Server;
use ferroterm_testkit::fhir::{ANIMALS, VS_PETS};

/// A `batch` Bundle of `entries`.
fn batch(entries: &[Value]) -> Value {
    json!({"resourceType": "Bundle", "type": "batch", "entry": entries})
}

/// A `GET` entry for `url`.
fn get(url: &str) -> Value {
    json!({"request": {"method": "GET", "url": url}})
}

/// A `POST` entry invoking `url` with a `Parameters` body.
fn post(url: &str, parameters: &[Value]) -> Value {
    json!({
        "request": {"method": "POST", "url": url},
        "resource": {"resourceType": "Parameters", "parameter": parameters}
    })
}

/// The `response.status` of each entry, in order.
fn statuses(body: &Value) -> Vec<&str> {
    body["entry"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|entry| entry.get("response")?.get("status")?.as_str())
        .collect()
}

/// The named parameter of an entry's `Parameters` resource.
fn parameter<'a>(entry: &'a Value, name: &str) -> Option<&'a Value> {
    entry
        .get("resource")?
        .get("parameter")?
        .as_array()?
        .iter()
        .find(|held| held["name"] == name)
}

#[tokio::test]
async fn a_batch_runs_every_operation_and_answers_one_entry_each_in_order() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .post(
            "/r4b",
            &batch(&[
                get(&format!("CodeSystem/$lookup?system={ANIMALS}&code=cat")),
                post(
                    "ValueSet/$expand",
                    &[json!({"name": "url", "valueUri": VS_PETS})],
                ),
                post(
                    "ValueSet/$validate-code",
                    &[
                        json!({"name": "url", "valueUri": VS_PETS}),
                        json!({"name": "system", "valueUri": ANIMALS}),
                        json!({"name": "code", "valueCode": "kitten"}),
                    ],
                ),
                get(&format!("CodeSystem/$validate-code?url={ANIMALS}&code=cat")),
            ]),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["resourceType"], "Bundle");
    assert_eq!(body["type"], "batch-response");
    assert_eq!(
        statuses(&body),
        ["200 OK", "200 OK", "200 OK", "200 OK"],
        "one entry per request, in the order they were sent"
    );

    let entries = body["entry"].as_array().expect("entries");
    let lookup = entries.first().expect("the lookup");
    assert_eq!(lookup["resource"]["resourceType"], "Parameters");
    assert_eq!(
        parameter(lookup, "display").map(|held| &held["valueString"]),
        Some(&json!("Cat"))
    );

    let expansion = entries.get(1).expect("the expansion");
    assert_eq!(expansion["resource"]["resourceType"], "ValueSet");
    assert_eq!(expansion["resource"]["expansion"]["total"], 2);

    let validated = entries.get(2).expect("the value set validation");
    assert_eq!(
        parameter(validated, "result").map(|held| &held["valueBoolean"]),
        Some(&json!(true))
    );
}

#[tokio::test]
async fn a_failing_entry_carries_its_own_outcome_and_leaves_the_others_standing() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .post(
            "/r4b",
            &batch(&[
                get(&format!("CodeSystem/$lookup?system={ANIMALS}&code=nope")),
                get(&format!("CodeSystem/$lookup?system={ANIMALS}&code=cat")),
                get("CodeSystem/$nonesuch?code=cat"),
                json!({"resource": {"resourceType": "Parameters"}}),
            ]),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a batch answers 200 even when entries fail: {body}"
    );
    assert_eq!(
        statuses(&body),
        [
            "400 Bad Request",
            "200 OK",
            "404 Not Found",
            "400 Bad Request"
        ]
    );
    let entries = body["entry"].as_array().expect("entries");
    let refused = entries.first().expect("the unknown code");
    assert_eq!(
        refused["resource"]["resourceType"], "OperationOutcome",
        "a failed entry answers the body the same request would have answered alone"
    );
    assert!(
        refused["response"]["outcome"].is_null(),
        "`response.outcome` carries hints and warnings, never the error"
    );
    assert_eq!(
        entries.get(1).expect("the good lookup")["resource"]["resourceType"],
        "Parameters",
        "the entry beside a failure still answers"
    );
    assert!(
        entries.get(3).expect("the entry without a request")["resource"]["issue"][0]["diagnostics"]
            .as_str()
            .is_some_and(|text| text.contains("request")),
        "an entry without a request says so: {body}"
    );
}

#[tokio::test]
async fn a_transaction_bundle_is_refused_with_not_supported() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .post(
            "/r4b",
            &json!({
                "resourceType": "Bundle",
                "type": "transaction",
                "entry": [get(&format!("CodeSystem/$lookup?system={ANIMALS}&code=cat"))]
            }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["resourceType"], "OperationOutcome");
    assert_eq!(body["issue"][0]["code"], "not-supported");
    assert_eq!(
        body["issue"][0]["details"]["coding"][0]["code"], "not-supported",
        "the refusal carries the ecosystem's issue type"
    );
}

#[tokio::test]
async fn a_bundle_of_another_type_and_a_body_that_is_no_bundle_are_refused() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .post(
            "/r4b",
            &json!({"resourceType": "Bundle", "type": "searchset"}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["issue"][0]["code"], "value");

    let (status, body) = server
        .post("/r4b", &json!({"resourceType": "Parameters"}))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["resourceType"], "OperationOutcome");
}

#[tokio::test]
async fn an_entry_method_the_server_does_not_answer_is_refused_alone() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .post(
            "/r4b",
            &batch(&[
                json!({"request": {"method": "PATCH", "url": "ValueSet/$expand"}}),
                get(&format!("CodeSystem/$lookup?system={ANIMALS}&code=cat")),
            ]),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(statuses(&body), ["405 Method Not Allowed", "200 OK"]);
}

#[tokio::test]
async fn a_batch_answers_on_every_served_version() {
    let server = Server::start_with_resources();
    for base in ["/r4", "/r4b", "/r5", "/r6"] {
        let (status, body) = server
            .post(
                base,
                &batch(&[get(&format!(
                    "CodeSystem/$lookup?system={ANIMALS}&code=cat"
                ))]),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["type"], "batch-response", "{base}");
        assert_eq!(statuses(&body), ["200 OK"], "{base}");
    }
}

#[tokio::test]
async fn an_entry_keeps_its_full_url_so_a_client_can_pair_it_with_its_request() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .post(
            "/r4b",
            &batch(&[json!({
                "fullUrl": "urn:uuid:1c2f0a3e-0000-4000-8000-000000000001",
                "request": {"method": "GET", "url": format!("CodeSystem/$lookup?system={ANIMALS}&code=cat")}
            })]),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["entry"][0]["fullUrl"],
        "urn:uuid:1c2f0a3e-0000-4000-8000-000000000001"
    );
}

#[tokio::test]
async fn the_capability_statement_declares_the_batch_interaction() {
    let server = Server::start_with_resources();
    let (status, body) = server.get("/r4b/metadata").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let modes: Vec<&str> = body["rest"][0]["interaction"]
        .as_array()
        .expect("the system interactions")
        .iter()
        .filter_map(|held| held["code"].as_str())
        .collect();
    assert_eq!(modes, ["batch"]);
}

// The bearer gate of #586 wraps the write routes, and `POST [base]` is open:
// that only holds because a batch entry can never reach a write. This pins the
// invariant so an arm added to `version/batch.rs` cannot open a bypass silently.
#[tokio::test]
async fn a_batch_entry_never_reaches_a_write() {
    let server = Server::start_persisting();
    let system = json!({
        "resourceType": "CodeSystem",
        "url": "http://ferroterm.test/CodeSystem/smuggled",
        "status": "active",
        "content": "complete",
        "concept": [{"code": "a"}]
    });
    let entry = |method: &str, url: &str, resource: Option<&Value>| {
        let mut held = json!({"request": {"method": method, "url": url}});
        if let Some(resource) = resource {
            held["resource"] = resource.clone();
        }
        held
    };
    let (status, body) = server
        .post(
            "/r4b",
            &batch(&[
                entry("PUT", "CodeSystem/smuggled", Some(&system)),
                entry("POST", "CodeSystem", Some(&system)),
                entry("DELETE", "CodeSystem/smuggled", None),
                entry("PATCH", "CodeSystem/smuggled", Some(&system)),
            ]),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let refused = statuses(&body);
    assert_eq!(refused.len(), 4, "{body}");
    for (index, entry_status) in refused.iter().enumerate() {
        assert!(
            entry_status.starts_with("405") || entry_status.starts_with("400"),
            "entry {index} was answered {entry_status}, not refused: {body}"
        );
    }
    let (status, _) = server.get("/r4b/CodeSystem/smuggled").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "nothing was written through the batch"
    );
}
