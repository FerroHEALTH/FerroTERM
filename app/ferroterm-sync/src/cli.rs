// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The command line, and what each command does.
//!
//! With no subcommand, or with `serve`, the process serves: it binds the admin
//! listener and runs the schedule until it is stopped. `run-once` performs one
//! run and exits, which is what a cron-style deployment or a first manual run
//! uses; `config check` and `healthcheck` are the family's shared commands.

use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use ferrohealth_server::cli::{Common, ConfigArgs};
use ferrohealth_server::healthcheck::Outcome;
use ferrohealth_server::section::ServerSection;
use tokio::net::TcpListener;

use crate::clock::SystemClock;
use crate::config::{Config, ConfigError};
use crate::record::{RunRecord, Trigger};
use crate::run::{Service, ServiceError};
use crate::source::RegistryError;
use ferrohealth_telemetry::telemetry::Telemetry;

/// The `ferroterm-sync` command line.
#[derive(Debug, Parser)]
#[command(
    name = "ferroterm-sync",
    version,
    about = "The FerroTERM synchronisation service"
)]
pub struct Cli {
    /// `--config` and `--set`, which every subcommand takes.
    #[command(flatten)]
    pub config: ConfigArgs,
    /// The subcommand, or `None` to serve.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// What the binary does: the family's commands and its own.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// `serve`, `config check` and `healthcheck`.
    #[command(flatten)]
    Common(Common),
    /// Performs one run and exits.
    RunOnce,
}

/// How long the health probe waits for the readiness answer.
pub const HEALTHCHECK_TIMEOUT: Duration = Duration::from_secs(3);

/// A service that could not start.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// The configuration does not read.
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
    Serve(#[from] ferrohealth_server::serve::ServeError),
}

/// Reads the configuration `args` names.
///
/// # Errors
///
/// Returns [`StartError::Config`] when the configuration does not read.
pub fn load(args: &ConfigArgs) -> Result<ferrohealth_config::loader::Loaded<Config>, StartError> {
    Ok(Config::load(args.config.as_deref(), &args.sets)?)
}

/// Builds the service `config` describes, measured on `telemetry`.
///
/// # Errors
///
/// Returns [`StartError::Registry`] when a source cannot be built, and
/// [`StartError::Service`] when the service cannot be built.
pub fn service(config: Config, telemetry: Arc<Telemetry>) -> Result<Arc<Service>, StartError> {
    let sources = crate::source::configure(&config.source, &config.nts)?;
    Ok(Arc::new(Service::new(
        config,
        sources,
        Arc::new(SystemClock),
        telemetry,
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
pub async fn serve(config: Config, telemetry: Arc<Telemetry>) -> Result<(), StartError> {
    let service = service(config, telemetry)?;
    let guard = crate::auth::AdminAuth::start(&service.config().auth).await?;
    let unguarded = crate::admin::routes(Arc::clone(&service));
    let routes = if let Some(guard) = &guard {
        guard.guard(unguarded)
    } else {
        tracing::warn!(
            "[auth] enabled = false: the admin listener authenticates nobody, so only trusted callers may reach it"
        );
        unguarded
    };
    let admin = crate::admin::app(&service, routes);
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
    let section = ServerSection {
        listen: address,
        ..ServerSection::default()
    };
    let admin = ferrohealth_server::serve::run(
        listener,
        admin,
        &section,
        service.lifecycle().clone(),
        ferrohealth_server::serve::shutdown_signal(),
    );
    let schedule =
        Arc::clone(&service).serve_schedule(ferrohealth_server::serve::shutdown_signal());
    let (admin, ()) = tokio::join!(admin, schedule);
    admin?;
    tracing::info!("ferroterm-sync stopped");
    Ok(())
}

/// Performs one run and answers its record.
///
/// # Errors
///
/// Returns the start-up errors of [`service`]; a run that fails is a record
/// with a failed outcome, never an error here.
pub async fn run_once(config: Config, telemetry: Arc<Telemetry>) -> Result<RunRecord, StartError> {
    let service = service(config, telemetry)?;
    Ok(service.run(Trigger::RunOnce).await)
}

/// Asks the running service for its readiness over the loopback when it
/// listens on every interface, as the container probe does.
pub async fn healthcheck(config: &Config) -> Outcome {
    let target = ferrohealth_server::healthcheck::target(config.listen);
    ferrohealth_server::healthcheck::check(target, "", HEALTHCHECK_TIMEOUT).await
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use clap::Parser;

    use ferrohealth_server::cli::{Common, ConfigCommand};
    use ferrohealth_server::healthcheck::target;

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
            cli.config.config,
            Some(std::path::PathBuf::from("/tmp/s.toml")),
            "the configuration file is named on the command line"
        );
        Ok(())
    }

    #[test]
    fn config_check_is_a_subcommand() -> Result<(), clap::Error> {
        let cli =
            Cli::try_parse_from(["ferroterm-sync", "config", "check", "--set", "retention=3"])?;
        assert_eq!(
            cli.command,
            Some(Command::Common(Common::Config {
                command: ConfigCommand::Check,
            }))
        );
        assert_eq!(cli.config.sets.len(), 1, "--set follows the subcommand");
        Ok(())
    }

    #[test]
    fn serve_and_healthcheck_are_the_family_s_subcommands() -> Result<(), clap::Error> {
        let serve = Cli::try_parse_from(["ferroterm-sync", "serve"])?;
        assert_eq!(serve.command, Some(Command::Common(Common::Serve)));
        let probe = Cli::try_parse_from(["ferroterm-sync", "healthcheck"])?;
        assert_eq!(probe.command, Some(Command::Common(Common::Healthcheck)));
        Ok(())
    }

    #[test]
    fn a_service_on_every_interface_is_probed_on_the_loopback() {
        assert_eq!(
            target("0.0.0.0:8181".parse().expect("an address")),
            "127.0.0.1:8181".parse().expect("an address"),
            "a container reaches itself on the loopback"
        );
    }

    #[test]
    fn a_named_address_is_probed_as_it_is() {
        assert_eq!(
            target("127.0.0.1:9000".parse().expect("an address")),
            "127.0.0.1:9000".parse().expect("an address"),
            "the configured address is the one probed"
        );
    }
}
