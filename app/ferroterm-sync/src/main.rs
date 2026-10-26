// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ferroterm-sync` binary: the command line in,
//! [`ferroterm_sync::cli`] out.
#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a refused configuration is reported before any log subscriber exists, and `run-once` reports its outcome to a person"
)]

use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use ferrohealth_server::cli::{Common, ConfigCommand};
use ferrohealth_telemetry::telemetry::{Console, Service, Telemetry};
use ferroterm_sync::cli::{Cli, Command};
use ferroterm_sync::config::Config;
use ferroterm_sync::record::Outcome;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let loaded = match ferroterm_sync::cli::load(&cli.config) {
        Ok(loaded) => loaded,
        Err(error) => {
            let error = anyhow::Error::from(error);
            eprintln!("ferroterm-sync: cannot start: {error:#}");
            return ExitCode::FAILURE;
        }
    };
    match cli.command {
        Some(Command::Common(Common::Config {
            command: ConfigCommand::Check,
        })) => match ferrohealth_config::report::check_report(&loaded) {
            Ok(report) => {
                println!("{report}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("ferroterm-sync config check: {error}");
                ExitCode::FAILURE
            }
        },
        Some(Command::Common(Common::Healthcheck)) => healthcheck(loaded.settings()).await,
        Some(Command::RunOnce) => {
            let config = loaded.into_parts().0;
            match telemetry(&config) {
                Ok(telemetry) => {
                    finished(run_once(config, Arc::clone(&telemetry)).await, &telemetry)
                }
                Err(code) => code,
            }
        }
        Some(Command::Common(Common::Serve)) | None => {
            let config = loaded.into_parts().0;
            match telemetry(&config) {
                Ok(telemetry) => finished(
                    Box::pin(serve(config, Arc::clone(&telemetry))).await,
                    &telemetry,
                ),
                Err(code) => code,
            }
        }
    }
}

/// Builds and installs the telemetry `[telemetry]` describes, with the
/// family's panic line.
fn telemetry(config: &Config) -> Result<Arc<Telemetry>, ExitCode> {
    let service = Service::new("ferroterm-sync", env!("CARGO_PKG_VERSION"));
    let telemetry = match Telemetry::new(&config.telemetry, service) {
        Ok(telemetry) => Arc::new(telemetry),
        Err(error) => {
            eprintln!("ferroterm-sync: cannot start: {error}");
            return Err(ExitCode::FAILURE);
        }
    };
    if let Err(error) = telemetry.install(&config.telemetry, &Console::detect()) {
        eprintln!("ferroterm-sync: cannot start: {error}");
        return Err(ExitCode::FAILURE);
    }
    ferrohealth_telemetry::panic::install_hook();
    Ok(telemetry)
}

/// Flushes the telemetry before the process exits with `code`.
fn finished(code: ExitCode, telemetry: &Telemetry) -> ExitCode {
    if let Err(error) = telemetry.shutdown() {
        tracing::warn!(%error, "the telemetry did not flush");
    }
    code
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
