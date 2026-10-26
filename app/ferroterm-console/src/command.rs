// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The run path of the `ferroterm-console` binary: the command line parsed,
//! the configuration read, and each job run to its exit code.

use std::error::Error;
use std::fmt::Write as _;
use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Command, ConfigCommand};
use crate::config::Config;
use crate::config::settings::Settings;
use crate::server::ConsoleState;

/// The exit code of a configuration the console will not start on, from the
/// BSD `sysexits.h` set (`EX_CONFIG`).
pub const EXIT_CONFIG: u8 = 78;

/// Runs the binary with `args` and returns the process exit code.
///
/// `args` is the whole argument vector, the program name included, so the
/// command-line parser reports usage under the right name.
#[must_use]
#[expect(
    clippy::print_stderr,
    reason = "a refused configuration is reported before any log subscriber exists"
)]
pub fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            // NOTE: no specification governs this: our own design; clap prints
            // the usage or the version itself and names the exit code.
            let code = error.exit_code();
            if error.print().is_err() {
                return ExitCode::FAILURE;
            }
            return u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from);
        }
    };
    let settings = match Config::load(cli.config.as_deref()).and_then(|config| config.resolve()) {
        Ok(settings) => settings,
        Err(error) if cli.command == Command::Healthcheck => {
            eprintln!("ferroterm-console: not up: {}", chain(&error));
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("ferroterm-console: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    match cli.command {
        Command::Healthcheck => runtime_then(|| healthcheck(&settings)),
        Command::Config {
            command: ConfigCommand::Check,
        } => config_check(settings),
        Command::Serve => runtime_then(|| serve(settings)),
    }
}

/// Runs `job` on a multi-threaded runtime.
#[expect(
    clippy::print_stderr,
    reason = "a runtime that cannot start is reported before any log subscriber exists"
)]
fn runtime_then<F, J>(job: J) -> ExitCode
where
    J: FnOnce() -> F,
    F: Future<Output = ExitCode>,
{
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(job()),
        Err(error) => {
            eprintln!("ferroterm-console: cannot start the runtime: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Runs `config check`: the state the server would start with is built, and
/// nothing is bound.
#[expect(
    clippy::print_stdout,
    reason = "the job's one answer goes to standard output"
)]
#[expect(
    clippy::print_stderr,
    reason = "the job's refusal goes to standard error"
)]
fn config_check(settings: Settings) -> ExitCode {
    match ConsoleState::new(settings) {
        Ok(_state) => {
            println!("ferroterm-console: the configuration is valid");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ferroterm-console: cannot start: {}", chain(&error));
            ExitCode::from(EXIT_CONFIG)
        }
    }
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
    let target = crate::healthcheck::target(settings.listen);
    let outcome = crate::healthcheck::check(target, crate::healthcheck::TIMEOUT).await;
    if outcome.is_up() {
        println!("ferroterm-console: {outcome}");
        ExitCode::SUCCESS
    } else {
        eprintln!("ferroterm-console: {outcome}");
        ExitCode::FAILURE
    }
}

/// Runs `serve`: the subscriber, the state, and the server until `SIGTERM`
/// or `SIGINT`.
#[expect(
    clippy::print_stderr,
    reason = "a refusal before the subscriber starts has no other place to go"
)]
async fn serve(settings: Settings) -> ExitCode {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_unset| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
    if let Err(error) = subscriber {
        eprintln!("ferroterm-console: cannot start the log: {error}");
        return ExitCode::FAILURE;
    }
    let listen = settings.listen;
    let state = match ConsoleState::new(settings) {
        Ok(state) => state,
        Err(error) => {
            tracing::error!(error = %chain(&error), "cannot start");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let listener = match tokio::net::TcpListener::bind(listen).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(%listen, %error, "cannot bind the listen address");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!(%listen, "the operator console is listening");
    let lifecycle = state.lifecycle().clone();
    let draining = async move {
        shutdown_signal().await;
        lifecycle.drain();
    };
    match crate::server::serve(listener, crate::server::router(state), draining).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "the listener failed");
            ExitCode::FAILURE
        }
    }
}

/// Completes when the process receives `SIGTERM` or `SIGINT`.
///
/// A failure to install a handler is logged and that arm never completes, so
/// the console keeps serving and the runtime's own kill stays the backstop.
async fn shutdown_signal() {
    let interrupt = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "cannot listen for SIGINT");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => tracing::info!("SIGINT received, stopping"),
        () = terminate => tracing::info!("SIGTERM received, stopping"),
    }
}

/// Renders `error` with every cause in its chain, colon-separated.
#[must_use]
pub fn chain(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        // NOTE: no specification governs this: our own design; writing to a
        // String cannot fail, so the result carries nothing.
        if write!(text, ": {cause}").is_err() {
            break;
        }
        source = cause.source();
    }
    text
}
