// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ferroterm` binary: configuration in, [`ferroterm_server::serve`] out.
//!
//! The order is the one that makes the console right: read the configuration,
//! print the banner when a person is reading, install the telemetry, load
//! the artifacts (their summary is the first thing logged), bind, serve.
#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a failed health probe is reported before any log subscriber exists, and `config check` prints the running tree for a person"
)]

use std::io::IsTerminal;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use ferrohealth_server::cli::{Common, ConfigCommand};
use ferrohealth_server::run::{EXIT_CONFIG, EXIT_FAILURE, Failure, Run};
use ferrohealth_server::telemetry::build as build_telemetry;
use ferrohealth_telemetry::telemetry::{Console, Service, Telemetry};
use ferroterm_server::cli::{Cli, Command};
use ferroterm_server::config::Config;
use ferroterm_server::reload::Serving;
use ferroterm_server::smart::Smart;
use ferroterm_server::state::AppState;
use ferroterm_server::{banner, reload};
use tokio::net::TcpListener;

fn main() -> ExitCode {
    let started = Instant::now();
    Run::new("ferroterm").run(std::env::args_os(), |cli: Cli| dispatch(cli, started))
}

/// Runs the job `cli` names; a configuration the server refuses exits
/// `EX_CONFIG`, except for `healthcheck`, which reports the server not up.
async fn dispatch(cli: Cli, started: Instant) -> Result<ExitCode, Failure> {
    let refused = if cli.command == Some(Command::Common(Common::Healthcheck)) {
        EXIT_FAILURE
    } else {
        EXIT_CONFIG
    };
    let loaded = Config::load(cli.config.config.as_deref(), &cli.config.sets).map_err(|error| {
        Failure::from(error)
            .with_context("cannot start")
            .with_code(refused)
    })?;
    match cli.command {
        Some(Command::Common(Common::Healthcheck)) => {
            return Ok(healthcheck(&loaded.settings().server).await);
        }
        Some(Command::Common(Common::Config {
            command: ConfigCommand::Check,
        })) => {
            let report = ferrohealth_config::report::check_report(&loaded);
            println!(
                "{}",
                report.map_err(|error| Failure::from(error).with_code(EXIT_CONFIG))?
            );
            return Ok(ExitCode::SUCCESS);
        }
        Some(Command::Common(Common::Serve)) | None => {}
    }
    let (config, _provenance) = loaded.into_parts();
    banner::print(
        config
            .telemetry
            .format
            .resolve(std::io::stdout().is_terminal()),
    );
    let service = Service::new("ferroterm", env!("CARGO_PKG_VERSION"));
    let telemetry = Arc::new(
        build_telemetry(&config.telemetry, service)
            .map_err(|error| Failure::from(error).with_context("cannot start"))?,
    );
    telemetry
        .install(&config.telemetry, &Console::detect())
        .map_err(|error| Failure::from(error).with_context("cannot start"))?;
    ferrohealth_telemetry::panic::install_hook();
    let outcome = match run(config, Arc::clone(&telemetry), started).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error = format!("{error:#}"), "cannot start");
            ExitCode::FAILURE
        }
    };
    if let Err(error) = telemetry.shutdown() {
        tracing::warn!(%error, "the telemetry did not flush");
    }
    Ok(outcome)
}

/// `ferroterm healthcheck`: silent when the server is ready, the reason on
/// stderr otherwise.
async fn healthcheck(section: &ferrohealth_server::section::ServerSection) -> ExitCode {
    let outcome = ferroterm_server::healthcheck(section).await;
    if outcome.exit_code() == 0 {
        return ExitCode::SUCCESS;
    }
    eprintln!("ferroterm healthcheck: {outcome}");
    ExitCode::FAILURE
}

async fn run(config: Config, telemetry: Arc<Telemetry>, started: Instant) -> anyhow::Result<()> {
    use anyhow::Context;

    let state = AppState::load(&config).context("loading the artifacts terminology.index names")?;
    let summaries = state
        .summaries()
        .context("summarising the loaded code systems")?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        fhir_versions = ferroterm_server::served::SEGMENTS.join(","),
        code_systems = summaries.len(),
        "ferroterm starting"
    );
    if summaries.is_empty() {
        tracing::warn!(
            "no code systems loaded: set terminology.index to one or more artifact directories"
        );
    }
    for summary in &summaries {
        tracing::info!(
            id = summary.id,
            system = summary.url,
            version = summary.version,
            concepts = summary.concepts,
            languages = summary.languages.join(","),
            path = summary.path.as_ref().map(|p| p.display().to_string()),
            "serving code system"
        );
    }
    // The issuers are read before the listener binds, so a server that cannot
    // check a credential never answers on a surface it has declared protected.
    let smart = Smart::start(&config)
        .await
        .context("starting the resource server [auth] names")?
        .map(Arc::new);
    match &smart {
        Some(smart) => tracing::info!(
            issuer = smart.issuer(),
            token_endpoint = smart.token_endpoint(),
            admin_scope = smart.admin_scope(),
            "requiring a credential on the write routes and the admin listener"
        ),
        None if !config.auth.enabled
            && (config.terminology.resources.is_some() || config.admin_listen().is_some()) =>
        {
            tracing::warn!(
                "[auth] is off: the write routes and the admin listener authenticate no caller"
            );
        }
        None => {}
    }
    let listener = TcpListener::bind(&config.server.listen)
        .await
        .with_context(|| {
            format!(
                "binding {} (set server.listen to change it)",
                config.server.listen
            )
        })?;
    tracing::info!(
        listen = %config.server.listen,
        bases = ferroterm_server::served::BASES.join(","),
        "listening"
    );
    let admin = match config.admin_listen() {
        Some(address) => {
            let bound = TcpListener::bind(address).await.with_context(|| {
                format!("binding the admin listener on {address} (set admin.listen to change it)")
            })?;
            tracing::info!(listen = %address, route = "/reload", "listening for admin requests");
            Some(bound)
        }
        None => None,
    };
    let serving = Serving::with_smart(config, Arc::new(state), smart, telemetry);
    // NOTE: dropping the handle detaches the task, which then runs until the
    // process ends (<https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html>).
    let _hangup = tokio::spawn(reload::on_hangup(serving.clone()));
    // The report never blocks boot: the task it starts is detached the same way.
    let ci = ferrohealth_usage_report::section::ci_of(|name| std::env::var(name).ok());
    let _usage = serving.start_usage_report(started, ci).await;
    let drained = ferroterm_server::serve(listener, admin, serving)
        .await
        .context("serving HTTP")?;
    tracing::info!(?drained, "ferroterm stopped");
    Ok(())
}
