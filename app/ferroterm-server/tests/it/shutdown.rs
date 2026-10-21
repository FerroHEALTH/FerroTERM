// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server stops when its shutdown future completes, after answering the
//! requests in flight.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

use crate::fixture::Server;

#[tokio::test]
async fn serve_until_returns_once_the_shutdown_future_completes()
-> Result<(), Box<dyn std::error::Error>> {
    let server_state = Server::start();
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (stop, stopped) = oneshot::channel::<()>();
    let serving = server_state.serving.clone();
    let server = tokio::spawn(ferroterm_server::serve_until(listener, serving, async {
        // A dropped sender also completes the future; both mean "stop".
        let _stop_or_dropped = stopped.await;
    }));

    // The server answers while up: one HTTP/1.1 request over a plain socket.
    let mut stream = TcpStream::connect(address).await?;
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    assert!(
        response.starts_with("HTTP/1.1 200 "),
        "unexpected response: {response:?}"
    );

    stop.send(())
        .map_err(|()| "the server dropped its shutdown receiver")?;
    let outcome = tokio::time::timeout(Duration::from_secs(5), server).await??;
    outcome?;
    Ok(())
}
