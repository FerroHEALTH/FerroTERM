// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ferroterm-fixture` writes the scaled edition where it is told, and
//! refuses a command line it does not know.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_ferroterm-fixture");

#[test]
fn the_scaled_edition_is_written_into_a_directory_it_creates() {
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().join("index").join("snomed");
    let status = Command::new(BIN)
        .args(["snomed-scaled", dir.to_str().expect("UTF-8"), "100"])
        .status()
        .expect("runs");
    assert!(status.success(), "{status}");
    for file in ["store.redb", "hierarchy.bin", "text.bin", "manifest.json"] {
        assert!(dir.join(file).is_file(), "{file} is written");
    }
}

#[test]
fn an_unknown_fixture_or_a_count_that_is_no_number_is_a_usage_error() {
    let root = tempfile::tempdir().expect("tempdir");
    let dir = root.path().to_str().expect("UTF-8");
    for args in [
        vec!["snomed", dir, "100"],
        vec!["snomed-scaled", dir, "many"],
        vec!["snomed-scaled", dir],
    ] {
        let status = Command::new(BIN).args(&args).status().expect("runs");
        assert_eq!(Some(2), status.code(), "{args:?}");
    }
}
