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

use clap::Parser;
use ferrohealth_server::cli::ConfigCommand;
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
        Some(Command::Config {
            command: ConfigCommand::Check,
        }) => match ferrohealth_config::report::check_report(&loaded) {
            Ok(report) => {
                println!("{report}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("ferroterm-sync config check: {error}");
                ExitCode::FAILURE
            }
        },
        Some(Command::Healthcheck { url }) => healthcheck(loaded.settings(), url).await,
        Some(Command::RunOnce) => logged(run_once(loaded.into_parts().0)).await,
        None => Box::pin(logged(serve(loaded.into_parts().0))).await,
    }
}

/// Installs the log subscriber and then runs `command`.
async fn logged(command: impl Future<Output = ExitCode>) -> ExitCode {
    if let Err(error) = ferroterm_sync::cli::init_logging() {
        eprintln!("ferroterm-sync: cannot start: {error}");
        return ExitCode::FAILURE;
    }
    command.await
}

/// `ferroterm-sync healthcheck`: silent on `200 OK`, the reason otherwise.
async fn healthcheck(config: &Config, url: Option<String>) -> ExitCode {
    let url = ferroterm_sync::cli::health_url(config, url);
    match ferroterm_sync::healthcheck::probe(&url).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let error = anyhow::Error::from(error);
            eprintln!("ferroterm-sync healthcheck: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Serves until the process is stopped.
async fn serve(config: Config) -> ExitCode {
    match ferroterm_sync::cli::serve(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let error = anyhow::Error::from(error);
            eprintln!("ferroterm-sync: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Performs one run, reports it, and exits non-zero when the run failed.
async fn run_once(config: Config) -> ExitCode {
    match ferroterm_sync::cli::run_once(config).await {
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
