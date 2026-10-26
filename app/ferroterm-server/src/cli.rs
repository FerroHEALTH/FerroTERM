// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The command line: no subcommand serves, `config check` prints the running
//! configuration, `healthcheck` probes a server.

use clap::{Parser, Subcommand};
use ferrohealth_server::cli::{ConfigArgs, ConfigCommand};

/// The `ferroterm` command line.
///
/// Serving is the default so the image's exec-form entrypoint keeps working
/// with no arguments; a subcommand is something the container runs beside it.
#[derive(Debug, Parser)]
#[command(
    name = "ferroterm",
    version,
    about = "The FerroTERM FHIR terminology server"
)]
pub struct Cli {
    /// `--config` and `--set`, which every subcommand takes.
    #[command(flatten)]
    pub config: ConfigArgs,
    /// The subcommand, or `None` to serve.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// What the binary does besides serving.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Works on the configuration.
    Config {
        /// The job to run.
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Asks a running server for `GET /health` and exits 0 on `200 OK`.
    ///
    /// The image has no shell and no HTTP client, so this is its HEALTHCHECK
    /// command. Without `--url` the probe follows `server.listen`, on the
    /// loopback when the server listens on every interface.
    Healthcheck {
        /// The URL to probe instead of the one `server.listen` implies.
        #[arg(long, value_name = "URL")]
        url: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use ferrohealth_server::cli::ConfigCommand;

    use super::{Cli, Command};

    #[test]
    fn no_arguments_means_serve() {
        let cli = Cli::try_parse_from(["ferroterm"]).expect("parses");
        assert_eq!(cli.command, None);
    }

    #[test]
    fn healthcheck_takes_an_optional_url() {
        let defaulted = Cli::try_parse_from(["ferroterm", "healthcheck"]).expect("parses");
        assert_eq!(defaulted.command, Some(Command::Healthcheck { url: None }));
        let explicit =
            Cli::try_parse_from(["ferroterm", "healthcheck", "--url", "http://h:1/health"])
                .expect("parses");
        assert_eq!(
            explicit.command,
            Some(Command::Healthcheck {
                url: Some(String::from("http://h:1/health")),
            })
        );
    }

    #[test]
    fn config_check_takes_the_file_and_the_sets() {
        let cli = Cli::try_parse_from([
            "ferroterm",
            "config",
            "check",
            "--config",
            "/etc/ferroterm.toml",
            "--set",
            "server.listen=0.0.0.0:8080",
        ])
        .expect("parses");
        assert_eq!(
            cli.command,
            Some(Command::Config {
                command: ConfigCommand::Check,
            })
        );
        assert_eq!(
            cli.config.config.as_deref(),
            Some(std::path::Path::new("/etc/ferroterm.toml"))
        );
        assert_eq!(cli.config.sets.len(), 1);
    }

    #[test]
    fn an_unknown_subcommand_is_refused() {
        assert!(
            Cli::try_parse_from(["ferroterm", "serve"]).is_err(),
            "an unknown word must not fall through to serving"
        );
    }
}
