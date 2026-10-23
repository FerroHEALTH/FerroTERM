// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The binary's run path to its exit code: a usage error, a configuration
//! the console refuses (`EX_CONFIG`, BSD `sysexits.h`), `config check` and
//! `healthcheck`.

use std::error::Error;
use std::fmt;
use std::path::Path;
use std::process::ExitCode;

use ferroterm_console::command::{EXIT_CONFIG, chain, run};

/// Runs the binary with `args` after its name.
fn exit(args: &[&str]) -> ExitCode {
    run(std::iter::once("ferroterm-console")
        .chain(args.iter().copied())
        .map(str::to_owned))
}

/// Writes `text` as a configuration file under `dir` and returns its path.
fn config_file(dir: &Path, text: &str) -> String {
    let path = dir.join("console.toml");
    std::fs::write(&path, text).expect("the file is written");
    path.to_string_lossy().into_owned()
}

#[test]
fn a_command_line_clap_refuses_exits_with_its_usage_code() {
    assert_eq!(ExitCode::from(2), exit(&["frobnicate"]));
    assert_eq!(ExitCode::from(2), exit(&[]), "a job is required");
}

#[test]
fn the_version_flag_exits_successfully() {
    assert_eq!(ExitCode::SUCCESS, exit(&["--version"]));
}

#[test]
fn config_check_accepts_a_valid_configuration() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = config_file(dir.path(), "[server]\nlisten = \"127.0.0.1:0\"\n");
    assert_eq!(
        ExitCode::SUCCESS,
        exit(&["--config", &path, "config", "check"])
    );
}

#[test]
fn a_configuration_that_does_not_resolve_exits_with_ex_config() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = config_file(dir.path(), "[ferroterm]\ntimeout_ms = 0\n");
    assert_eq!(
        ExitCode::from(EXIT_CONFIG),
        exit(&["--config", &path, "config", "check"])
    );
    assert_eq!(
        ExitCode::from(EXIT_CONFIG),
        exit(&["--config", &path, "serve"]),
        "serve refuses it before it binds anything"
    );
}

#[test]
fn a_configuration_file_that_does_not_parse_exits_with_ex_config() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = config_file(dir.path(), "[server]\nunknown_key = 1\n");
    assert_eq!(
        ExitCode::from(EXIT_CONFIG),
        exit(&["--config", &path, "config", "check"])
    );
}

#[test]
fn a_configuration_file_that_cannot_be_read_exits_with_ex_config() {
    let dir = tempfile::tempdir().expect("a directory");
    let missing = dir.path().join("absent.toml");
    assert_eq!(
        ExitCode::from(EXIT_CONFIG),
        exit(&["--config", &missing.to_string_lossy(), "config", "check"])
    );
}

#[test]
fn a_healthcheck_with_a_refused_configuration_reports_not_up_rather_than_ex_config() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = config_file(dir.path(), "[ferroterm]\ntimeout_ms = 0\n");
    assert_eq!(ExitCode::FAILURE, exit(&["--config", &path, "healthcheck"]));
}

#[test]
fn a_healthcheck_with_no_console_listening_fails() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let address = listener.local_addr().expect("an address");
    drop(listener);
    let dir = tempfile::tempdir().expect("a directory");
    let path = config_file(dir.path(), &format!("[server]\nlisten = \"{address}\"\n"));
    assert_eq!(ExitCode::FAILURE, exit(&["--config", &path, "healthcheck"]));
}

#[test]
fn a_healthcheck_of_a_serving_console_succeeds() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .expect("a free port");
    let address = listener.local_addr().expect("an address");
    let state = crate::provider::console(None, "");
    let router = ferroterm_console::server::router(state);
    runtime.spawn(ferroterm_console::server::serve(
        listener,
        router,
        std::future::pending(),
    ));
    let dir = tempfile::tempdir().expect("a directory");
    let path = config_file(dir.path(), &format!("[server]\nlisten = \"{address}\"\n"));
    assert_eq!(ExitCode::SUCCESS, exit(&["--config", &path, "healthcheck"]));
}

/// An error with an optional cause, for the chain.
#[derive(Debug)]
struct Layer {
    text: &'static str,
    source: Option<Box<Layer>>,
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.text)
    }
}

impl Error for Layer {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|layer| -> &(dyn Error + 'static) { layer })
    }
}

#[test]
fn an_error_renders_with_every_cause_in_its_chain() {
    let error = Layer {
        text: "cannot start",
        source: Some(Box::new(Layer {
            text: "the file cannot be read",
            source: Some(Box::new(Layer {
                text: "permission denied",
                source: None,
            })),
        })),
    };
    assert_eq!(
        "cannot start: the file cannot be read: permission denied",
        chain(&error)
    );
    assert_eq!(
        "alone",
        chain(&Layer {
            text: "alone",
            source: None
        })
    );
}
