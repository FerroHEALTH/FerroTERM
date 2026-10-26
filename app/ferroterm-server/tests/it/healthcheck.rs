// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ferroterm healthcheck` against a running instance, a draining one and
//! nothing.

use std::net::SocketAddr;
use std::process::{Command, Output};
use std::time::Duration;

use ferrohealth_server::healthcheck::Outcome;
use ferrohealth_server::section::ServerSection;
use ferrohealth_server::serve::{Drained, ServeError};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::fixture::Server;

/// The override of `server.listen`, which the probe follows.
const LISTEN_ENV: &str = "FERROTERM__SERVER__LISTEN";

/// The fixture edition running on an ephemeral loopback port, until `stop`.
struct Served {
    address: SocketAddr,
    stop: oneshot::Sender<()>,
    task: JoinHandle<Result<Drained, ServeError>>,
}

async fn serve(server: &Server) -> Result<Served, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (stop, stopped) = oneshot::channel::<()>();
    let task = tokio::spawn(ferroterm_server::serve_until(
        listener,
        None,
        server.serving.clone(),
        async {
            let _stop_or_dropped = stopped.await;
        },
    ));
    Ok(Served {
        address,
        stop,
        task,
    })
}

/// A loopback port nothing listens on.
async fn closed_port() -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    drop(listener);
    Ok(address)
}

/// The `[server]` section of a server listening on `listen`.
fn listening_on(listen: SocketAddr) -> ServerSection {
    ServerSection {
        listen,
        ..ServerSection::default()
    }
}

/// Runs the built binary off the runtime thread, so the running instance in
/// this same test keeps answering while the child runs.
async fn ferroterm(
    arguments: &[&str],
    environment: &[(&str, &str)],
) -> Result<Output, Box<dyn std::error::Error>> {
    let arguments: Vec<String> = arguments.iter().map(|a| (*a).to_owned()).collect();
    let environment: Vec<(String, String)> = environment
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_ferroterm"))
            .args(&arguments)
            .env_remove(LISTEN_ENV)
            .envs(environment)
            .output()
    })
    .await??;
    Ok(output)
}

#[tokio::test]
async fn the_probe_passes_a_ready_instance_and_names_why_it_fails_otherwise()
-> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start();
    let running = serve(&server).await?;
    assert_eq!(
        Outcome::Healthy,
        ferroterm_server::healthcheck(&listening_on(running.address)).await
    );

    server.serving.lifecycle().drain();
    assert_eq!(
        Outcome::Unhealthy(503),
        ferroterm_server::healthcheck(&listening_on(running.address)).await,
        "a draining server is not ready"
    );

    let closed = closed_port().await?;
    let refused = ferroterm_server::healthcheck(&listening_on(closed)).await;
    assert!(
        matches!(&refused, Outcome::Failed(reason) if reason.starts_with("cannot connect")),
        "nothing listening is a connection failure: {refused:?}"
    );

    running
        .stop
        .send(())
        .map_err(|()| "the server dropped its shutdown receiver")?;
    tokio::time::timeout(Duration::from_secs(15), running.task).await???;
    Ok(())
}

#[tokio::test]
async fn the_probe_of_an_unspecified_address_asks_the_loopback()
-> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start();
    let running = serve(&server).await?;
    let every_interface = SocketAddr::from(([0, 0, 0, 0], running.address.port()));
    assert_eq!(
        Outcome::Healthy,
        ferroterm_server::healthcheck(&listening_on(every_interface)).await
    );
    running
        .stop
        .send(())
        .map_err(|()| "the server dropped its shutdown receiver")?;
    tokio::time::timeout(Duration::from_secs(15), running.task).await???;
    Ok(())
}

#[tokio::test]
async fn the_subcommand_exits_zero_against_a_serving_instance_and_non_zero_against_a_stopped_one()
-> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start();
    let running = serve(&server).await?;
    let address = running.address.to_string();

    // The probe follows server.listen, the way the image runs it, and prints
    // nothing on success.
    let ready = ferroterm(&["healthcheck"], &[(LISTEN_ENV, &address)]).await?;
    assert!(
        ready.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&ready.stderr)
    );
    assert!(ready.stdout.is_empty(), "a passing probe prints nothing");

    running
        .stop
        .send(())
        .map_err(|()| "the server dropped its shutdown receiver")?;
    tokio::time::timeout(Duration::from_secs(15), running.task).await???;

    // The same address, now with nothing behind it.
    let stopped = ferroterm(&["healthcheck"], &[(LISTEN_ENV, &address)]).await?;
    assert!(!stopped.status.success(), "a stopped server is unhealthy");
    let stderr = String::from_utf8_lossy(&stopped.stderr);
    assert!(
        stderr.starts_with(&format!(
            "ferroterm healthcheck: unhealthy: cannot connect to {address}"
        )),
        "the reason names the address: {stderr}"
    );
    Ok(())
}
