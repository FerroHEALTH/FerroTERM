// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

use axum::body::Body;
use http::{Request, StatusCode};
use tower::ServiceExt;

use crate::fixture::{Server, problem};

#[tokio::test]
async fn health_answers_ok() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start();
    let request = Request::get("/health").body(Body::empty())?;
    let response = server.router().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

/// The status and the JSON body `path` answers on `server`.
async fn health_of(server: &Server, path: &str) -> (StatusCode, serde_json::Value) {
    let request = Request::get(path).body(Body::empty()).expect("a request");
    let response = server.router().oneshot(request).await.expect("an answer");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .expect("the body");
    (status, serde_json::from_slice(&body).expect("a JSON body"))
}

#[tokio::test]
async fn the_three_health_routes_answer_a_serving_server() {
    let server = Server::start();
    assert_eq!(
        health_of(&server, "/health").await,
        (StatusCode::OK, serde_json::json!({ "state": "up" }))
    );
    assert_eq!(
        health_of(&server, "/health/liveness").await,
        (StatusCode::OK, serde_json::json!({ "state": "up" }))
    );
    assert_eq!(
        health_of(&server, "/health/readiness").await,
        (
            StatusCode::OK,
            serde_json::json!({ "state": "up", "phase": "serving", "indicators": [] })
        )
    );
}

#[tokio::test]
async fn readiness_is_withdrawn_once_the_server_drains() {
    let server = Server::start();
    server.serving.lifecycle().drain();
    assert_eq!(
        health_of(&server, "/health").await,
        (
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({ "state": "down" })
        )
    );
    let (status, body) = health_of(&server, "/health/readiness").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["phase"], "draining");
    assert_eq!(
        health_of(&server, "/health/liveness").await,
        (StatusCode::OK, serde_json::json!({ "state": "up" })),
        "a draining process is still alive"
    );
}

#[tokio::test]
async fn unknown_route_is_an_operation_outcome_under_a_fhir_base_and_a_problem_elsewhere() {
    let server = Server::start();
    let (status, body) = server.get("/r4/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["resourceType"], "OperationOutcome");
    assert_eq!(body["issue"][0]["code"], "not-found");
    assert_eq!(body["issue"][0]["severity"], "error");
    let request = Request::get("/nope").body(Body::empty()).expect("request");
    let (status, body) = problem(server.send(request).await).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["type"], "about:blank");
    assert_eq!(body["status"], 404);
    assert!(
        !body.to_string().contains("nope"),
        "a problem quotes nothing of the request: {body}"
    );
}
