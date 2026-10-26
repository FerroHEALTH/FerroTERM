// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The command line: one binary, one subcommand per job.
//!
//! `serve` runs the console, `config check` reads and resolves the
//! configuration the way `serve` would and exits, and `healthcheck` asks the
//! console on this host whether it answers, for a container runtime with no
//! HTTP client of its own. No specification governs the command line: our own
//! design.

use clap::Parser;
use ferrohealth_server::cli::{Common, ConfigArgs};

/// The `ferroterm-console` command line.
#[derive(Debug, Parser, PartialEq, Eq)]
#[command(name = "ferroterm-console", version, about = "The FerroTERM Console")]
pub struct Cli {
    /// `--config`, overriding `FERROTERM_CONSOLE_CONFIG`, and `--set`.
    #[command(flatten)]
    pub config: ConfigArgs,
    /// The job to run: the family's `serve`, `config check` and
    /// `healthcheck`.
    #[command(subcommand)]
    pub command: Common,
}

#[cfg(test)]
mod tests {
    use ferrohealth_server::cli::{Common, ConfigCommand};
    use ferrohealth_server::healthcheck::target;
    use std::net::SocketAddr;

    use super::Cli;
    use clap::{CommandFactory, Parser};

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_job_parses_with_and_without_a_configuration_file() {
        for (args, command) in [
            (vec!["ferroterm-console", "serve"], Common::Serve),
            (
                vec!["ferroterm-console", "config", "check"],
                Common::Config {
                    command: ConfigCommand::Check,
                },
            ),
            (
                vec!["ferroterm-console", "healthcheck"],
                Common::Healthcheck,
            ),
        ] {
            let parsed = Cli::try_parse_from(&args).expect("the job parses");
            assert_eq!(command, parsed.command, "{args:?}");
            assert_eq!(None, parsed.config.config, "{args:?}");
        }
        let parsed = Cli::try_parse_from(["ferroterm-console", "serve", "--config", "viewer.toml"])
            .expect("the job parses");
        assert_eq!(Some("viewer.toml".into()), parsed.config.config);
    }

    #[test]
    fn a_wildcard_listen_address_is_asked_on_loopback_and_any_other_as_it_is() {
        for (listen, asked) in [
            ("0.0.0.0:3000", "127.0.0.1:3000"),
            ("[::]:3000", "[::1]:3000"),
            ("10.0.0.7:3000", "10.0.0.7:3000"),
        ] {
            let listen: SocketAddr = listen.parse().expect("a socket address");
            let asked: SocketAddr = asked.parse().expect("a socket address");
            assert_eq!(asked, target(listen), "{listen}");
        }
    }
}
