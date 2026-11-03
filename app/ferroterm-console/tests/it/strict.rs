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
