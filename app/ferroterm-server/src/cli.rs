// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The command line: no subcommand serves, and the family's `serve`,
//! `config check` and `healthcheck` are the subcommands.

use clap::{Parser, Subcommand};
use ferrohealth_server::cli::{Common, ConfigArgs};

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

/// What the binary does: the family's commands.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// `serve`, `config check` and `healthcheck`.
    #[command(flatten)]
    Common(Common),
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use ferrohealth_server::cli::{Common, ConfigCommand};

    use super::{Cli, Command};

    #[test]
    fn no_arguments_means_serve() {
        let cli = Cli::try_parse_from(["ferroterm"]).expect("parses");
        assert_eq!(cli.command, None);
        let serve = Cli::try_parse_from(["ferroterm", "serve"]).expect("parses");
        assert_eq!(serve.command, Some(Command::Common(Common::Serve)));
    }

    #[test]
    fn healthcheck_is_a_subcommand() {
        let probe = Cli::try_parse_from(["ferroterm", "healthcheck"]).expect("parses");
        assert_eq!(probe.command, Some(Command::Common(Common::Healthcheck)));
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
            Some(Command::Common(Common::Config {
                command: ConfigCommand::Check,
            }))
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
            Cli::try_parse_from(["ferroterm", "frobnicate"]).is_err(),
            "an unknown word must not fall through to serving"
        );
    }
}
