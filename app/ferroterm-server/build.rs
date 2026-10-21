// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Reads the release date of the version being built from `CHANGELOG.md`
//! (`## [<version>] - <date>`) into `FERROTERM_RELEASE_DATE`, for
//! `CapabilityStatement.software.releaseDate`. An unreleased version has no
//! heading of its own and the statement omits the date; a changelog that
//! cannot be read and a heading whose date is malformed are warned about, and
//! the statement omits the date rather than carrying a value the FHIR
//! specification does not admit.

use std::path::Path;

// The parser is the library's, so the source the build runs and the source the
// tests exercise cannot drift.
include!("src/release_date.rs");

fn main() {
    release_date_env();
    git_sha_env();
}

/// Reads `CHANGELOG.md` for the release date of the version being built.
fn release_date_env() {
    let changelog = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../CHANGELOG.md");
    println!("cargo:rerun-if-changed={}", changelog.display());
    println!("cargo:rerun-if-changed=src/release_date.rs");
    let text = match std::fs::read_to_string(&changelog) {
        Ok(text) => text,
        Err(error) => {
            println!(
                "cargo::warning=cannot read {}: {error}; the capability statement omits its release date",
                changelog.display()
            );
            return;
        }
    };
    match release_date(&text, env!("CARGO_PKG_VERSION")) {
        ReleaseDate::Released(date) => {
            println!("cargo:rustc-env=FERROTERM_RELEASE_DATE={date}");
        }
        ReleaseDate::Unreleased => {}
        ReleaseDate::Malformed(date) => println!(
            "cargo::warning=the heading of the version being built in {} says `{date}`, which is no FHIR date; the capability statement omits its release date",
            changelog.display()
        ),
    }
}

/// Records the commit being built into `FERROTERM_GIT_SHA`, for the usage
/// report.
///
/// The release lane builds on a GitHub runner, which names the commit in
/// `GITHUB_SHA`; `FERROTERM_GIT_SHA` overrides it. A build that names none
/// records the empty string, and the report then states seven zeros.
fn git_sha_env() {
    println!("cargo::rerun-if-env-changed=FERROTERM_GIT_SHA");
    println!("cargo::rerun-if-env-changed=GITHUB_SHA");
    let sha = std::env::var("FERROTERM_GIT_SHA")
        .or_else(|_unset| std::env::var("GITHUB_SHA"))
        .unwrap_or_default();
    println!("cargo:rustc-env=FERROTERM_GIT_SHA={}", sha.trim());
}
