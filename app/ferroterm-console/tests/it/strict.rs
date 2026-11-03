// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An answer that does not decode at the release that served it: the page
//! draws the console's error state, which names the resource, the element
//! path and the version, and never a partial page.

use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::provider::{console, get, send, text};

#[tokio::test]
async fn capabilities_that_do_not_decode_at_their_release_draw_the_error_state() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/r4b/metadata"))
        .and(query_param("mode", "terminology"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "TerminologyCapabilities",
            "status": "active",
            "date": "2026-01-01",
            "kind": "instance",
            "codeSystem": [{ "uri": "https://terminology.example/x", "content": "complete" }]
        })))
        .mount(&server)
        .await;
    let state = console(
        None,
        &format!("[ferroterm]\nbase_url = \"{}/\"\n", server.uri()),
    )
    .await;
    let page = text(send(&state, get("/systems?fhir=r4b", &[])).await).await;
    assert!(
        page.contains("FHIR R4B: ")
            && page.contains("TerminologyCapabilities.codeSystem[0]")
            && page.contains("content"),
        "the error names the version, the resource and the element path: {page}"
    );
    assert!(
        !page.contains("https://terminology.example/x</"),
        "no partial page draws the system the answer named"
    );
}

/// The About page of a console whose server is reached at `base`.
async fn about_at(base: &str) -> String {
    let state = console(None, &format!("[ferroterm]\nbase_url = \"{base}/\"\n")).await;
    text(send(&state, get("/about?fhir=r4", &[])).await).await
}

#[tokio::test]
async fn the_about_page_draws_a_statement_that_does_not_decode_as_its_refusal() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/r4/metadata"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "CapabilityStatement",
            "fhirVersion": "4.0.1"
        })))
        .mount(&server)
        .await;
    let page = about_at(&server.uri()).await;
    assert!(
        page.contains("The answer did not read") && page.contains("FHIR R4: CapabilityStatement"),
        "the page names the cause: {page}"
    );
    assert!(
        !page.contains("The server did not answer"),
        "a server that answered is never said not to have"
    );
}

#[tokio::test]
async fn the_about_page_draws_an_unreachable_server_as_its_refusal() {
    let page = about_at("http://127.0.0.1:9").await;
    assert!(
        page.contains("The request got no answer") && page.contains("127.0.0.1:9/r4/metadata"),
        "the page names the cause and the address asked: {page}"
    );
}

#[tokio::test]
async fn a_value_set_that_does_not_decode_at_its_release_draws_the_error_state() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/r4b/ValueSet/animals"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "ValueSet",
            "id": "animals",
            "url": "https://terminology.example/ValueSet/animals"
        })))
        .mount(&server)
        .await;
    let state = console(
        None,
        &format!("[ferroterm]\nbase_url = \"{}/\"\n", server.uri()),
    )
    .await;
    let page = text(send(&state, get("/valuesets/animals?fhir=r4b", &[])).await).await;
    assert!(
        page.contains("FHIR R4B: ") && page.contains("ValueSet") && page.contains("status"),
        "the error names the version, the resource and the element it lacks: {page}"
    );
}

#[tokio::test]
async fn a_concept_map_that_does_not_decode_at_its_release_draws_the_error_state() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/r5/ConceptMap/animals"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "ConceptMap",
            "id": "animals",
            "status": "active",
            "sourceUri": "https://terminology.example/ValueSet/a"
        })))
        .mount(&server)
        .await;
    let state = console(
        None,
        &format!("[ferroterm]\nbase_url = \"{}/\"\n", server.uri()),
    )
    .await;
    let page = text(send(&state, get("/conceptmaps/animals?fhir=r5", &[])).await).await;
    assert!(
        page.contains("FHIR R5: ") && page.contains("ConceptMap") && page.contains("sourceUri"),
        "R5 renamed `sourceUri`, so the error names the member R5 does not define: {page}"
    );
}

#[tokio::test]
async fn a_lookup_answer_that_does_not_decode_at_its_release_draws_the_error_state() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/r5/CodeSystem/$lookup"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "Parameters",
            "parameter": [{ "name": "display", "valueString": "Red", "colour": "red" }]
        })))
        .mount(&server)
        .await;
    let state = console(
        None,
        &format!("[ferroterm]\nbase_url = \"{}/\"\n", server.uri()),
    )
    .await;
    let page = text(
        send(
            &state,
            get(
                "/lookup?fhir=r5&system=https%3A%2F%2Fterminology.example%2Fcolours&code=red",
                &[],
            ),
        )
        .await,
    )
    .await;
    assert!(
        page.contains("FHIR R5: ")
            && page.contains("Parameters.parameter[0]")
            && page.contains("colour"),
        "the error names the version, the resource and the member R5 does not define: {page}"
    );
    assert!(
        !page.contains(">Red<"),
        "no partial page draws the display the answer named"
    );
}

#[tokio::test]
async fn a_validation_answer_that_does_not_decode_at_its_release_draws_the_error_state() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/r4/ValueSet/$validate-code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "Parameters",
            "parameter": [
                { "name": "result", "valueBoolean": false },
                { "name": "issues", "resource": {
                    "resourceType": "OperationOutcome",
                    "issue": [{ "code": "invalid", "diagnostics": "Wrong display" }]
                } }
            ]
        })))
        .mount(&server)
        .await;
    let state = console(
        None,
        &format!("[ferroterm]\nbase_url = \"{}/\"\n", server.uri()),
    )
    .await;
    let page = text(
        send(
            &state,
            get(
                "/validate?fhir=r4&url=https%3A%2F%2Fterminology.example%2Fvs\
                 &system=https%3A%2F%2Fterminology.example%2Fcolours&code=red",
                &[],
            ),
        )
        .await,
    )
    .await;
    assert!(
        page.contains("FHIR R4: ")
            && page.contains("Parameters.parameter[1]")
            && page.contains("severity"),
        "the error names the version, the resource and the element the issue lacks: {page}"
    );
    assert!(
        !page.contains("Wrong display"),
        "no partial page draws the issue the answer carried"
    );
}

#[tokio::test]
async fn a_history_that_does_not_decode_at_its_release_draws_the_error_state() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/r4b/CodeSystem/_history"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "Bundle",
            "type": "history",
            "entry": [{
                "resource": {
                    "resourceType": "CodeSystem",
                    "id": "colours",
                    "status": "active",
                    "title": "Colours"
                },
                "request": { "method": "PUT", "url": "CodeSystem/colours" }
            }]
        })))
        .mount(&server)
        .await;
    let state = console(
        None,
        &format!("[ferroterm]\nbase_url = \"{}/\"\n", server.uri()),
    )
    .await;
    let page = text(send(&state, get("/?fhir=r4b", &[])).await).await;
    assert!(
        page.contains("FHIR R4B: ") && page.contains("Bundle.entry[0]") && page.contains("content"),
        "the error names the version, the resource and the element the entry lacks: {page}"
    );
    assert!(
        !page.contains(">Colours<"),
        "no partial page draws the write the answer named"
    );
}

#[tokio::test]
async fn a_listing_that_does_not_decode_at_its_release_draws_the_error_state() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/r4b/metadata"))
        .and(query_param("mode", "terminology"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "../../fixtures/terminology-capabilities-r4b.json"
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/r4b/CodeSystem"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "Bundle",
            "type": "searchset",
            "entry": [{ "resource": {
                "resourceType": "CodeSystem",
                "url": "https://terminology.example/CodeSystem/listed",
                "title": "Listed",
                "status": "active"
            } }]
        })))
        .mount(&server)
        .await;
    let state = console(
        None,
        &format!("[ferroterm]\nbase_url = \"{}/\"\n", server.uri()),
    )
    .await;
    let page = text(send(&state, get("/systems?fhir=r4b", &[])).await).await;
    assert!(
        page.contains("FHIR R4B: ") && page.contains("Bundle.entry[0]") && page.contains("content"),
        "the error names the version, the resource and the element the entry lacks: {page}"
    );
    assert!(
        !page.contains(">Listed<"),
        "no partial page draws the system the answer named"
    );
}

#[tokio::test]
async fn a_code_system_that_does_not_decode_at_its_release_draws_the_error_state() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/r4b/metadata"))
        .and(query_param("mode", "terminology"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "../../fixtures/terminology-capabilities-r4b.json"
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/r4b/CodeSystem"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "Bundle",
            "type": "searchset",
            "entry": [{ "resource": {
                "resourceType": "CodeSystem",
                "url": "http://example.org/fhir/CodeSystem/animals",
                "publisher": "A synthetic publisher",
                "content": "complete"
            } }]
        })))
        .mount(&server)
        .await;
    let state = console(
        None,
        &format!("[ferroterm]\nbase_url = \"{}/\"\n", server.uri()),
    )
    .await;
    let page = text(
        send(
            &state,
            get(
                "/systems/http%3A%2F%2Fexample.org%2Ffhir%2FCodeSystem%2Fanimals?fhir=r4b",
                &[],
            ),
        )
        .await,
    )
    .await;
    assert!(
        page.contains("FHIR R4B: ") && page.contains("Bundle.entry[0]") && page.contains("status"),
        "the error names the version, the resource and the element the entry lacks: {page}"
    );
    assert!(
        !page.contains("A synthetic publisher"),
        "no partial page draws the facts the answer named"
    );
}

#[tokio::test]
async fn names_that_do_not_decode_at_their_release_draw_the_operation_page_s_error_state() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/r4b/metadata"))
        .and(query_param("mode", "terminology"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "../../fixtures/terminology-capabilities-r4b.json"
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/r4b/CodeSystem"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "Bundle",
            "type": "searchset",
            "entry": [{ "resource": {
                "resourceType": "CodeSystem",
                "url": "https://terminology.example/CodeSystem/offered",
                "title": "Offered",
                "status": "active"
            } }]
        })))
        .mount(&server)
        .await;
    let state = console(
        None,
        &format!("[ferroterm]\nbase_url = \"{}/\"\n", server.uri()),
    )
    .await;
    let page = text(send(&state, get("/lookup?fhir=r4b", &[])).await).await;
    assert!(
        page.contains("FHIR R4B: ") && page.contains("Bundle.entry[0]") && page.contains("content"),
        "the error names the version, the resource and the element the entry lacks: {page}"
    );
    assert!(
        !page.contains(">Offered<"),
        "no picker offers what the answer named"
    );
}
