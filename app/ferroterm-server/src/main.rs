//! The `ferroterm` binary: configuration in, [`ferroterm_server::serve`] out.
//!
//! The order is the one that makes the console right: read the configuration,
//! print the banner when a person is reading, install the log subscriber, load
//! the artifacts (their summary is the first thing logged), bind, serve.
#![expect(
    clippy::print_stderr,
    reason = "a refused configuration and a failed health probe are reported before any log subscriber exists"
)]

use std::io::IsTerminal;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use ferroterm_server::cli::{Cli, Command};
use ferroterm_server::config::{ADMIN_LISTEN_ENV, Config, INDEX_ENV, LISTEN_ENV};
use ferroterm_server::reload::Serving;
use ferroterm_server::smart::Smart;
use ferroterm_server::state::AppState;
use ferroterm_server::telemetry::ResolvedFormat;
use ferroterm_server::{banner, healthcheck, reload, telemetry, usage_report};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> ExitCode {
    let started = std::time::Instant::now();
    let cli = Cli::parse();
    if let Some(Command::Healthcheck { url }) = cli.command {
        return healthcheck_main(url).await;
    }
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("ferroterm: cannot start: {error}");
            return ExitCode::FAILURE;
        }
    };
    let stdout_is_terminal = std::io::stdout().is_terminal();
    if config.log_format.resolve(stdout_is_terminal) == ResolvedFormat::Pretty {
        banner::print();
    }
    if let Err(error) = telemetry::init(config.log_format, &config.log_filter, stdout_is_terminal) {
        eprintln!("ferroterm: cannot start: {error}");
        return ExitCode::FAILURE;
    }
    match run(config, started).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error = format!("{error:#}"), "cannot start");
            ExitCode::FAILURE
        }
    }
}

/// `ferroterm healthcheck`: silent on `200 OK`, the reason on stderr otherwise.
async fn healthcheck_main(url: Option<String>) -> ExitCode {
    let url = url.unwrap_or_else(healthcheck::url_from_env);
    match healthcheck::probe(&url).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let error = anyhow::Error::from(error);
            eprintln!("ferroterm healthcheck: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(config: Config, started: std::time::Instant) -> anyhow::Result<()> {
    use anyhow::Context;

    let state = AppState::load(&config)
        .with_context(|| format!("loading the artifacts named by {INDEX_ENV}"))?;
    let summaries = state
        .summaries()
        .context("summarising the loaded code systems")?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        fhir_versions = ferroterm_server::FHIR_BASES
            .map(|base| base.trim_start_matches('/'))
            .join(","),
        code_systems = summaries.len(),
        "ferroterm starting"
    );
    if summaries.is_empty() {
        tracing::warn!(
            "no code systems loaded: set {INDEX_ENV} to one or more artifact directories"
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
            && (config.resources.is_some() || config.admin_listen.is_some()) =>
        {
            tracing::warn!(
                "[auth] is off: the write routes and the admin listener authenticate no caller"
            );
        }
        None => {}
    }
    let listener = TcpListener::bind(&config.listen)
        .await
        .with_context(|| format!("binding {} (set {LISTEN_ENV} to change it)", config.listen))?;
    tracing::info!(
        listen = %config.listen,
        bases = ferroterm_server::FHIR_BASES.join(","),
        "listening"
    );
    let admin = match &config.admin_listen {
        Some(address) => {
            let bound = TcpListener::bind(address).await.with_context(|| {
                format!(
                    "binding the admin listener on {address} (set {ADMIN_LISTEN_ENV} to change it)"
                )
            })?;
            tracing::info!(listen = %address, route = "/reload", "listening for admin requests");
            Some(bound)
        }
        None => None,
    };
    let serving = Serving::with_smart(config, Arc::new(state), smart);
    // NOTE: dropping the handle detaches the task, which then runs until the
    // process ends (<https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html>).
    let _hangup = tokio::spawn(reload::on_hangup(serving.clone()));
    // The report never blocks boot: the task it starts is detached the same way.
    let _usage =
        usage_report::reporter::start(&serving.config().usage_report, &serving, started).await;
    match admin {
        Some(admin) => {
            let fhir = ferroterm_server::serve(listener, serving.clone());
            let admin = ferroterm_server::serve_admin_until(
                admin,
                serving,
                ferroterm_server::shutdown_signal(),
            );
            let ((), ()) = tokio::try_join!(fhir, admin).context("serving HTTP")?;
        }
        None => ferroterm_server::serve(listener, serving)
            .await
            .context("serving HTTP")?,
    }
    tracing::info!("ferroterm stopped");
    Ok(())
}
