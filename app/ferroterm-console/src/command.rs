// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The run path of the `ferroterm-console` binary: the command line parsed,
//! the configuration read, and each job run to its exit code.

use std::process::ExitCode;

use ferrohealth_server::cli::{Common, ConfigCommand};
use ferrohealth_server::run::{EXIT_CONFIG, Failure, Run, chain};
use ferrohealth_server::telemetry::install;
use ferrohealth_telemetry::telemetry::Service;

use crate::cli::Cli;
use crate::config::Config;
use crate::config::settings::Settings;
use crate::server::ConsoleState;

/// How long `healthcheck` waits for the readiness answer.
pub const HEALTHCHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Runs the binary with `args` and returns the process exit code.
///
/// `args` is the whole argument vector, the program name included, so the
/// command-line parser reports usage under the right name. A configuration
/// the console refuses exits `EX_CONFIG`
/// ([`ferrohealth_server::run::EXIT_CONFIG`]), except for `healthcheck`,
/// which reports the console not up.
#[must_use]
pub fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    Run::new("ferroterm-console").run(args, dispatch)
}

/// Reads the configuration and runs the job `cli` names.
async fn dispatch(cli: Cli) -> Result<ExitCode, Failure> {
    let settings = Config::load(cli.config.config.as_deref(), &cli.config.sets)
        .and_then(Settings::from_loaded)
        .map_err(|error| cli.command.refused(error))?;
    match cli.command {
        Common::Healthcheck => Ok(healthcheck(&settings).await),
        Common::Config {
            command: ConfigCommand::Check,
        } => config_check(&settings),
        Common::Serve => serve(settings).await,
    }
}

/// Runs `config check`: the settings are resolved and the terminology
/// server's client is built, and nothing is bound or asked, so the check
/// works offline; the provider's metadata is read when the console starts.
#[expect(
    clippy::print_stdout,
    reason = "the job's one answer goes to standard output"
)]
fn config_check(settings: &Settings) -> Result<ExitCode, Failure> {
    crate::server::upstream_client(settings).map_err(Failure::refused_configuration)?;
    println!("ferroterm-console: the configuration is valid");
    Ok(ExitCode::SUCCESS)
}

/// Runs `healthcheck` against the console this configuration describes.
#[expect(
    clippy::print_stdout,
    reason = "the job's one-line answer goes to standard output"
)]
#[expect(
    clippy::print_stderr,
    reason = "the job's one-line refusal goes to standard error"
)]
async fn healthcheck(settings: &Settings) -> ExitCode {
    let target = ferrohealth_server::healthcheck::target(settings.listen);
    let outcome = ferrohealth_server::healthcheck::check(target, "", HEALTHCHECK_TIMEOUT).await;
    if outcome.exit_code() == 0 {
        println!("ferroterm-console: {outcome}");
        ExitCode::SUCCESS
    } else {
        eprintln!("ferroterm-console: {outcome}");
        ExitCode::FAILURE
    }
}

/// Runs `serve`: the subscriber, the state, and the server until `SIGTERM`
/// or `SIGINT`.
async fn serve(settings: Settings) -> Result<ExitCode, Failure> {
    let service = Service::new("ferroterm-console", env!("CARGO_PKG_VERSION"))
        .export_crate("ferroterm_console");
    let installed = install(&settings.telemetry, service)
        .map_err(|error| Failure::from(error).with_context("cannot start the log"))?;
    let listen = settings.listen;
    let state = match ConsoleState::new(settings).await {
        Ok(state) => state,
        Err(error) => {
            tracing::error!(error = %chain(&error), "cannot start");
            return Ok(ExitCode::from(EXIT_CONFIG.get()));
        }
    };
    let listener = match tokio::net::TcpListener::bind(listen).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(%listen, %error, "cannot bind the listen address");
            return Ok(ExitCode::FAILURE);
        }
    };
    tracing::info!(%listen, "the operator console is listening");
    let lifecycle = state.lifecycle().clone();
    let served = crate::server::serve(
        listener,
        crate::server::router(state),
        lifecycle,
        ferrohealth_server::serve::shutdown_signal(),
    );
    let code = match served.await {
        Ok(_drained) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "the listener failed");
            ExitCode::FAILURE
        }
    };
    installed.shutdown();
    Ok(code)
}
