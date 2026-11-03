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
    );
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
