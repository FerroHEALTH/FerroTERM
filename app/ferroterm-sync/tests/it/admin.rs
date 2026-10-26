// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The service's admin listener: the manual triggers, the run records, the
//! metrics, and the health probe.
//!
//! No FHIR specification governs these routes, so these are our own design
//! (#579).

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt as _;

use crate::support::{Entry, Harness, TestClock};

/// One request, answered as JSON.
async fn json(router: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("the listener answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("the body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("JSON")
    };
    (status, body)
}

/// The exposition `GET /metrics` answers.
async fn scrape(router: &Router) -> String {
    let request = Request::get("/metrics")
        .body(Body::empty())
        .expect("a request");
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("the listener answers");
    assert_eq!(response.status(), StatusCode::OK, "the scrape answers");
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body");
    String::from_utf8(bytes.to_vec()).expect("UTF-8")
}

#[tokio::test]
async fn a_manual_run_answers_its_summary_and_shows_on_runs() {
    let harness = Harness::new().await;
    harness
        .publish(&[Entry::rf2(
            "11000146104",
            "20260930",
            "2026-09-30T09:00:00Z",
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );
    let router = ferroterm_sync::admin::router(service);

    let (status, summary) = json(
        &router,
        Request::post("/run")
            .body(Body::empty())
            .expect("a request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "the run answers when it is done");
    assert_eq!(summary["outcome"], "ok", "with its outcome: {summary}");
    assert_eq!(summary["trigger"], "manual", "and what started it");
    assert_eq!(summary["entries_taken"], 1, "and what it took");
    let id = summary["id"].as_str().expect("the run has an identifier");

    let (status, runs) = json(
        &router,
        Request::get("/runs")
            .body(Body::empty())
            .expect("a request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the listing answers");
    assert_eq!(
        runs.as_array().map(Vec::len),
        Some(1),
        "the run is in the listing: {runs}"
    );

    let (status, record) = json(
        &router,
        Request::get(format!("/runs/{id}"))
            .body(Body::empty())
            .expect("a request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the record answers");
    assert_eq!(
        record["activation"]["activated"].as_array().map(Vec::len),
        Some(1),
        "and carries what reached the server: {record}"
    );
    assert_eq!(
        record["sources"][0]["entries_seen"], 1,
        "and what the feed offered"
    );

    let text = scrape(&router).await;
    assert!(
        text.contains("ferroterm_sync_runs_total{outcome=\"ok\"} 1"),
        "the metrics count the run: {text}"
    );
    assert!(
        text.contains("ferroterm_sync_entries_taken_total 1"),
        "and what it took: {text}"
    );
    assert!(
        text.contains("ferroterm_sync_bytes_staged_total"),
        "and what it staged: {text}"
    );
}

#[tokio::test]
async fn activate_serves_what_a_manual_mode_run_staged() {
    let mut harness = Harness::new().await;
    harness.config.activation = ferroterm_sync::config::Activation::Manual;
    harness
        .publish(&[Entry::rf2(
            "11000146104",
            "20260930",
            "2026-09-30T09:00:00Z",
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );
    let router = ferroterm_sync::admin::router(service);

    let (_, staged) = json(
        &router,
        Request::post("/run")
            .body(Body::empty())
            .expect("a request"),
    )
    .await;
    assert_eq!(staged["activated"], 0, "the run staged and stopped");

    let (status, activation) = json(
        &router,
        Request::post("/activate")
            .body(Body::empty())
            .expect("a request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "the activation answers");
    assert_eq!(
        activation["activated"], 1,
        "and put the staged release in front of the server: {activation}"
    );
    assert!(
        harness
            .index_root()
            .join("snomed-11000146104-20260930")
            .is_dir(),
        "which is what the index root now holds"
    );
}

#[tokio::test]
async fn a_failed_run_shows_on_runs_and_on_metrics() {
    let mut harness = Harness::new().await;
    harness.config.build_command = crate::support::failing_build(harness.dir.path());
    harness
        .publish(&[Entry::rf2(
            "11000146104",
            "20260930",
            "2026-09-30T09:00:00Z",
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );
    let router = ferroterm_sync::admin::router(service);

    let (_, summary) = json(
        &router,
        Request::post("/run")
            .body(Body::empty())
            .expect("a request"),
    )
    .await;
    assert_eq!(summary["outcome"], "failed", "the run failed: {summary}");

    let (_, runs) = json(
        &router,
        Request::get("/runs")
            .body(Body::empty())
            .expect("a request"),
    )
    .await;
    assert_eq!(
        runs[0]["outcome"], "failed",
        "the failure is on the listing: {runs}"
    );
    let text = scrape(&router).await;
    assert!(
        text.contains("ferroterm_sync_runs_total{outcome=\"failed\"} 1"),
        "and on the scrape: {text}"
    );
}

#[tokio::test]
async fn health_answers_and_an_unknown_route_does_not() {
    let harness = Harness::new().await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );
    let router = ferroterm_sync::admin::router(service);

    let health = router
        .clone()
        .oneshot(
            Request::get("/health")
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("the listener answers");
    assert_eq!(health.status(), StatusCode::OK, "the probe answers");

    let unknown = router
        .clone()
        .oneshot(
            Request::get("/reload")
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("the listener answers");
    assert_eq!(
        unknown.status(),
        StatusCode::NOT_FOUND,
        "the service's listener serves only its own routes"
    );
}

#[tokio::test]
async fn health_withdraws_readiness_once_the_service_drains() {
    let harness = Harness::new().await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );
    let router = ferroterm_sync::admin::router(std::sync::Arc::clone(&service));

    let (status, body) = json(
        &router,
        Request::get("/health")
            .body(Body::empty())
            .expect("a request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "a serving service is ready");
    assert_eq!(body, serde_json::json!({ "state": "up" }));

    service.lifecycle().drain();
    let (status, body) = json(
        &router,
        Request::get("/health")
            .body(Body::empty())
            .expect("a request"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "a draining service is not"
    );
    assert_eq!(body, serde_json::json!({ "state": "down" }));
}

#[tokio::test]
async fn a_run_identifier_that_is_a_path_is_refused() {
    let harness = Harness::new().await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );
    let router = ferroterm_sync::admin::router(service);

    let (status, body) = json(
        &router,
        Request::get("/runs/..%2F..%2Fstate%2Fstate")
            .body(Body::empty())
            .expect("a request"),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a record is read by identifier, never by path: {body}"
    );
}
