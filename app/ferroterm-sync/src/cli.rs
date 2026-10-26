// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The command line, and what each command does.
//!
//! With no subcommand the process serves: it binds the admin listener and runs
//! the schedule until it is stopped. `run-once` performs one run and exits,
//! which is what a cron-style deployment or a first manual run uses.

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tokio::net::TcpListener;

use crate::clock::SystemClock;
use crate::config::{Config, ConfigError};
use crate::record::{RunRecord, Trigger};
use crate::run::{Service, ServiceError};
use crate::source::RegistryError;

/// The `ferroterm-sync` command line.
#[derive(Debug, Parser)]
#[command(
    name = "ferroterm-sync",
    version,
    about = "The FerroTERM synchronisation service"
)]
pub struct Cli {
    /// The configuration file to read.
    #[arg(long, value_name = "FILE", default_value = "/etc/ferroterm/sync.toml")]
    pub config: PathBuf,
    /// The subcommand, or `None` to serve.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// What the binary does besides serving.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Performs one run and exits.
    RunOnce,
    /// Asks the running service for `GET /health` and exits 0 on `200 OK`.
    ///
    /// The image has no shell and no HTTP client, so this is its HEALTHCHECK
    /// command. Without `--url` the probe follows the configured listen
    /// address, on the loopback when the service listens on every interface.
    Healthcheck {
        /// The URL to probe instead of the one the configuration implies.
        #[arg(long, value_name = "URL")]
        url: Option<String>,
    },
}

/// A service that could not start.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// The configuration file does not read.
    #[error("the configuration does not read")]
    Config(#[from] ConfigError),
    /// A configured source cannot be built.
    #[error("a configured source cannot be built")]
    Registry(#[from] RegistryError),
    /// The service cannot be built.
    #[error("the service cannot be built")]
    Service(#[from] ServiceError),
    /// The admin listener's `[auth]` cannot guard it.
    #[error("the admin listener cannot be guarded")]
    Auth(#[from] crate::auth::AuthError),
    /// The admin listener does not bind.
    #[error("cannot bind the admin listener on {address}")]
    Bind {
        /// The address the service tried to bind.
        address: String,
        /// Why it did not bind.
        #[source]
        source: std::io::Error,
    },
    /// The admin listener stopped with an error.
    #[error("the admin listener stopped")]
    Serve {
        /// Why it stopped.
        #[source]
        source: std::io::Error,
    },
}

/// Builds the service the configuration at `path` describes.
///
/// # Errors
///
/// Returns [`StartError::Config`] when the file does not read,
/// [`StartError::Registry`] when a source cannot be built, and
/// [`StartError::Service`] when the service cannot be built.
pub fn service(path: &std::path::Path) -> Result<Arc<Service>, StartError> {
    let config = Config::load(path)?;
    let sources = crate::source::configure(&config.source)?;
    Ok(Arc::new(Service::new(
        config,
        sources,
        Arc::new(SystemClock),
    )?))
}

/// Serves the admin listener and runs the schedule until the process stops.
///
/// # Errors
///
/// Returns [`StartError::Auth`] when `[auth]` cannot guard the admin
/// listener, [`StartError::Bind`] when the listener does not bind and
/// [`StartError::Serve`] when it stops with an error; the start-up errors of
/// [`service`] reach the caller unchanged.
pub async fn serve(path: &std::path::Path) -> Result<(), StartError> {
    let service = service(path)?;
    let guard = crate::auth::AdminAuth::start(&service.config().auth).await?;
    let unguarded = crate::admin::router(Arc::clone(&service));
    let admin = if let Some(guard) = &guard {
        guard.guard(unguarded)
    } else {
        tracing::warn!(
            "[auth] enabled = false: the admin listener authenticates nobody, so only trusted callers may reach it"
        );
        unguarded
    };
    let address = service.config().listen;
    let listener = TcpListener::bind(address)
        .await
        .map_err(|source| StartError::Bind {
            address: address.to_string(),
            source,
        })?;
    tracing::info!(
        listen = %address,
        activation = service.config().activation.as_str(),
        sources = service.config().source.len(),
        guarded = guard.is_some(),
        "ferroterm-sync started"
    );
    let lifecycle = service.lifecycle().clone();
    let draining = async move {
        shutdown_signal().await;
        lifecycle.drain();
    };
    let admin = axum::serve(listener, admin).with_graceful_shutdown(draining);
    let schedule = Arc::clone(&service).serve_schedule(shutdown_signal());
    let served = tokio::join!(admin, schedule);
    let (admin, ()) = served;
    admin.map_err(|source| StartError::Serve { source })?;
    tracing::info!("ferroterm-sync stopped");
    Ok(())
}

/// Performs one run and answers its record.
///
/// # Errors
///
/// Returns the start-up errors of [`service`]; a run that fails is a record
/// with a failed outcome, never an error here.
pub async fn run_once(path: &std::path::Path) -> Result<RunRecord, StartError> {
    let service = service(path)?;
    Ok(service.run(Trigger::RunOnce).await)
}

/// The address the health probe reads, from `url` or the configuration.
///
/// # Errors
///
/// Returns [`StartError::Config`] when no address was given and the
/// configuration does not read.
pub fn health_url(path: &std::path::Path, url: Option<String>) -> Result<String, StartError> {
    match url {
        Some(url) => Ok(url),
        None => Ok(crate::healthcheck::url_for(Config::load(path)?.listen)),
    }
}

/// Installs the log subscriber.
///
/// `RUST_LOG` is the filter; without it the service logs at `info` and leaves
/// the HTTP stack at `warn`.
///
/// # Errors
///
/// Returns the reason when a subscriber is already installed.
pub fn init_logging() -> Result<(), tracing_subscriber::util::TryInitError> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,hyper=warn,tower=warn"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .try_init()
}

/// Completes on `SIGTERM` or `SIGINT`, which is how a container stops.
async fn shutdown_signal() {
    let interrupt = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "cannot listen for SIGINT");
        }
    };
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                if signal.recv().await.is_none() {
                    tracing::debug!("the SIGTERM stream ended");
                }
            }
            Err(error) => tracing::error!(%error, "cannot listen for SIGTERM"),
        }
    };
    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use clap::Parser;

    use super::{Cli, Command};

    #[test]
    fn no_subcommand_means_serve() -> Result<(), clap::Error> {
        let cli = Cli::try_parse_from(["ferroterm-sync"])?;
        assert_eq!(cli.command, None, "the process serves by default");
        Ok(())
    }

    #[test]
    fn run_once_is_a_subcommand() -> Result<(), clap::Error> {
        let cli = Cli::try_parse_from(["ferroterm-sync", "--config", "/tmp/s.toml", "run-once"])?;
        assert_eq!(
            cli.command,
            Some(Command::RunOnce),
            "one run and exit is its own command"
        );
        assert_eq!(
            cli.config,
            std::path::PathBuf::from("/tmp/s.toml"),
            "the configuration file is named on the command line"
        );
        Ok(())
    }
}
