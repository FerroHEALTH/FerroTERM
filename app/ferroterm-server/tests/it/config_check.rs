// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What `ferroterm config check` prints of a configured issuer: an issuer
//! identifier written with a password in its userinfo renders masked, on
//! both streams (RFC 3986 §3.2.1: the password is never rendered in clear
//! text), in an `[auth.issuer.<name>]` table and in an issuer carrying each
//! of the four tables that sit under an issuer.

use std::process::Command;

/// The `[auth.issuer.<name>]` of an issuer written with a synthetic password.
const ISSUER: &str = "[auth]\nenabled = true\n\n[auth.issuer.idp]\nissuer = \"https://u:SYNTH_ISSUER_PW@as.example.org/realms/ferro\"\nkind = \"persons\"\naudience = [\"https://term.example.org/fhir\"]\n";

/// An issuer carrying each table that sits under an issuer, by the host it
/// names, written with a synthetic password in the issuer's userinfo.
const TABLES: [(&str, &str); 4] = [
    (
        "attributes.example.org",
        "\n[auth.issuer.attributes]\nissuer = \"https://u:SYNTH_ISSUER_PW@attributes.example.org/realms/ferro\"\nkind = \"persons\"\naudience = [\"https://term.example.org/fhir\"]\n\n[auth.issuer.attributes.attributes]\nmapping = \"iua\"\n",
    ),
    (
        "federation.example.org",
        "\n[auth.issuer.federation]\nissuer = \"https://u:SYNTH_ISSUER_PW@federation.example.org/realms/ferro\"\nkind = \"federation\"\naudience = [\"https://term.example.org/fhir\"]\n\n[auth.issuer.federation.federation]\ncontact_point = false\n",
    ),
    (
        "party.example.org",
        "\n[auth.issuer.party]\nissuer = \"https://u:SYNTH_ISSUER_PW@party.example.org/realms/ferro\"\nkind = \"persons\"\naudience = [\"https://term.example.org/fhir\"]\n\n[auth.issuer.party.party]\nidentifier_issuer = \"urn:example:people\"\nidentifier_type = \"guid\"\n",
    ),
    (
        "scopeless.example.org",
        "\n[auth.issuer.scopeless]\nissuer = \"https://u:SYNTH_ISSUER_PW@scopeless.example.org/realms/ferro\"\nkind = \"machines\"\naudience = [\"https://term.example.org/fhir\"]\nscopeless_subjects = [\"service\"]\n",
    ),
];

/// Runs `ferroterm config check` over `text` and returns both streams.
fn config_check(text: &str) -> (String, String) {
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("issuer.toml");
    std::fs::write(&path, text).expect("the file is written");
    let output = Command::new(env!("CARGO_BIN_EXE_ferroterm"))
        .arg("--config")
        .arg(&path)
        .args(["config", "check"])
        .env("FERROTERM__USAGE_REPORT__ENABLED", "false")
        .output()
        .expect("the binary runs");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn config_check_never_renders_an_issuers_userinfo() {
    let (stdout, stderr) = config_check(ISSUER);
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

#[test]
fn config_check_never_renders_the_userinfo_of_an_issuer_carrying_a_table() {
    for (host, table) in TABLES {
        let (stdout, stderr) = config_check(&format!("{ISSUER}{table}"));
        assert!(
            stdout.contains(&format!("https://***@{host}/realms/ferro")),
            "{host}: the issuer renders masked: {stdout}{stderr}"
        );
        for shown in [&stdout, &stderr] {
            assert!(
                !shown.contains("SYNTH_ISSUER_PW"),
                "{host}: no issuer password reaches config check"
            );
        }
    }
}
