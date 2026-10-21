// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

use axum::body::Body;
use http::{Request, StatusCode};
use tower::ServiceExt;

use crate::fixture::Server;

#[tokio::test]
async fn health_answers_ok() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start();
    let request = Request::get("/health").body(Body::empty())?;
    let response = server.router().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn unknown_route_is_an_operation_outcome() {
    let server = Server::start();
    let (status, body) = server.get("/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["resourceType"], "OperationOutcome");
    assert_eq!(body["issue"][0]["code"], "not-found");
    assert_eq!(body["issue"][0]["severity"], "error");
}
