// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server stops when its shutdown future completes: readiness turns
//! `503`, the requests in flight finish, and the server returns once
//! `server.shutdown_timeout_ms` passes with one still open, so it decides its
//! own exit.

use std::net::SocketAddr;
use std::time::Duration;

use ferrohealth_server::section::ServerSection;
use ferrohealth_server::serve::{Drained, ServeError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::fixture::Server;

/// A running instance on ephemeral loopback ports, until `stop`.
struct Served {
    fhir: SocketAddr,
    admin: SocketAddr,
    stop: oneshot::Sender<()>,
    task: JoinHandle<Result<Drained, ServeError>>,
}

async fn serve(server: &Server) -> Result<Served, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let admin = TcpListener::bind("127.0.0.1:0").await?;
    let (fhir, admin_address) = (listener.local_addr()?, admin.local_addr()?);
    let (stop, stopped) = oneshot::channel::<()>();
    let task = tokio::spawn(ferroterm_server::serve_until(
        listener,
        Some(admin),
        server.serving.clone(),
        async {
            // A dropped sender also completes the future; both mean "stop".
            let _stop_or_dropped = stopped.await;
        },
    ));
    Ok(Served {
        fhir,
        admin: admin_address,
        stop,
        task,
    })
}

/// The status line and body of `GET path` on `address`.
async fn get(address: SocketAddr, path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(address).await?;
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await?;
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    Ok(response)
}

#[tokio::test]
async fn serve_until_returns_once_the_shutdown_future_completes()
-> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start();
    let running = serve(&server).await?;

    for address in [running.fhir, running.admin] {
        let response = get(address, "/health/readiness").await?;
        assert!(
            response.starts_with("HTTP/1.1 200 "),
            "{address} answers while up: {response:?}"
        );
    }

    running
        .stop
        .send(())
        .map_err(|()| "the server dropped its shutdown receiver")?;
    let drained = tokio::time::timeout(Duration::from_secs(15), running.task).await???;
    assert_eq!(Drained::Complete, drained);
    for address in [running.fhir, running.admin] {
        assert!(
            TcpStream::connect(address).await.is_err(),
            "{address} is closed"
        );
    }
    Ok(())
}

#[tokio::test]
async fn readiness_is_withdrawn_during_the_drain_delay() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start_with_server(ServerSection {
        drain_delay_ms: 600,
        ..ServerSection::default()
    });
    let running = serve(&server).await?;
    running
        .stop
        .send(())
        .map_err(|()| "the server dropped its shutdown receiver")?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let response = get(running.fhir, "/health/readiness").await?;
    assert!(
        response.starts_with("HTTP/1.1 503 "),
        "readiness is withdrawn while the listener still accepts: {response:?}"
    );
    let response = get(running.fhir, "/health/liveness").await?;
    assert!(response.starts_with("HTTP/1.1 200 "), "{response:?}");
    let drained = tokio::time::timeout(Duration::from_secs(15), running.task).await???;
    assert_eq!(Drained::Complete, drained);
    Ok(())
}

#[tokio::test]
async fn the_server_returns_once_the_shutdown_bound_passes_with_a_request_open()
-> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start_with_server(ServerSection {
        shutdown_timeout_ms: 300,
        ..ServerSection::default()
    });
    let running = serve(&server).await?;

    // A `$lookup` whose body never arrives in full keeps its handler waiting
    // on the body, well inside the request timeout.
    let mut held = TcpStream::connect(running.fhir).await?;
    held.write_all(
        b"POST /r4/CodeSystem/$lookup HTTP/1.1\r\nHost: localhost\r\n\
          Content-Type: application/fhir+json\r\nContent-Length: 1000\r\n\r\n{\"resourceType\"",
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    running
        .stop
        .send(())
        .map_err(|()| "the server dropped its shutdown receiver")?;
    let drained = tokio::time::timeout(Duration::from_secs(5), running.task)
        .await
        .map_err(|_elapsed| "the drain is bounded")???;
    // The server returns with the request still open, so `main` returns and
    // the process exits with it.
    assert_eq!(Drained::Cut, drained);
    drop(held);
    Ok(())
}
