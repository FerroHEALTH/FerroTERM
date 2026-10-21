// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The command line: no subcommand serves, `healthcheck` probes a server.

use clap::{Parser, Subcommand};

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
    /// The subcommand, or `None` to serve.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// What the binary does besides serving.
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Asks a running server for `GET /health` and exits 0 on `200 OK`.
    ///
    /// The image has no shell and no HTTP client, so this is its HEALTHCHECK
    /// command. Without `--url` the probe follows `FERROTERM_LISTEN`, on the
    /// loopback when the server listens on every interface.
    Healthcheck {
        /// The URL to probe instead of the one `FERROTERM_LISTEN` implies.
        #[arg(long, value_name = "URL")]
        url: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use clap::Parser;

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
    fn an_unknown_subcommand_is_refused() {
        assert!(
            Cli::try_parse_from(["ferroterm", "serve"]).is_err(),
            "an unknown word must not fall through to serving"
        );
    }
}
