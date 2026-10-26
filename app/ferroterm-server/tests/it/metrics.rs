// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The scrape endpoint and the request id
//! (<https://prometheus.io/docs/instrumenting/exposition_formats/>; no FHIR
//! specification governs either, so both are our own design, #122). The
//! request histogram is `http.server.request.duration` of the OpenTelemetry
//! semantic conventions, which Prometheus reads as
//! `http_server_request_duration_seconds`.

use axum::body::Body;
use http::{Request, StatusCode};

use crate::fixture::Server;
use ferroterm_testkit::snomed::{CAT, item, sctid};

/// The body of a plain `GET`, as text.
async fn text(server: &Server, uri: &str) -> (StatusCode, String) {
    let request = Request::get(uri).body(Body::empty()).expect("request");
    let response = server.send(request).await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn the_scrape_carries_the_loaded_systems_and_the_answered_requests() {
    let server = Server::start();
    let (status, before) = text(&server, "/metrics").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        before.contains("ferroterm_code_system_loaded{system=\"http://snomed.info/sct\""),
        "the loaded edition is declared before any request: {before}"
    );
    let code = sctid(item(CAT));
    let (status, _) = server
        .get(&format!(
            "/r4b/CodeSystem/$lookup?system=http://snomed.info/sct&code={code}"
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, after) = text(&server, "/metrics").await;
    assert!(
        after.lines().any(|line| {
            line.starts_with("http_server_request_duration_seconds_count{")
                && line.contains("http_request_method=\"GET\"")
                && line.contains("http_route=\"/r4b/CodeSystem/$lookup\"")
                && line.contains("http_response_status_code=\"200\"")
                && line.ends_with(" 1")
        }),
        "the operation is counted by its route, not its URI: {after}"
    );
    assert!(
        after.contains("http_server_request_duration_seconds_sum{"),
        "the duration histogram is exposed: {after}"
    );
}

#[tokio::test]
async fn a_refusal_is_counted_under_its_own_status() {
    let server = Server::start();
    let (status, _) = server.get("/r4b/CodeSystem/$lookup?system=nothing").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, scrape) = text(&server, "/metrics").await;
    assert!(
        scrape.contains("http_response_status_code=\"400\""),
        "a client error keeps its own series: {scrape}"
    );
}

#[tokio::test]
async fn every_response_carries_a_request_id_and_echoes_the_client_s() {
    let server = Server::start();
    let request = Request::get("/health")
        .body(Body::empty())
        .expect("request");
    let response = server.send(request).await;
    let generated = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .expect("a request id");
    assert_eq!(generated.len(), 36, "a UUID: {generated}");

    let request = Request::get("/health")
        .header("x-request-id", "the-client-s-own")
        .body(Body::empty())
        .expect("request");
    let response = server.send(request).await;
    assert_eq!(
        response
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok()),
        Some("the-client-s-own"),
        "the client's id is echoed"
    );
}

#[tokio::test]
async fn an_unusable_request_id_is_replaced_rather_than_echoed() {
    let server = Server::start();
    let request = Request::get("/health")
        .header("x-request-id", "x".repeat(200))
        .body(Body::empty())
        .expect("request");
    let response = server.send(request).await;
    let answered = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .expect("a request id");
    assert_eq!(answered.len(), 36, "a fresh UUID, not the long one");
}
