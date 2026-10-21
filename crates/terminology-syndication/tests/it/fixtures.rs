// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading the synthetic fixtures and the vendored listings from disk.

use std::path::PathBuf;

/// The directory holding the synthetic Atom documents.
fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// The directory holding the vendored public listings.
fn vendored() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/feeds")
}

/// One synthetic fixture, by file name.
pub(crate) fn synthetic(name: &str) -> String {
    let path = fixtures().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// One vendored listing, by file name.
pub(crate) fn corpus(name: &str) -> String {
    let path = vendored().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}
