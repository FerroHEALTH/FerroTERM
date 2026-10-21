// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ferroterm healthcheck` against a served instance and against nothing.

use std::net::SocketAddr;
use std::process::{Command, Output};
use std::time::Duration;

use ferroterm_server::config::LISTEN_ENV;
use ferroterm_server::healthcheck::{self, HealthcheckError};
use http::StatusCode;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::fixture::Server;

/// The fixture edition served on an ephemeral loopback port, until `stop`.
struct Served {
    address: SocketAddr,
    stop: oneshot::Sender<()>,
    task: JoinHandle<std::io::Result<()>>,
}

async fn serve() -> Result<Served, Box<dyn std::error::Error>> {
    let server = Server::start();
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (stop, stopped) = oneshot::channel::<()>();
    let serving = server.serving.clone();
    let task = tokio::spawn(ferroterm_server::serve_until(listener, serving, async {
        let _stop_or_dropped = stopped.await;
    }));
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

/// Runs the built binary off the runtime thread, so the served instance in
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
async fn the_probe_passes_a_serving_instance_and_names_why_it_fails_otherwise()
-> Result<(), Box<dyn std::error::Error>> {
    let served = serve().await?;
    let url = healthcheck::url_of(&served.address.to_string());
    assert_eq!(url, format!("http://{}/health", served.address));
    healthcheck::probe(&url).await?;

    let not_found = healthcheck::probe(&format!("http://{}/nope", served.address)).await;
    assert!(
        matches!(
            not_found,
            Err(HealthcheckError::Status { status, .. }) if status == StatusCode::NOT_FOUND
        ),
        "a route that is not /health is a status failure: {not_found:?}"
    );

    let closed = closed_port().await?;
    let refused = healthcheck::probe(&healthcheck::url_of(&closed.to_string())).await;
    assert!(
        matches!(refused, Err(HealthcheckError::Request { .. })),
        "nothing listening is a request failure: {refused:?}"
    );

    let unparsable = healthcheck::probe("not a url").await;
    assert!(
        matches!(unparsable, Err(HealthcheckError::Url { .. })),
        "{unparsable:?}"
    );

    served
        .stop
        .send(())
        .map_err(|()| "the server dropped its shutdown receiver")?;
    tokio::time::timeout(Duration::from_secs(5), served.task).await???;
    Ok(())
}

#[tokio::test]
async fn the_subcommand_exits_zero_against_a_serving_instance_and_non_zero_against_a_stopped_one()
-> Result<(), Box<dyn std::error::Error>> {
    let served = serve().await?;
    let address = served.address.to_string();

    // With `--url`: the documented exit codes, and nothing on stdout on success.
    let explicit = ferroterm(
        &["healthcheck", "--url", &format!("http://{address}/health")],
        &[],
    )
    .await?;
    assert!(
        explicit.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&explicit.stderr)
    );
    assert!(explicit.stdout.is_empty(), "a passing probe prints nothing");

    // Without `--url`: the probe follows FERROTERM_LISTEN, the way the image
    // runs it.
    let from_env = ferroterm(&["healthcheck"], &[(LISTEN_ENV, &address)]).await?;
    assert!(
        from_env.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&from_env.stderr)
    );

    served
        .stop
        .send(())
        .map_err(|()| "the server dropped its shutdown receiver")?;
    tokio::time::timeout(Duration::from_secs(5), served.task).await???;

    // The same address, now with nothing behind it.
    let stopped = ferroterm(&["healthcheck"], &[(LISTEN_ENV, &address)]).await?;
    assert!(!stopped.status.success(), "a stopped server is unhealthy");
    let stderr = String::from_utf8_lossy(&stopped.stderr);
    assert!(
        stderr.starts_with("ferroterm healthcheck: cannot reach http://"),
        "the reason names the URL: {stderr}"
    );
    Ok(())
}
