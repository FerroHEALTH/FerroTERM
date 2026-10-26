// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stack of `ferrohealth-server` around the FHIR routes: a panic, a
//! request past `server.request_timeout_ms` and a body past
//! `server.body_limit_bytes` each answer an `OperationOutcome`.

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::routing::get;
use ferrohealth_server::section::ServerSection;
use ferrohealth_server::stack;
use http::{Request, StatusCode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tower::ServiceExt;

use crate::fixture::{Server, json};

/// The one issue of an `OperationOutcome` body.
fn issue(body: &serde_json::Value) -> &serde_json::Value {
    assert_eq!(body["resourceType"], "OperationOutcome", "{body}");
    body.pointer("/issue/0").expect("one issue")
}

#[tokio::test]
async fn a_request_past_the_timeout_is_an_operation_outcome_408()
-> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start_with_server(ServerSection {
        request_timeout_ms: 200,
        ..ServerSection::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (stop, stopped) = oneshot::channel::<()>();
    let running = tokio::spawn(ferroterm_server::serve_until(
        listener,
        None,
        server.serving.clone(),
        async {
            let _stop_or_dropped = stopped.await;
        },
    ));

    // A `$lookup` whose body never arrives in full outlasts the timeout.
    let mut stream = TcpStream::connect(address).await?;
    stream
        .write_all(
            b"POST /r4/CodeSystem/$lookup HTTP/1.1\r\nHost: localhost\r\n\
              Content-Type: application/fhir+json\r\nContent-Length: 1000\r\n\r\n{",
        )
        .await?;
    let mut answer = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut answer)).await??;
    let answer = String::from_utf8(answer)?;
    assert!(answer.starts_with("HTTP/1.1 408 "), "{answer}");
    let (_head, body) = answer.split_once("\r\n\r\n").ok_or("a head and a body")?;
    let body: serde_json::Value = serde_json::from_str(body)?;
    let issue = issue(&body);
    assert_eq!(issue["code"], "timeout");
    assert_eq!(issue["severity"], "error");

    stop.send(())
        .map_err(|()| "the server dropped its shutdown receiver")?;
    tokio::time::timeout(Duration::from_secs(15), running).await???;
    Ok(())
}

#[tokio::test]
async fn a_body_past_the_limit_is_an_operation_outcome_413() {
    let server = Server::start_with_server(ServerSection {
        body_limit_bytes: 64,
        ..ServerSection::default()
    });
    let body = serde_json::json!({
        "resourceType": "Parameters",
        "parameter": [{ "name": "system", "valueUri": "http://snomed.info/sct" }],
    })
    .to_string();
    assert!(body.len() > 64, "the body is past the limit");
    let request = Request::post("/r4/CodeSystem/$lookup")
        .header(http::header::CONTENT_TYPE, "application/fhir+json")
        .header(http::header::CONTENT_LENGTH, body.len())
        .body(Body::from(body))
        .expect("request");
    let response = server.router().oneshot(request).await.expect("response");
    let (status, body) = json(response).await;
    assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, status);
    assert_eq!(issue(&body)["code"], "too-long");
}

/// A handler that panics with a message the answer must not carry.
async fn boom() -> &'static str {
    panic!("patient Qz7sentinel")
}

#[tokio::test]
async fn a_panic_is_an_operation_outcome_500_without_its_message() {
    let server = Server::start();
    // No FerroTERM route panics on purpose, so the panic comes from a route
    // mounted under the server's own stack policy.
    let product = Router::new().route("/r4/boom", get(boom));
    let app = stack::app(
        product,
        server.serving.health().clone(),
        &ServerSection::default(),
        ferroterm_server::stack_policy(&server.serving),
    );
    let response = app
        .oneshot(
            Request::get("/r4/boom")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .expect("a request id");
    let (status, body) = json(response).await;
    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, status);
    let issue = issue(&body);
    assert_eq!(issue["code"], "exception");
    let diagnostics = issue["diagnostics"].as_str().expect("diagnostics");
    assert!(diagnostics.contains(&request_id), "{diagnostics}");
    assert!(!body.to_string().contains("Qz7sentinel"), "{body}");
}
