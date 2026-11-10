// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ferroterm-sync` binary: the command line in,
//! [`ferroterm_sync::cli`] out.
#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a failed health probe and a failed job are reported on standard error, and `run-once` answers a person"
)]

use std::process::ExitCode;
use std::sync::Arc;

use ferrohealth_server::cli::{Common, ConfigCommand};
use ferrohealth_server::run::{Failure, Run, config_check};
use ferrohealth_server::telemetry::{Installed, install};
use ferrohealth_telemetry::telemetry::{Service, Telemetry};
use ferroterm_sync::cli::{Cli, Command};
use ferroterm_sync::config::Config;
use ferroterm_sync::record::Outcome;

fn main() -> ExitCode {
    Run::new("ferroterm-sync").run(std::env::args_os(), dispatch)
}

/// Runs the job `cli` names; a configuration the service refuses exits
/// `EX_CONFIG`, except for `healthcheck`, which reports the service not up.
async fn dispatch(cli: Cli) -> Result<ExitCode, Failure> {
    let job = match &cli.command {
        Some(Command::Common(common)) => common.clone(),
        Some(Command::RunOnce) | None => Common::Serve,
    };
    let loaded = ferroterm_sync::cli::load(&cli.config).map_err(|error| job.refused(error))?;
    match cli.command {
        Some(Command::Common(Common::Config {
            command: ConfigCommand::Check,
        })) => config_check(&loaded),
        Some(Command::Common(Common::Healthcheck)) => Ok(healthcheck(loaded.settings()).await),
        Some(Command::RunOnce) => {
            let config = loaded.into_parts().0;
            let installed = telemetry(&config)?;
            let code = Box::pin(run_once(config, Arc::clone(installed.telemetry()))).await;
            installed.shutdown();
            Ok(code)
        }
        Some(Command::Common(Common::Serve)) | None => {
            let config = loaded.into_parts().0;
            let installed = telemetry(&config)?;
            let code = Box::pin(serve(config, Arc::clone(installed.telemetry()))).await;
            installed.shutdown();
            Ok(code)
        }
    }
}

/// Builds and installs the telemetry `[telemetry]` describes, with the
/// family's panic line.
fn telemetry(config: &Config) -> Result<Installed, Failure> {
    let service = Service::new("ferroterm-sync", env!("CARGO_PKG_VERSION"));
    install(&config.telemetry, service)
        .map_err(|error| Failure::from(error).with_context("cannot start"))
}

/// `ferroterm-sync healthcheck`: silent when ready, the reason otherwise.
async fn healthcheck(config: &Config) -> ExitCode {
    let outcome = ferroterm_sync::cli::healthcheck(config).await;
    if outcome.exit_code() == 0 {
        return ExitCode::SUCCESS;
    }
    eprintln!("ferroterm-sync healthcheck: {outcome}");
    ExitCode::FAILURE
}

/// Serves until the process is stopped.
async fn serve(config: Config, telemetry: Arc<Telemetry>) -> ExitCode {
    match ferroterm_sync::cli::serve(config, telemetry).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let error = anyhow::Error::from(error);
            eprintln!("ferroterm-sync: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Performs one run, reports it, and exits non-zero when the run failed.
async fn run_once(config: Config, telemetry: Arc<Telemetry>) -> ExitCode {
    match ferroterm_sync::cli::run_once(config, telemetry).await {
        Ok(record) => {
            println!(
                "{}: {} in {} ms, {} entries taken, {} activated",
                record.id,
                record.outcome.as_str(),
                record.duration_ms,
                record.entries_taken,
                record.activation.activated.len()
            );
            for error in &record.errors {
                eprintln!("ferroterm-sync: {error}");
            }
            match record.outcome {
                Outcome::Ok => ExitCode::SUCCESS,
                Outcome::Failed => ExitCode::FAILURE,
            }
        }
        Err(error) => {
            let error = anyhow::Error::from(error);
            eprintln!("ferroterm-sync: {error:#}");
            ExitCode::FAILURE
        }
    }
}
