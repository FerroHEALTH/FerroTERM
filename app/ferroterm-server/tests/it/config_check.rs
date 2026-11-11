// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What `ferroterm config check` prints of a configured issuer: an issuer
//! identifier written with a password in its userinfo renders masked, on
//! both streams (RFC 3986 §3.2.1: the password is never rendered in clear
//! text).

use std::process::Command;

/// The `[[auth.issuer]]` of an issuer written with a synthetic password.
const ISSUER: &str = "[auth]\nenabled = true\n\n[[auth.issuer]]\nissuer = \"https://u:SYNTH_ISSUER_PW@as.example.org/realms/ferro\"\nkind = \"persons\"\naudience = [\"https://term.example.org/fhir\"]\n";

#[test]
fn config_check_never_renders_an_issuers_userinfo() {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("issuer.toml");
    std::fs::write(&path, ISSUER).expect("the file is written");
    let output = Command::new(env!("CARGO_BIN_EXE_ferroterm"))
        .arg("--config")
        .arg(&path)
        .args(["config", "check"])
        .env("FERROTERM__USAGE_REPORT__ENABLED", "false")
        .output()
        .expect("the binary runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("https://***@as.example.org/realms/ferro"),
        "the issuer renders masked: {stdout}{stderr}"
    );
    for shown in [&stdout, &stderr] {
        assert!(
            !shown.contains("SYNTH_ISSUER_PW"),
            "no issuer password reaches config check"
        );
    }
}
