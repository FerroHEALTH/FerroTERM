// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The binary's exit code on a configuration the synchronisation service refuses:
//! `EX_CONFIG` (78, `sysexits(3)`) for `config check` and `serve`, and 1
//! for `healthcheck`, which reports the synchronisation service not up.

use std::process::{Command, Output};

/// Runs the built binary with a configuration file holding a table it does
/// not know, followed by `arguments`.
fn refused(arguments: &[&str]) -> Output {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("refused.toml");
    std::fs::write(&path, "[unknown_table]\nkey = 1\n").expect("the file is written");
    Command::new(env!("CARGO_BIN_EXE_ferroterm-sync"))
        .arg("--config")
        .arg(&path)
        .args(arguments)
        .output()
        .expect("the binary runs")
}

#[test]
fn a_refused_configuration_exits_with_ex_config() {
    for job in [&["config", "check"][..], &["serve"][..]] {
        let output = refused(job);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(Some(78), output.status.code(), "{job:?}: {stderr}");
        assert!(
            stderr.starts_with("ferroterm-sync: cannot start: ")
                && stderr.contains("unknown_table"),
            "{job:?} names the refused key: {stderr}"
        );
    }
}

#[test]
fn a_healthcheck_with_a_refused_configuration_exits_one() {
    let output = refused(&["healthcheck"]);
    assert_eq!(
        Some(1),
        output.status.code(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
