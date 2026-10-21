// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The lexical forms of FHIR primitives on the write path, refused by the
//! generated codec (<https://hl7.org/fhir/R5/datatypes.html#primitive>) and
//! answered as a `400` with the element path
//! (<https://hl7.org/fhir/R4B/http.html#2.21.0.10.1>).

use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::{self, Server};

const SYSTEM: &str = "http://ferroterm.test/CodeSystem/forms";

fn forms() -> Value {
    json!({
        "resourceType": "CodeSystem",
        "url": SYSTEM,
        "version": "1.0",
        "status": "active",
        "date": "2001-06-15T12:00:00Z",
        "content": "complete",
        "property": [{"code": "retirementDate", "type": "dateTime"}],
        "concept": [
            {"code": "one", "display": "One",
             "property": [{"code": "retirementDate", "valueDateTime": "2999-01-01"}]}
        ]
    })
}

async fn refused(server: &Server, version: &str, body: &Value) -> Value {
    let response = server
        .put(&format!("/{version}/CodeSystem/forms"), body)
        .await;
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "{version}: {body}"
    );
    let (_, outcome) = fixture::json(response).await;
    assert_eq!(
        outcome.get("resourceType").and_then(Value::as_str),
        Some("OperationOutcome"),
        "{outcome}"
    );
    assert_eq!(
        outcome.pointer("/issue/0/code").and_then(Value::as_str),
        Some("invalid"),
        "{outcome}"
    );
    outcome
}

fn expression(outcome: &Value) -> String {
    outcome
        .pointer("/issue/0/expression/0")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

async fn stored(server: &Server, version: &str, body: &Value) {
    let response = server
        .put(&format!("/{version}/CodeSystem/forms"), body)
        .await;
    assert!(
        response.status() == StatusCode::CREATED || response.status() == StatusCode::OK,
        "{version}: a well-formed resource is stored, got {}: {body}",
        response.status()
    );
}

// A string outside its primitive's regex is refused by the codec of the
// version the resource arrives in, and the outcome names the element.
#[tokio::test]
async fn a_primitive_outside_its_lexical_form_is_refused_with_its_path() {
    let server = Server::start_persisting();
    for version in ["r4", "r4b", "r5", "r6"] {
        stored(&server, version, &forms()).await;

        let mut body = forms();
        body["date"] = json!("yesterday");
        let outcome = refused(&server, version, &body).await;
        assert_eq!(
            expression(&outcome),
            "CodeSystem.date",
            "{version}: {outcome}"
        );

        let mut body = forms();
        body["concept"][0]["property"][0]["valueDateTime"] = json!("2001-6");
        let outcome = refused(&server, version, &body).await;
        assert!(
            expression(&outcome).starts_with("CodeSystem.concept[0].property[0].value"),
            "{version}: {outcome}"
        );

        let mut body = forms();
        body["url"] = json!("http://ferroterm.test/Code System/forms");
        let outcome = refused(&server, version, &body).await;
        assert_eq!(expression(&outcome), "CodeSystem.url", "{version}");

        let mut body = forms();
        body["status"] = json!(" active");
        let outcome = refused(&server, version, &body).await;
        assert_eq!(expression(&outcome), "CodeSystem.status", "{version}");
    }
}

// The partial forms the specification allows stay accepted.
#[tokio::test]
async fn every_partial_date_time_form_of_the_specification_is_stored() {
    let server = Server::start_persisting();
    for date in [
        "2001",
        "2001-06",
        "2001-06-15",
        "2001-06-15T12:00:00.123+02:00",
    ] {
        let mut body = forms();
        body["date"] = json!(date);
        stored(&server, "r4b", &body).await;
    }
}

// The packages differ on a time without its offset: R4, R4B, and the R6 ballot
// keep the offset inside the time group, R5 makes it optional. The codec reads
// each version's own regex, so the wire follows the package, not a hand copy.
#[tokio::test]
async fn a_time_without_its_offset_follows_each_versions_own_regex() {
    let server = Server::start_persisting();
    let mut body = forms();
    body["date"] = json!("2001-06-15T12:00:00");
    for version in ["r4", "r4b", "r6"] {
        let outcome = refused(&server, version, &body).await;
        assert_eq!(expression(&outcome), "CodeSystem.date", "{version}");
    }
    stored(&server, "r5", &body).await;
}
